//! The capability matrix — the Visibility Panel's source of truth.
//!
//! Generated from probe declarations plus live permission state, so it cannot
//! drift from what the collector actually does (ARCHITECTURE.md §9).

use crate::data_class::DataClass;
use crate::declaration::{Permission, Platform, ProbeDeclaration, ProbeId};
use serde::{Deserialize, Serialize};

/// Live state of an operating-system permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    Granted,
    Denied,
    /// Never asked for. Treated exactly like `Denied` for capability purposes:
    /// the user cannot see it yet either way.
    NotRequested,
}

/// Observed permission states. Anything absent is `NotRequested`.
#[derive(Debug, Clone, Default)]
pub struct PermissionSet {
    entries: Vec<(Permission, PermissionState)>,
}

impl PermissionSet {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with(mut self, permission: Permission, state: PermissionState) -> Self {
        self.entries.retain(|(p, _)| *p != permission);
        self.entries.push((permission, state));
        self
    }

    pub fn state_of(&self, permission: Permission) -> PermissionState {
        self.entries
            .iter()
            .find(|(p, _)| *p == permission)
            .map(|(_, s)| *s)
            .unwrap_or(PermissionState::NotRequested)
    }
}

/// One of the four columns of the Visibility Panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CapabilityState {
    /// Working now. The mechanism is named so the claim is checkable.
    Observed { mechanism: &'static str },
    /// One permission grant away.
    Available { missing: Permission },
    /// Blocked by the platform at this privilege level. JRX does not ask for
    /// elevation (TECH_DECISIONS.md ADR-002).
    NotPossible { reason: &'static str },
}

/// A capability derived from one probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityRow {
    pub probe: ProbeId,
    pub describes: &'static str,
    pub state: CapabilityState,
}

/// A data class JRX refuses to collect. Always shown, on every platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RefusedRow {
    pub class: DataClass,
}

/// The complete Visibility Panel contents for one platform and permission state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityMatrix {
    rows: Vec<CapabilityRow>,
    refused: Vec<RefusedRow>,
}

impl CapabilityMatrix {
    pub fn build(
        probes: &[ProbeDeclaration],
        platform: Platform,
        permissions: &PermissionSet,
    ) -> Self {
        let rows = probes
            .iter()
            .map(|probe| CapabilityRow {
                probe: probe.id,
                describes: probe.describes,
                state: Self::state_for(probe, platform, permissions),
            })
            .collect();

        // Refused rows are unconditional. They do not depend on platform or
        // permission, because the refusal is a product decision, not a
        // technical limit.
        let refused = DataClass::REFUSED
            .into_iter()
            .map(|class| RefusedRow { class })
            .collect();

        Self { rows, refused }
    }

    fn state_for(
        probe: &ProbeDeclaration,
        platform: Platform,
        permissions: &PermissionSet,
    ) -> CapabilityState {
        if !probe.platforms.contains(&platform) {
            return CapabilityState::NotPossible {
                reason: "Not available on this platform",
            };
        }

        for required in probe.requires {
            if permissions.state_of(*required) != PermissionState::Granted {
                return CapabilityState::Available { missing: *required };
            }
        }

        CapabilityState::Observed {
            mechanism: probe.mechanism,
        }
    }

    pub fn rows(&self) -> impl Iterator<Item = &CapabilityRow> {
        self.rows.iter()
    }

    pub fn refused(&self) -> impl Iterator<Item = &RefusedRow> {
        self.refused.iter()
    }

    pub fn row(&self, probe: ProbeId) -> Option<&CapabilityRow> {
        self.rows.iter().find(|r| r.probe == probe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_class::DataClass;
    use crate::declaration::{Permission, Platform, Posture, ProbeDeclaration, ProbeId};

    fn mdns() -> ProbeDeclaration {
        ProbeDeclaration {
            id: ProbeId::Mdns,
            posture: Posture::Passive,
            describes: "Device names and services",
            mechanism: "mDNS service discovery",
            requires: &[Permission::LocalNetwork],
            reads: &[DataClass::ServiceAdvertisement],
            platforms: &[Platform::MacOs, Platform::Windows],
        }
    }

    fn arp() -> ProbeDeclaration {
        ProbeDeclaration {
            id: ProbeId::Arp,
            posture: Posture::Passive,
            describes: "Devices already known to this machine",
            mechanism: "ARP/NDP neighbour cache",
            requires: &[],
            reads: &[DataClass::NeighborTable],
            platforms: &[Platform::MacOs, Platform::Windows],
        }
    }

    #[test]
    fn probe_with_granted_permission_is_observed() {
        let perms = PermissionSet::new().with(Permission::LocalNetwork, PermissionState::Granted);
        let matrix = CapabilityMatrix::build(&[mdns()], Platform::MacOs, &perms);

        let row = matrix.row(ProbeId::Mdns).expect("mdns row present");
        assert_eq!(
            row.state,
            CapabilityState::Observed {
                mechanism: "mDNS service discovery"
            }
        );
    }

    #[test]
    fn probe_needing_no_permission_is_observed() {
        let matrix = CapabilityMatrix::build(&[arp()], Platform::MacOs, &PermissionSet::new());

        let row = matrix.row(ProbeId::Arp).expect("arp row present");
        assert!(matches!(row.state, CapabilityState::Observed { .. }));
    }

    #[test]
    fn denied_permission_makes_capability_available_not_observed() {
        let perms = PermissionSet::new().with(Permission::LocalNetwork, PermissionState::Denied);
        let matrix = CapabilityMatrix::build(&[mdns()], Platform::MacOs, &perms);

        let row = matrix.row(ProbeId::Mdns).expect("mdns row present");
        assert_eq!(
            row.state,
            CapabilityState::Available {
                missing: Permission::LocalNetwork
            }
        );
    }

    #[test]
    fn unrequested_permission_makes_capability_available() {
        let matrix = CapabilityMatrix::build(&[mdns()], Platform::MacOs, &PermissionSet::new());

        let row = matrix.row(ProbeId::Mdns).expect("mdns row present");
        assert_eq!(
            row.state,
            CapabilityState::Available {
                missing: Permission::LocalNetwork
            }
        );
    }

    #[test]
    fn probe_unsupported_on_platform_is_not_possible() {
        let ios_blind = ProbeDeclaration {
            platforms: &[Platform::Windows],
            ..arp()
        };
        let matrix = CapabilityMatrix::build(&[ios_blind], Platform::MacOs, &PermissionSet::new());

        let row = matrix.row(ProbeId::Arp).expect("arp row present");
        assert!(matches!(row.state, CapabilityState::NotPossible { .. }));
    }

    #[test]
    fn matrix_always_lists_every_refused_class() {
        let matrix = CapabilityMatrix::build(&[], Platform::MacOs, &PermissionSet::new());

        let refused: Vec<DataClass> = matrix.refused().map(|r| r.class).collect();
        for class in DataClass::REFUSED {
            assert!(
                refused.contains(&class),
                "{class:?} missing from refused rows"
            );
        }
    }

    #[test]
    fn refused_rows_are_present_even_when_no_probe_is_observed() {
        let matrix = CapabilityMatrix::build(&[], Platform::MacOs, &PermissionSet::new());
        assert_eq!(matrix.refused().count(), DataClass::REFUSED.len());
        assert_eq!(matrix.rows().count(), 0);
    }
}
