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

        // A tunnel is a route, not a kind of link. Whatever is underneath it
        // is what the machine is actually connected to, so the caller resolves
        // the physical interface before asking this question.
        if iface.is_some_and(|i| i.is_point_to_point) {
            return ConnectionType::Unknown;
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
            ConnectionType::Unknown => "Unknown connection",
        }
    }
}

/// One entry from the routing table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RouteEntry {
    /// "default", "192.168.1", "127" — as the platform prints it.
    pub destination: String,
    pub gateway: Option<IpAddr>,
    pub interface: String,
}

/// A tunnel carrying the default route.
///
/// Reported alongside the physical connection, never instead of it: "you are
/// on Wi-Fi, and your traffic leaves through a VPN" is two facts, and
/// collapsing them loses the one the user asked about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tunnel {
    /// "utun6"
    pub interface: String,
    pub gateway: Option<IpAddr>,
    pub local_ip: Option<Ipv4Addr>,
}

/// An interface that is up and addressed, but is not the primary one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActiveInterface {
    pub interface: String,
    pub label: Option<String>,
    pub connection: ConnectionType,
    pub local_ip: Option<Ipv4Addr>,
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
    /// Set when a tunnel carries the default route. The fields above continue
    /// to describe the physical network.
    pub tunnel: Option<Tunnel>,
    /// Other interfaces that are up and addressed.
    pub other_active: Vec<ActiveInterface>,
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

    /// A tunnel is a route, not a kind of link. `classify` is asked only
    /// about physical interfaces, so a tunnel handed to it is not something it
    /// can describe — the caller resolves what lies beneath first.
    #[test]
    fn a_tunnel_is_not_a_kind_of_physical_link() {
        let mut tunnel = iface("utun6");
        tunnel.is_point_to_point = true;
        tunnel.mac = None;

        let got = ConnectionType::classify("utun6", &[tunnel], &[]);
        assert_eq!(got, ConnectionType::Unknown);
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
    ///
    /// A tunnel holding the default route does not become the connection. The
    /// physical link beneath it is resolved first, and the tunnel is reported
    /// alongside it.
    pub fn assemble(
        routes: &[RouteEntry],
        interfaces: &[InterfaceInfo],
        ports: &[HardwarePort],
        dns_servers: Vec<IpAddr>,
        wifi: WifiStatus,
    ) -> NetworkIdentity {
        let default_route = routes.iter().find(|r| r.destination == "default");
        let describe = |name: &str| {
            ports
                .iter()
                .find(|p| p.device == name)
                .map(|p| p.label.clone())
        };

        let Some(default_route) = default_route else {
            return NetworkIdentity {
                connection: ConnectionType::Unknown,
                interface: String::new(),
                interface_label: None,
                local_ip: None,
                subnet: None,
                gateway: None,
                dns_servers,
                wifi,
                tunnel: None,
                other_active: Vec::new(),
            };
        };

        let routed = interfaces
            .iter()
            .find(|i| i.name == default_route.interface);

        // A tunnel carries traffic; it is not what the machine is attached to.
        let tunnel = routed.filter(|i| i.is_point_to_point).map(|i| Tunnel {
            interface: i.name.clone(),
            gateway: default_route.gateway,
            local_ip: i.ipv4,
        });

        // With a tunnel in the way, the physical link has to be found beneath
        // it. Without one, the routed interface *is* the physical link.
        let physical = if tunnel.is_some() {
            physical_beneath_tunnel(routes, interfaces)
        } else {
            routed
        };

        let Some(physical) = physical else {
            // A tunnel with nothing identifiable underneath. Say so, rather
            // than presenting the tunnel as the physical connection.
            return NetworkIdentity {
                connection: ConnectionType::Unknown,
                interface: String::new(),
                interface_label: None,
                local_ip: None,
                subnet: None,
                gateway: None,
                dns_servers,
                wifi,
                tunnel,
                other_active: other_active(interfaces, "", ports),
            };
        };

        let name = physical.name.as_str();
        let connection = ConnectionType::classify(name, interfaces, ports);

        // The gateway of the network the user is on, which is not the tunnel's.
        let gateway = if tunnel.is_some() {
            routes
                .iter()
                .find(|r| r.interface == name && r.gateway.is_some())
                .and_then(|r| r.gateway)
        } else {
            default_route.gateway
        };

        NetworkIdentity {
            connection,
            interface: name.to_string(),
            interface_label: describe(name),
            local_ip: physical.ipv4,
            subnet: physical
                .ipv4
                .zip(physical.prefix_len)
                .and_then(|(addr, prefix)| Subnet::of(addr, prefix)),
            gateway,
            dns_servers,
            wifi,
            tunnel,
            other_active: other_active(interfaces, name, ports),
        }
    }
}

