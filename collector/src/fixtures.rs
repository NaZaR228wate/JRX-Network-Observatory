//! Deterministic network scenarios, for validation and the 30-second test.
//!
//! Every fixture runs through the *real* pipeline: it supplies routes,
//! interfaces and observations, and those go through `NetworkIdentity::assemble`,
//! `DeviceTable` and `TopologyOverview` exactly as live data does. There is no
//! parallel demo model — a fixture that behaved differently from production
//! would validate nothing.
//!
//! Development only. See the compile-time guard below.

#![cfg(feature = "fixtures")]

// A fixture-backed build must never reach a user. Enabling this feature in a
// release build is a mistake serious enough to stop compilation rather than
// ship an app that invents a network.
#[cfg(not(debug_assertions))]
compile_error!(
    "the `fixtures` feature must never be enabled in a release build: it would \
     ship an application that fabricates network data"
);

use std::net::{IpAddr, Ipv4Addr};

use jrx_core::capability::{PermissionSet, PermissionState};
use jrx_core::declaration::Permission;
use jrx_core::device::{DeviceTable, DiscoveryMethod, MacAddress, Observation};
use jrx_core::network::{
    Band, HardwarePort, InterfaceInfo, NetworkIdentity, RouteEntry, WifiDetails, WifiStatus,
};
use jrx_core::topology::{DiscoverySummary, Topology, TopologyOverview};

use crate::discovery::quality::{SourceQuality, assess};
use crate::discovery::{DiscoveryReport, SourceStatus};
use crate::oui;

/// The scenarios worth validating against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fixture {
    /// A typical home network: router, this Mac, phones, TV, printer.
    HomeWifi,
    /// A large managed network: many devices, mostly private addresses,
    /// partial mDNS, no SSDP.
    UniversityWifi,
    /// Wired, Wi-Fi radio off.
    Ethernet,
    /// A phone sharing its connection. Very few peers.
    Hotspot,
    /// A VPN over Wi-Fi. The physical connection must survive.
    Vpn,
    /// A network that stops devices seeing each other.
    IsolatedNetwork,
    /// Wi-Fi connected, but macOS withholds the network name.
    PermissionLimited,
    /// 500 devices. Not a network anyone has — a load case, for checking that
    /// level 1 and level 2 stay bounded and nothing jitters.
    Stress500,
}

impl Fixture {
    /// Read `JRX_FIXTURE`. Returns `None` when unset or unrecognised.
    pub fn from_env() -> Option<Fixture> {
        Fixture::parse(&std::env::var("JRX_FIXTURE").ok()?)
    }

    pub fn parse(name: &str) -> Option<Fixture> {
        match name.trim().to_ascii_lowercase().as_str() {
            "home_wifi" => Some(Fixture::HomeWifi),
            "university_wifi" => Some(Fixture::UniversityWifi),
            "ethernet" => Some(Fixture::Ethernet),
            "hotspot" => Some(Fixture::Hotspot),
            "vpn" => Some(Fixture::Vpn),
            "isolated_network" => Some(Fixture::IsolatedNetwork),
            "permission_limited" => Some(Fixture::PermissionLimited),
            "stress_500" => Some(Fixture::Stress500),
            _ => None,
        }
    }

