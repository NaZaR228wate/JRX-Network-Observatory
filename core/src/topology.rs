//! The radial topology: router at the centre, this device highlighted, and
//! everything else grouped by category.
//!
//! Data model only — the drawing happens in M4 (MVP_ROADMAP.md). Groups are
//! emitted in a fixed order so each category owns a stable angular sector and
//! devices do not jump around the ring between renders
//! (TECH_DECISIONS.md ADR-007).

use serde::Serialize;

use crate::device::{
    Category, Confidence, Device, DeviceFamily, DiscoveryMethod, Evidence, Isolation,
    assess_isolation,
};

/// One device as it appears on the map.
///
/// Carries enough to draw it *and* to answer "what is this, and why do you
/// think so?" without a second lookup. In M4 that question is one click away,
/// so the answer travels with the node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TopologyNode {
    /// References the full record in the device list.
    pub device_id: String,
    pub display_name: String,

    // ---- conclusion ----
    pub category: Category,
    pub confidence: Confidence,
    pub family: Option<DeviceFamily>,
    pub rationale: &'static str,
    /// Only the evidence that produced the conclusion — never a vendor.
    pub evidence: Vec<Evidence>,

    // ---- identity, as observed ----
    pub vendor: Option<String>,
    /// The device is rotating its hardware address on purpose. This is why an
    /// Unknown node is Unknown, and it belongs next to the node.
    pub mac_randomised: bool,
    pub sources: Vec<DiscoveryMethod>,
    pub is_self: bool,
    pub is_gateway: bool,
}

impl TopologyNode {
    fn of(device: &Device) -> TopologyNode {
        TopologyNode {
            device_id: device.id.clone(),
            display_name: device.display_name(),
            category: device.inference.category,
            confidence: device.inference.confidence,
            family: device.inference.family,
            rationale: device.inference.rationale,
            evidence: device.inference.supporting.clone(),
            vendor: device.facts.vendor.clone(),
            mac_randomised: device.facts.mac_randomised,
            sources: device.facts.sources.clone(),
            is_self: device.is_self,
            is_gateway: device.is_gateway,
        }
    }
}

/// Devices sharing one category, in one ring sector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TopologyGroup {
    pub category: Category,
    pub label: &'static str,
    pub devices: Vec<TopologyNode>,
}

/// The map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Topology {
    /// The router. The one device identified with certainty.
    pub center: Option<TopologyNode>,
    /// This machine, so the UI can highlight it.
    pub self_id: Option<String>,
    /// Always all five categories, always in the same order.
    pub groups: Vec<TopologyGroup>,
}

impl Topology {
    pub fn build(devices: &[Device]) -> Topology {
        let center = devices.iter().find(|d| d.is_gateway).map(TopologyNode::of);
        let self_id = devices.iter().find(|d| d.is_self).map(|d| d.id.clone());

        let groups = Category::ORDER
            .into_iter()
            .map(|category| TopologyGroup {
                category,
                label: category.label(),
                devices: devices
                    .iter()
                    // The centre is drawn in the middle, not in a ring.
                    .filter(|d| !d.is_gateway && d.category() == category)
                    .map(TopologyNode::of)
                    .collect(),
            })
            .collect();

        Topology {
            center,
            self_id,
            groups,
        }
    }
}

/// Counts for the header line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscoverySummary {
    pub total: usize,
    /// Devices JRX declined to categorise. Reported plainly: this is an
    /// informative outcome, not an error (TECH_DECISIONS.md ADR-008).
    pub unidentified: usize,
    pub by_category: Vec<(Category, usize)>,
    pub isolation: Isolation,
}

