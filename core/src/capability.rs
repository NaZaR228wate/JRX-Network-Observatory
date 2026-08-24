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
    /// The platform provides no way to ask. Not the same as denied — we were
    /// never told anything.
    Unknown,
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

/// How much we actually know about a missing capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Certainty {
    /// The operating system told us.
    Confirmed,
    /// The operating system offers no way to ask, so this is our best
    /// statement rather than a reported fact.
    Unverifiable,
}

/// Row counts for the Visibility Panel header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CapabilitySummary {
    pub observed: usize,
    pub available: usize,
    pub not_possible: usize,
    pub refused: usize,
}

/// One of the four columns of the Visibility Panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CapabilityState {
    /// Working now. The mechanism is named so the claim is checkable.
    Observed { mechanism: &'static str },
    /// One permission grant away. `certainty` says whether the OS told us so.
    Available {
        missing: Permission,
        certainty: Certainty,
    },
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

/// Something JRX cannot do at this privilege level. Not a refusal and not a
/// permission problem — a hard limit of running unprivileged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LimitationRow {
    pub describes: &'static str,
    pub reason: &'static str,
}

/// The capabilities users most often expect and JRX genuinely cannot provide.
///
/// Listing these is the point of the third column: without it the panel
/// silently implies the tool sees everything it wants to.
static LIMITATIONS: &[LimitationRow] = &[
    LimitationRow {
        describes: "Per-app bandwidth — how much data each program uses",
        reason: "Requires administrator access (ETW on Windows, BPF on macOS). \
                 JRX does not ask for administrator access, so this number does \
                 not exist for us to show.",
    },
    LimitationRow {
        describes: "How much data went to each destination",
        reason: "Byte counts per connection need the same elevated access. JRX \
                 can show total throughput and which services you are connected \
                 to, but not how much went to each one.",
    },
    LimitationRow {
        describes: "Devices on a network that isolates its clients",
        reason: "Guest and corporate Wi-Fi often block devices from seeing each \
                 other. That is the network working correctly, and no software \
                 can see through it.",
    },
    LimitationRow {
        describes: "The identity of devices using a randomised MAC address",
        reason: "Modern phones deliberately change their hardware address per \
                 network. JRX reports that a device is protecting its identity \
                 rather than guessing who it is.",
    },
];

/// Everything the UI needs to render a permission and its grant action,
/// sourced from the same definitions the collector uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PermissionInfo {
    pub permission: Permission,
    pub label: &'static str,
    pub grant_hint: &'static str,
    /// False when the platform offers no way to ask for the current state.
    pub queryable: bool,
    pub state: PermissionState,
}

/// A data class JRX refuses to collect. Always shown, on every platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RefusedRow {
    pub class: DataClass,
    pub rationale: &'static str,
}

/// The complete Visibility Panel contents for one platform and permission state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityMatrix {
    rows: Vec<CapabilityRow>,
    refused: Vec<RefusedRow>,
    limitations: &'static [LimitationRow],
    permissions: Vec<PermissionInfo>,
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
            .map(|class| RefusedRow {
                class,
                rationale: class.refusal_rationale(),
            })
            .collect();

        let permissions = Permission::ALL
            .into_iter()
            .map(|permission| PermissionInfo {
                permission,
                label: permission.label(),
                grant_hint: permission.grant_hint(),
                queryable: permission.is_queryable(),
                state: permissions.state_of(permission),
            })
            .collect();

        Self {
            rows,
            refused,
            limitations: LIMITATIONS,
            permissions,
        }
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
            let state = permissions.state_of(*required);
            if state == PermissionState::Granted {
                continue;
            }
            return CapabilityState::Available {
                missing: *required,
                // Only claim the OS refused us when the OS can actually be
                // asked. Otherwise say plainly that we could not verify it.
                certainty: if required.is_queryable() && state != PermissionState::Unknown {
                    Certainty::Confirmed
                } else {
                    Certainty::Unverifiable
                },
            };
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

    pub fn limitations(&self) -> impl Iterator<Item = &'static LimitationRow> {
        LIMITATIONS.iter()
    }

    pub fn permissions(&self) -> impl Iterator<Item = &PermissionInfo> {
        self.permissions.iter()
    }

    pub fn row(&self, probe: ProbeId) -> Option<&CapabilityRow> {
        self.rows.iter().find(|r| r.probe == probe)
    }

    pub fn summary(&self) -> CapabilitySummary {
        let count =
            |f: fn(&CapabilityState) -> bool| self.rows.iter().filter(|r| f(&r.state)).count();
        CapabilitySummary {
            observed: count(|s| matches!(s, CapabilityState::Observed { .. })),
            available: count(|s| matches!(s, CapabilityState::Available { .. })),
            not_possible: count(|s| matches!(s, CapabilityState::NotPossible { .. })),
            refused: self.refused.len(),
        }
    }
}