    pub const ALL: [Fixture; 8] = [
        Fixture::HomeWifi,
        Fixture::UniversityWifi,
        Fixture::Ethernet,
        Fixture::Hotspot,
        Fixture::Vpn,
        Fixture::IsolatedNetwork,
        Fixture::PermissionLimited,
        Fixture::Stress500,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Fixture::HomeWifi => "home_wifi",
            Fixture::UniversityWifi => "university_wifi",
            Fixture::Ethernet => "ethernet",
            Fixture::Hotspot => "hotspot",
            Fixture::Vpn => "vpn",
            Fixture::IsolatedNetwork => "isolated_network",
            Fixture::PermissionLimited => "permission_limited",
            Fixture::Stress500 => "stress_500",
        }
    }

    /// The network identity, built by the real assembler from fixture routes.
    pub fn identity(self) -> NetworkIdentity {
        let s = self.spec();
        NetworkIdentity::assemble(&s.routes, &s.interfaces, &s.ports, s.dns, s.wifi)
    }

    /// What the permission probes would report.
    pub fn permissions(self) -> PermissionSet {
        let location = match self {
            Fixture::PermissionLimited => PermissionState::Denied,
            Fixture::Ethernet | Fixture::IsolatedNetwork => PermissionState::NotRequested,
            _ => PermissionState::Granted,
        };
        PermissionSet::new()
            .with(Permission::LocationServices, location)
            // macOS never reports this; the fixture must not pretend otherwise.
            .with(Permission::LocalNetwork, PermissionState::Unknown)
    }

    /// A full discovery result, produced by the real pipeline.
    pub fn report(self) -> DiscoveryReport {
        let subnet = self.identity().subnet;
        let spec = self.spec();
        let mut table = DeviceTable::new();
        for observation in spec.observations {
            table.observe(observation);
        }
        if let Some(gateway) = spec.gateway {
            table.mark_gateway(gateway);
        }
        if let Some(local) = spec.self_ip {
            table.mark_self(IpAddr::V4(local));
        }

        let devices = table.finish(oui::vendor_of);
        let others = devices
            .iter()
            .filter(|d| !d.is_self && !d.is_gateway)
            .count();

        DiscoveryReport {
            overview: TopologyOverview::build(&devices),
            topology: Topology::build(&devices),
            summary: DiscoverySummary::of(&devices, subnet),
            quality: assess(&spec.sources, others),
            took_ms: 3120,
            devices,
        }
    }

    fn spec(self) -> Spec {
        match self {
            Fixture::HomeWifi => home_wifi(),
            Fixture::UniversityWifi => university_wifi(),
            Fixture::Ethernet => ethernet(),
            Fixture::Hotspot => hotspot(),
            Fixture::Vpn => vpn(),
            Fixture::IsolatedNetwork => isolated_network(),
            Fixture::PermissionLimited => permission_limited(),
            Fixture::Stress500 => stress_500(),
        }
    }
}

struct Spec {
    routes: Vec<RouteEntry>,
    interfaces: Vec<InterfaceInfo>,
    ports: Vec<HardwarePort>,
    dns: Vec<IpAddr>,
    wifi: WifiStatus,
    observations: Vec<Observation>,
    gateway: Option<IpAddr>,
    self_ip: Option<Ipv4Addr>,
    sources: Vec<SourceQuality>,
}

// ---------- building blocks ----------

fn ip(s: &str) -> IpAddr {
    s.parse().expect("fixture address")
}

fn route(destination: &str, gateway: Option<&str>, interface: &str) -> RouteEntry {
    RouteEntry {
        destination: destination.into(),
        gateway: gateway.map(ip),
        interface: interface.into(),
    }
}

fn physical(name: &str, address: &str, prefix: u8, mac: &str) -> InterfaceInfo {
    InterfaceInfo {
        name: name.into(),
        is_up: true,
        is_running: true,
        is_point_to_point: false,
        is_loopback: false,
        mac: Some(mac.into()),
        ipv4: Some(address.parse().expect("fixture address")),
        prefix_len: Some(prefix),
    }
}

fn tunnel_iface(name: &str, address: &str) -> InterfaceInfo {
    InterfaceInfo {
        name: name.into(),
        is_up: true,
        is_running: true,
        is_point_to_point: true,
        is_loopback: false,
        mac: None,
        ipv4: Some(address.parse().expect("fixture address")),
        prefix_len: Some(24),
    }
}

fn arp(address: &str, mac: &str) -> Observation {
    Observation::new(ip(address), DiscoveryMethod::ArpCache).with_mac(MacAddress::parse(mac))
}

fn announced(address: &str, hostname: &str, services: &[&str]) -> Observation {
    let mut o =
        Observation::new(ip(address), DiscoveryMethod::Mdns).with_hostname(Some(hostname.into()));
    for service in services {
        o = o.with_service(*service);
    }
    o
}

fn source(
    method: DiscoveryMethod,
    observations: usize,
    names: usize,
    types: usize,
) -> SourceQuality {
    SourceQuality {
        method,
        label: method.label(),
        status: SourceStatus::Ok { observations },
        observations,
        names_resolved: names,
        services_seen: types,
    }
}

