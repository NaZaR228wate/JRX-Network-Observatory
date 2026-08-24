//! Passive device discovery.
//!
//! All three sources are passive in the sense that matters: the ARP cache is
//! read from the OS with nothing sent, and mDNS and SSDP emit only the
//! standard multicast queries every Mac, phone and TV on the network already
//! sends continuously (ARCHITECTURE.md §6.3).
//!
//! No subnet sweep runs here. That is the opt-in active probe, and it is
//! deliberately not part of passive discovery (TECH_DECISIONS.md ADR-009).

pub mod mdns;
pub mod quality;
pub mod ssdp;

use std::net::IpAddr;
use std::time::{Duration, Instant};

use jrx_core::device::{Device, DeviceTable, DiscoveryMethod, Observation};
use jrx_core::network::NetworkIdentity;
use jrx_core::topology::{DiscoverySummary, Topology};
use serde::Serialize;

use crate::oui;
use crate::probe::ProbeError;

/// How long the multicast sources are given to answer.
///
/// Devices spread their replies deliberately, so a short window silently
/// under-reports. Eight seconds matches the enrichment phase in
/// ARCHITECTURE.md §7.1.
const MULTICAST_WINDOW: Duration = Duration::from_secs(3);

/// What one discovery source produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SourceStatus {
    /// Ran, and found this many observations.
    Ok { observations: usize },
    /// Failed. Reported as a fault in JRX, never as an absence of devices.
    Failed { reason: String },
}

/// Everything discovery produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscoveryReport {
    pub devices: Vec<Device>,
    pub topology: Topology,
    pub summary: DiscoverySummary,
    /// How much to trust this run, and why. An empty device list means
    /// something different depending on whether our sources worked
    /// (ARCHITECTURE.md §12).
    pub quality: quality::DiscoveryQuality,
    pub took_ms: u64,
}

