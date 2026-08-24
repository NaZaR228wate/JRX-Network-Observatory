//! Network identity — the immediate answer to "where am I?"
//!
//! ARCHITECTURE.md §7.2. Everything here is pure logic over parsed inputs, so
//! every classification rule is testable against fixtures without a network.

use std::net::{IpAddr, Ipv4Addr};

use serde::Serialize;

/// One network interface, as observed by the platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InterfaceInfo {
    pub name: String,
    pub is_up: bool,
    pub is_running: bool,
    /// Tunnel-like: utun, ppp, ipsec. A VPN runs over one of these, but so do
    /// several macOS system services, so this alone means nothing.
    pub is_point_to_point: bool,
    pub is_loopback: bool,
    pub mac: Option<String>,
    pub ipv4: Option<Ipv4Addr>,
    pub prefix_len: Option<u8>,
}

/// A macOS hardware port entry: the human label for a device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HardwarePort {
    /// "Wi-Fi", "Ethernet", "iPhone USB", or a chipset name like "AX88179B".
    pub label: String,
    /// "en0"
    pub device: String,
}

/// How this device reaches the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionType {
    Wifi,
    Ethernet,
    /// A phone sharing its cellular connection over USB.
    UsbTether,
    /// A tunnel holds the default route.
    Vpn,
    /// Not determinable from available evidence. Reported honestly rather than
    /// guessed (TECH_DECISIONS.md ADR-008).
    Unknown,
}

impl ConnectionType {
    /// Classify the interface that currently holds the default route.
    ///
    /// Order matters. VPN is decided first because a tunnel holding the
    /// default route overrides whatever physical interface sits beneath it.
    pub fn classify(
        active: &str,
        interfaces: &[InterfaceInfo],
        ports: &[HardwarePort],
    ) -> ConnectionType {
        let iface = interfaces.iter().find(|i| i.name == active);

        // A tunnel is only a VPN when it carries the default route. Idle utun
        // interfaces are normal on macOS and must not trigger this.
        if iface.is_some_and(|i| i.is_point_to_point) {
            return ConnectionType::Vpn;
        }

        let label = ports
            .iter()
            .find(|p| p.device == active)
            .map(|p| p.label.as_str());

        if let Some(label) = label
            && let Some(kind) = Self::from_port_label(label)
        {
            return kind;
        }

        // No recognised label. An interface with a hardware address is a wired
        // link; that is a fact, not a guess. Without one, say Unknown.
        match iface {
            Some(i) if i.mac.is_some() => ConnectionType::Ethernet,
            _ => ConnectionType::Unknown,
        }
    }

    /// Match a macOS hardware port label.
    ///
    /// Tether detection keys on the phone, never on "USB": a USB-to-Ethernet
    /// dongle is a wired connection and reporting it as a phone tether would
    /// be exactly the kind of confident wrong answer ADR-008 forbids.
    fn from_port_label(label: &str) -> Option<ConnectionType> {
        let lower = label.to_ascii_lowercase();

        if lower.contains("wi-fi") || lower.contains("wifi") || lower.contains("airport") {
            return Some(ConnectionType::Wifi);
        }
        if lower.contains("iphone") || lower.contains("ipad") || lower.contains("android") {
            return Some(ConnectionType::UsbTether);
        }
        if lower.contains("ethernet") || lower.contains("thunderbolt bridge") {
            return Some(ConnectionType::Ethernet);
        }
        None
    }

    /// Plain-language phrasing for the Network Identity screen.
    pub fn label(self) -> &'static str {
        match self {
            ConnectionType::Wifi => "Wi-Fi",
            ConnectionType::Ethernet => "Ethernet (wired)",
            ConnectionType::UsbTether => "Phone hotspot over USB",
            ConnectionType::Vpn => "VPN tunnel",
            ConnectionType::Unknown => "Unknown connection",
        }
    }
}

/// The default route: which interface leaves this machine, and via which hop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DefaultRoute {
    pub gateway: IpAddr,
    pub interface: String,
}

/// IPv4 address plus prefix length, for showing the subnet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Subnet {
    pub network: Ipv4Addr,
    pub prefix_len: u8,
}

/// Everything the Network Identity screen shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NetworkIdentity {
    pub connection: ConnectionType,
    /// "en7"
    pub interface: String,
    /// "AX88179B" — the platform's own label, shown verbatim.
    pub interface_label: Option<String>,
    pub local_ip: Option<Ipv4Addr>,
    pub subnet: Option<Subnet>,
    pub gateway: Option<IpAddr>,
    pub dns_servers: Vec<IpAddr>,
    pub wifi: WifiStatus,
    /// True when a tunnel holds the default route.
    pub vpn_active: bool,
}