/// Whether an interface could be a physical link carrying real traffic.
fn is_physical_candidate(iface: &InterfaceInfo) -> bool {
    iface.is_up
        && iface.is_running
        && !iface.is_loopback
        && !iface.is_point_to_point
        && iface.mac.is_some()
        && iface.ipv4.is_some()
}

/// Find the interface a tunnel is running over.
///
/// The routing table is the evidence: the physical interface keeps routes of
/// its own — its subnet, and usually a host route to the VPN endpoint — while
/// the tunnel holds only the default. Whichever candidate carries the most
/// routes is the one actually attached to a network. Ties break by name so the
/// answer never depends on enumeration order.
fn physical_beneath_tunnel<'a>(
    routes: &[RouteEntry],
    interfaces: &'a [InterfaceInfo],
) -> Option<&'a InterfaceInfo> {
    interfaces
        .iter()
        .filter(|i| is_physical_candidate(i))
        .max_by(|a, b| {
            let weight = |name: &str| routes.iter().filter(|r| r.interface == name).count();
            weight(&a.name)
                .cmp(&weight(&b.name))
                // Reversed so the *lowest* name wins a tie, via max_by.
                .then_with(|| b.name.cmp(&a.name))
        })
}

/// Interfaces that are up and addressed but are not the primary one.
fn other_active(
    interfaces: &[InterfaceInfo],
    primary: &str,
    ports: &[HardwarePort],
) -> Vec<ActiveInterface> {
    interfaces
        .iter()
        .filter(|i| is_physical_candidate(i) && i.name != primary)
        .map(|i| ActiveInterface {
            interface: i.name.clone(),
            label: ports
                .iter()
                .find(|p| p.device == i.name)
                .map(|p| p.label.clone()),
            connection: ConnectionType::classify(&i.name, interfaces, ports),
            local_ip: i.ipv4,
        })
        .collect()
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

    fn default_via(iface: &str, gw: &str) -> RouteEntry {
        RouteEntry {
            destination: "default".into(),
            gateway: Some(gw.parse().unwrap()),
            interface: iface.into(),
        }
    }

    /// The live state of the development machine.
    #[test]
    fn assembles_wired_identity_from_real_machine_shape() {
        let id = NetworkIdentity::assemble(
            &[default_via("en7", "172.16.0.1")],
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
        assert!(id.tunnel.is_none());
    }

    /// A /23 network address is not simply the address with a zeroed last
    /// octet. 172.16.0.89/23 sits in 172.16.0.0/23.
    #[test]
    fn computes_subnet_from_prefix_not_from_octet_assumption() {
        let id = NetworkIdentity::assemble(
            &[default_via("en7", "172.16.0.1")],
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

        let id = NetworkIdentity::assemble(
            &[default_via("en0", "192.168.1.1")],
            &[en0],
            &[HardwarePort {
                label: "Wi-Fi".into(),
                device: "en0".into(),
            }],
            vec![],
            WifiStatus::Associated(WifiDetails {
                ssid: Some("Observatory-5G".into()),
                band: Some(Band::Ghz5),
                ..Default::default()
            }),
        );

        assert_eq!(id.connection, ConnectionType::Wifi);
        let WifiStatus::Associated(w) = &id.wifi else {
            panic!("expected association")
        };
        assert_eq!(w.ssid.as_deref(), Some("Observatory-5G"));
    }

    #[test]
    fn interface_without_prefix_has_no_subnet_rather_than_a_default() {
        let mut bare = en7();
        bare.prefix_len = None;
        let id = NetworkIdentity::assemble(
            &[default_via("en7", "172.16.0.1")],
            &[bare],
            &[],
            vec![],
            WifiStatus::RadioOff,
        );
        assert!(id.subnet.is_none());
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
}

#[cfg(test)]
mod network_state_tests {
    use super::*;

    fn iface(name: &str, ip: Option<&str>, mac: Option<&str>) -> InterfaceInfo {
        InterfaceInfo {
            name: name.into(),
            is_up: true,
            is_running: true,
            is_point_to_point: name.starts_with("utun") || name.starts_with("ppp"),
            is_loopback: name == "lo0",
            mac: mac.map(str::to_string),
            ipv4: ip.map(|a| a.parse().unwrap()),
            prefix_len: ip.map(|_| 24),
        }
    }
    fn route(dest: &str, gw: Option<&str>, iface: &str) -> RouteEntry {
        RouteEntry {
            destination: dest.into(),
            gateway: gw.map(|g| g.parse().unwrap()),
            interface: iface.into(),
        }
    }
    fn wifi_port() -> HardwarePort {
        HardwarePort {
            label: "Wi-Fi".into(),
            device: "en0".into(),
        }
    }

    // ---- A. Ethernet, the physically validated case ----

    #[test]
    fn a_wired_connection_reports_no_tunnel() {
        let id = NetworkIdentity::assemble(
            &[
                route("default", Some("172.16.0.1"), "en7"),
                route("172.16", None, "en7"),
            ],
            &[iface("en7", Some("172.16.0.89"), Some("9c:69:d3:6c:38:28"))],
            &[HardwarePort {
                label: "AX88179B".into(),
                device: "en7".into(),
            }],
            vec![],
            WifiStatus::RadioOff,
        );

        assert_eq!(id.connection, ConnectionType::Ethernet);
        assert_eq!(id.interface, "en7");
        assert!(id.tunnel.is_none());
    }

    // ---- F. VPN must not replace the physical connection ----

    /// The tunnel is how traffic leaves, not what the machine is connected to.
    /// Reporting "VPN" as the connection loses the fact that the user is on
    /// Wi-Fi, which is the thing they actually asked about.
    #[test]
    fn a_vpn_over_wifi_still_reports_wifi_as_the_connection() {
        let id = NetworkIdentity::assemble(
            &[
                route("default", Some("10.8.0.1"), "utun6"),
                route("192.168.1", None, "en0"),
                route("198.51.100.7", Some("192.168.1.1"), "en0"),
            ],
            &[
                iface("utun6", Some("10.8.0.2"), None),
                iface("en0", Some("192.168.1.50"), Some("a4:83:e7:11:22:33")),
            ],
            &[wifi_port()],
            vec![],
            WifiStatus::Associated(WifiDetails {
                ssid: Some("Home".into()),
                ..Default::default()
            }),
        );

        assert_eq!(
            id.connection,
            ConnectionType::Wifi,
            "the physical link is Wi-Fi"
        );
        assert_eq!(
            id.interface, "en0",
            "the physical interface, not the tunnel"
        );
        assert_eq!(
            id.local_ip.map(|a| a.to_string()).as_deref(),
            Some("192.168.1.50"),
            "the address on the network the user is actually on"
        );

        let tunnel = id
            .tunnel
            .as_ref()
            .expect("the tunnel is reported separately");
        assert_eq!(tunnel.interface, "utun6");
    }

    #[test]
    fn a_vpn_over_ethernet_still_reports_ethernet() {
        let id = NetworkIdentity::assemble(
            &[
                route("default", Some("10.8.0.1"), "utun4"),
                route("172.16", None, "en7"),
            ],
            &[
                iface("utun4", Some("10.8.0.2"), None),
                iface("en7", Some("172.16.0.89"), Some("9c:69:d3:6c:38:28")),
            ],
            &[HardwarePort {
                label: "AX88179B".into(),
                device: "en7".into(),
            }],
            vec![],
            WifiStatus::RadioOff,
        );

        assert_eq!(id.connection, ConnectionType::Ethernet);
        assert!(id.tunnel.is_some());
    }

    /// Observed on the development machine: six utun interfaces are UP and
    /// RUNNING with no VPN active. macOS uses them for its own services.
    #[test]
    fn idle_tunnels_are_not_reported_as_a_vpn() {
        let id = NetworkIdentity::assemble(
            &[
                route("default", Some("172.16.0.1"), "en7"),
                route("172.16", None, "en7"),
            ],
            &[
                iface("utun0", None, None),
                iface("utun1", None, None),
                iface("en7", Some("172.16.0.89"), Some("9c:69:d3:6c:38:28")),
            ],
            &[],
            vec![],
            WifiStatus::RadioOff,
        );
        assert!(
            id.tunnel.is_none(),
            "a live tunnel is not a VPN unless it carries traffic"
        );
    }

    // ---- D. Wi-Fi with the network name withheld ----

    /// "Wi-Fi, network name unavailable" and "unknown connection" are
    /// different facts. macOS withholding the SSID says nothing about what
    /// kind of link this is.
    #[test]
    fn a_withheld_ssid_does_not_downgrade_the_connection_to_unknown() {
        let id = NetworkIdentity::assemble(
            &[route("default", Some("192.168.1.1"), "en0")],
            &[iface(
                "en0",
                Some("192.168.1.50"),
                Some("a4:83:e7:11:22:33"),
            )],
            &[wifi_port()],
            vec![],
            WifiStatus::PermissionRequired,
        );

        assert_eq!(id.connection, ConnectionType::Wifi);
        assert_eq!(id.wifi, WifiStatus::PermissionRequired);
    }

    // ---- C. Radio off ----

    #[test]
    fn wifi_hardware_with_the_radio_off_is_not_the_active_connection() {
        let id = NetworkIdentity::assemble(
            &[route("default", Some("172.16.0.1"), "en7")],
            &[
                iface("en0", None, Some("fc:b2:14:b9:60:8b")),
                iface("en7", Some("172.16.0.89"), Some("9c:69:d3:6c:38:28")),
            ],
            &[
                wifi_port(),
                HardwarePort {
                    label: "AX88179B".into(),
                    device: "en7".into(),
                },
            ],
            vec![],
            WifiStatus::RadioOff,
        );
        assert_eq!(id.connection, ConnectionType::Ethernet);
    }

    // ---- H. Multiple active interfaces ----

    #[test]
    fn other_active_interfaces_are_reported_without_being_confused_for_the_primary() {
        let id = NetworkIdentity::assemble(
            &[
                route("default", Some("172.16.0.1"), "en7"),
                route("192.168.1", None, "en0"),
            ],
            &[
                iface("en7", Some("172.16.0.89"), Some("9c:69:d3:6c:38:28")),
                iface("en0", Some("192.168.1.50"), Some("a4:83:e7:11:22:33")),
            ],
            &[wifi_port()],
            vec![],
            WifiStatus::Associated(WifiDetails::default()),
        );

        assert_eq!(id.interface, "en7", "the default route decides the primary");
        let others: Vec<&str> = id
            .other_active
            .iter()
            .map(|i| i.interface.as_str())
            .collect();
        assert_eq!(others, vec!["en0"]);
    }

    #[test]
    fn loopback_is_never_reported_as_an_active_interface() {
        let id = NetworkIdentity::assemble(
            &[route("default", Some("172.16.0.1"), "en7")],
            &[
                iface("lo0", Some("127.0.0.1"), None),
                iface("en7", Some("172.16.0.89"), Some("9c:69:d3:6c:38:28")),
            ],
            &[],
            vec![],
            WifiStatus::RadioOff,
        );
        assert!(id.other_active.iter().all(|i| i.interface != "lo0"));
    }

    // ---- G. No usable network ----

    #[test]
    fn no_default_route_reports_unknown_without_inventing_an_interface() {
        let id = NetworkIdentity::assemble(&[], &[], &[], vec![], WifiStatus::RadioOff);

        assert_eq!(id.connection, ConnectionType::Unknown);
        assert!(id.interface.is_empty());
        assert!(id.gateway.is_none());
        assert!(id.tunnel.is_none());
    }

    /// A tunnel with no discoverable physical link beneath it must say so
    /// rather than presenting the tunnel as the physical connection.
    #[test]
    fn a_tunnel_with_no_physical_link_beneath_it_reports_unknown_not_vpn() {
        let id = NetworkIdentity::assemble(
            &[route("default", Some("10.8.0.1"), "utun6")],
            &[iface("utun6", Some("10.8.0.2"), None)],
            &[],
            vec![],
            WifiStatus::RadioOff,
        );

        assert_eq!(id.connection, ConnectionType::Unknown);
        assert!(id.tunnel.is_some(), "the tunnel itself is still reported");
    }
}
