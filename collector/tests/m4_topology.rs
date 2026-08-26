//! M4: the topology as a product surface.
//!
//! Covers the requirements that only appear at scale — bounded rendering,
//! deterministic placement, and the degraded states the map must explain
//! rather than simply looking broken.

use std::net::IpAddr;
use std::time::Instant;

use jrx_collector::discovery::SourceStatus;
use jrx_collector::discovery::quality::{
    DiscoveryVerdict, LocalNetworkInference, SourceQuality, assess,
};
use jrx_collector::oui;
use jrx_core::device::{
    Category, Device, DeviceTable, DiscoveryMethod, Isolation, MacAddress, Observation,
    assess_isolation,
};
use jrx_core::topology::{DiscoverySummary, GroupView, TopologyOverview};

fn ip(n: usize) -> IpAddr {
    format!("10.0.{}.{}", n / 250, n % 250 + 1).parse().unwrap()
}

/// A network of `unknowns` anonymous devices, plus a router, this Mac, a
/// printer and a phone.
fn network(unknowns: usize) -> Vec<Device> {
    let mut t = DeviceTable::new();
    t.observe(
        Observation::new(ip(0), DiscoveryMethod::ArpCache)
            .with_mac(MacAddress::parse("b8:27:eb:00:00:01")),
    );
    t.observe(
        Observation::new(ip(1), DiscoveryMethod::ArpCache)
            .with_mac(MacAddress::parse("a4:83:e7:00:00:01")),
    );
    t.observe(
        Observation::new(ip(2), DiscoveryMethod::Mdns)
            .with_hostname(Some("HP-LaserJet".into()))
            .with_service("_ipp._tcp"),
    );
    t.observe(
        Observation::new(ip(3), DiscoveryMethod::Mdns)
            .with_hostname(Some("Nazars-iPhone".into()))
            .with_service("_apple-mobdev2._tcp"),
    );
    for i in 0..unknowns {
        let first = if i % 2 == 0 { 0x9e } else { 0x3c };
        let mac = format!(
            "{first:02x}:aa:bb:{:02x}:{:02x}:{:02x}",
            i / 65536,
            (i / 256) % 256,
            i % 256
        );
        t.observe(
            Observation::new(ip(i + 4), DiscoveryMethod::ArpCache)
                .with_mac(MacAddress::parse(&mac)),
        );
    }
    t.mark_gateway(ip(0));
    t.mark_self(ip(1));
    t.finish(oui::vendor_of)
}

// ---- scale ----

/// Whatever the size of the network, level 1 hands the renderer the same
/// handful of things. This is what keeps a 500-device network from becoming
/// 500 dots.
#[test]
fn the_overview_is_bounded_at_every_network_size() {
    for size in [10usize, 60, 150, 500] {
        let devices = network(size);
        let overview = TopologyOverview::build(&devices);

        let drawn = overview.groups.len()
            + usize::from(overview.center.is_some())
            + usize::from(overview.self_node.is_some());
        assert!(
            drawn <= 8,
            "level 1 would draw {drawn} nodes at {size} devices"
        );
        assert_eq!(overview.total, size + 4);
    }
}

/// A group page is bounded too, so the DOM never grows with the network.
#[test]
fn a_group_page_is_bounded_at_every_network_size() {
    for size in [10usize, 60, 150, 500] {
        let devices = network(size);
        let view = GroupView::build(&devices, Category::Unknown, 0);
        assert!(
            view.devices.len() <= GroupView::PAGE_SIZE,
            "{} nodes at {size} devices",
            view.devices.len()
        );
    }
}

/// Not a benchmark, a regression guard: these are pure transforms and should
/// stay in the sub-millisecond range. If one ever becomes visibly slow, this
/// fails long before a user notices.
#[test]
fn building_views_stays_fast_even_at_five_hundred_devices() {
    let devices = network(500);

    let started = Instant::now();
    let overview = TopologyOverview::build(&devices);
    let overview_ms = started.elapsed().as_millis();

    let started = Instant::now();
    let group = GroupView::build(&devices, Category::Unknown, 3);
    let group_ms = started.elapsed().as_millis();

    assert_eq!(overview.total, 504);
    assert!(!group.devices.is_empty());
    assert!(overview_ms < 50, "overview took {overview_ms} ms");
    assert!(group_ms < 50, "group view took {group_ms} ms");
}

// ---- stability ----

/// Rebuilding from the same devices must produce identical output, or nodes
/// would appear to move for no reason between renders.
#[test]
fn views_are_stable_across_rebuilds() {
    let devices = network(150);
    assert_eq!(
        TopologyOverview::build(&devices),
        TopologyOverview::build(&devices)
    );
    assert_eq!(
        GroupView::build(&devices, Category::Unknown, 2),
        GroupView::build(&devices, Category::Unknown, 2)
    );
}

/// Live discovery adds devices while the user is looking at the map. Devices
/// already placed must keep their position in the ordering, or the map would
/// reshuffle under the cursor.
#[test]
fn adding_devices_does_not_reorder_the_ones_already_placed() {
    let before = GroupView::build(&network(60), Category::Unknown, 0);
    let after = GroupView::build(&network(90), Category::Unknown, 0);

    let ids_before: Vec<&str> = before
        .devices
        .iter()
        .map(|n| n.device_id.as_str())
        .collect();
    let ids_after: Vec<&str> = after.devices.iter().map(|n| n.device_id.as_str()).collect();

    // Every device on the first page before is still on the first page after,
    // in the same relative order.
    let surviving: Vec<&str> = ids_after
        .iter()
        .copied()
        .filter(|id| ids_before.contains(id))
        .collect();
    let expected: Vec<&str> = ids_before
        .iter()
        .copied()
        .filter(|id| ids_after.contains(id))
        .collect();
    assert_eq!(
        surviving, expected,
        "the ordering shifted under live updates"
    );
}

