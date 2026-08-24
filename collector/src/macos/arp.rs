//! Parsing the neighbour cache macOS has already built.
//!
//! Purely a read of existing OS state: nothing is transmitted. This is why
//! devices appear within a second of launch (ARCHITECTURE.md §7.1), and why
//! the result is not a network scan and must never be presented as one.

use std::net::IpAddr;

use jrx_core::device::MacAddress;

/// One neighbour-cache entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArpEntry {
    pub address: IpAddr,
    pub mac: Option<MacAddress>,
    pub interface: String,
}

/// Parse `arp -an`.
///
/// Lines look like:
/// `? (172.16.0.1) at 0:11:22:33:44:55 on en7 ifscope [ethernet]`
///
/// Entries macOS could not resolve are marked `(incomplete)` and are dropped:
/// they record a failed lookup, not a device. Broadcast and multicast rows are
/// kept here and rejected by the device table, so that the decision about what
/// counts as a device lives in exactly one place.
pub fn parse_arp(output: &str) -> Vec<ArpEntry> {
    output
        .lines()
        .filter_map(|line| {
            let (_, rest) = line.split_once('(')?;
            let (address, rest) = rest.split_once(')')?;
            let address: IpAddr = address.parse().ok()?;

            let rest = rest.trim_start().strip_prefix("at ")?;
            let (hardware, rest) = rest.split_once(' ')?;
            if hardware.starts_with('(') {
                return None; // (incomplete)
            }

            let interface = rest
                .strip_prefix("on ")
                .and_then(|r| r.split_whitespace().next())
                .unwrap_or_default()
                .to_string();

            Some(ArpEntry {
                address,
                mac: MacAddress::parse(hardware),
                interface,
            })
        })
        .filter(|entry| entry.mac.is_some())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shape taken verbatim from `arp -an` on macOS 26.
    const ARP: &str = concat!(
        "? (172.16.0.1) at 0:11:22:33:44:55 on en7 ifscope [ethernet]\n",
        "? (172.16.0.89) at 9c:69:d3:6c:38:28 on en7 ifscope permanent [ethernet]\n",
        "? (172.16.1.200) at 1a:64:6f:aa:bb:cc on en7 [ethernet]\n",
        "? (169.254.6.83) at 4:5e:2f:aa:bb:cc on en7 [ethernet]\n",
        "? (172.16.1.255) at ff:ff:ff:ff:ff:ff on en7 ifscope [ethernet]\n",
        "? (224.0.0.251) at 1:0:5e:0:0:fb on en7 ifscope permanent [ethernet]\n",
        "? (172.16.0.77) at (incomplete) on en7 [ethernet]\n",
    );

    #[test]
    fn reads_address_and_hardware_address() {
        let entries = parse_arp(ARP);
        let gateway = entries
            .iter()
            .find(|e| e.address.to_string() == "172.16.0.1");
        assert_eq!(
            gateway
                .and_then(|e| e.mac)
                .map(|m| m.to_string())
                .as_deref(),
            Some("00:11:22:33:44:55"),
            "single-digit octets from arp must be normalised"
        );
    }

    /// macOS keeps entries it could not resolve. They are not devices.
    #[test]
    fn incomplete_entries_are_skipped() {
        let entries = parse_arp(ARP);
        assert!(
            !entries
                .iter()
                .any(|e| e.address.to_string() == "172.16.0.77")
        );
    }

    #[test]
    fn every_parsed_entry_has_a_usable_hardware_address() {
        for entry in parse_arp(ARP) {
            assert!(
                entry.mac.is_some(),
                "{} parsed without a MAC",
                entry.address
            );
        }
    }

    #[test]
    fn records_the_interface_each_entry_was_seen_on() {
        assert!(parse_arp(ARP).iter().all(|e| e.interface == "en7"));
    }

    /// Broadcast and multicast rows are kept by the parser and rejected later
    /// by the device table, which is the single place that decides what counts
    /// as a device.
    #[test]
    fn parser_does_not_silently_drop_non_device_addresses() {
        let entries = parse_arp(ARP);
        assert!(
            entries
                .iter()
                .any(|e| e.address.to_string() == "224.0.0.251")
        );
    }

    #[test]
    fn garbage_input_yields_no_entries_rather_than_panicking() {
        assert!(parse_arp("total garbage\n\n???\n").is_empty());
    }
}
