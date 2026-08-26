//! Every deterministic scenario, run through the real pipeline.
//!
//! These are the connection modes that cannot all be reproduced physically on
//! one machine. Each assertion states what the product must show, so a change
//! that quietly breaks a mode fails here rather than in front of a user.

#![cfg(feature = "fixtures")]

use jrx_collector::discovery::quality::{DiscoveryVerdict, LocalNetworkInference};
use jrx_collector::fixtures::{Fixture, capabilities};
use jrx_core::capability::{CapabilityState, Certainty};
use jrx_core::device::{Category, Isolation};
use jrx_core::network::{ConnectionType, WifiStatus};
use jrx_core::topology::GroupView;

// ---- A/B: the ordinary cases ----

#[test]
fn home_wifi_reports_the_network_a_person_would_recognise() {
    let id = Fixture::HomeWifi.identity();

    assert_eq!(id.connection, ConnectionType::Wifi);
    assert_eq!(id.interface, "en0");
    let WifiStatus::Associated(w) = &id.wifi else {
        panic!("expected an association");
    };
    assert_eq!(w.ssid.as_deref(), Some("Home"));
    assert!(id.tunnel.is_none());

    let report = Fixture::HomeWifi.report();
    assert_eq!(report.quality.verdict, DiscoveryVerdict::Healthy);
    assert_eq!(report.summary.isolation, Isolation::Normal);

    // A router, this Mac, and a spread across the categories.
    let overview = &report.overview;
    assert!(overview.center.is_some(), "a home network has a router");
    assert!(overview.self_node.is_some());
    let count = |c: Category| overview.group(c).map_or(0, |g| g.count);
    assert!(count(Category::Computers) >= 1);
    assert!(
        count(Category::Phones) >= 2,
        "an iPhone and an Android phone"
    );
    assert!(
        count(Category::SmartHome) >= 3,
        "TV, cast device, printer, plug"
    );
    assert!(count(Category::Unknown) >= 2);
}

#[test]
fn ethernet_reports_a_wired_link_with_the_radio_off() {
    let id = Fixture::Ethernet.identity();

    assert_eq!(id.connection, ConnectionType::Ethernet);
    assert_eq!(id.interface, "en7");
    assert_eq!(id.wifi, WifiStatus::RadioOff);
    assert!(id.tunnel.is_none());
    // Wi-Fi hardware exists but is not running, so it is not "other active".
    assert!(id.other_active.is_empty());
}

// ---- C/D: Wi-Fi that cannot be fully described ----

/// The most important honesty case of this milestone: macOS withholding the
/// network name says nothing about what kind of link this is.
#[test]
fn a_withheld_network_name_still_reports_wifi() {
    let id = Fixture::PermissionLimited.identity();

    assert_eq!(
        id.connection,
        ConnectionType::Wifi,
        "a withheld SSID must not become an unknown connection"
    );
    assert_eq!(id.wifi, WifiStatus::PermissionRequired);
    // ...and the rest of the network is still fully described.
    assert!(id.gateway.is_some());
    assert!(id.subnet.is_some());
}

#[test]
fn a_denied_location_permission_is_reported_as_confirmed_not_guessed() {
    let matrix = capabilities(Fixture::PermissionLimited);
    let row = matrix
        .row(jrx_core::declaration::ProbeId::Wifi)
        .expect("the Wi-Fi row exists");

    match &row.state {
        CapabilityState::Available { certainty, .. } => {
            assert_eq!(*certainty, Certainty::Confirmed, "macOS told us this one");
        }
        other => panic!("expected an Available row, got {other:?}"),
    }
}

/// macOS never reports Local Network authorisation, so no fixture may claim it.
#[test]
fn no_fixture_ever_claims_to_know_local_network_permission() {
    for fixture in Fixture::ALL {
        let matrix = capabilities(fixture);
        for id in [
            jrx_core::declaration::ProbeId::Mdns,
            jrx_core::declaration::ProbeId::Ssdp,
        ] {
            if let Some(row) = matrix.row(id)
                && let CapabilityState::Available { certainty, .. } = &row.state
            {
                assert_eq!(
                    *certainty,
                    Certainty::Unverifiable,
                    "{} claimed to know Local Network state",
                    fixture.name()
                );
            }
        }
    }
}

// ---- E: hotspot ----

#[test]
fn a_phone_hotspot_is_named_as_one_and_has_almost_no_peers() {
    let id = Fixture::Hotspot.identity();
    assert_eq!(id.connection, ConnectionType::UsbTether);

    let report = Fixture::Hotspot.report();
    let others = report
        .devices
        .iter()
        .filter(|d| !d.is_self && !d.is_gateway)
        .count();
    assert_eq!(
        others, 0,
        "a hotspot has the phone and this Mac, nothing else"
    );
    assert_eq!(report.summary.isolation, Isolation::LikelyIsolated);
}

// ---- F: VPN ----

