//! MAC prefix to manufacturer.
//!
//! The IEEE MA-L registry, trimmed to prefix and organisation and embedded in
//! the binary. Lookup is offline by design: querying a service for every
//! unknown device would leak the user's device inventory to a third party,
//! which is categorically incompatible with the privacy model
//! (TECH_DECISIONS.md ADR-010).
//!
//! A vendor is an observed fact and never a device category. "Apple" cannot
//! distinguish a MacBook from an iPhone from an Apple TV (ADR-008).

use std::sync::OnceLock;

use jrx_core::device::MacAddress;

/// `PREFIX\tVendor` per line, sorted by prefix.
static REGISTRY: &str = include_str!("../data/oui.tsv");

/// Parsed once, then binary-searched.
fn table() -> &'static [(u32, &'static str)] {
    static TABLE: OnceLock<Vec<(u32, &'static str)>> = OnceLock::new();
    TABLE.get_or_init(|| {
        REGISTRY
            .lines()
            .filter_map(|line| {
                let (prefix, vendor) = line.split_once('\t')?;
                Some((u32::from_str_radix(prefix, 16).ok()?, vendor))
            })
            .collect()
    })
}

/// The manufacturer that registered this address's prefix, if any.
pub fn vendor_of(mac: MacAddress) -> Option<&'static str> {
    let [a, b, c] = mac.oui();
    let key = u32::from_be_bytes([0, a, b, c]);

    table()
        .binary_search_by_key(&key, |(prefix, _)| *prefix)
        .ok()
        .map(|index| table()[index].1)
}

/// Number of registry entries loaded.
pub fn entry_count() -> usize {
    table().len()
}

/// Whether the embedded registry is sorted, which binary search requires.
pub fn is_sorted() -> bool {
    table().windows(2).all(|w| w[0].0 <= w[1].0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jrx_core::device::MacAddress;

    fn mac(s: &str) -> MacAddress {
        MacAddress::parse(s).unwrap()
    }

    #[test]
    fn resolves_a_known_vendor() {
        assert_eq!(vendor_of(mac("a4:83:e7:11:22:33")), Some("Apple"));
    }

    /// Observed on the development machine: the USB-Ethernet dongle carrying
    /// the default route.
    #[test]
    fn resolves_the_usb_ethernet_dongle_on_this_machine() {
        assert_eq!(
            vendor_of(mac("9c:69:d3:6c:38:28")),
            Some("ASIX Electronics")
        );
    }

    #[test]
    fn an_unassigned_prefix_resolves_to_nothing() {
        assert_eq!(vendor_of(mac("fe:ff:fe:00:00:01")), None);
    }

    /// The registry must actually be loaded; an empty table would make every
    /// lookup silently return None and look like "no vendors found".
    #[test]
    fn the_registry_is_populated() {
        assert!(
            entry_count() > 30_000,
            "only {} entries loaded",
            entry_count()
        );
    }

    #[test]
    fn entries_are_sorted_so_binary_search_is_valid() {
        assert!(is_sorted(), "OUI table must be sorted by prefix");
    }
}
