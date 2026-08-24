//! Pure parsers for macOS command output.
//!
//! Every function here takes a string and returns typed data, so the whole
//! platform layer is testable against fixtures with no network and no OS
//! calls. `exec` supplies the strings; this module never touches the system.
//!
//! Fixtures in the tests below are shapes taken verbatim from macOS 26.5.

use std::net::{IpAddr, Ipv4Addr};

use jrx_core::network::{Band, DefaultRoute, HardwarePort, InterfaceInfo, WifiDetails, WifiStatus};

/// Parse `netstat -rn -f inet`.
pub fn parse_default_route(output: &str) -> Option<DefaultRoute> {
    output.lines().find_map(|line| {
        let mut cols = line.split_whitespace();
        if cols.next()? != "default" {
            return None;
        }
        let gateway: IpAddr = cols.next()?.parse().ok()?;
        // Columns are: destination, gateway, flags, netif.
        let _flags = cols.next()?;
        let interface = cols.next()?.to_string();
        Some(DefaultRoute { gateway, interface })
    })
}

/// Parse `ifconfig -a`.
pub fn parse_interfaces(output: &str) -> Vec<InterfaceInfo> {
    let mut interfaces: Vec<InterfaceInfo> = Vec::new();

    for line in output.lines() {
        // A new interface block starts unindented, as "name: flags=...".
        if !line.starts_with(char::is_whitespace) {
            if let Some((name, rest)) = line.split_once(": ") {
                let flags = rest
                    .split_once('<')
                    .and_then(|(_, f)| f.split_once('>'))
                    .map(|(f, _)| f)
                    .unwrap_or("");

                interfaces.push(InterfaceInfo {
                    name: name.to_string(),
                    is_up: flags.contains("UP"),
                    is_running: flags.contains("RUNNING"),
                    is_point_to_point: flags.contains("POINTOPOINT"),
                    is_loopback: flags.contains("LOOPBACK"),
                    mac: None,
                    ipv4: None,
                    prefix_len: None,
                });
            }
            continue;
        }

        let Some(current) = interfaces.last_mut() else {
            continue;
        };
        let trimmed = line.trim();

        if let Some(mac) = trimmed.strip_prefix("ether ") {
            current.mac = Some(mac.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix("inet ") {
            let mut cols = rest.split_whitespace();
            if let Some(addr) = cols.next().and_then(|a| a.parse::<Ipv4Addr>().ok()) {
                current.ipv4 = Some(addr);
            }
            // "netmask 0xfffffe00" — hex, and not always /24.
            while let Some(token) = cols.next() {
                if token == "netmask" {
                    current.prefix_len = cols.next().and_then(parse_netmask_prefix);
                    break;
                }
            }
        }
    }

    interfaces
}

/// Decode a macOS hex netmask such as `0xfffffe00` into a prefix length.
fn parse_netmask_prefix(token: &str) -> Option<u8> {
    let hex = token.strip_prefix("0x")?;
    let mask = u32::from_str_radix(hex, 16).ok()?;
    // Reject non-contiguous masks rather than reporting a misleading number.
    if mask.leading_ones() + mask.trailing_zeros() != 32 {
        return None;
    }
    Some(mask.leading_ones() as u8)
}

/// Parse `networksetup -listallhardwareports`.
pub fn parse_hardware_ports(output: &str) -> Vec<HardwarePort> {
    let mut ports = Vec::new();
    let mut label: Option<String> = None;

    for line in output.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("Hardware Port: ") {
            label = Some(name.trim().to_string());
        } else if let Some(device) = line.strip_prefix("Device: ")
            && let Some(label) = label.take()
        {
            ports.push(HardwarePort {
                label,
                device: device.trim().to_string(),
            });
        }
    }

    ports
}

/// Parse `scutil --dns`, taking the default resolver only.
///
/// Later resolver blocks are domain-scoped (`local` for mDNS, split-DNS
/// entries for VPNs) and are not what the machine uses by default.
pub fn parse_dns_servers(output: &str) -> Vec<IpAddr> {
    let mut servers = Vec::new();
    let mut in_first_resolver = false;

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("resolver #") {
            if in_first_resolver {
                break; // past the default resolver
            }
            in_first_resolver = trimmed == "resolver #1";
            continue;
        }

        if in_first_resolver
            && trimmed.starts_with("nameserver[")
            && let Some((_, value)) = trimmed.split_once(" : ")
            && let Ok(addr) = value.trim().parse::<IpAddr>()
        {
            servers.push(addr);
        }
    }

    servers
}