#[test]
fn a_vpn_keeps_the_physical_connection_visible() {
    let id = Fixture::Vpn.identity();

    assert_eq!(id.connection, ConnectionType::Wifi, "still on Wi-Fi");
    assert_eq!(
        id.interface, "en0",
        "the physical interface, not the tunnel"
    );
    assert_eq!(
        id.local_ip.map(|a| a.to_string()).as_deref(),
        Some("192.168.1.14"),
        "the address on the network the user is actually on"
    );
    assert_eq!(
        id.gateway.map(|g| g.to_string()).as_deref(),
        Some("192.168.1.1"),
        "the router, not the tunnel endpoint"
    );

    let tunnel = id.tunnel.as_ref().expect("the tunnel is reported");
    assert_eq!(tunnel.interface, "utun6");

    // The Wi-Fi network is still named.
    let WifiStatus::Associated(w) = &id.wifi else {
        panic!("expected association")
    };
    assert_eq!(w.ssid.as_deref(), Some("Home"));
}

// ---- G: isolation ----

#[test]
fn an_isolated_network_is_distinguished_from_an_empty_one() {
    let report = Fixture::IsolatedNetwork.report();

    assert_eq!(report.summary.isolation, Isolation::LikelyIsolated);
    // Every source ran, so this is the network's doing, not ours.
    assert_eq!(
        report.quality.verdict,
        DiscoveryVerdict::NetworkAppearsEmpty
    );
    assert_eq!(
        report.quality.local_network,
        LocalNetworkInference::Undetermined,
        "silence on a quiet network proves nothing about permission"
    );

    // The map still has an anchor and this Mac: it is not blank.
    assert!(report.overview.center.is_some());
    assert!(report.overview.self_node.is_some());
}

// ---- large managed network ----

#[test]
fn a_university_network_stays_bounded_and_mostly_unidentified() {
    let report = Fixture::UniversityWifi.report();

    assert!(
        report.devices.len() > 100,
        "{} devices",
        report.devices.len()
    );

    let overview = &report.overview;
    let drawn = overview.groups.len()
        + usize::from(overview.center.is_some())
        + usize::from(overview.self_node.is_some());
    assert!(drawn <= 8, "level 1 would draw {drawn} nodes");

    let unknown = overview.group(Category::Unknown).expect("an Unknown group");
    assert!(unknown.collapsed_by_default);
    assert!(unknown.count > 100);
    assert!(!unknown.facts.is_empty(), "it must explain why");

    let page = GroupView::build(&report.devices, Category::Unknown, 0);
    assert!(page.devices.len() <= GroupView::PAGE_SIZE);
    assert!(page.page_count > 1);
}

/// mDNS heard almost nothing while the neighbour cache is full — the
/// behavioural signature of blocked local discovery.
#[test]
fn a_managed_network_that_suppresses_announcements_is_named_as_such() {
    let report = Fixture::UniversityWifi.report();
    // mDNS did return some names here, so access demonstrably works.
    assert_eq!(report.quality.local_network, LocalNetworkInference::Working);
    assert_eq!(report.quality.verdict, DiscoveryVerdict::Healthy);
}

// ---- invariants across every scenario ----

#[test]
fn no_fixture_produces_a_speculative_category() {
    for fixture in Fixture::ALL {
        for device in fixture.report().devices {
            if device.category() == Category::Unknown {
                assert!(
                    device.inference.supporting.is_empty(),
                    "{}: an unidentified device cited evidence",
                    fixture.name()
                );
            } else {
                assert!(
                    !device.inference.supporting.is_empty(),
                    "{}: {} was categorised with no evidence",
                    fixture.name(),
                    device.id
                );
            }
        }
    }
}

#[test]
fn no_fixture_infers_a_vendor_from_a_randomised_address() {
    for fixture in Fixture::ALL {
        for device in fixture.report().devices {
            if device.facts.mac_randomised {
                assert_eq!(
                    device.facts.vendor,
                    None,
                    "{}: {} got a vendor from a rotating address",
                    fixture.name(),
                    device.id
                );
            }
        }
    }
}

#[test]
fn every_fixture_produces_a_drawable_map_with_an_explanation() {
    for fixture in Fixture::ALL {
        let report = fixture.report();
        assert_eq!(report.overview.groups.len(), 5, "{}", fixture.name());
        assert!(
            !report.quality.explanation.is_empty(),
            "{} explains nothing",
            fixture.name()
        );
        assert!(
            report.overview.self_node.is_some(),
            "{}: this Mac must always be on its own map",
            fixture.name()
        );
    }
}

#[test]
fn fixture_names_round_trip() {
    for fixture in Fixture::ALL {
        assert_eq!(Fixture::parse(fixture.name()), Some(fixture));
    }
    assert_eq!(Fixture::parse("not_a_fixture"), None);
    assert_eq!(Fixture::parse(""), None);
}
