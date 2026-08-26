//! Demo readiness: a synthetic home network, end to end.
//!
//! This is the network JRX will be opened on in front of someone. It runs the
//! real classification rules and the real IEEE registry, and asserts the three
//! questions in the M3.5 acceptance criterion can be answered for every device:
//!
//!   "What is this?"      -> category and family
//!   "Why do we think that?" -> supporting evidence and a rationale
//!   "What do we actually know?" -> observed facts, separated from inference

use std::net::IpAddr;

use jrx_collector::oui;
use jrx_core::device::{
    Category, Confidence, Device, DeviceFamily, DeviceTable, DiscoveryMethod, MacAddress,
    Observation,
};
use jrx_core::topology::{DiscoverySummary, Topology};

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

/// Router, MacBook, iPhone, Apple TV, printer, and one phone hiding behind a
/// randomised address.
fn home_network() -> Vec<Device> {
    let mut table = DeviceTable::new();

    // The router: seen in the neighbour cache and holding the default route.
    table.observe(
        Observation::new(ip("192.168.1.1"), DiscoveryMethod::ArpCache)
            .with_mac(MacAddress::parse("b8:27:eb:11:22:33")),
    );

    // A MacBook: file sharing and remote login.
    table.observe(
        Observation::new(ip("192.168.1.10"), DiscoveryMethod::ArpCache)
            .with_mac(MacAddress::parse("a4:83:e7:00:00:01")),
    );
    table.observe(
        Observation::new(ip("192.168.1.10"), DiscoveryMethod::Mdns)
            .with_hostname(Some("Nazars-MacBook-Pro".into()))
            .with_service("_smb._tcp")
            .with_service("_ssh._tcp"),
    );

    // An iPhone: advertises the iOS pairing service.
    table.observe(
        Observation::new(ip("192.168.1.20"), DiscoveryMethod::ArpCache)
            .with_mac(MacAddress::parse("a4:83:e7:00:00:02")),
    );
    table.observe(
        Observation::new(ip("192.168.1.20"), DiscoveryMethod::Mdns)
            .with_hostname(Some("Nazars-iPhone".into()))
            .with_service("_apple-mobdev2._tcp"),
    );

    // An Apple TV: AirPlay only, which a Mac also advertises — so the name is
    // what carries it, and only to Medium confidence.
    table.observe(
        Observation::new(ip("192.168.1.30"), DiscoveryMethod::ArpCache)
            .with_mac(MacAddress::parse("a4:83:e7:00:00:03")),
    );
    table.observe(
        Observation::new(ip("192.168.1.30"), DiscoveryMethod::Mdns)
            .with_hostname(Some("Living-Room-Apple-TV".into()))
            .with_service("_airplay._tcp")
            .with_service("_raop._tcp"),
    );

    // A printer.
    table.observe(
        Observation::new(ip("192.168.1.40"), DiscoveryMethod::Mdns)
            .with_hostname(Some("HP-LaserJet-4000".into()))
            .with_service("_ipp._tcp"),
    );

    // A phone using a randomised address and announcing nothing.
    table.observe(
        Observation::new(ip("192.168.1.50"), DiscoveryMethod::ArpCache)
            .with_mac(MacAddress::parse("9e:aa:bb:cc:dd:ee")),
    );

    table.mark_gateway(ip("192.168.1.1"));
    table.mark_self(ip("192.168.1.10"));

    table.finish(oui::vendor_of)
}

fn find<'a>(devices: &'a [Device], address: &str) -> &'a Device {
    devices
        .iter()
        .find(|d| d.id == address)
        .unwrap_or_else(|| panic!("{address} missing from the device list"))
}

#[test]
fn the_demo_network_classifies_exactly_as_expected() {
    let devices = home_network();
    assert_eq!(devices.len(), 6);

    let expect = |address: &str, category: Category, confidence: Confidence| {
        let device = find(&devices, address);
        assert_eq!(device.category(), category, "{address} category");
        assert_eq!(device.confidence(), confidence, "{address} confidence");
    };

    expect("192.168.1.1", Category::Infrastructure, Confidence::High);
    expect("192.168.1.10", Category::Computers, Confidence::High);
    expect("192.168.1.20", Category::Phones, Confidence::High);
    expect("192.168.1.30", Category::SmartHome, Confidence::Medium);
    expect("192.168.1.40", Category::SmartHome, Confidence::High);
    expect("192.168.1.50", Category::Unknown, Confidence::None);
}

