//! The rules the Activity view must not break.
//!
//! Each guards a specific way a network tool starts telling people things it
//! does not know.

use std::net::IpAddr;
use std::time::Duration;

use jrx_collector::activity::nettop;
use jrx_core::activity::owner::{application_name, is_local, network_owner};
use jrx_core::activity::session::ActivitySession;
use jrx_core::activity::{Protocol, SocketObservation};

const TICK: Duration = Duration::from_secs(1);

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}
fn unresolved(_: u32) -> Option<String> {
    None
}

fn socket(pid: u32, name: &str, remote: &str, bin: u64, bout: u64) -> SocketObservation {
    SocketObservation {
        protocol: Protocol::Tcp,
        local_address: ip("192.168.1.10"),
        local_port: 52000,
        remote_address: Some(ip(remote)),
        remote_port: Some(443),
        state: Some("Established".into()),
        bytes_in: bin,
        bytes_out: bout,
        pid,
        reported_name: name.into(),
        executable_path: None,
    }
}

// ---- an address is never a website ----

/// The single most important rule in M5. One Cloudflare address fronts
/// millions of sites; knowing the address says nothing about which.
#[test]
fn a_network_owner_never_becomes_a_hostname_or_a_website() {
    let mut session = ActivitySession::new("en0");
    session.observe_sockets(vec![socket(500, "Safari", "104.18.32.1", 0, 0)], TICK);
    session.observe_sockets(vec![socket(500, "Safari", "104.18.32.1", 1_000, 0)], TICK);

    let program = session.programs().next().expect("a program");
    let connection = &program.connections[0];

    assert_eq!(connection.network_owner, Some("Cloudflare"));

    // The serialised shape must contain no field a website could live in.
    let json = serde_json::to_string(&connection).expect("serialises");
    for forbidden in ["hostname", "domain", "website", "site", "url", "service"] {
        assert!(
            !json.contains(&format!("\"{forbidden}\"")),
            "the connection model exposes a `{forbidden}` field"
        );
    }
}

#[test]
fn an_address_in_no_published_range_stays_unidentified() {
    // Observed live: Telegram and Anthropic endpoints matched nothing.
    assert_eq!(network_owner(ip("149.154.167.91")), None);
    assert_eq!(network_owner(ip("160.79.104.10")), None);
}

#[test]
fn ipv6_owners_are_unknown_rather_than_guessed_from_the_ipv4_table() {
    assert_eq!(network_owner(ip("2606:4700:20::681a:1")), None);
}

#[test]
fn local_addresses_are_recognised_and_not_treated_as_destinations() {
    for local in [
        "127.0.0.1",
        "192.168.1.1",
        "10.0.0.5",
        "172.16.0.1",
        "169.254.1.1",
    ] {
        assert!(is_local(ip(local)), "{local} should be local");
    }
    for remote in ["104.18.32.1", "17.242.218.132", "8.8.8.8"] {
        assert!(!is_local(ip(remote)));
    }
}

// ---- ownership is observed, never guessed ----

#[test]
fn a_socket_with_no_owning_process_is_dropped_not_attributed() {
    let orphan = "tcp4 10.0.0.1:1<->10.0.0.2:2,Established,500,600,\n";
    assert!(nettop::parse(orphan, unresolved).is_empty());
}

/// Attribution comes from the executable's own path. There is deliberately no
/// table mapping process names to applications, because that would be guessing
/// dressed as knowledge.
#[test]
fn an_application_is_named_only_when_the_path_proves_it() {
    assert_eq!(
        application_name("/Applications/Telegram.app/Contents/MacOS/Telegram"),
        Some("Telegram")
    );
    // A WebKit XPC service lives in a system framework shared by every WebKit
    // client, so it cannot be attributed to Safari or anything else.
    assert_eq!(
        application_name(
            "/System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.Networking.xpc/Contents/MacOS/com.apple.WebKit.Networking"
        ),
        None
    );
    assert_eq!(application_name("/usr/libexec/rapportd"), None);
}