fn wifi(ssid: &str, band: Band, channel: u16, dbm: i16) -> WifiStatus {
    WifiStatus::Associated(WifiDetails {
        ssid: Some(ssid.into()),
        bssid: None,
        channel: Some(channel),
        band: Some(band),
        signal_dbm: Some(dbm),
        noise_dbm: Some(-92),
        security: Some("WPA3 Personal".into()),
        phy_mode: Some("802.11ax".into()),
    })
}

fn wifi_port() -> HardwarePort {
    HardwarePort {
        label: "Wi-Fi".into(),
        device: "en0".into(),
    }
}

// ---------- the scenarios ----------

fn home_wifi() -> Spec {
    Spec {
        routes: vec![
            route("default", Some("192.168.1.1"), "en0"),
            route("192.168.1", None, "en0"),
        ],
        interfaces: vec![physical("en0", "192.168.1.14", 24, "a4:83:e7:2c:11:80")],
        ports: vec![wifi_port()],
        dns: vec![ip("192.168.1.1")],
        wifi: wifi("Home", Band::Ghz5, 44, -47),
        observations: vec![
            arp("192.168.1.1", "00:0c:42:41:9a:02"),
            Observation::new(ip("192.168.1.1"), DiscoveryMethod::Ssdp)
                .with_upnp_type("urn:schemas-upnp-org:device:InternetGatewayDevice:1"),
            arp("192.168.1.14", "a4:83:e7:2c:11:80"),
            announced(
                "192.168.1.14",
                "Nazars-MacBook-Pro",
                &["_smb._tcp", "_ssh._tcp"],
            ),
            arp("192.168.1.23", "f0:99:bf:31:c4:7a"),
            announced("192.168.1.23", "Nazars-iPhone", &["_apple-mobdev2._tcp"]),
            arp("192.168.1.27", "9e:4a:11:20:38:d1"),
            announced("192.168.1.27", "Pixel-8", &["_rdlink._tcp"]),
            arp("192.168.1.40", "70:3e:ac:19:d5:66"),
            announced(
                "192.168.1.40",
                "Living-Room-Apple-TV",
                &["_airplay._tcp", "_raop._tcp"],
            ),
            arp("192.168.1.44", "54:60:09:aa:12:03"),
            announced("192.168.1.44", "Chromecast-Kitchen", &["_googlecast._tcp"]),
            arp("192.168.1.55", "3c:d9:2b:60:88:14"),
            announced(
                "192.168.1.55",
                "HP-LaserJet-M404",
                &["_ipp._tcp", "_printer._tcp"],
            ),
            arp("192.168.1.61", "1c:90:ff:22:6e:41"),
            announced("192.168.1.61", "smart-plug-hall", &["_hap._tcp"]),
            arp("192.168.1.72", "9e:1d:44:b7:05:c3"),
            arp("192.168.1.88", "b0:4a:39:71:2c:5e"),
        ],
        gateway: Some(ip("192.168.1.1")),
        self_ip: Some("192.168.1.14".parse().unwrap()),
        sources: vec![
            source(DiscoveryMethod::ArpCache, 10, 0, 0),
            source(DiscoveryMethod::Mdns, 13, 7, 9),
            source(DiscoveryMethod::Ssdp, 1, 0, 0),
        ],
    }
}

