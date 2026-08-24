//! Asking macOS what JRX is actually allowed to do.
//!
//! The Visibility Panel is only worth anything if it reports the real state,
//! so this module asks the OS rather than assuming. Where the OS cannot be
//! asked, it says so instead of guessing (TECH_DECISIONS.md ADR-008).

use jrx_core::capability::{PermissionSet, PermissionState};
use jrx_core::declaration::Permission;

/// Map a `CLAuthorizationStatus` to a permission state.
///
/// Split out from the FFI call so the mapping — the part with actual
/// decisions in it — is testable without CoreLocation.
fn map_location_status(raw: i32) -> PermissionState {
    match raw {
        // kCLAuthorizationStatusNotDetermined
        0 => PermissionState::NotRequested,
        // kCLAuthorizationStatusRestricted: blocked by policy. The user cannot
        // grant it by clicking, so treat it as denied rather than sending them
        // to a settings pane that cannot help.
        1 => PermissionState::Denied,
        // kCLAuthorizationStatusDenied
        2 => PermissionState::Denied,
        // kCLAuthorizationStatusAuthorizedAlways / ...WhenInUse
        3 | 4 => PermissionState::Granted,
        // Never optimistically assume access we were not told we have.
        _ => PermissionState::Unknown,
    }
}

/// Query Location Services authorisation.
///
/// macOS requires this merely to read the current Wi-Fi network name
/// (ARCHITECTURE.md §12). Reads the status only: it never starts location
/// updates and never requests a position.
///
/// Uses the `+[CLLocationManager authorizationStatus]` class method rather
/// than the instance property. Apple documents that CLLocationManager
/// instances must be created on a thread with an active run loop, and Tauri
/// command handlers run on a worker thread — so instantiating one here would
/// be unsound. The class method reads a process-wide value and takes no
/// receiver state.
#[cfg(target_os = "macos")]
pub fn location_services_state() -> PermissionState {
    use objc2::msg_send;
    use objc2::runtime::AnyClass;

    let Some(class) = AnyClass::get(c"CLLocationManager") else {
        // CoreLocation unavailable. Report that we do not know, rather than
        // guessing in either direction.
        return PermissionState::Unknown;
    };

    // SAFETY: `authorizationStatus` is a CoreLocation class method taking no
    // arguments and returning CLAuthorizationStatus, which is int32_t on
    // macOS (objc2 verifies this signature at runtime and panics on a
    // mismatch). It reads a process-wide value, starts nothing, and is
    // callable from any thread.
    #[allow(unsafe_code)]
    let status: i32 = unsafe { msg_send![class, authorizationStatus] };

    map_location_status(status)
}

#[cfg(not(target_os = "macos"))]
pub fn location_services_state() -> PermissionState {
    PermissionState::Unknown
}

/// Local Network access state.
///
/// Always `Unknown`. macOS provides no API to query it — there is no
/// `authorizationStatus` equivalent — and denial is silent: multicast simply
/// returns nothing. JRX therefore refuses to report a status it was never
/// given. Once discovery runs (M3) the result becomes observable behaviourally,
/// and the panel can say so from evidence rather than assumption.
pub fn local_network_state() -> PermissionState {
    PermissionState::Unknown
}

/// Everything JRX can currently determine about its own permissions.
pub fn observe() -> PermissionSet {
    PermissionSet::new()
        .with(Permission::LocationServices, location_services_state())
        .with(Permission::LocalNetwork, local_network_state())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
#[link(name = "CoreLocation", kind = "framework")]
unsafe extern "C" {}

#[cfg(test)]
mod tests {
    use super::*;
    use jrx_core::capability::PermissionState;

    // CLAuthorizationStatus values, from CoreLocation.
    const NOT_DETERMINED: i32 = 0;
    const RESTRICTED: i32 = 1;
    const DENIED: i32 = 2;
    const AUTHORIZED_ALWAYS: i32 = 3;
    const AUTHORIZED_WHEN_IN_USE: i32 = 4;

    #[test]
    fn never_asked_is_not_requested() {
        assert_eq!(
            map_location_status(NOT_DETERMINED),
            PermissionState::NotRequested
        );
    }

    #[test]
    fn denied_is_denied() {
        assert_eq!(map_location_status(DENIED), PermissionState::Denied);
    }

    /// Restricted means a policy blocks it — the user cannot grant it by
    /// clicking. For capability purposes the effect is the same as denied, and
    /// reporting it as "not yet asked" would send the user to a settings pane
    /// that cannot help them.
    #[test]
    fn restricted_by_policy_is_denied_not_not_requested() {
        assert_eq!(map_location_status(RESTRICTED), PermissionState::Denied);
    }

    #[test]
    fn authorized_is_granted() {
        assert_eq!(
            map_location_status(AUTHORIZED_ALWAYS),
            PermissionState::Granted
        );
        assert_eq!(
            map_location_status(AUTHORIZED_WHEN_IN_USE),
            PermissionState::Granted
        );
    }

    /// An unrecognised value must not be optimistically read as granted.
    #[test]
    fn unknown_status_is_not_assumed_to_be_granted() {
        assert_eq!(map_location_status(99), PermissionState::Unknown);
        assert_eq!(map_location_status(-1), PermissionState::Unknown);
    }

    /// macOS exposes no API for Local Network access, so the probe must report
    /// Unknown rather than inventing a status.
    #[test]
    fn local_network_state_is_unknown_because_macos_cannot_be_asked() {
        assert_eq!(local_network_state(), PermissionState::Unknown);
    }
}
