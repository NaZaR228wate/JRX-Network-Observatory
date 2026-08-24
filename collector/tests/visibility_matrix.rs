//! The Visibility Panel over the real probe registry, under every permission
//! combination — including all-denied, which is M2's exit criterion.
//!
//! These run against `ALL_PROBES` rather than synthetic fixtures, so they fail
//! if a newly added probe quietly makes the panel useless in some state.

use jrx_collector::registry::ALL_PROBES;
use jrx_core::capability::{
    CapabilityMatrix, CapabilityState, Certainty, PermissionSet, PermissionState,
};
use jrx_core::declaration::{Permission, Platform, ProbeId};

fn matrix(location: PermissionState, local_network: PermissionState) -> CapabilityMatrix {
    let perms = PermissionSet::new()
        .with(Permission::LocationServices, location)
        .with(Permission::LocalNetwork, local_network);
    CapabilityMatrix::build(ALL_PROBES, Platform::MacOs, &perms)
}

/// The exit criterion for M2: with everything denied, the app must still be
/// fully comprehensible rather than an empty screen.
#[test]
fn with_all_permissions_denied_the_panel_is_still_informative() {
    let m = matrix(PermissionState::Denied, PermissionState::Denied);
    let s = m.summary();

    // Passive reads that need no permission keep working. If this ever reaches
    // zero, a denied-permissions launch shows a blank product.
    assert!(
        s.observed >= 5,
        "only {} capabilities survive an all-denied launch",
        s.observed
    );
    assert!(s.available > 0, "nothing offered to the user to fix");
    assert_eq!(s.refused, 5, "refusals are unconditional");
    assert!(m.limitations().count() > 0);

    // Every row must explain itself. A row with no text is a blank line in the
    // one panel that exists to explain things.
    for row in m.rows() {
        assert!(
            !row.describes.is_empty(),
            "{:?} has no description",
            row.probe
        );
        match &row.state {
            CapabilityState::Observed { mechanism } => assert!(!mechanism.is_empty()),
            CapabilityState::NotPossible { reason } => assert!(!reason.is_empty()),
            CapabilityState::Available { missing, .. } => {
                assert!(!missing.grant_hint().is_empty())
            }
        }
    }
    for row in m.refused() {
        assert!(
            !row.rationale.is_empty(),
            "{:?} refused with no reason",
            row.class
        );
    }
}

/// The best case macOS can actually report: Location granted, Local Network
/// unknowable.
#[test]
fn best_realistic_macos_state_marks_local_network_unverifiable() {
    let m = matrix(PermissionState::Granted, PermissionState::Unknown);

    assert!(matches!(
        m.row(ProbeId::Wifi).unwrap().state,
        CapabilityState::Observed { .. }
    ));

    for id in [ProbeId::Mdns, ProbeId::Ssdp] {
        assert_eq!(
            m.row(id).unwrap().state,
            CapabilityState::Available {
                missing: Permission::LocalNetwork,
                certainty: Certainty::Unverifiable,
            },
            "{id:?} must not claim macOS denied it",
        );
    }
}

/// A denial macOS actually reported must be stated as such, not softened.
#[test]
fn denied_location_is_reported_as_confirmed() {
    let m = matrix(PermissionState::Denied, PermissionState::Unknown);
    assert_eq!(
        m.row(ProbeId::Wifi).unwrap().state,
        CapabilityState::Available {
            missing: Permission::LocationServices,
            certainty: Certainty::Confirmed,
        }
    );
}

#[test]
fn granting_everything_never_reveals_a_refused_capability() {
    let m = matrix(PermissionState::Granted, PermissionState::Granted);
    assert_eq!(m.summary().refused, 5);
    assert!(
        m.limitations().count() > 0,
        "limits are not permission-dependent"
    );
}

/// Permission state must change what the panel says. If these matched, the
/// panel would be decorative.
#[test]
fn permission_state_actually_changes_the_matrix() {
    let denied = matrix(PermissionState::Denied, PermissionState::Denied);
    let granted = matrix(PermissionState::Granted, PermissionState::Granted);
    assert!(granted.summary().observed > denied.summary().observed);
}
