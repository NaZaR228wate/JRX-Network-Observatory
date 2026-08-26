//! The rules the activity view must not break.
//!
//! Each one guards a specific way a network tool starts telling people things
//! it does not know.

#![cfg(feature = "activity-spike")]

use std::net::IpAddr;

use jrx_collector::activity::owner::{is_local, network_owner};
use jrx_collector::activity::{Protocol, nettop};

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

fn none(_: u32) -> Option<String> {
    None
}

// ---- an address is not a domain ----

/// The single most important rule. A Cloudflare address fronts millions of
/// sites; knowing the address tells us nothing about which one.
#[test]
fn a_remote_address_never_becomes_a_hostname() {
    let sample = concat!(
        "Safari.900,,,,10,20,\n",
        "tcp4 192.168.1.5:52000<->104.18.32.1:443,Established,10,20,\n",
    );
    let connections = nettop::parse(sample, none);
    let remote = connections[0].remote.as_ref().expect("a peer");

    assert_eq!(remote.address.to_string(), "104.18.32.1");
    assert_eq!(
        remote.hostname, None,
        "a hostname must never be produced from an address"
    );
    // ...even though the owner *is* known.
    assert_eq!(remote.network_owner, Some("Cloudflare"));
}

/// Knowing who owns a range is not knowing which service was used. The two
/// must stay separate fields with separate meanings.
#[test]
fn a_network_owner_is_not_a_service_name() {
    assert_eq!(network_owner(ip("104.18.32.1")), Some("Cloudflare"));
    // There is deliberately no API that turns that into "the user visited X".
    // If one is ever added, this test should be the thing that has to change.
    let sample = "Safari.900,,,,1,1,\ntcp4 10.0.0.1:1<->104.18.32.1:443,Established,1,1,\n";
    let c = nettop::parse(sample, none);
    let remote = c[0].remote.as_ref().unwrap();
    assert!(remote.hostname.is_none());
}

#[test]
fn an_address_in_no_published_range_stays_unidentified() {
    // Observed live: Telegram and Anthropic endpoints matched nothing.
    assert_eq!(network_owner(ip("149.154.167.91")), None);
    assert_eq!(network_owner(ip("160.79.104.10")), None);
}

/// IPv6 has no table here, and guessing from the IPv4 one would be inventing.
#[test]
fn ipv6_owners_are_reported_as_unknown_rather_than_guessed() {
    assert_eq!(network_owner(ip("2606:4700:20::681a:1")), None);
}

// ---- process ownership is observed, not guessed ----

#[test]
fn a_connection_with_no_owning_process_is_dropped_not_attributed() {
    let orphan = "tcp4 10.0.0.1:1<->10.0.0.2:2,Established,500,600,\n";
    assert!(
        nettop::parse(orphan, none).is_empty(),
        "a socket with no owner must not be attached to some other process"
    );
}

#[test]
fn a_truncated_process_name_is_never_extended_by_guessing() {
    let sample = concat!(
        "com.apple.WebKi.842,,,,1,1,\n",
        "tcp4 10.0.0.1:1<->10.0.0.2:2,Established,1,1,\n",
    );
    let connections = nettop::parse(sample, none);
    let process = &connections[0].process;

    assert_eq!(process.reported_name, "com.apple.WebKi");
    assert_eq!(process.full_name, None);
    assert!(
        process.name_is_truncated(),
        "the shortfall must be admitted"
    );
    assert_eq!(
        process.display(),
        "com.apple.WebKi",
        "display must show what was observed, not a completion of it"
    );
}

// ---- byte counts are measured, not modelled ----

#[test]
fn byte_counts_are_never_invented_from_a_connection_existing() {
    let sample = concat!(
        "Safari.900,,,,0,0,\n",
        "tcp4 10.0.0.1:1<->104.18.32.1:443,Established,,,\n",
    );
    let connections = nettop::parse(sample, none);

    assert_eq!(connections[0].bytes_in, 0);
    assert_eq!(connections[0].bytes_out, 0);
}

#[test]
fn a_listening_socket_reports_no_peer_and_no_traffic_to_one() {
    let sample = concat!(
        "nginx.500,,,,0,0,\n",
        "tcp4 127.0.0.1:8021<->*:*,Listen,,,\n",
    );
    let connections = nettop::parse(sample, none);
    assert!(connections[0].remote.is_none());
}

// ---- local traffic is not internet traffic ----

#[test]
fn loopback_and_private_addresses_are_recognised_as_local() {
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
        assert!(!is_local(ip(remote)), "{remote} should not be local");
    }
}

// ---- nothing about content ----

/// The activity model is checked at the source level for any field that could
/// hold content. A field that does not exist cannot be filled in later by
/// accident.
#[test]
fn the_activity_model_has_no_field_that_could_hold_content() {
    let source = std::fs::read_to_string("src/activity/mod.rs").expect("the module");

    for forbidden in [
        "payload",
        "cookie",
        "credential",
        "password",
        "body",
        "content",
        "request_uri",
        "url",
        "query",
        "user_agent",
        "certificate",
    ] {
        // Only struct fields, which are the things that get serialised.
        for line in source.lines() {
            let line = line.trim();
            let is_field = line.starts_with("pub ") && line.contains(':') && !line.contains("fn ");
            assert!(
                !(is_field && line.to_lowercase().contains(forbidden)),
                "the model gained a `{forbidden}` field: {line}"
            );
        }
    }
}

#[test]
fn the_spike_reads_only_counters_and_socket_tables() {
    let source = std::fs::read_to_string("src/activity/observe.rs").expect("the module");

    // Anything that would mean capturing rather than counting.
    for forbidden in [
        "tcpdump",
        "pcap",
        "bpf",
        "/dev/bpf",
        "tshark",
        "sudo",
        "osascript",
    ] {
        assert!(
            !source.contains(forbidden),
            "the activity spike reached for `{forbidden}`"
        );
    }
    // The two it is allowed to run.
    assert!(source.contains("/usr/bin/nettop"));
    assert!(source.contains("/usr/sbin/netstat"));
}

#[test]
fn protocols_are_reported_only_as_what_the_os_said() {
    let sample = concat!(
        "Safari.900,,,,1,1,\n",
        "tcp4 10.0.0.1:1<->10.0.0.2:2,Established,1,1,\n",
        "udp4 10.0.0.1:3<->10.0.0.2:4,,1,1,\n",
    );
    let connections = nettop::parse(sample, none);
    assert_eq!(connections[0].protocol, Protocol::Tcp);
    assert_eq!(connections[1].protocol, Protocol::Udp);
    // Port 443 is not evidence of HTTPS, and there is no field claiming it is.
}
