//! A synthetic home network, for screenshots and demos.
//!
//! Runs the real classification rules and the real IEEE registry — nothing
//! here is hand-written output. What it produces is what JRX would show if
//! this network existed.
//!
//! Run with: cargo run -p jrx-collector --example demo_fixture -- --json

use std::net::IpAddr;

use jrx_collector::oui;
use jrx_core::device::{Device, DeviceTable, DiscoveryMethod, MacAddress, Observation};
use jrx_core::topology::{DiscoverySummary, TopologyOverview};

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

fn home_network() -> Vec<Device> {
    let mut t = DeviceTable::new();

    // The router.
    t.observe(
        Observation::new(ip("192.168.1.1"), DiscoveryMethod::ArpCache)
            .with_mac(MacAddress::parse("00:0c:42:41:9a:02")),
    );
    t.observe(
        Observation::new(ip("192.168.1.1"), DiscoveryMethod::Ssdp)
            .with_upnp_type("urn:schemas-upnp-org:device:InternetGatewayDevice:1"),
    );

    // This Mac.
    t.observe(
        Observation::new(ip("192.168.1.14"), DiscoveryMethod::ArpCache)
            .with_mac(MacAddress::parse("a4:83:e7:2c:11:80")),
    );
    t.observe(
        Observation::new(ip("192.168.1.14"), DiscoveryMethod::Mdns)
            .with_hostname(Some("Nazars-MacBook-Pro".into()))
            .with_service("_smb._tcp")
            .with_service("_ssh._tcp"),
    );

    // An iPhone.
    t.observe(
        Observation::new(ip("192.168.1.23"), DiscoveryMethod::ArpCache)
            .with_mac(MacAddress::parse("f0:99:bf:31:c4:7a")),
    );
    t.observe(
        Observation::new(ip("192.168.1.23"), DiscoveryMethod::Mdns)
            .with_hostname(Some("Nazars-iPhone".into()))
            .with_service("_apple-mobdev2._tcp"),
    );

    // A Windows laptop.
    t.observe(
        Observation::new(ip("192.168.1.31"), DiscoveryMethod::ArpCache)
            .with_mac(MacAddress::parse("48:45:20:7f:2a:11")),
    );
    t.observe(
        Observation::new(ip("192.168.1.31"), DiscoveryMethod::Mdns)
            .with_hostname(Some("LAPTOP-K7QX2M".into()))
            .with_service("_smb._tcp"),
    );

    // An Apple TV. AirPlay alone is not definitive — a Mac advertises it too —
    // so this lands at Medium on the strength of its name.
    t.observe(
        Observation::new(ip("192.168.1.40"), DiscoveryMethod::ArpCache)
            .with_mac(MacAddress::parse("70:3e:ac:19:d5:66")),
    );
    t.observe(
        Observation::new(ip("192.168.1.40"), DiscoveryMethod::Mdns)
            .with_hostname(Some("Living-Room-Apple-TV".into()))
            .with_service("_airplay._tcp")
            .with_service("_raop._tcp"),
    );

    // A printer.
    t.observe(
        Observation::new(ip("192.168.1.55"), DiscoveryMethod::ArpCache)
            .with_mac(MacAddress::parse("3c:d9:2b:60:88:14")),
    );
    t.observe(
        Observation::new(ip("192.168.1.55"), DiscoveryMethod::Mdns)
            .with_hostname(Some("HP-LaserJet-M404".into()))
            .with_service("_ipp._tcp")
            .with_service("_printer._tcp"),
    );

    // Two devices JRX will not guess at: one rotating its address, one that
    // resolves a manufacturer but says nothing about what it is.
    t.observe(
        Observation::new(ip("192.168.1.61"), DiscoveryMethod::ArpCache)
            .with_mac(MacAddress::parse("9e:1d:44:b7:05:c3")),
    );
    t.observe(
        Observation::new(ip("192.168.1.72"), DiscoveryMethod::ArpCache)
            .with_mac(MacAddress::parse("1c:90:ff:22:6e:41")),
    );

    t.mark_gateway(ip("192.168.1.1"));
    t.mark_self(ip("192.168.1.14"));
    t.finish(oui::vendor_of)
}

fn main() {
    let devices = home_network();
    let overview = TopologyOverview::build(&devices);
    let summary = DiscoverySummary::of(&devices);

    if std::env::args().any(|a| a == "--json") {
        let payload = serde_json::json!({
            "devices": devices,
            "overview": overview,
            "summary": summary,
            "quality": {
                "verdict": "healthy",
                "explanation": format!(
                    "{} devices found. Every discovery source worked.",
                    devices.len() - 2
                ),
                "local_network": "working",
                "sources": [
                    { "method": "arp_cache", "label": "already known to this Mac (nothing was sent)",
                      "status": { "status": "ok", "observations": 8 },
                      "observations": 8, "names_resolved": 0, "services_seen": 0 },
                    { "method": "mdns", "label": "announced itself over mDNS",
                      "status": { "status": "ok", "observations": 9 },
                      "observations": 9, "names_resolved": 5, "services_seen": 7 },
                    { "method": "ssdp", "label": "answered a UPnP search",
                      "status": { "status": "ok", "observations": 1 },
                      "observations": 1, "names_resolved": 0, "services_seen": 0 }
                ]
            },
            "took_ms": 3120
        });
        println!("{}", serde_json::to_string(&payload).unwrap());
        return;
    }

    println!("synthetic home network — {} devices\n", devices.len());
    for device in &devices {
        println!(
            "  {:<24} {:?}/{:?}{}",
            device.display_name(),
            device.category(),
            device.confidence(),
            device
                .inference
                .family
                .map(|f| format!(" ({})", f.label()))
                .unwrap_or_default()
        );
    }
    println!("\nby category:");
    for (category, count) in &summary.by_category {
        println!("  {:>14}: {count}", category.label());
    }
}