/// Discover devices on the current network.
///
/// Sources run concurrently: the ARP read returns in milliseconds while the
/// multicast sources are still listening, so the map populates immediately and
/// then enriches.
pub fn observe(identity: &NetworkIdentity) -> Result<DiscoveryReport, ProbeError> {
    let started = Instant::now();

    let (arp_result, mdns_result, ssdp_result) = std::thread::scope(|scope| {
        let arp = scope.spawn(read_arp);
        let mdns = scope.spawn(|| mdns::discover(MULTICAST_WINDOW));
        let ssdp = scope.spawn(|| match identity.local_ip {
            Some(local) => ssdp::discover(local, MULTICAST_WINDOW),
            None => Err(ProbeError::Failed(
                "no local address on the interface carrying the default route".into(),
            )),
        });
        (arp.join(), mdns.join(), ssdp.join())
    });

    let panicked = || ProbeError::Failed("discovery thread panicked".into());
    let arp_result = arp_result.map_err(|_| panicked())?;
    let mdns_result = mdns_result.map_err(|_| panicked())?;
    let ssdp_result = ssdp_result.map_err(|_| panicked())?;

    let mut table = DeviceTable::new();
    let mut sources = Vec::new();

    // --- ARP: devices the OS already knew about ---
    sources.push(quality::SourceQuality {
        method: DiscoveryMethod::ArpCache,
        label: DiscoveryMethod::ArpCache.label(),
        status: status_of(arp_result.as_ref().map(Vec::len)),
        observations: arp_result.as_ref().map_or(0, Vec::len),
        names_resolved: 0,
        services_seen: 0,
    });
    if let Ok(entries) = &arp_result {
        for entry in entries {
            if !belongs_to_this_network(identity, entry.address) {
                continue;
            }
            table.observe(
                Observation::new(entry.address, DiscoveryMethod::ArpCache).with_mac(entry.mac),
            );
        }
    }

    // --- mDNS: names and services ---
    // Counted distinctly: "20 observations" is far less informative than
    // "6 names and 9 service types", which is what tells a user whether mDNS
    // is actually producing identity rather than just noise.
    let (names_resolved, services_seen) = mdns_result.as_ref().map_or((0, 0), |found| {
        let mut names: Vec<&str> = Vec::new();
        let mut types: Vec<&str> = Vec::new();
        for service in found {
            if let Some(host) = service.hostname.as_deref()
                && !names.contains(&host)
            {
                names.push(host);
            }
            if !types.contains(&service.service_type.as_str()) {
                types.push(&service.service_type);
            }
        }
        (names.len(), types.len())
    });
    sources.push(quality::SourceQuality {
        method: DiscoveryMethod::Mdns,
        label: DiscoveryMethod::Mdns.label(),
        status: status_of(mdns_result.as_ref().map(Vec::len)),
        observations: mdns_result.as_ref().map_or(0, Vec::len),
        names_resolved,
        services_seen,
    });
    if let Ok(services) = &mdns_result {
        for service in services {
            if !belongs_to_this_network(identity, service.address) {
                continue;
            }
            table.observe(
                Observation::new(service.address, DiscoveryMethod::Mdns)
                    .with_hostname(service.hostname.clone())
                    .with_service(service.service_type.clone()),
            );
        }
    }

    // --- SSDP: routers, media devices, printers ---
    sources.push(quality::SourceQuality {
        method: DiscoveryMethod::Ssdp,
        label: DiscoveryMethod::Ssdp.label(),
        status: status_of(ssdp_result.as_ref().map(Vec::len)),
        observations: ssdp_result.as_ref().map_or(0, Vec::len),
        names_resolved: 0,
        services_seen: 0,
    });
    if let Ok(responses) = &ssdp_result {
        for response in responses {
            let Some(address) = response.address.or_else(|| response.location_host()) else {
                continue;
            };
            if !belongs_to_this_network(identity, address) {
                continue;
            }
            let mut observation = Observation::new(address, DiscoveryMethod::Ssdp);
            if let Some(urn) = &response.device_type {
                observation = observation.with_upnp_type(urn.clone());
            }
            table.observe(observation);
        }
    }

    // The router and this machine always belong on the map, even if neither
    // announced itself.
    if let Some(gateway) = identity.gateway {
        table.mark_gateway(gateway);
    }
    if let Some(local) = identity.local_ip {
        table.mark_self(IpAddr::V4(local));
    }

    let devices = table.finish(oui::vendor_of);
    let topology = Topology::build(&devices);
    let summary = DiscoverySummary::of(&devices);

    // Devices other than ourselves and the router: this machine appearing on
    // its own map is not evidence that discovery worked.
    let others = devices
        .iter()
        .filter(|d| !d.is_self && !d.is_gateway)
        .count();
    let quality = quality::assess(&sources, others);

    Ok(DiscoveryReport {
        devices,
        topology,
        summary,
        quality,
        took_ms: started.elapsed().as_millis() as u64,
    })
}

/// Keep discovery to the network the user is actually on.
///
/// Link-local addresses belong to devices that failed DHCP, and anything
/// outside the subnet is not part of the local picture. Without this the list
/// fills with entries the user has no way to recognise.
fn belongs_to_this_network(identity: &NetworkIdentity, address: IpAddr) -> bool {
    let IpAddr::V4(v4) = address else {
        return false; // IPv6 neighbours land in a later milestone
    };
    identity.subnet.is_some_and(|subnet| subnet.contains(v4))
}

fn status_of(result: Result<usize, &ProbeError>) -> SourceStatus {
    match result {
        Ok(observations) => SourceStatus::Ok { observations },
        Err(e) => SourceStatus::Failed {
            reason: e.to_string(),
        },
    }
}

#[cfg(target_os = "macos")]
fn read_arp() -> Result<Vec<crate::macos::arp::ArpEntry>, ProbeError> {
    Ok(crate::macos::arp::parse_arp(
        &crate::macos::exec::arp_table()?,
    ))
}

#[cfg(not(target_os = "macos"))]
fn read_arp() -> Result<Vec<ArpStub>, ProbeError> {
    Err(ProbeError::Unsupported)
}

#[cfg(not(target_os = "macos"))]
pub struct ArpStub {
    pub address: IpAddr,
    pub mac: Option<jrx_core::device::MacAddress>,
}
