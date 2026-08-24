//! What a probe is permitted to read — and what JRX refuses to read at all.
//!
//! The refused variants exist deliberately. Naming them is what lets the
//! Visibility Panel say "we could build this and chose not to"
//! (ARCHITECTURE.md §9). A test in `crate::invariants` proves that no probe
//! ever declares one.

use serde::{Deserialize, Serialize};

/// A category of data a probe reads from the operating system or the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataClass {
    // ---- Permitted: readable without elevation (ARCHITECTURE.md §6.2-6.4) ----
    /// Interface names, link types, MAC and IP addresses, MTU, up/down state.
    InterfaceMetadata,
    /// Default gateway and per-interface route metrics.
    RouteTable,
    /// SSID, BSSID, band, channel, signal strength, security mode.
    WifiAssociation,
    /// The ARP/NDP neighbour cache the OS has already populated.
    NeighborTable,
    /// Cumulative per-interface byte counters, sampled for throughput.
    InterfaceCounters,
    /// Local and remote endpoints of active sockets, and the owning process.
    SocketTable,
    /// mDNS/DNS-SD and SSDP service advertisements broadcast on the LAN.
    ServiceAdvertisement,
    /// Whether a host on the local subnet answers an ICMP echo.
    HostLiveness,

    // ---- Refused by design: never collected, at any version ----
    /// Contents of network packets.
    PacketPayload,
    /// Which names this device resolved, and when.
    DnsQueryHistory,
    /// URLs or sites visited.
    BrowsingHistory,
    /// Passwords, cookies, tokens, keys.
    Credential,
    /// Full process command lines and their arguments.
    ProcessCommandLine,
}

impl DataClass {
    /// Every variant, in declaration order.
    pub const ALL: [DataClass; 13] = [
        DataClass::InterfaceMetadata,
        DataClass::RouteTable,
        DataClass::WifiAssociation,
        DataClass::NeighborTable,
        DataClass::InterfaceCounters,
        DataClass::SocketTable,
        DataClass::ServiceAdvertisement,
        DataClass::HostLiveness,
        DataClass::PacketPayload,
        DataClass::DnsQueryHistory,
        DataClass::BrowsingHistory,
        DataClass::Credential,
        DataClass::ProcessCommandLine,
    ];

    /// The classes JRX will never collect. Rendered as the fourth column of
    /// the Visibility Panel.
    pub const REFUSED: [DataClass; 5] = [
        DataClass::PacketPayload,
        DataClass::DnsQueryHistory,
        DataClass::BrowsingHistory,
        DataClass::Credential,
        DataClass::ProcessCommandLine,
    ];

    /// True if JRX refuses to collect this class at any version.
    pub fn is_refused_by_design(self) -> bool {
        Self::REFUSED.contains(&self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_payload_is_refused_by_design() {
        assert!(DataClass::PacketPayload.is_refused_by_design());
    }

    #[test]
    fn neighbor_table_is_permitted() {
        assert!(!DataClass::NeighborTable.is_refused_by_design());
    }

    #[test]
    fn every_refused_class_is_listed_in_refused_all() {
        for class in DataClass::ALL {
            assert_eq!(
                class.is_refused_by_design(),
                DataClass::REFUSED.contains(&class),
                "{class:?} disagrees between is_refused_by_design() and REFUSED",
            );
        }
    }
}