fn university_wifi() -> Spec {
    let mut observations = vec![
        arp("10.20.0.1", "00:0c:42:11:22:33"),
        arp("10.20.4.90", "a4:83:e7:2c:11:80"),
        announced(
            "10.20.4.90",
            "Nazars-MacBook-Pro",
            &["_smb._tcp", "_ssh._tcp"],
        ),
        announced(
            "10.20.5.11",
            "MacBook-Air-Yaroslava",
            &["_airplay._tcp", "_raop._tcp"],
        ),
        announced("10.20.5.24", "iPad-kate67", &["_companion-link._tcp"]),
        announced("10.20.6.2", "switch9db1a0", &["_http._tcp"]),
        announced("10.20.6.77", "LAPTOP-FSPI6LK4", &["_spotify-connect._tcp"]),
    ];
    // A managed network is mostly phones rotating their addresses, with a
    // minority of devices whose manufacturer resolves but whose type does not.
    // Real registered prefixes, so the OUI lookup behaves as it does live.
    const REGISTERED: [&str; 4] = ["48:45:20", "b0:4a:39", "1c:90:ff", "14:b5:cd"];
    for i in 0..128 {
        let address = format!("10.20.{}.{}", 8 + i / 200, i % 200 + 1);
        let mac = if i % 3 == 0 {
            format!(
                "{}:{:02x}:{:02x}:{:02x}",
                REGISTERED[(i / 3) % REGISTERED.len()],
                i / 256,
                i % 256,
                (i * 7) % 256
            )
        } else {
            // The locally-administered bit set: a rotating address.
            format!("9e:aa:bb:{:02x}:{:02x}:01", i / 256, i % 256)
        };
        observations.push(arp(&address, &mac));
    }
    Spec {
        routes: vec![
            route("default", Some("10.20.0.1"), "en0"),
            route("10.20", None, "en0"),
        ],
        interfaces: vec![physical("en0", "10.20.4.90", 16, "a4:83:e7:2c:11:80")],
        ports: vec![wifi_port()],
        dns: vec![ip("10.20.0.53")],
        wifi: wifi("eduroam", Band::Ghz5, 108, -63),
        observations,
        gateway: Some(ip("10.20.0.1")),
        self_ip: Some("10.20.4.90".parse().unwrap()),
        sources: vec![
            source(DiscoveryMethod::ArpCache, 135, 0, 0),
            source(DiscoveryMethod::Mdns, 11, 5, 5),
            // Managed networks routinely block UPnP.
            source(DiscoveryMethod::Ssdp, 0, 0, 0),
        ],
    }
}

fn ethernet() -> Spec {
    Spec {
        routes: vec![
            route("default", Some("172.16.0.1"), "en7"),
            route("172.16", None, "en7"),
        ],
        interfaces: vec![
            physical("en7", "172.16.0.89", 23, "9c:69:d3:6c:38:28"),
            InterfaceInfo {
                name: "en0".into(),
                is_up: true,
                is_running: false,
                is_point_to_point: false,
                is_loopback: false,
                mac: Some("fc:b2:14:b9:60:8b".into()),
                ipv4: None,
                prefix_len: None,
            },
        ],
        ports: vec![
            wifi_port(),
            HardwarePort {
                label: "AX88179B".into(),
                device: "en7".into(),
            },
        ],
        dns: vec![ip("172.16.0.1")],
        wifi: WifiStatus::RadioOff,
        observations: vec![
            arp("172.16.0.1", "00:0c:42:41:9a:02"),
            arp("172.16.0.89", "9c:69:d3:6c:38:28"),
            announced(
                "172.16.0.89",
                "Nazars-MacBook-Pro",
                &["_smb._tcp", "_ssh._tcp"],
            ),
            arp("172.16.0.30", "3c:d9:2b:60:88:14"),
            announced("172.16.0.30", "HP-LaserJet-M404", &["_ipp._tcp"]),
            arp("172.16.0.41", "b0:4a:39:71:2c:5e"),
        ],
        gateway: Some(ip("172.16.0.1")),
        self_ip: Some("172.16.0.89".parse().unwrap()),
        sources: vec![
            source(DiscoveryMethod::ArpCache, 4, 0, 0),
            source(DiscoveryMethod::Mdns, 3, 2, 3),
            source(DiscoveryMethod::Ssdp, 0, 0, 0),
        ],
    }
}

fn hotspot() -> Spec {
    Spec {
        routes: vec![
            route("default", Some("172.20.10.1"), "en5"),
            route("172.20.10", None, "en5"),
        ],
        interfaces: vec![physical("en5", "172.20.10.3", 28, "9e:88:1a:44:0c:71")],
        ports: vec![HardwarePort {
            label: "iPhone USB".into(),
            device: "en5".into(),
        }],
        dns: vec![ip("172.20.10.1")],
        wifi: WifiStatus::RadioOff,
        observations: vec![
            arp("172.20.10.1", "f0:99:bf:31:c4:7a"),
            arp("172.20.10.3", "9e:88:1a:44:0c:71"),
            announced("172.20.10.3", "Nazars-MacBook-Pro", &["_smb._tcp"]),
        ],
        gateway: Some(ip("172.20.10.1")),
        self_ip: Some("172.20.10.3".parse().unwrap()),
        sources: vec![
            source(DiscoveryMethod::ArpCache, 2, 0, 0),
            source(DiscoveryMethod::Mdns, 1, 1, 1),
            source(DiscoveryMethod::Ssdp, 0, 0, 0),
        ],
    }
}

