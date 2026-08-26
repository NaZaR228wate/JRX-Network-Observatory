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

/// One factual observation about a group of devices.
///
/// Deliberately not a category. "46 resolve a manufacturer" is something we
/// measured; "46 IoT devices" would be something we invented
/// (TECH_DECISIONS.md ADR-008).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroupFact {
    pub count: usize,
    pub description: &'static str,
    /// Always false. Present so the UI cannot accidentally render a fact as a
    /// category, and so a test can assert it never becomes one.
    pub is_category: bool,
}

/// One category sector of the overview ring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CategorySummary {
    pub category: Category,
    pub label: &'static str,
    pub count: usize,
    /// Independent factual counts, which may overlap. Empty for categories
    /// that need no explanation.
    pub facts: Vec<GroupFact>,
    /// True when `facts` are overlapping observations rather than a partition.
    pub facts_are_independent: bool,
    /// True when the group is drawn as a single node until the user opens it.
    pub collapsed_by_default: bool,
}

/// Level 1: the whole network at a glance.
///
/// Whatever the network size, this draws the same handful of things — a
/// centre, this machine, and five sectors. Rendering hundreds of anonymous
/// dots would be noise rather than information, so scale lives behind the
/// group view instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TopologyOverview {
    /// The router: the one device identified with certainty.
    pub center: Option<TopologyNode>,
    /// This machine, always identifiable.
    pub self_node: Option<TopologyNode>,
    /// Always all five categories, always in the same order.
    pub groups: Vec<CategorySummary>,
    pub total: usize,
}

/// Above this many members, a group arrives collapsed.
///
/// Small groups are worth drawing individually; a hundred identical dots are
/// not.
const COLLAPSE_THRESHOLD: usize = 12;

impl TopologyOverview {
    pub fn build(devices: &[Device]) -> TopologyOverview {
        let center = devices.iter().find(|d| d.is_gateway).map(TopologyNode::of);
        let self_node = devices.iter().find(|d| d.is_self).map(TopologyNode::of);

        let groups = Category::ORDER
            .into_iter()
            .map(|category| {
                // The centre is drawn in the middle, so counting it in its ring
                // would put the router on the map twice.
                let members: Vec<&Device> = devices
                    .iter()
                    .filter(|d| !d.is_gateway && d.category() == category)
                    .collect();

                let facts = if category == Category::Unknown {
                    unknown_facts(&members)
                } else {
                    Vec::new()
                };

                CategorySummary {
                    category,
                    label: category.label(),
                    count: members.len(),
                    facts_are_independent: !facts.is_empty(),
                    facts,
                    collapsed_by_default: category == Category::Unknown
                        || members.len() > COLLAPSE_THRESHOLD,
                }
            })
            .collect();

        TopologyOverview {
            center,
            self_node,
            groups,
            total: devices.len(),
        }
    }

    pub fn group(&self, category: Category) -> Option<&CategorySummary> {
        self.groups.iter().find(|g| g.category == category)
    }
}

/// Why the unidentified devices are unidentified.
///
/// Three independent counts over the same set. They overlap — a device can
/// both rotate its address and announce a name — so they are never summed and
/// never drawn as slices of a whole.
fn unknown_facts(members: &[&Device]) -> Vec<GroupFact> {
    if members.is_empty() {
        return Vec::new();
    }

    let count_where = |f: fn(&Device) -> bool| members.iter().filter(|d| f(d)).count();

    [
        GroupFact {
            count: count_where(|d| d.facts.mac_randomised),
            description: "rotating their hardware address on purpose",
            is_category: false,
        },
        GroupFact {
            count: count_where(|d| d.facts.vendor.is_some()),
            description: "resolve a manufacturer, but nothing that says what they are",
            is_category: false,
        },
        GroupFact {
            count: count_where(|d| d.facts.hostname.is_some()),
            description: "announced a name that does not identify a type",
            is_category: false,
        },
    ]
    .into_iter()
    .filter(|fact| fact.count > 0)
    .collect()
}

/// Level 2: the members of one category.
///
/// Paginated rather than virtualised, because a page boundary is something a
/// user can see and reason about, while a scroll position that silently drops
/// nodes is not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroupView {
    pub category: Category,
    pub label: &'static str,
    /// Members in the whole group, not just this page.
    pub total: usize,
    /// The same factual breakdown the overview shows, so opening a group needs
    /// no second lookup.
    pub facts: Vec<GroupFact>,
    pub facts_are_independent: bool,
    pub page: usize,
    pub page_size: usize,
    pub page_count: usize,
    /// This page only.
    pub devices: Vec<TopologyNode>,
}