/// Wi-Fi radio band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    Ghz2_4,
    Ghz5,
    Ghz6,
}

impl Band {
    pub fn label(self) -> &'static str {
        match self {
            Band::Ghz2_4 => "2.4 GHz",
            Band::Ghz5 => "5 GHz",
            Band::Ghz6 => "6 GHz",
        }
    }
}

/// Wi-Fi association details. Every field is optional because each one can be
/// individually unavailable depending on OS version and permission state.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct WifiDetails {
    pub ssid: Option<String>,
    pub bssid: Option<String>,
    pub channel: Option<u16>,
    pub band: Option<Band>,
    pub signal_dbm: Option<i16>,
    pub noise_dbm: Option<i16>,
    pub security: Option<String>,
    pub phy_mode: Option<String>,
}

/// Why Wi-Fi information is or is not present.
///
/// These are distinct states with distinct explanations, never one empty
/// result (ARCHITECTURE.md §12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WifiStatus {
    /// No Wi-Fi hardware on this machine.
    NoHardware,
    /// Wi-Fi hardware present, radio switched off.
    RadioOff,
    /// Radio on, not joined to a network.
    NotAssociated,
    /// Associated, and the details we could read.
    Associated(WifiDetails),
    /// The Wi-Fi read itself failed. Distinct from "no hardware" and from
    /// "radio off": those are facts about the machine, this is a fact about
    /// our own probe.
    Unavailable { reason: String },
    /// Associated, but macOS withholds the network name without Location
    /// Services. Resolved in M2; surfaced honestly here.
    PermissionRequired,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(name: &str) -> InterfaceInfo {
        InterfaceInfo {
            name: name.to_string(),
            is_up: true,
            is_running: true,
            is_point_to_point: false,
            is_loopback: false,
            mac: Some("aa:bb:cc:dd:ee:ff".into()),
            ipv4: Some("192.168.1.50".parse().unwrap()),
            prefix_len: Some(24),
        }
    }

    fn port(label: &str, device: &str) -> HardwarePort {
        HardwarePort {
            label: label.into(),
            device: device.into(),
        }
    }

    #[test]
    fn wifi_hardware_port_classifies_as_wifi() {
        let got = ConnectionType::classify("en0", &[iface("en0")], &[port("Wi-Fi", "en0")]);
        assert_eq!(got, ConnectionType::Wifi);
    }

    #[test]
    fn built_in_ethernet_classifies_as_ethernet() {
        let got = ConnectionType::classify("en1", &[iface("en1")], &[port("Ethernet", "en1")]);
        assert_eq!(got, ConnectionType::Ethernet);
    }

    /// Observed on the development machine: the default route runs over a
    /// USB-to-Ethernet dongle whose hardware port is the chipset name. It is a
    /// wired connection and must not be reported as a phone tether just
    /// because the transport is USB.
    #[test]
    fn usb_ethernet_dongle_classifies_as_ethernet_not_tether() {
        let got = ConnectionType::classify("en7", &[iface("en7")], &[port("AX88179B", "en7")]);
        assert_eq!(got, ConnectionType::Ethernet);
    }

    #[test]
    fn iphone_usb_classifies_as_usb_tether() {
        let got = ConnectionType::classify("en5", &[iface("en5")], &[port("iPhone USB", "en5")]);
        assert_eq!(got, ConnectionType::UsbTether);
    }

    #[test]
    fn point_to_point_interface_holding_default_route_is_vpn() {
        let mut tunnel = iface("utun6");
        tunnel.is_point_to_point = true;
        tunnel.mac = None;

        let got = ConnectionType::classify("utun6", &[tunnel], &[]);
        assert_eq!(got, ConnectionType::Vpn);
    }

    /// Observed on the development machine: six utun interfaces are UP and
    /// RUNNING with no VPN active. macOS uses them for its own services, so
    /// their mere existence must never be read as a VPN.
    #[test]
    fn idle_tunnels_alongside_a_wired_route_are_not_a_vpn() {
        let mut tunnel = iface("utun0");
        tunnel.is_point_to_point = true;
        tunnel.mac = None;

        let got =
            ConnectionType::classify("en7", &[iface("en7"), tunnel], &[port("AX88179B", "en7")]);
        assert_eq!(got, ConnectionType::Ethernet);
    }

    #[test]
    fn unknown_interface_is_not_guessed() {
        let got = ConnectionType::classify("en9", &[], &[]);
        assert_eq!(got, ConnectionType::Unknown);
    }

    #[test]
    fn interface_with_no_mac_and_no_hardware_port_is_unknown() {
        let mut bare = iface("en9");
        bare.mac = None;
        let got = ConnectionType::classify("en9", &[bare], &[]);
        assert_eq!(got, ConnectionType::Unknown);
    }
}