#[test]
fn definitive_devices_carry_a_family_and_uncertain_ones_do_not() {
    let devices = home_network();

    assert_eq!(
        find(&devices, "192.168.1.1").inference.family,
        Some(DeviceFamily::Router)
    );
    assert_eq!(
        find(&devices, "192.168.1.10").inference.family,
        Some(DeviceFamily::Workstation)
    );
    assert_eq!(
        find(&devices, "192.168.1.20").inference.family,
        Some(DeviceFamily::Handheld)
    );
    assert_eq!(
        find(&devices, "192.168.1.40").inference.family,
        Some(DeviceFamily::Printer)
    );

    // The Apple TV was placed by its name alone, which is not enough to say
    // what kind of appliance it is.
    assert_eq!(find(&devices, "192.168.1.30").inference.family, None);
    assert_eq!(find(&devices, "192.168.1.50").inference.family, None);
}

/// "What is this? Why do we think that? What do we actually know?"
#[test]
fn every_device_can_answer_all_three_questions() {
    for device in home_network() {
        // What is this?
        assert!(
            !device.display_name().is_empty(),
            "{} has no name",
            device.id
        );

        // Why do we think that?
        assert!(
            !device.inference.rationale.is_empty(),
            "{} gives no reason",
            device.id
        );
        if device.category() != Category::Unknown {
            assert!(
                !device.inference.supporting.is_empty(),
                "{} was classified with no cited evidence",
                device.id
            );
            assert!(
                !device.inference.history.is_empty(),
                "{} has a category with no recorded reason for it",
                device.id
            );
        }

        // What do we actually know?
        assert!(
            !device.facts.sources.is_empty(),
            "{} was never actually observed",
            device.id
        );
    }
}

#[test]
fn the_randomised_phone_is_honest_about_why_it_is_unidentified() {
    let devices = home_network();
    let phone = find(&devices, "192.168.1.50");

    assert!(phone.facts.mac_randomised);
    assert_eq!(
        phone.facts.vendor, None,
        "a rotating address names no manufacturer"
    );
    assert!(phone.inference.supporting.is_empty());
    assert_eq!(phone.category(), Category::Unknown);
}

#[test]
fn observed_vendors_are_reported_without_deciding_the_category() {
    let devices = home_network();

    // The Apple TV's vendor is known...
    assert_eq!(
        find(&devices, "192.168.1.30").facts.vendor.as_deref(),
        Some("Apple")
    );
    // ...and no vendor appears among the reasons for its category.
    for device in &devices {
        assert!(
            !device
                .inference
                .supporting
                .iter()
                .any(|e| e.kind == jrx_core::device::EvidenceKind::Vendor),
            "{} cites a vendor as support",
            device.id
        );
    }
}

#[test]
fn the_topology_places_the_router_at_the_centre_and_highlights_this_mac() {
    let devices = home_network();
    let topology = Topology::build(&devices);

    let centre = topology.center.as_ref().expect("a centre");
    assert_eq!(centre.device_id, "192.168.1.1");
    assert_eq!(centre.category, Category::Infrastructure);
    assert_eq!(topology.self_id.as_deref(), Some("192.168.1.10"));

    let placed: Vec<&str> = topology
        .groups
        .iter()
        .flat_map(|g| g.devices.iter().map(|n| n.device_id.as_str()))
        .collect();
    assert_eq!(placed.len(), 5, "every device except the centre is placed");

    // Every node can explain itself where it is drawn.
    for node in topology.groups.iter().flat_map(|g| &g.devices) {
        assert!(!node.display_name.is_empty());
        assert!(!node.rationale.is_empty());
    }
}

#[test]
fn the_summary_counts_the_unidentified_device_plainly() {
    let devices = home_network();
    let summary = DiscoverySummary::of(
        &devices,
        Some(jrx_core::network::Subnet {
            network: "192.168.1.0".parse().unwrap(),
            prefix_len: 24,
        }),
    );

    assert_eq!(summary.total, 6);
    assert_eq!(summary.unidentified, 1);
    assert_eq!(summary.isolation, jrx_core::device::Isolation::Normal);
}