impl GroupView {
    /// The most nodes handed to the renderer at once.
    ///
    /// Bounds the DOM regardless of network size: a 500-device group still
    /// draws this many.
    pub const PAGE_SIZE: usize = 40;

    pub fn build(devices: &[Device], category: Category, page: usize) -> GroupView {
        // The centre is drawn in the middle of the map, never inside a group.
        let mut members: Vec<&Device> = devices
            .iter()
            .filter(|d| !d.is_gateway && d.category() == category)
            .collect();

        // Deterministic, and most informative first: a page of anonymous
        // entries buries the one device the user came to find. The address is
        // the final tie-break so the order never depends on discovery timing.
        members.sort_by(|a, b| {
            informativeness(a)
                .cmp(&informativeness(b))
                .then_with(|| a.id.cmp(&b.id))
        });

        let total = members.len();
        let page_count = total.div_ceil(Self::PAGE_SIZE).max(1);
        let page = page.min(page_count - 1);
        let start = page * Self::PAGE_SIZE;

        let facts = if category == Category::Unknown {
            unknown_facts(&members)
        } else {
            Vec::new()
        };

        GroupView {
            category,
            label: category.label(),
            total,
            facts_are_independent: !facts.is_empty(),
            facts,
            page,
            page_size: Self::PAGE_SIZE,
            page_count,
            devices: members
                .into_iter()
                .skip(start)
                .take(Self::PAGE_SIZE)
                .map(TopologyNode::of)
                .collect(),
        }
    }
}

