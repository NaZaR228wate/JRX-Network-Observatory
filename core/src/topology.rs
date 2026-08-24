//! The radial topology: router at the centre, this device highlighted, and
//! everything else grouped by category.
//!
//! Data model only — the drawing happens in M4 (MVP_ROADMAP.md). Groups are
//! emitted in a fixed order so each category owns a stable angular sector and
//! devices do not jump around the ring between renders
//! (TECH_DECISIONS.md ADR-007).

use serde::Serialize;

use crate::device::{Category, Device, Isolation, assess_isolation};

/// Devices sharing one category, in one ring sector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TopologyGroup {
    pub category: Category,
    pub label: &'static str,
    /// Device ids, referencing the device list.
    pub devices: Vec<String>,
}

/// The map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Topology {
    /// The router. The one device identified with certainty.
    pub center: Option<String>,
    /// This machine, so the UI can highlight it.
    pub self_id: Option<String>,
    /// Always all five categories, always in the same order.
    pub groups: Vec<TopologyGroup>,
}

impl Topology {
    pub fn build(devices: &[Device]) -> Topology {
        let center = devices.iter().find(|d| d.is_gateway).map(|d| d.id.clone());
        let self_id = devices.iter().find(|d| d.is_self).map(|d| d.id.clone());

        let groups = Category::ORDER
            .into_iter()
            .map(|category| TopologyGroup {
                category,
                label: category.label(),
                devices: devices
                    .iter()
                    // The centre is drawn in the middle, not in a ring.
                    .filter(|d| !d.is_gateway && d.category == category)
                    .map(|d| d.id.clone())
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
                .filter(|d| d.category == Category::Unknown)
                .count(),
            by_category: Category::ORDER
                .into_iter()
                .map(|category| {
                    (
                        category,
                        devices.iter().filter(|d| d.category == category).count(),
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
    use crate::device::{Category, DeviceTable, DiscoveryMethod, MacAddress, Observation};

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
        assert_eq!(topology.center.as_deref(), Some("192.168.1.1"));
    }

    #[test]
    fn this_device_is_identified_for_highlighting() {
        let topology = Topology::build(&home_network());
        assert_eq!(topology.self_id.as_deref(), Some("192.168.1.10"));
    }

    /// The centre must not also appear in a ring, or the router would render
    /// twice.
    #[test]
    fn the_centre_is_not_repeated_in_a_group() {
        let topology = Topology::build(&home_network());
        let placed: Vec<&str> = topology
            .groups
            .iter()
            .flat_map(|g| g.devices.iter().map(String::as_str))
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
            .flat_map(|g| g.devices.iter().map(String::as_str))
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
        assert!(
            group(Category::Computers)
                .devices
                .contains(&"192.168.1.10".to_string())
        );
        assert!(
            group(Category::SmartHome)
                .devices
                .contains(&"192.168.1.20".to_string())
        );
        assert!(
            group(Category::Unknown)
                .devices
                .contains(&"192.168.1.30".to_string())
        );
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