impl DiscoverySummary {
    pub fn of(devices: &[Device]) -> DiscoverySummary {
        DiscoverySummary {
            total: devices.len(),
            unidentified: devices
                .iter()
                .filter(|d| d.category() == Category::Unknown)
                .count(),
            by_category: Category::ORDER
                .into_iter()
                .map(|category| {
                    (
                        category,
                        devices.iter().filter(|d| d.category() == category).count(),
                    )
                })
                .collect(),
            isolation: assess_isolation(devices),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{
        Category, Confidence, DeviceTable, DiscoveryMethod, MacAddress, Observation,
    };

    fn ip(s: &str) -> std::net::IpAddr {
        s.parse().unwrap()
    }
    fn none(_: MacAddress) -> Option<&'static str> {
        None
    }

    fn home_network() -> Vec<crate::device::Device> {
        let mut t = DeviceTable::new();
        t.observe(
            Observation::new(ip("192.168.1.1"), DiscoveryMethod::ArpCache)
                .with_mac(MacAddress::parse("aa:bb:cc:00:00:01")),
        );
        t.observe(
            Observation::new(ip("192.168.1.10"), DiscoveryMethod::ArpCache)
                .with_mac(MacAddress::parse("a4:83:e7:11:22:33")),
        );
        t.observe(
            Observation::new(ip("192.168.1.20"), DiscoveryMethod::Mdns)
                .with_hostname(Some("Apple-TV.local".into()))
                .with_service("_airplay._tcp"),
        );
        t.observe(
            Observation::new(ip("192.168.1.30"), DiscoveryMethod::ArpCache)
                .with_mac(MacAddress::parse("9e:11:22:33:44:55")),
        );
        t.mark_gateway(ip("192.168.1.1"));
        t.mark_self(ip("192.168.1.10"));
        t.finish(none)
    }

    #[test]
    fn the_router_is_the_centre() {
        let topology = Topology::build(&home_network());
        assert_eq!(
            topology.center.as_ref().map(|n| n.device_id.as_str()),
            Some("192.168.1.1")
        );
    }

    #[test]
    fn this_device_is_identified_for_highlighting() {
        let topology = Topology::build(&home_network());
        assert_eq!(topology.self_id.as_deref(), Some("192.168.1.10"));
        let me = topology
            .groups
            .iter()
            .flat_map(|g| &g.devices)
            .find(|n| n.is_self)
            .expect("this device is placed");
        assert_eq!(me.device_id, "192.168.1.10");
    }

    /// The centre must not also appear in a ring, or the router would render
    /// twice.
    /// Every node must be explainable where it is drawn. In M4 a user will
    /// click a dot and ask "what is this, and why do you think so?" — the
    /// answer has to travel with the node.
    #[test]
    fn a_node_carries_its_identity_evidence_and_conclusion() {
        let topology = Topology::build(&home_network());
        let node = topology
            .groups
            .iter()
            .flat_map(|g| &g.devices)
            .find(|n| n.device_id == "192.168.1.20")
            .expect("the Apple TV is placed");

        assert_eq!(node.display_name, "Apple-TV");
        assert_eq!(node.category, Category::SmartHome);
        assert!(!node.evidence.is_empty(), "a node must carry its evidence");
        assert!(!node.rationale.is_empty(), "a node must say why");
        assert!(node.sources.contains(&DiscoveryMethod::Mdns));
    }

    /// A node JRX declined to identify must still say so out loud rather than
    /// arriving as a bare dot with no explanation.
    #[test]
    fn an_unknown_node_still_explains_itself() {
        let topology = Topology::build(&home_network());
        let node = topology
            .groups
            .iter()
            .flat_map(|g| &g.devices)
            .find(|n| n.device_id == "192.168.1.30")
            .expect("the randomised device is placed");

        assert_eq!(node.category, Category::Unknown);
        assert!(
            node.mac_randomised,
            "the reason it is unknown must travel with it"
        );
        assert!(node.rationale.to_lowercase().contains("not identified"));
    }

    /// The centre is the router, and it needs the same treatment.
    #[test]
    fn the_centre_node_is_available_with_its_evidence() {
        let topology = Topology::build(&home_network());
        let centre = topology.center.as_ref().expect("a centre");
        assert_eq!(centre.category, Category::Infrastructure);
        assert_eq!(centre.confidence, Confidence::High);
        assert!(!centre.evidence.is_empty());
    }

    #[test]
    fn the_centre_is_not_repeated_in_a_group() {
        let topology = Topology::build(&home_network());
        let placed: Vec<&str> = topology
            .groups
            .iter()
            .flat_map(|g| g.devices.iter().map(|n| n.device_id.as_str()))
            .collect();
        assert!(!placed.contains(&"192.168.1.1"));
    }

    /// Every non-centre device must be placed exactly once, or devices would
    /// silently vanish from the map.
    #[test]
    fn every_other_device_is_placed_exactly_once() {
        let devices = home_network();
        let topology = Topology::build(&devices);
        let placed: Vec<&str> = topology
            .groups
            .iter()
            .flat_map(|g| g.devices.iter().map(|n| n.device_id.as_str()))
            .collect();

        assert_eq!(placed.len(), devices.len() - 1);
        for device in devices.iter().filter(|d| !d.is_gateway) {
            assert!(
                placed.contains(&device.id.as_str()),
                "{} was lost",
                device.id
            );
        }
    }

    /// Fixed sectors: all five groups are always present, in the same order,
    /// so a device does not jump to a different part of the ring between
    /// renders just because a category emptied.
    #[test]
    fn all_five_groups_are_always_present_in_a_fixed_order() {
        let topology = Topology::build(&[]);
        let order: Vec<Category> = topology.groups.iter().map(|g| g.category).collect();
        assert_eq!(order, Category::ORDER.to_vec());
    }

    #[test]
    fn devices_land_in_their_own_category() {
        let topology = Topology::build(&home_network());
        let group = |c: Category| {
            topology
                .groups
                .iter()
                .find(|g| g.category == c)
                .expect("group")
        };
        let holds = |c: Category, id: &str| group(c).devices.iter().any(|n| n.device_id == id);
        assert!(holds(Category::Computers, "192.168.1.10"));
        assert!(holds(Category::SmartHome, "192.168.1.20"));
        assert!(holds(Category::Unknown, "192.168.1.30"));
    }

    /// Unknown is reported as a count, not hidden and not dressed up.
    #[test]
    fn summary_reports_unidentified_devices_plainly() {
        let devices = home_network();
        let summary = DiscoverySummary::of(&devices);

        assert_eq!(summary.total, 4);
        assert_eq!(summary.unidentified, 1);
        assert_eq!(summary.isolation, crate::device::Isolation::Normal);
    }

    #[test]
    fn an_empty_network_yields_an_empty_but_valid_topology() {
        let topology = Topology::build(&[]);
        assert!(topology.center.is_none());
        assert!(topology.self_id.is_none());
        assert!(topology.groups.iter().all(|g| g.devices.is_empty()));
    }
}