/// Sort key: lower is shown first. How much we can tell the user about it.
fn informativeness(device: &Device) -> u8 {
    match (
        device.facts.hostname.is_some(),
        device.facts.vendor.is_some(),
        device.category() != Category::Unknown,
    ) {
        (_, _, true) => 0,          // classified
        (true, _, false) => 1,      // has a name
        (false, true, false) => 2,  // has a manufacturer
        (false, false, false) => 3, // an address and nothing else
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
    pub fn of(devices: &[Device], subnet: Option<crate::network::Subnet>) -> DiscoverySummary {
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
            isolation: assess_isolation(devices, subnet),
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
        let summary = DiscoverySummary::of(
            &devices,
            Some(crate::network::Subnet {
                network: "192.168.1.0".parse().unwrap(),
                prefix_len: 24,
            }),
        );

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

#[cfg(test)]
mod overview_tests {
    use super::*;
    use crate::device::{Category, DeviceTable, DiscoveryMethod, MacAddress, Observation};

    fn ip(n: usize) -> std::net::IpAddr {
        format!("192.168.{}.{}", n / 250, n % 250 + 1)
            .parse()
            .unwrap()
    }
    pub(crate) fn none(_: MacAddress) -> Option<&'static str> {
        None
    }
    pub(crate) fn any_vendor(_: MacAddress) -> Option<&'static str> {
        Some("Some Manufacturer")
    }

    /// A network of `unknowns` anonymous devices plus a router, this Mac, and
    /// one named printer.
    pub(crate) fn network(
        unknowns: usize,
        vendor: fn(MacAddress) -> Option<&'static str>,
    ) -> Vec<crate::device::Device> {
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
        for i in 0..unknowns {
            // Half rotate their address, half do not.
            let first = if i % 2 == 0 { 0x9e } else { 0x3c };
            let mac = format!(
                "{first:02x}:aa:bb:{:02x}:{:02x}:{:02x}",
                i / 65536,
                (i / 256) % 256,
                i % 256
            );
            t.observe(
                Observation::new(ip(i + 3), DiscoveryMethod::ArpCache)
                    .with_mac(MacAddress::parse(&mac)),
            );
        }
        t.mark_gateway(ip(0));
        t.mark_self(ip(1));
        t.finish(vendor)
    }

    // ---- level 1: the overview ----

    #[test]
    fn the_router_anchors_the_overview_and_this_mac_is_identifiable() {
        let overview = TopologyOverview::build(&network(10, none));

        let center = overview.center.as_ref().expect("a router at the centre");
        assert!(center.is_gateway);
        assert_eq!(center.category, Category::Infrastructure);

        let me = overview
            .self_node
            .as_ref()
            .expect("this Mac is identifiable");
        assert!(me.is_self);
    }

    /// Hundreds of anonymous dots are noise, not information. Unknown arrives
    /// as one node carrying a count.
    #[test]
    fn unknown_devices_arrive_as_a_single_collapsed_group() {
        let overview = TopologyOverview::build(&network(111, none));
        let unknown = overview.group(Category::Unknown).expect("an Unknown group");

        assert_eq!(unknown.count, 111);
        assert!(
            unknown.collapsed_by_default,
            "Unknown must not explode onto the map"
        );
    }

    /// Small groups are worth drawing individually.
    #[test]
    fn identified_groups_are_not_collapsed() {
        let overview = TopologyOverview::build(&network(10, none));
        for category in [
            Category::Computers,
            Category::SmartHome,
            Category::Infrastructure,
        ] {
            let group = overview.group(category).expect("group present");
            assert!(
                !group.collapsed_by_default,
                "{category:?} should be expandable"
            );
        }
    }

    /// All five sectors always exist, in the same order, so a category
    /// emptying does not move everything else around the ring.
    #[test]
    fn all_five_groups_are_always_present_in_a_fixed_order() {
        let overview = TopologyOverview::build(&[]);
        let order: Vec<Category> = overview.groups.iter().map(|g| g.category).collect();
        assert_eq!(order, Category::ORDER.to_vec());
        assert!(overview.groups.iter().all(|g| g.count == 0));
    }

    /// This Mac is a computer and is counted as one. Removing it from its own
    /// category would make the counts disagree with the device list.
    #[test]
    fn this_mac_is_counted_inside_its_own_category() {
        let devices = network(10, none);
        let overview = TopologyOverview::build(&devices);
        let computers = overview.group(Category::Computers).expect("group");

        let actual = devices
            .iter()
            .filter(|d| d.category() == Category::Computers)
            .count();
        assert_eq!(computers.count, actual);
        assert!(actual >= 1);
    }

    /// The router is drawn at the centre, so it must not also inflate the
    /// Infrastructure ring count as a second dot.
    #[test]
    fn the_centre_is_excluded_from_its_ring_group() {
        let devices = network(5, none);
        let overview = TopologyOverview::build(&devices);
        let infra = overview.group(Category::Infrastructure).expect("group");

        assert_eq!(
            infra.count, 0,
            "the only infrastructure device is the centre"
        );
    }

    // ---- the Unknown breakdown: facts, never categories ----

    #[test]
    fn the_unknown_group_explains_itself_with_facts() {
        let overview = TopologyOverview::build(&network(100, any_vendor));
        let unknown = overview.group(Category::Unknown).expect("group");

        assert!(
            !unknown.facts.is_empty(),
            "Unknown must say why it is unknown"
        );

        let randomised = unknown
            .facts
            .iter()
            .find(|f| f.description.contains("rotating"))
            .expect("randomised count present");
        assert_eq!(randomised.count, 50, "half the fixture rotates its address");

        let vendor_known = unknown
            .facts
            .iter()
            .find(|f| f.description.contains("manufacturer"))
            .expect("vendor-known count present");
        assert_eq!(vendor_known.count, 50, "the other half resolves a vendor");
    }

    /// The counts are independent observations that overlap, not a partition.
    /// Presenting them as slices of a whole would be a quiet lie.
    #[test]
    fn unknown_facts_are_independent_counts_not_a_partition() {
        let overview = TopologyOverview::build(&network(100, any_vendor));
        let unknown = overview.group(Category::Unknown).expect("group");

        assert!(
            unknown.facts.iter().all(|f| f.count <= unknown.count),
            "no single fact may exceed the group size"
        );
        assert!(
            unknown.facts_are_independent,
            "must be flagged as overlapping"
        );
    }

    /// Splitting Unknown by vendor or address shape would be inventing exactly
    /// the categories ADR-008 forbids.
    #[test]
    fn unknown_is_never_split_into_invented_categories() {
        let overview = TopologyOverview::build(&network(100, any_vendor));
        let unknown = overview.group(Category::Unknown).expect("group");

        assert_eq!(
            overview.groups.len(),
            Category::ORDER.len(),
            "no category may be added beyond the five"
        );
        assert!(
            unknown.facts.iter().all(|f| !f.is_category),
            "a factual count must never be presented as a category"
        );
    }

    // ---- scale ----

    #[test]
    fn overview_stays_bounded_from_ten_to_five_hundred_devices() {
        for size in [10usize, 60, 150, 500] {
            let overview = TopologyOverview::build(&network(size, none));

            // Whatever the network size, level 1 draws the same handful of
            // things: a centre, this Mac, and five sectors.
            assert_eq!(overview.groups.len(), 5, "at {size} devices");
            assert!(overview.center.is_some(), "at {size} devices");
            assert_eq!(overview.total, size + 3, "at {size} devices");
        }
    }

    #[test]
    fn building_the_overview_is_deterministic() {
        let devices = network(150, any_vendor);
        assert_eq!(
            TopologyOverview::build(&devices),
            TopologyOverview::build(&devices)
        );
    }
}

#[cfg(test)]
mod group_view_tests {
    use super::overview_tests::{any_vendor, network, none};
    use super::*;
    use crate::device::Category;

    #[test]
    fn a_small_group_fits_on_one_page() {
        let devices = network(10, none);
        let view = GroupView::build(&devices, Category::Unknown, 0);

        assert_eq!(view.total, 10);
        assert_eq!(view.page_count, 1);
        assert_eq!(view.devices.len(), 10);
    }

    /// A group of hundreds must never hand the renderer hundreds of nodes.
    #[test]
    fn a_large_group_is_capped_to_one_page_of_nodes() {
        let devices = network(500, none);
        let view = GroupView::build(&devices, Category::Unknown, 0);

        assert_eq!(view.total, 500);
        assert!(
            view.devices.len() <= GroupView::PAGE_SIZE,
            "handed {} nodes to the renderer",
            view.devices.len()
        );
        assert!(view.page_count > 1);
    }

    #[test]
    fn every_member_is_reachable_by_paging() {
        let devices = network(150, none);
        let mut seen = std::collections::BTreeSet::new();

        let pages = GroupView::build(&devices, Category::Unknown, 0).page_count;
        for page in 0..pages {
            for node in GroupView::build(&devices, Category::Unknown, page).devices {
                seen.insert(node.device_id);
            }
        }

        assert_eq!(seen.len(), 150, "paging must not lose or duplicate members");
    }

    /// Opening the same group twice must produce the same order, or nodes
    /// would appear to move for no reason.
    #[test]
    fn group_expansion_is_deterministic() {
        let devices = network(150, any_vendor);
        assert_eq!(
            GroupView::build(&devices, Category::Unknown, 1),
            GroupView::build(&devices, Category::Unknown, 1)
        );
    }

    /// Identified devices come first: a page of anonymous entries buries the
    /// one device the user was looking for.
    #[test]
    fn the_most_informative_devices_are_shown_first() {
        let devices = network(60, any_vendor);
        let view = GroupView::build(&devices, Category::Unknown, 0);

        let named_positions: Vec<usize> = view
            .devices
            .iter()
            .enumerate()
            .filter(|(_, n)| n.vendor.is_some())
            .map(|(i, _)| i)
            .collect();
        let anonymous_positions: Vec<usize> = view
            .devices
            .iter()
            .enumerate()
            .filter(|(_, n)| n.vendor.is_none())
            .map(|(i, _)| i)
            .collect();

        if let (Some(last_named), Some(first_anonymous)) =
            (named_positions.last(), anonymous_positions.first())
        {
            assert!(
                last_named < first_anonymous,
                "devices we know something about must sort ahead of ones we do not"
            );
        }
    }

    #[test]
    fn a_page_beyond_the_end_clamps_instead_of_returning_nothing() {
        let devices = network(10, none);
        let view = GroupView::build(&devices, Category::Unknown, 99);

        assert_eq!(view.page, view.page_count - 1);
        assert!(!view.devices.is_empty());
    }

    #[test]
    fn an_empty_group_is_valid_and_says_so() {
        let view = GroupView::build(&[], Category::Phones, 0);
        assert_eq!(view.total, 0);
        assert_eq!(view.page_count, 1);
        assert!(view.devices.is_empty());
    }

    /// The group view carries the same factual breakdown as the overview, so
    /// opening Unknown explains itself without another lookup.
    #[test]
    fn the_unknown_group_view_carries_its_facts() {
        let devices = network(100, any_vendor);
        let view = GroupView::build(&devices, Category::Unknown, 0);
        assert!(!view.facts.is_empty());
    }

    /// The router is drawn at the centre and must not also appear inside a
    /// group page.
    #[test]
    fn the_centre_never_appears_in_a_group_page() {
        let devices = network(5, none);
        let view = GroupView::build(&devices, Category::Infrastructure, 0);
        assert!(view.devices.iter().all(|n| !n.is_gateway));
    }

    /// Every node carries the evidence that produced its conclusion, so the
    /// detail panel needs no second fetch.
    #[test]
    fn group_nodes_carry_the_evidence_behind_their_conclusion() {
        let devices = network(10, none);
        let view = GroupView::build(&devices, Category::Computers, 0);

        for node in &view.devices {
            assert!(!node.rationale.is_empty());
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