/// Parse `system_profiler SPAirPortDataType -json`.
///
/// Returns a specific state for every case, never a bare empty result
/// (ARCHITECTURE.md §12).
pub fn parse_airport(json: &str) -> WifiStatus {
    let Ok(root) = serde_json::from_str::<serde_json::Value>(json) else {
        return WifiStatus::NoHardware;
    };

    let Some(iface) = root["SPAirPortDataType"]
        .get(0)
        .and_then(|d| d["spairport_airport_interfaces"].get(0))
    else {
        return WifiStatus::NoHardware;
    };

    if iface["spairport_status_information"].as_str() == Some("spairport_status_off") {
        return WifiStatus::RadioOff;
    }

    let Some(network) = iface.get("spairport_current_network_information") else {
        return WifiStatus::NotAssociated;
    };

    // macOS withholds the network name without Location Services. An absent or
    // redacted name is a permission state, not an unnamed network.
    let ssid = network["_name"].as_str().filter(|s| !s.is_empty());
    let Some(ssid) = ssid else {
        return WifiStatus::PermissionRequired;
    };
    if ssid.starts_with('<') && ssid.ends_with('>') {
        return WifiStatus::PermissionRequired;
    }

    let channel_field = network["spairport_network_channel"].as_str();
    let (channel, band) = channel_field.map_or((None, None), parse_channel);
    let (signal_dbm, noise_dbm) = network["spairport_signal_noise"]
        .as_str()
        .map_or((None, None), parse_signal_noise);

    WifiStatus::Associated(WifiDetails {
        ssid: Some(ssid.to_string()),
        bssid: network["spairport_network_bssid"]
            .as_str()
            .map(str::to_owned),
        channel,
        band,
        signal_dbm,
        noise_dbm,
        security: network["spairport_security_mode"]
            .as_str()
            .map(prettify_security),
        phy_mode: network["spairport_network_phymode"]
            .as_str()
            .map(str::to_owned),
    })
}

/// "36 (5GHz, 80MHz)" -> (36, 5 GHz)
fn parse_channel(field: &str) -> (Option<u16>, Option<Band>) {
    let channel = field
        .split_whitespace()
        .next()
        .and_then(|c| c.parse::<u16>().ok());

    let band = if field.contains("6GHz") {
        Some(Band::Ghz6)
    } else if field.contains("5GHz") {
        Some(Band::Ghz5)
    } else if field.contains("2GHz") || field.contains("2.4GHz") {
        Some(Band::Ghz2_4)
    } else {
        None
    };

    (channel, band)
}

/// "-45 dBm / -92 dBm" -> (-45, -92)
fn parse_signal_noise(field: &str) -> (Option<i16>, Option<i16>) {
    let mut parts = field.split('/').map(|p| {
        p.split_whitespace()
            .next()
            .and_then(|v| v.parse::<i16>().ok())
    });
    (parts.next().flatten(), parts.next().flatten())
}