impl Subnet {
    /// Whether an address belongs to this subnet.
    ///
    /// Used to keep discovery to "your network": link-local addresses from
    /// devices that failed DHCP, and anything routed elsewhere, are not part
    /// of the local picture.
    pub fn contains(self, address: Ipv4Addr) -> bool {
        Subnet::of(address, self.prefix_len).is_some_and(|s| s.network == self.network)
    }

    /// Mask an address down to its network address.
    fn of(addr: Ipv4Addr, prefix_len: u8) -> Option<Subnet> {
        if prefix_len > 32 {
            return None;
        }
        // A /0 shift would overflow, so handle it explicitly.
        let mask: u32 = if prefix_len == 0 {
            0
        } else {
            u32::MAX << (32 - prefix_len)
        };
        Some(Subnet {
            network: Ipv4Addr::from(u32::from(addr) & mask),
            prefix_len,
        })
    }
}

impl NetworkIdentity {
    /// Build the identity from parsed platform data.
    ///
    /// Pure: every input is already-parsed data, so all of the reasoning here
    /// is testable against fixtures. Nothing is inferred when the evidence is
    /// absent -- a missing default route yields Unknown, not a guess at which
    /// interface is probably in use.
    pub fn assemble(
        route: Option<DefaultRoute>,
        interfaces: &[InterfaceInfo],
        ports: &[HardwarePort],
        dns_servers: Vec<IpAddr>,
        wifi: WifiStatus,
    ) -> NetworkIdentity {
        let Some(route) = route else {
            return NetworkIdentity {
                connection: ConnectionType::Unknown,
                interface: String::new(),
                interface_label: None,
                local_ip: None,
                subnet: None,
                gateway: None,
                dns_servers,
                wifi,
                vpn_active: false,
            };
        };

        let active = route.interface.as_str();
        let connection = ConnectionType::classify(active, interfaces, ports);
        let iface = interfaces.iter().find(|i| i.name == active);

        let local_ip = iface.and_then(|i| i.ipv4);
        let subnet = iface
            .and_then(|i| Some((i.ipv4?, i.prefix_len?)))
            .and_then(|(addr, prefix)| Subnet::of(addr, prefix));

        NetworkIdentity {
            connection,
            interface: route.interface.clone(),
            interface_label: ports
                .iter()
                .find(|p| p.device == active)
                .map(|p| p.label.clone()),
            local_ip,
            subnet,
            gateway: Some(route.gateway),
            dns_servers,
            wifi,
            vpn_active: connection == ConnectionType::Vpn,
        }
    }
}

#[cfg(test)]
mod subnet_tests {
    use super::*;

    fn subnet(net: &str, prefix: u8) -> Subnet {
        Subnet {
            network: net.parse().unwrap(),
            prefix_len: prefix,
        }
    }

    /// The development machine sits on a /23, where assuming /24 would place
    /// half the network outside its own subnet.
    #[test]
    fn a_slash_23_spans_both_halves() {
        let s = subnet("172.16.0.0", 23);
        assert!(s.contains("172.16.0.89".parse().unwrap()));
        assert!(s.contains("172.16.1.200".parse().unwrap()));
        assert!(!s.contains("172.16.2.1".parse().unwrap()));
    }

    #[test]
    fn link_local_addresses_are_outside_a_normal_subnet() {
        let s = subnet("192.168.1.0", 24);
        assert!(!s.contains("169.254.6.83".parse().unwrap()));
    }

    #[test]
    fn a_slash_32_contains_only_itself() {
        let s = subnet("10.0.0.5", 32);
        assert!(s.contains("10.0.0.5".parse().unwrap()));
        assert!(!s.contains("10.0.0.6".parse().unwrap()));
    }
}

#[cfg(test)]
mod assemble_tests {
    use super::*;

    fn en7() -> InterfaceInfo {
        InterfaceInfo {
            name: "en7".into(),
            is_up: true,
            is_running: true,
            is_point_to_point: false,
            is_loopback: false,
            mac: Some("9c:69:d3:6c:38:28".into()),
            ipv4: Some("172.16.0.89".parse().unwrap()),
            prefix_len: Some(23),
        }
    }