#[cfg(test)]
mod certainty_tests {
    use super::*;
    use crate::data_class::DataClass;
    use crate::declaration::{Permission, Platform, Posture, ProbeDeclaration, ProbeId};

    fn probe(id: ProbeId, requires: &'static [Permission]) -> ProbeDeclaration {
        ProbeDeclaration {
            id,
            posture: Posture::Passive,
            describes: "something",
            mechanism: "some mechanism",
            requires,
            reads: &[DataClass::NeighborTable],
            platforms: &[Platform::MacOs, Platform::Windows],
        }
    }

    /// macOS can be asked about Location Services, so a denial there is a fact
    /// the OS told us.
    #[test]
    fn queryable_permission_denied_is_confirmed() {
        let perms =
            PermissionSet::new().with(Permission::LocationServices, PermissionState::Denied);
        let matrix = CapabilityMatrix::build(
            &[probe(ProbeId::Wifi, &[Permission::LocationServices])],
            Platform::MacOs,
            &perms,
        );

        assert_eq!(
            matrix.row(ProbeId::Wifi).unwrap().state,
            CapabilityState::Available {
                missing: Permission::LocationServices,
                certainty: Certainty::Confirmed,
            }
        );
    }

    /// macOS offers no API to query Local Network access. Claiming it is
    /// "denied" would be asserting something we were never told, so the state
    /// is marked unverifiable instead (TECH_DECISIONS.md ADR-008).
    #[test]
    fn unqueryable_permission_is_unverifiable_not_denied() {
        let matrix = CapabilityMatrix::build(
            &[probe(ProbeId::Mdns, &[Permission::LocalNetwork])],
            Platform::MacOs,
            &PermissionSet::new().with(Permission::LocalNetwork, PermissionState::Unknown),
        );

        assert_eq!(
            matrix.row(ProbeId::Mdns).unwrap().state,
            CapabilityState::Available {
                missing: Permission::LocalNetwork,
                certainty: Certainty::Unverifiable,
            }
        );
    }

    #[test]
    fn local_network_is_not_queryable_but_location_is() {
        assert!(Permission::LocationServices.is_queryable());
        assert!(!Permission::LocalNetwork.is_queryable());
    }

    /// Every permission must tell the user where to go. An Available row with
    /// no route to fixing it is just a complaint.
    #[test]
    fn every_permission_names_where_to_grant_it() {
        for permission in Permission::ALL {
            assert!(!permission.grant_hint().is_empty(), "{permission:?}");
            assert!(!permission.label().is_empty(), "{permission:?}");
        }
    }

    /// The fourth column is the product thesis; a refusal with no stated
    /// reason is decoration.
    #[test]
    fn every_refused_class_states_why_it_is_refused() {
        for class in DataClass::REFUSED {
            let why = class.refusal_rationale();
            assert!(!why.is_empty(), "{class:?} has no rationale");
        }
    }

    #[test]
    fn permitted_classes_have_no_refusal_rationale() {
        assert!(DataClass::NeighborTable.refusal_rationale().is_empty());
    }

    // ---- the all-denied case: the milestone's exit criterion ----

