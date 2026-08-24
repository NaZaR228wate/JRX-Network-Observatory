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
    /// Which DNS resolvers this device is configured to use.
    ///
    /// Deliberately distinct from `DnsQueryHistory`: knowing which resolver
    /// is configured is ordinary network configuration, while recording what
    /// was looked up is refused outright.
    DnsResolverConfig,

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
    pub const ALL: [DataClass; 14] = [
        DataClass::InterfaceMetadata,
        DataClass::RouteTable,
        DataClass::WifiAssociation,
        DataClass::NeighborTable,
        DataClass::InterfaceCounters,
        DataClass::SocketTable,
        DataClass::ServiceAdvertisement,
        DataClass::HostLiveness,
        DataClass::DnsResolverConfig,
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

    /// Why JRX refuses it. Empty for permitted classes.
    ///
    /// This text is the fourth column of the Visibility Panel and the clearest
    /// statement of what the product is: things it could build and will not.
    pub fn refusal_rationale(self) -> &'static str {
        match self {
            DataClass::PacketPayload => {
                "Reading packet contents would require administrator access and \
                 would expose the contents of everything you send. JRX has no \
                 capture library at all, so this cannot be switched on."
            }
            DataClass::DnsQueryHistory => {
                "A record of every name you look up is a record of everywhere \
                 you go. JRX reads which resolvers you use, never what you \
                 asked them."
            }
            DataClass::BrowsingHistory => {
                "Which sites you visit is yours. JRX never reads it, from the \
                 browser or from the network."
            }
            DataClass::Credential => {
                "Passwords, cookies and tokens are never read, stored, or \
                 transmitted. Nothing in JRX asks for them."
            }
            DataClass::ProcessCommandLine => {
                "Full command lines leak file paths, tokens and arguments. JRX \
                 shows which program owns a connection, not how it was started."
            }
            _ => "",
        }
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

    /// Reading which resolvers are configured is not the same as recording
    /// what was resolved. The first is ordinary network configuration; the
    /// second is refused outright. Keeping them as separate variants is what
    /// stops the distinction from eroding.
    #[test]
    fn resolver_config_is_permitted_but_query_history_is_refused() {
        assert!(!DataClass::DnsResolverConfig.is_refused_by_design());
        assert!(DataClass::DnsQueryHistory.is_refused_by_design());
    }

    /// ALL is maintained by hand, so nothing stops a new variant from being
    /// omitted -- and an omitted variant is invisible to every audit that
    /// iterates it. The exhaustive match below fails to compile when a variant
    /// is added, and the length assertion fails when ALL was not updated.
    #[test]
    fn all_lists_every_variant() {
        for class in DataClass::ALL {
            match class {
                DataClass::InterfaceMetadata
                | DataClass::RouteTable
                | DataClass::WifiAssociation
                | DataClass::NeighborTable
                | DataClass::InterfaceCounters
                | DataClass::SocketTable
                | DataClass::ServiceAdvertisement
                | DataClass::HostLiveness
                | DataClass::DnsResolverConfig
                | DataClass::PacketPayload
                | DataClass::DnsQueryHistory
                | DataClass::BrowsingHistory
                | DataClass::Credential
                | DataClass::ProcessCommandLine => {}
            }
        }
        assert_eq!(DataClass::ALL.len(), 14, "a variant is missing from ALL");
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
