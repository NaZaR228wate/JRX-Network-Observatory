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
use jrx_core::topology::{DiscoverySummary, Topology, TopologyOverview};
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
    /// The operating system refused the request before it reached the
    /// network. Distinct from a failure, because it says something specific:
    /// JRX is not permitted to do this here.
    Refused { reason: String },
}

/// Everything discovery produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscoveryReport {
    pub devices: Vec<Device>,
    /// Level 1: the whole network at a glance, bounded regardless of size.
    pub overview: TopologyOverview,
    pub topology: Topology,
    pub summary: DiscoverySummary,
    /// How much to trust this run, and why. An empty device list means
    /// something different depending on whether our sources worked
    /// (ARCHITECTURE.md §12).
    pub quality: quality::DiscoveryQuality,
    pub took_ms: u64,
}

/// A step in a discovery run, reported as it happens.
///
/// The multicast sources listen for three seconds. Showing nothing for three
/// seconds would be a blank loading screen, so the fast source reports first
/// and the map fills in behind it (ARCHITECTURE.md §7.1).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum DiscoveryStage {
    /// Listening has begun.
    Started,
    /// One source finished. Reported individually so progress is honest:
    /// the total amount of work is unknown, so there is no percentage to show.
    SourceFinished { source: quality::SourceQuality },
    /// What is known so far. Emitted after the neighbour cache is read, which
    /// takes milliseconds.
    ///
    /// Boxed: this variant carries a whole overview and device list, and is
    /// sent at most once per run, so the other variants should not pay for its
    /// size.
    Partial(Box<PartialDiscovery>),
}

/// A snapshot of what discovery knows part-way through a run.
#[derive(Debug, Clone, Serialize)]
pub struct PartialDiscovery {
    pub overview: TopologyOverview,
    pub devices: Vec<Device>,
}

/// Discover devices on the current network.
pub fn observe(identity: &NetworkIdentity) -> Result<DiscoveryReport, ProbeError> {
    observe_with_progress(identity, &|_| {})
}

/// Discover devices, reporting each stage as it completes.
///
/// Sources run concurrently and are joined in order of expected speed: the ARP
/// read returns in milliseconds while the multicast sources are still
/// listening, so the map populates immediately and then enriches.
pub fn observe_with_progress(
    identity: &NetworkIdentity,
    on_stage: &(dyn Fn(DiscoveryStage) + Sync),
) -> Result<DiscoveryReport, ProbeError> {
    let started = Instant::now();

    on_stage(DiscoveryStage::Started);
    let panicked = || ProbeError::Failed("discovery thread panicked".into());

    let (arp_result, mdns_result, ssdp_result) = std::thread::scope(|scope| {
        let arp = scope.spawn(read_arp);
        let mdns = scope.spawn(|| mdns::discover(MULTICAST_WINDOW));
        let ssdp = scope.spawn(|| match identity.local_ip {
            Some(local) => ssdp::discover(local, MULTICAST_WINDOW),
            None => Err(ProbeError::Failed(
                "no local address on the interface carrying the default route".into(),
            )),
        });

        // Joined in order of expected speed, not spawn order. The neighbour
        // cache is ready almost immediately; waiting for the multicast window
        // before showing it would waste the only fast source we have.
        let arp_result = arp.join();
        if let Ok(entries) = &arp_result {
            on_stage(DiscoveryStage::SourceFinished {
                source: arp_quality(entries.as_ref().map(Vec::len)),
            });
            if let Ok(entries) = entries {
                let devices = partial_devices(identity, entries);
                on_stage(DiscoveryStage::Partial(Box::new(PartialDiscovery {
                    overview: TopologyOverview::build(&devices),
                    devices,
                })));
            }
        }

        (arp_result, mdns.join(), ssdp.join())
    });

    let arp_result = arp_result.map_err(|_| panicked())?;
    let mdns_result = mdns_result.map_err(|_| panicked())?;
    let ssdp_result = ssdp_result.map_err(|_| panicked())?;

    let mut table = DeviceTable::new();
    let mut sources = Vec::new();

    // --- ARP: devices the OS already knew about ---
    sources.push(arp_quality(arp_result.as_ref().map(Vec::len)));
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
        name_this_computer(&mut table, local);
        table.mark_self(IpAddr::V4(local));
    }

    let devices = table.finish(oui::vendor_of);
    let overview = TopologyOverview::build(&devices);
    let topology = Topology::build(&devices);
    let summary = DiscoverySummary::of(&devices, identity.subnet);

    // Devices other than ourselves and the router: this machine appearing on
    // its own map is not evidence that discovery worked.
    let others = devices
        .iter()
        .filter(|d| !d.is_self && !d.is_gateway)
        .count();
    let quality = quality::assess(&sources, others);

    Ok(DiscoveryReport {
        devices,
        overview,
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

fn arp_quality(result: Result<usize, &ProbeError>) -> quality::SourceQuality {
    quality::SourceQuality {
        method: DiscoveryMethod::ArpCache,
        label: DiscoveryMethod::ArpCache.label(),
        status: status_of(result),
        observations: result.unwrap_or(0),
        names_resolved: 0,
        services_seen: 0,
    }
}

/// The devices knowable from the neighbour cache alone.
///
/// Deliberately excludes the gateway and self marks: those are added once, at
/// the end, so a device does not change identity between the partial and final
/// pictures.
#[cfg(target_os = "macos")]
fn partial_devices(
    identity: &NetworkIdentity,
    entries: &[crate::macos::arp::ArpEntry],
) -> Vec<Device> {
    let mut table = DeviceTable::new();
    for entry in entries {
        if belongs_to_this_network(identity, entry.address) {
            table.observe(
                Observation::new(entry.address, DiscoveryMethod::ArpCache).with_mac(entry.mac),
            );
        }
    }
    if let Some(gateway) = identity.gateway {
        table.mark_gateway(gateway);
    }
    if let Some(local) = identity.local_ip {
        name_this_computer(&mut table, local);
        table.mark_self(IpAddr::V4(local));
    }
    table.finish(oui::vendor_of)
}

/// Record this computer's own name, read from the OS.
///
/// Without it, a Mac whose traffic runs over a USB Ethernet adapter is
/// displayed as an "ASIX Electronics device" until it happens to announce
/// itself over mDNS — which on a network that suppresses announcements is
/// never.
#[cfg(target_os = "macos")]
fn name_this_computer(table: &mut DeviceTable, local: std::net::Ipv4Addr) {
    if let Ok(name) = crate::macos::exec::computer_name()
        && !name.is_empty()
    {
        table.observe(
            Observation::new(IpAddr::V4(local), DiscoveryMethod::SelfInterface)
                .with_hostname(Some(name)),
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn name_this_computer(_: &mut DeviceTable, _: std::net::Ipv4Addr) {}

#[cfg(not(target_os = "macos"))]
fn partial_devices(_: &NetworkIdentity, _: &[ArpStub]) -> Vec<Device> {
    Vec::new()
}

fn status_of(result: Result<usize, &ProbeError>) -> SourceStatus {
    match result {
        Ok(observations) => SourceStatus::Ok { observations },
        Err(ProbeError::Refused(reason)) => SourceStatus::Refused {
            reason: reason.clone(),
        },
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
