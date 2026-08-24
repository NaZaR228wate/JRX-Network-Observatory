//! The single place every probe declares itself.
//!
//! These declarations are the input to the Visibility Panel
//! (ARCHITECTURE.md §9) and the subject of the privacy invariants below.
//! Adding a probe means adding it here; the tests will not let a probe read
//! something it did not declare.

use jrx_core::data_class::DataClass;
use jrx_core::declaration::{Permission, Platform, Posture, ProbeDeclaration, ProbeId};

pub use crate::probe::{Probe, ProbeCtx, ProbeError, ProbeFuture};

const DESKTOP: &[Platform] = &[Platform::MacOs, Platform::Windows];

/// Every probe JRX ships. Implementations land in M1-M3; the contract is
/// fixed here first so the capability model and its audits exist from day one.
pub static ALL_PROBES: &[ProbeDeclaration] = &[
    ProbeDeclaration {
        id: ProbeId::Interfaces,
        posture: Posture::Passive,
        describes: "This device's network interfaces",
        mechanism: "Operating system interface list",
        requires: &[],
        reads: &[DataClass::InterfaceMetadata],
        platforms: DESKTOP,
    },
    ProbeDeclaration {
        id: ProbeId::Routes,
        posture: Posture::Passive,
        describes: "Your router and how traffic leaves this device",
        mechanism: "Operating system routing table",
        requires: &[],
        reads: &[DataClass::RouteTable],
        platforms: DESKTOP,
    },
    ProbeDeclaration {
        id: ProbeId::Wifi,
        posture: Posture::Passive,
        describes: "Network name, band and signal strength",
        mechanism: "CoreWLAN on macOS, WlanAPI on Windows",
        // macOS requires Location Services merely to read an SSID; Windows
        // does not. The matrix resolves this per platform at runtime.
        requires: &[Permission::LocationServices],
        reads: &[DataClass::WifiAssociation],
        platforms: DESKTOP,
    },
    ProbeDeclaration {
        id: ProbeId::Arp,
        posture: Posture::Passive,
        describes: "Devices already known to this machine",
        mechanism: "ARP/NDP neighbour cache — nothing is sent",
        requires: &[],
        reads: &[DataClass::NeighborTable],
        platforms: DESKTOP,
    },
    ProbeDeclaration {
        id: ProbeId::IfCounters,
        posture: Posture::Passive,
        describes: "How much data is moving, right now",
        mechanism: "Per-interface byte counters",
        requires: &[],
        reads: &[DataClass::InterfaceCounters],
        platforms: DESKTOP,
    },
    ProbeDeclaration {
        id: ProbeId::Sockets,
        posture: Posture::Passive,
        describes: "Which services this device is connected to",
        mechanism: "Operating system socket table",
        requires: &[],
        reads: &[DataClass::SocketTable],
        platforms: DESKTOP,
    },
    ProbeDeclaration {
        id: ProbeId::Mdns,
        posture: Posture::Passive,
        describes: "Device names and the services they offer",
        mechanism: "mDNS service discovery",
        requires: &[Permission::LocalNetwork],
        reads: &[DataClass::ServiceAdvertisement],
        platforms: DESKTOP,
    },
    ProbeDeclaration {
        id: ProbeId::Ssdp,
        posture: Posture::Passive,
        describes: "Routers, media devices and printers",
        mechanism: "SSDP/UPnP discovery",
        requires: &[Permission::LocalNetwork],
        reads: &[DataClass::ServiceAdvertisement],
        platforms: DESKTOP,
    },
    ProbeDeclaration {
        id: ProbeId::IcmpSweep,
        posture: Posture::Active,
        describes: "Devices that stay silent until asked",
        mechanism: "ICMP echo to the local subnet — you approve each run",
        requires: &[Permission::LocalNetwork],
        reads: &[DataClass::HostLiveness],
        platforms: DESKTOP,
    },
];

/// Look up a declaration by id.
pub fn lookup(id: ProbeId) -> Option<&'static ProbeDeclaration> {
    ALL_PROBES.iter().find(|d| d.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jrx_core::declaration::Posture;

    /// The invariant this whole architecture exists to protect
    /// (TECH_DECISIONS.md ADR-002, ARCHITECTURE.md §6.1).
    #[test]
    fn no_probe_declares_a_refused_data_class() {
        for decl in ALL_PROBES {
            for class in decl.reads {
                assert!(
                    !class.is_refused_by_design(),
                    "probe {:?} declares refused class {:?}",
                    decl.id,
                    class,
                );
            }
        }
    }

    /// ARCHITECTURE.md §6.4: exactly one probe emits onto the network, and it
    /// never runs automatically.
    #[test]
    fn icmp_sweep_is_the_only_active_probe() {
        let active: Vec<_> = ALL_PROBES
            .iter()
            .filter(|d| d.posture == Posture::Active)
            .map(|d| d.id)
            .collect();
        assert_eq!(active, vec![ProbeId::IcmpSweep]);
    }

    #[test]
    fn probe_ids_are_unique() {
        let mut seen = Vec::new();
        for decl in ALL_PROBES {
            assert!(!seen.contains(&decl.id), "duplicate probe id {:?}", decl.id);
            seen.push(decl.id);
        }
    }

    #[test]
    fn every_probe_declares_what_it_reads() {
        for decl in ALL_PROBES {
            assert!(
                !decl.reads.is_empty(),
                "probe {:?} declares no reads",
                decl.id
            );
            assert!(
                !decl.describes.is_empty(),
                "probe {:?} has no description",
                decl.id
            );
            assert!(
                !decl.mechanism.is_empty(),
                "probe {:?} names no mechanism",
                decl.id
            );
            assert!(
                !decl.platforms.is_empty(),
                "probe {:?} supports no platform",
                decl.id
            );
        }
    }

    /// A probe must be reachable through the trait, and the trait must expose
    /// exactly the declaration the registry audits — not a second copy that
    /// could drift from it.
    #[test]
    fn trait_object_exposes_the_registry_declaration() {
        struct FakeArp;
        impl Probe for FakeArp {
            fn declaration(&self) -> &'static ProbeDeclaration {
                lookup(ProbeId::Arp).expect("arp is registered")
            }
            fn run<'a>(&'a self, _ctx: &'a ProbeCtx) -> ProbeFuture<'a> {
                Box::pin(async { Ok(Vec::new()) })
            }
        }

        let probe: &dyn Probe = &FakeArp;
        assert_eq!(probe.declaration().id, ProbeId::Arp);
        assert!(!probe.declaration().reads.is_empty());
    }
}