fn vpn() -> Spec {
    let mut spec = home_wifi();
    spec.routes = vec![
        route("default", Some("10.8.0.1"), "utun6"),
        route("192.168.1", None, "en0"),
        route("198.51.100.7", Some("192.168.1.1"), "en0"),
    ];
    spec.interfaces.insert(0, tunnel_iface("utun6", "10.8.0.2"));
    spec
}

fn isolated_network() -> Spec {
    Spec {
        routes: vec![
            route("default", Some("10.7.0.1"), "en0"),
            route("10.7", None, "en0"),
        ],
        interfaces: vec![physical("en0", "10.7.3.44", 20, "a4:83:e7:2c:11:80")],
        ports: vec![wifi_port()],
        dns: vec![ip("10.7.0.1")],
        wifi: wifi("Cafe-Guest", Band::Ghz2_4, 6, -71),
        observations: vec![
            arp("10.7.0.1", "00:0c:42:41:9a:02"),
            arp("10.7.3.44", "a4:83:e7:2c:11:80"),
        ],
        gateway: Some(ip("10.7.0.1")),
        self_ip: Some("10.7.3.44".parse().unwrap()),
        sources: vec![
            source(DiscoveryMethod::ArpCache, 2, 0, 0),
            source(DiscoveryMethod::Mdns, 0, 0, 0),
            source(DiscoveryMethod::Ssdp, 0, 0, 0),
        ],
    }
}

fn stress_500() -> Spec {
    let mut spec = university_wifi();
    const REGISTERED: [&str; 4] = ["48:45:20", "b0:4a:39", "1c:90:ff", "14:b5:cd"];
    for i in 128..500 {
        let address = format!("10.20.{}.{}", 8 + i / 200, i % 200 + 1);
        let mac = if i % 4 == 0 {
            format!(
                "{}:{:02x}:{:02x}:{:02x}",
                REGISTERED[(i / 4) % REGISTERED.len()],
                i / 256,
                i % 256,
                (i * 3) % 256
            )
        } else {
            format!("9e:cc:dd:{:02x}:{:02x}:02", i / 256, i % 256)
        };
        spec.observations.push(arp(&address, &mac));
    }
    spec.sources[0] = source(DiscoveryMethod::ArpCache, 505, 0, 0);
    spec
}

fn permission_limited() -> Spec {
    let mut spec = home_wifi();
    // Connected, but macOS withholds the name without Location Services.
    spec.wifi = WifiStatus::PermissionRequired;
    spec
}