    fn route(iface: &str, gw: &str) -> DefaultRoute {
        DefaultRoute {
            gateway: gw.parse().unwrap(),
            interface: iface.into(),
        }
    }

    /// The live state of the development machine.
    #[test]
    fn assembles_wired_identity_from_real_machine_shape() {
        let id = NetworkIdentity::assemble(
            Some(route("en7", "172.16.0.1")),
            &[en7()],
            &[HardwarePort {
                label: "AX88179B".into(),
                device: "en7".into(),
            }],
            vec!["158.193.86.5".parse().unwrap()],
            WifiStatus::RadioOff,
        );

        assert_eq!(id.connection, ConnectionType::Ethernet);
        assert_eq!(id.interface, "en7");
        assert_eq!(id.interface_label.as_deref(), Some("AX88179B"));
        assert_eq!(
            id.local_ip.map(|i| i.to_string()).as_deref(),
            Some("172.16.0.89")
        );
        assert_eq!(
            id.gateway.map(|g| g.to_string()).as_deref(),
            Some("172.16.0.1")
        );
        assert!(!id.vpn_active);
    }

    /// A /23 network address is not simply the address with a zeroed last
    /// octet. 172.16.0.89/23 sits in 172.16.0.0/23.
    #[test]
    fn computes_subnet_from_prefix_not_from_octet_assumption() {
        let id = NetworkIdentity::assemble(
            Some(route("en7", "172.16.0.1")),
            &[en7()],
            &[],
            vec![],
            WifiStatus::RadioOff,
        );
        let subnet = id.subnet.expect("subnet computed");
        assert_eq!(subnet.network.to_string(), "172.16.0.0");
        assert_eq!(subnet.prefix_len, 23);
    }

    #[test]
    fn wifi_identity_carries_association_details() {
        let mut en0 = en7();
        en0.name = "en0".into();
        let details = WifiDetails {
            ssid: Some("Observatory-5G".into()),
            band: Some(Band::Ghz5),
            ..Default::default()
        };

        let id = NetworkIdentity::assemble(
            Some(route("en0", "192.168.1.1")),
            &[en0],
            &[HardwarePort {
                label: "Wi-Fi".into(),
                device: "en0".into(),
            }],
            vec![],
            WifiStatus::Associated(details),
        );

        assert_eq!(id.connection, ConnectionType::Wifi);
        let WifiStatus::Associated(w) = &id.wifi else {
            panic!("expected association")
        };
        assert_eq!(w.ssid.as_deref(), Some("Observatory-5G"));
    }

    /// "The Wi-Fi command failed" and "this Mac has no Wi-Fi" are different
    /// facts. Collapsing them would make the UI state a lie whenever a probe
    /// errors, which is exactly the silent-empty-result failure mode
    /// ARCHITECTURE.md §12 exists to prevent.
    #[test]
    fn failed_wifi_read_is_distinct_from_absent_hardware() {
        let failed = WifiStatus::Unavailable {
            reason: "probe timed out".into(),
        };
        assert_ne!(failed, WifiStatus::NoHardware);
        assert_ne!(failed, WifiStatus::RadioOff);
    }

    #[test]
    fn vpn_holding_default_route_sets_vpn_active() {
        let tunnel = InterfaceInfo {
            name: "utun6".into(),
            is_point_to_point: true,
            mac: None,
            ipv4: Some("10.8.0.2".parse().unwrap()),
            prefix_len: Some(24),
            ..en7()
        };

        let id = NetworkIdentity::assemble(
            Some(route("utun6", "10.8.0.1")),
            &[tunnel, en7()],
            &[],
            vec![],
            WifiStatus::RadioOff,
        );

        assert_eq!(id.connection, ConnectionType::Vpn);
        assert!(id.vpn_active);
    }

    #[test]
    fn no_default_route_yields_unknown_without_inventing_an_interface() {
        let id = NetworkIdentity::assemble(None, &[en7()], &[], vec![], WifiStatus::RadioOff);

        assert_eq!(id.connection, ConnectionType::Unknown);
        assert!(id.interface.is_empty());
        assert!(id.gateway.is_none());
        assert!(id.local_ip.is_none());
    }

    #[test]
    fn interface_without_prefix_has_no_subnet_rather_than_a_default() {
        let mut bare = en7();
        bare.prefix_len = None;
        let id = NetworkIdentity::assemble(
            Some(route("en7", "172.16.0.1")),
            &[bare],
            &[],
            vec![],
            WifiStatus::RadioOff,
        );
        assert!(id.subnet.is_none());
    }
}