// ---- anchors ----

#[test]
fn the_router_is_always_the_anchor_and_never_a_ring_node() {
    for size in [10usize, 150, 500] {
        let devices = network(size);
        let overview = TopologyOverview::build(&devices);

        let center = overview.center.as_ref().expect("a router");
        assert!(center.is_gateway);

        for group in &overview.groups {
            let view = GroupView::build(&devices, group.category, 0);
            assert!(
                view.devices.iter().all(|n| !n.is_gateway),
                "the router appeared inside {:?} at {size} devices",
                group.category
            );
        }
    }
}

#[test]
fn this_mac_is_always_identifiable() {
    for size in [10usize, 150, 500] {
        let overview = TopologyOverview::build(&network(size));
        let me = overview.self_node.as_ref().expect("this Mac");
        assert!(me.is_self);
        assert!(!me.display_name.is_empty());
    }
}

// ---- Unknown stays honest ----

#[test]
fn unknown_is_one_group_and_is_never_split_into_invented_categories() {
    let devices = network(200);
    let overview = TopologyOverview::build(&devices);

    assert_eq!(overview.groups.len(), 5, "no sixth category may appear");
    let unknown = overview.group(Category::Unknown).expect("group");
    assert!(unknown.collapsed_by_default);
    assert!(unknown.facts.iter().all(|f| !f.is_category));
    assert!(unknown.facts_are_independent);
}

/// Every node the map can draw must be able to explain itself where it is
/// drawn — the detail panel needs no second lookup.
#[test]
fn every_drawn_node_carries_its_own_explanation() {
    let devices = network(60);

    for group in TopologyOverview::build(&devices).groups {
        for node in GroupView::build(&devices, group.category, 0).devices {
            assert!(
                !node.rationale.is_empty(),
                "{} explains nothing",
                node.device_id
            );
            if node.category != Category::Unknown {
                assert!(
                    !node.evidence.is_empty(),
                    "{} cites nothing",
                    node.device_id
                );
            }
        }
    }
}

// ---- degraded states ----

fn source(method: DiscoveryMethod, observations: usize) -> SourceQuality {
    SourceQuality {
        method,
        label: method.label(),
        status: SourceStatus::Ok { observations },
        observations,
        names_resolved: 0,
        services_seen: 0,
    }
}

#[test]
fn an_isolated_network_is_recognised_and_explained() {
    let mut t = DeviceTable::new();
    t.observe(
        Observation::new(ip(0), DiscoveryMethod::ArpCache)
            .with_mac(MacAddress::parse("b8:27:eb:00:00:01")),
    );
    t.mark_gateway(ip(0));
    t.mark_self(ip(1));
    let devices = t.finish(oui::vendor_of);

    assert_eq!(
        assess_isolation(
            &devices,
            Some(jrx_core::network::Subnet {
                network: "10.0.0.0".parse().unwrap(),
                prefix_len: 24
            })
        ),
        Isolation::LikelyIsolated
    );
    assert_eq!(
        DiscoverySummary::of(
            &devices,
            Some(jrx_core::network::Subnet {
                network: "10.0.0.0".parse().unwrap(),
                prefix_len: 24
            })
        )
        .isolation,
        Isolation::LikelyIsolated
    );

    // ...and the map still has an anchor and this Mac, so it is not blank.
    let overview = TopologyOverview::build(&devices);
    assert!(overview.center.is_some());
    assert!(overview.self_node.is_some());
}

#[test]
fn an_empty_result_is_attributed_correctly_in_both_directions() {
    let working = assess(
        &[
            source(DiscoveryMethod::ArpCache, 0),
            source(DiscoveryMethod::Mdns, 0),
        ],
        0,
    );
    assert_eq!(working.verdict, DiscoveryVerdict::NetworkAppearsEmpty);

    let broken = assess(
        &[
            SourceQuality {
                status: SourceStatus::Failed {
                    reason: "arp unavailable".into(),
                },
                ..source(DiscoveryMethod::ArpCache, 0)
            },
            source(DiscoveryMethod::Mdns, 0),
        ],
        0,
    );
    assert_eq!(broken.verdict, DiscoveryVerdict::DiscoveryBlocked);
    assert!(broken.explanation.contains("arp unavailable"));
}

#[test]
fn a_network_that_blocks_announcements_is_named_as_such() {
    let quality = assess(
        &[
            source(DiscoveryMethod::ArpCache, 120),
            source(DiscoveryMethod::Mdns, 0),
        ],
        120,
    );
    assert_eq!(quality.local_network, LocalNetworkInference::LikelyBlocked);
    // Devices are still visible, so this is not an empty network.
    assert_eq!(quality.verdict, DiscoveryVerdict::Healthy);
}

/// An empty network must still produce a valid, drawable map rather than a
/// crash or a blank frame.
#[test]
fn an_entirely_empty_result_still_produces_a_valid_map() {
    let overview = TopologyOverview::build(&[]);
    assert_eq!(overview.groups.len(), 5);
    assert_eq!(overview.total, 0);
    assert!(overview.center.is_none());

    let view = GroupView::build(&[], Category::Unknown, 0);
    assert_eq!(view.page_count, 1);
    assert!(view.devices.is_empty());
}