/// A plausible activity snapshot, built by the real session accounting.
///
/// Two observations are fed through `ActivitySession` exactly as live samples
/// are, so what the preview renders is what the production model produces —
/// including the distinction between what JRX watched and what the interface
/// has carried all along.
pub fn activity(fixture: Fixture) -> jrx_core::activity::ActivitySnapshot {
    use jrx_core::activity::session::ActivitySession;
    use jrx_core::activity::{ActivityHealth, CounterSample, Protocol, SocketObservation};

    let tick = std::time::Duration::from_secs(1);
    let identity = fixture.identity();
    let mut session = ActivitySession::new(&identity.interface);

    session.observe_counters(
        CounterSample {
            rx_bytes: 6_285_700_000,
            tx_bytes: 1_342_800_000,
        },
        tick,
    );
    session.observe_counters(
        CounterSample {
            rx_bytes: 6_285_742_000,
            tx_bytes: 1_342_834_000,
        },
        tick,
    );

    let socket = |pid, name: &str, path: Option<&str>, port, remote: &str, bin, bout, proto| {
        SocketObservation {
            protocol: proto,
            local_address: identity
                .local_ip
                .map_or_else(|| "192.168.1.14".parse().unwrap(), std::net::IpAddr::V4),
            local_port: port,
            remote_address: Some(remote.parse().unwrap()),
            remote_port: Some(443),
            state: Some("Established".into()),
            rtt_ms: Some(28.0),
            bytes_in: bin,
            bytes_out: bout,
            pid,
            reported_name: name.into(),
            executable_path: path.map(str::to_owned),
        }
    };

    const CHATGPT: &str = "/Applications/ChatGPT.app/Contents/Frameworks/Codex Framework.framework/Versions/151.0/Helpers/Codex (Service).app/Contents/MacOS/Codex (Service)";
    const CLAUDE: &str = "/Applications/Claude.app/Contents/MacOS/Claude";
    const WEBKIT: &str = "/System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.Networking.xpc/Contents/MacOS/com.apple.WebKit.Networking";

    let first = vec![
        socket(
            993,
            "Codex (Service)",
            Some(CHATGPT),
            52001,
            "104.18.32.1",
            0,
            0,
            Protocol::Tcp,
        ),
        socket(
            993,
            "Codex (Service)",
            Some(CHATGPT),
            52002,
            "104.18.32.9",
            0,
            0,
            Protocol::Tcp,
        ),
        socket(
            842,
            "com.apple.WebKi",
            Some(WEBKIT),
            52010,
            "17.248.150.10",
            0,
            0,
            Protocol::Tcp,
        ),
        socket(
            701,
            "Claude",
            Some(CLAUDE),
            52020,
            "160.79.104.10",
            0,
            0,
            Protocol::Tcp,
        ),
        socket(
            701,
            "Claude",
            Some(CLAUDE),
            52021,
            "160.79.104.11",
            0,
            0,
            Protocol::Udp,
        ),
        socket(
            686,
            "Spotify",
            Some("/Applications/Spotify.app/Contents/MacOS/Spotify"),
            52030,
            "35.186.224.25",
            0,
            0,
            Protocol::Tcp,
        ),
        socket(
            658,
            "rapportd",
            Some("/usr/libexec/rapportd"),
            52040,
            "17.57.144.12",
            0,
            0,
            Protocol::Tcp,
        ),
    ];
    session.observe_sockets(first, tick);

    let second = vec![
        socket(
            993,
            "Codex (Service)",
            Some(CHATGPT),
            52001,
            "104.18.32.1",
            18_300_000,
            1_100_000,
            Protocol::Tcp,
        ),
        socket(
            993,
            "Codex (Service)",
            Some(CHATGPT),
            52002,
            "104.18.32.9",
            2_400_000,
            180_000,
            Protocol::Tcp,
        ),
        socket(
            842,
            "com.apple.WebKi",
            Some(WEBKIT),
            52010,
            "17.248.150.10",
            4_100_000,
            260_000,
            Protocol::Tcp,
        ),
        socket(
            701,
            "Claude",
            Some(CLAUDE),
            52020,
            "160.79.104.10",
            210_000,
            10_500_000,
            Protocol::Tcp,
        ),
        socket(
            701,
            "Claude",
            Some(CLAUDE),
            52021,
            "160.79.104.11",
            646_000,
            2_751_000,
            Protocol::Udp,
        ),
        socket(
            686,
            "Spotify",
            Some("/Applications/Spotify.app/Contents/MacOS/Spotify"),
            52030,
            "35.186.224.25",
            9_800_000,
            40_000,
            Protocol::Tcp,
        ),
        socket(
            658,
            "rapportd",
            Some("/usr/libexec/rapportd"),
            52040,
            "17.57.144.12",
            1_200,
            900,
            Protocol::Tcp,
        ),
    ];
    session.observe_sockets(second, tick);

    session.set_health(match fixture {
        Fixture::IsolatedNetwork => ActivityHealth::Limited {
            reason: "nettop did not respond within 10s".into(),
        },
        Fixture::Hotspot => ActivityHealth::Initializing,
        _ => ActivityHealth::Full,
    });

    session.snapshot(tick)
}

/// The permission matrix for a fixture, built by the real capability model.
pub fn capabilities(fixture: Fixture) -> jrx_core::capability::CapabilityMatrix {
    jrx_core::capability::CapabilityMatrix::build(
        crate::registry::ALL_PROBES,
        jrx_core::declaration::Platform::MacOs,
        &fixture.permissions(),
    )
}
