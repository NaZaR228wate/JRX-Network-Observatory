//! The Tauri host. Command handlers and the event bus.
//!
//! ARCHITECTURE.md §5: the WebView is treated as untrusted. Commands take
//! typed arguments only — never a raw path, shell string, or arbitrary host —
//! and the renderer cannot widen the collection surface.

use std::time::Instant;

use jrx_collector::registry::ALL_PROBES;
use jrx_core::capability::{CapabilityMatrix, PermissionSet};
use jrx_core::declaration::{Permission, Platform};
use jrx_core::network::NetworkIdentity;
use serde::Serialize;

/// Everything JRX can and cannot see, here and now.
///
/// Generated from the probe declarations plus the live permission state, so it
/// cannot drift from what the collector actually does (ARCHITECTURE.md §9).
#[tauri::command]
fn get_capabilities() -> Result<CapabilityMatrix, String> {
    let platform = Platform::current().ok_or("unsupported platform")?;
    Ok(CapabilityMatrix::build(
        ALL_PROBES,
        platform,
        &observed_permissions(),
    ))
}

#[cfg(target_os = "macos")]
fn observed_permissions() -> PermissionSet {
    jrx_collector::macos::permissions::observe()
}

#[cfg(not(target_os = "macos"))]
fn observed_permissions() -> PermissionSet {
    PermissionSet::new()
}

#[derive(Serialize)]
struct NetworkIdentityReport {
    identity: NetworkIdentity,
    /// Wall-clock cost of the observation. Surfaced so the 400 ms cold-start
    /// budget (ARCHITECTURE.md §7.1) is measured in the running app rather
    /// than assumed.
    observed_in_ms: u64,
}

#[tauri::command]
fn get_network_identity() -> Result<NetworkIdentityReport, String> {
    let started = Instant::now();
    let identity = jrx_collector::identity::observe().map_err(|e| e.to_string())?;

    Ok(NetworkIdentityReport {
        identity,
        observed_in_ms: started.elapsed().as_millis() as u64,
    })
}

/// Open the System Settings pane where a permission can be granted.
///
/// Takes a typed `Permission`, never a URL. The renderer cannot ask the host
/// to open an arbitrary location, which is the whole point of keeping the
/// WebView untrusted (ARCHITECTURE.md §5).
#[cfg(target_os = "macos")]
#[tauri::command]
fn open_privacy_settings(permission: Permission) -> Result<(), String> {
    let anchor = match permission {
        Permission::LocationServices => {
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_LocationServices"
        }
        Permission::LocalNetwork => {
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_LocalNetwork"
        }
    };

    std::process::Command::new("/usr/bin/open")
        .arg(anchor)
        .status()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn open_privacy_settings(_permission: Permission) -> Result<(), String> {
    Err("not supported on this platform".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_capabilities,
            get_network_identity,
            open_privacy_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running JRX Observatory");
}