    #[test]
    fn with_everything_denied_unpermissioned_probes_still_work() {
        let perms = PermissionSet::new()
            .with(Permission::LocationServices, PermissionState::Denied)
            .with(Permission::LocalNetwork, PermissionState::Denied);

        let matrix = CapabilityMatrix::build(
            &[
                probe(ProbeId::Arp, &[]),
                probe(ProbeId::Wifi, &[Permission::LocationServices]),
                probe(ProbeId::Mdns, &[Permission::LocalNetwork]),
            ],
            Platform::MacOs,
            &perms,
        );

        // The app is not dead when permissions are refused: passive reads that
        // need nothing keep working, and that must be visible.
        assert!(matches!(
            matrix.row(ProbeId::Arp).unwrap().state,
            CapabilityState::Observed { .. }
        ));
        assert_eq!(matrix.summary().observed, 1);
        assert_eq!(matrix.summary().available, 2);
        assert_eq!(matrix.summary().refused, DataClass::REFUSED.len());
    }

    /// The third column must not be empty. The things JRX cannot do without
    /// administrator access are a core part of the honest picture — without
    /// them the panel implies the tool sees everything it wants to.
    #[test]
    fn matrix_lists_capabilities_that_are_impossible_without_elevation() {
        let matrix = CapabilityMatrix::build(&[], Platform::MacOs, &PermissionSet::new());

        assert!(
            matrix.limitations().count() > 0,
            "no impossible-capability rows; the Not possible column would be empty"
        );
        for row in matrix.limitations() {
            assert!(!row.describes.is_empty());
            assert!(!row.reason.is_empty());
        }
    }

    /// Per-application bandwidth is the canonical example: users expect it,
    /// it needs admin, and JRX will not ask (TECH_DECISIONS.md ADR-002).
    #[test]
    fn per_application_bandwidth_is_listed_as_impossible() {
        let matrix = CapabilityMatrix::build(&[], Platform::MacOs, &PermissionSet::new());
        assert!(
            matrix
                .limitations()
                .any(|r| r.describes.to_lowercase().contains("per-app")),
            "per-application bandwidth must be named explicitly"
        );
    }

    /// The panel is generated, not hand-written. If the UI had to keep its own
    /// copy of permission labels and grant hints, those strings could drift
    /// from what the collector actually requires — which is exactly the
    /// failure the generated matrix exists to prevent.
    #[test]
    fn matrix_carries_the_details_needed_to_render_a_grant_action() {
        let matrix = CapabilityMatrix::build(
            &[probe(ProbeId::Mdns, &[Permission::LocalNetwork])],
            Platform::MacOs,
            &PermissionSet::new(),
        );

        let info = matrix
            .permissions()
            .find(|p| p.permission == Permission::LocalNetwork)
            .expect("referenced permission is described");

        assert_eq!(info.label, Permission::LocalNetwork.label());
        assert_eq!(info.grant_hint, Permission::LocalNetwork.grant_hint());
        assert!(!info.queryable);
    }

    #[test]
    fn every_permission_is_described_exactly_once() {
        let matrix = CapabilityMatrix::build(&[], Platform::MacOs, &PermissionSet::new());
        assert_eq!(matrix.permissions().count(), Permission::ALL.len());
    }

    #[test]
    fn summary_counts_every_row_exactly_once() {
        let matrix = CapabilityMatrix::build(
            &[
                probe(ProbeId::Arp, &[]),
                probe(ProbeId::Mdns, &[Permission::LocalNetwork]),
            ],
            Platform::MacOs,
            &PermissionSet::new(),
        );
        let s = matrix.summary();
        assert_eq!(
            s.observed + s.available + s.not_possible,
            matrix.rows().count()
        );
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
                missing: Permission::LocalNetwork,
                // macOS cannot be asked about Local Network access.
                certainty: Certainty::Unverifiable,
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
                missing: Permission::LocalNetwork,
                // macOS cannot be asked about Local Network access.
                certainty: Certainty::Unverifiable,
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