#[test]
fn a_reused_pid_never_inherits_the_previous_programs_traffic() {
    let mut session = ActivitySession::new("en0");
    session.observe_sockets(vec![socket(500, "Safari", "104.18.32.1", 0, 0)], TICK);
    session.observe_sockets(vec![socket(500, "Safari", "104.18.32.1", 9_000, 0)], TICK);
    session.observe_sockets(vec![], TICK);
    session.observe_sockets(vec![socket(500, "curl", "104.18.32.1", 0, 0)], TICK);
    session.observe_sockets(vec![socket(500, "curl", "104.18.32.1", 40, 0)], TICK);

    let curl = session
        .programs()
        .find(|p| p.process_name == "curl")
        .expect("curl");
    assert_eq!(
        curl.session_bytes_in, 40,
        "curl must not inherit 9000 bytes"
    );
}

// ---- byte counts are measured, never modelled ----

#[test]
fn a_newly_seen_socket_contributes_nothing_it_carried_before_jrx_looked() {
    let mut session = ActivitySession::new("en0");
    session.observe_sockets(
        vec![socket(500, "Safari", "104.18.32.1", 50_000_000, 0)],
        TICK,
    );

    let program = session.programs().next().expect("a program");
    assert_eq!(
        program.session_bytes_in, 0,
        "50 MB moved before JRX was watching and is not ours to report"
    );
}

#[test]
fn a_counter_that_went_backwards_never_becomes_negative_traffic() {
    let mut session = ActivitySession::new("en0");
    session.observe_sockets(
        vec![socket(500, "Safari", "104.18.32.1", 5_000_000, 0)],
        TICK,
    );
    session.observe_sockets(
        vec![socket(500, "Safari", "104.18.32.1", 6_000_000, 0)],
        TICK,
    );
    session.observe_sockets(vec![socket(500, "Safari", "104.18.32.1", 4_000, 0)], TICK);

    let program = session.programs().next().expect("a program");
    assert_eq!(program.session_bytes_in, 1_000_000 + 4_000);
}

/// The finding that shapes the whole session model.
#[test]
fn observed_traffic_survives_the_socket_closing() {
    let mut session = ActivitySession::new("en0");
    session.observe_sockets(
        vec![socket(500, "Safari", "104.18.32.1", 10_000_000, 0)],
        TICK,
    );
    session.observe_sockets(
        vec![socket(500, "Safari", "104.18.32.1", 12_000_000, 0)],
        TICK,
    );
    session.observe_sockets(vec![], TICK);

    let program = session.programs().next().expect("a program");
    assert_eq!(program.session_bytes_in, 2_000_000);
    assert_eq!(program.active_connections, 0);
}

// ---- nothing about content ----

#[test]
fn the_activity_model_has_no_field_that_could_hold_content() {
    let source = std::fs::read_to_string("../core/src/activity/mod.rs").expect("the module");

    for forbidden in [
        "payload",
        "cookie",
        "credential",
        "password",
        "body",
        "request_uri",
        "url",
        "query",
        "user_agent",
        "certificate",
        "sni",
        "hostname",
        "domain",
    ] {
        for line in source.lines() {
            let line = line.trim();
            let is_field = line.starts_with("pub ") && line.contains(':') && !line.contains("fn ");
            assert!(
                !(is_field && line.to_lowercase().contains(forbidden)),
                "the activity model gained a `{forbidden}` field: {line}"
            );
        }
    }
}

#[test]
fn the_macos_provider_reads_only_counters_and_socket_tables() {
    let source = std::fs::read_to_string("src/activity/macos.rs").expect("the module");

    for forbidden in [
        "tcpdump", "pcap", "bpf", "/dev/bpf", "tshark", "sudo", "dig", "nslookup",
    ] {
        assert!(
            !source.contains(forbidden),
            "the activity provider reached for `{forbidden}`"
        );
    }
    assert!(source.contains("/usr/bin/nettop"));
    assert!(source.contains("/usr/sbin/netstat"));
}

/// A port is not a protocol and must not become one. 443 is not proof of
/// HTTPS, let alone of a website.
#[test]
fn a_port_number_is_never_turned_into_a_service_name() {
    let source = std::fs::read_to_string("../core/src/activity/mod.rs").expect("the module");
    assert!(
        !source.contains("443 =>") && !source.contains("\"https\""),
        "a port-to-service mapping appeared in the activity model"
    );
}