/// "spairport_security_mode_wpa3_personal" -> "WPA3 Personal"
fn prettify_security(raw: &str) -> String {
    let stripped = raw.trim_start_matches("spairport_security_mode_");
    if stripped == "none" {
        return "Open (no encryption)".to_string();
    }
    stripped
        .split('_')
        .map(|word| {
            if word.starts_with("wpa") || word.starts_with("wep") {
                word.to_uppercase()
            } else {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- fixtures: shapes taken verbatim from macOS 26.5 ----

    const NETSTAT: &str = "\
Routing tables

Internet:
Destination        Gateway            Flags               Netif Expire
default            172.16.0.1         UGScg                 en7
127                127.0.0.1          UCS                   lo0
127.0.0.1          127.0.0.1          UH                    lo0
169.254            link#13            UCS                   en7      !
";

    const IFCONFIG: &str = "\
lo0: flags=8049<UP,LOOPBACK,RUNNING,MULTICAST> mtu 16384
\tinet 127.0.0.1 netmask 0xff000000
en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500
\tether fc:b2:14:b9:60:8b
\tmedia: autoselect
\tstatus: inactive
utun0: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1380
\tinet6 fe80::9a5a:c0d1:5f2c:1a3b%utun0 prefixlen 64 scopeid 0xe
en7: flags=8963<UP,BROADCAST,SMART,RUNNING,PROMISC,SIMPLEX,MULTICAST> mtu 1500
\tether 9c:69:d3:6c:38:28
\tinet 172.16.0.89 netmask 0xfffffe00 broadcast 172.16.1.255
\tmedia: autoselect (1000baseT <full-duplex>)
\tstatus: active
";

    const HARDWARE_PORTS: &str = "\n\
Hardware Port: AX88179B
Device: en7
Ethernet Address: 9c:69:d3:6c:38:28

Hardware Port: Wi-Fi
Device: en0
Ethernet Address: fc:b2:14:b9:60:8b

Hardware Port: Thunderbolt Bridge
Device: bridge0
Ethernet Address: 36:5f:a0:51:26:40
";

    const SCUTIL_DNS: &str = "\
DNS configuration

resolver #1
  nameserver[0] : 158.193.86.5
  nameserver[1] : 158.193.86.2
  if_index : 13 (en7)
  flags    : Request A records
  reach    : 0x00000002 (Reachable)

resolver #2
  domain   : local
  options  : mdns
  timeout  : 5
  flags    : Request A records
";

    // ---- default route ----

    #[test]
    fn finds_default_route_gateway_and_interface() {
        let route = parse_default_route(NETSTAT).expect("a default route exists");
        assert_eq!(route.interface, "en7");
        assert_eq!(route.gateway.to_string(), "172.16.0.1");
    }

    #[test]
    fn absent_default_route_is_none_not_a_guess() {
        assert!(parse_default_route("Routing tables\n\nInternet:\n").is_none());
    }

    // ---- interfaces ----

    #[test]
    fn parses_active_interface_address_and_prefix() {
        let ifaces = parse_interfaces(IFCONFIG);
        let en7 = ifaces.iter().find(|i| i.name == "en7").expect("en7 parsed");

        assert_eq!(
            en7.ipv4.map(|a| a.to_string()).as_deref(),
            Some("172.16.0.89")
        );
        // 0xfffffe00 is a /23 — the hex netmask form must be decoded, not
        // assumed to be /24.
        assert_eq!(en7.prefix_len, Some(23));
        assert_eq!(en7.mac.as_deref(), Some("9c:69:d3:6c:38:28"));
        assert!(en7.is_up && en7.is_running);
        assert!(!en7.is_point_to_point);
    }

    #[test]
    fn marks_tunnel_interfaces_point_to_point() {
        let ifaces = parse_interfaces(IFCONFIG);
        let utun = ifaces
            .iter()
            .find(|i| i.name == "utun0")
            .expect("utun0 parsed");
        assert!(utun.is_point_to_point);
        assert!(utun.mac.is_none());
    }

    #[test]
    fn marks_loopback() {
        let ifaces = parse_interfaces(IFCONFIG);
        let lo = ifaces.iter().find(|i| i.name == "lo0").expect("lo0 parsed");
        assert!(lo.is_loopback);
    }

    #[test]
    fn interface_with_no_address_still_appears() {
        let ifaces = parse_interfaces(IFCONFIG);
        let en0 = ifaces.iter().find(|i| i.name == "en0").expect("en0 parsed");
        assert!(en0.ipv4.is_none());
        assert_eq!(en0.mac.as_deref(), Some("fc:b2:14:b9:60:8b"));
    }

    // ---- hardware ports ----

    #[test]
    fn maps_devices_to_their_hardware_labels() {
        let ports = parse_hardware_ports(HARDWARE_PORTS);
        let find = |dev: &str| {
            ports
                .iter()
                .find(|p| p.device == dev)
                .map(|p| p.label.clone())
        };
        assert_eq!(find("en0").as_deref(), Some("Wi-Fi"));
        assert_eq!(find("en7").as_deref(), Some("AX88179B"));
    }

    // ---- DNS ----

    #[test]
    fn reads_default_resolver_nameservers() {
        let servers = parse_dns_servers(SCUTIL_DNS);
        assert_eq!(
            servers.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            vec!["158.193.86.5", "158.193.86.2"],
        );
    }

    #[test]
    fn ignores_the_mdns_resolver_which_has_no_nameservers() {
        // resolver #2 is the mDNS resolver; it must not contribute entries.
        assert_eq!(parse_dns_servers(SCUTIL_DNS).len(), 2);
    }

    // ---- Wi-Fi ----

    /// The live state of the development machine: hardware present, radio off.
    #[test]
    fn radio_off_is_reported_as_radio_off() {
        let json = r#"{"SPAirPortDataType":[{"spairport_airport_interfaces":[
            {"_name":"en0","spairport_status_information":"spairport_status_off"}]}]}"#;
        assert_eq!(parse_airport(json), WifiStatus::RadioOff);
    }

    #[test]
    fn no_wifi_hardware_is_reported_as_no_hardware() {
        let json = r#"{"SPAirPortDataType":[{}]}"#;
        assert_eq!(parse_airport(json), WifiStatus::NoHardware);
    }

    #[test]
    fn radio_on_but_unjoined_is_reported_as_not_associated() {
        let json = r#"{"SPAirPortDataType":[{"spairport_airport_interfaces":[
            {"_name":"en0","spairport_status_information":"spairport_status_connected"}]}]}"#;
        assert_eq!(parse_airport(json), WifiStatus::NotAssociated);
    }

    #[test]
    fn associated_network_yields_ssid_channel_band_and_signal() {
        let json = r#"{"SPAirPortDataType":[{"spairport_airport_interfaces":[{
            "_name":"en0",
            "spairport_status_information":"spairport_status_connected",
            "spairport_current_network_information":{
                "_name":"Observatory-5G",
                "spairport_network_channel":"36 (5GHz, 80MHz)",
                "spairport_network_phymode":"802.11ax",
                "spairport_security_mode":"spairport_security_mode_wpa3_personal",
                "spairport_signal_noise":"-45 dBm / -92 dBm",
                "spairport_network_bssid":"a4:83:e7:11:22:33"
            }}]}]}"#;

        let WifiStatus::Associated(w) = parse_airport(json) else {
            panic!("expected Associated");
        };
        assert_eq!(w.ssid.as_deref(), Some("Observatory-5G"));
        assert_eq!(w.bssid.as_deref(), Some("a4:83:e7:11:22:33"));
        assert_eq!(w.channel, Some(36));
        assert_eq!(w.band, Some(Band::Ghz5));
        assert_eq!(w.signal_dbm, Some(-45));
        assert_eq!(w.noise_dbm, Some(-92));
        assert_eq!(w.phy_mode.as_deref(), Some("802.11ax"));
    }

    #[test]
    fn parses_two_point_four_gigahertz_band() {
        let json = r#"{"SPAirPortDataType":[{"spairport_airport_interfaces":[{
            "_name":"en0","spairport_status_information":"spairport_status_connected",
            "spairport_current_network_information":{
                "_name":"Home","spairport_network_channel":"11 (2GHz, 20MHz)"}}]}]}"#;
        let WifiStatus::Associated(w) = parse_airport(json) else {
            panic!()
        };
        assert_eq!(w.band, Some(Band::Ghz2_4));
        assert_eq!(w.channel, Some(11));
    }

    /// Associated, but macOS withholds the network name without Location
    /// Services. Must be a distinct state, not an empty SSID.
    #[test]
    fn associated_without_location_permission_is_permission_required() {
        let json = r#"{"SPAirPortDataType":[{"spairport_airport_interfaces":[{
            "_name":"en0","spairport_status_information":"spairport_status_connected",
            "spairport_current_network_information":{
                "spairport_network_channel":"36 (5GHz, 80MHz)"}}]}]}"#;
        assert_eq!(parse_airport(json), WifiStatus::PermissionRequired);
    }

    #[test]
    fn redacted_ssid_is_also_permission_required() {
        let json = r#"{"SPAirPortDataType":[{"spairport_airport_interfaces":[{
            "_name":"en0","spairport_status_information":"spairport_status_connected",
            "spairport_current_network_information":{"_name":"<redacted>"}}]}]}"#;
        assert_eq!(parse_airport(json), WifiStatus::PermissionRequired);
    }

    #[test]
    fn malformed_json_does_not_panic() {
        assert_eq!(parse_airport("not json at all"), WifiStatus::NoHardware);
    }
}
