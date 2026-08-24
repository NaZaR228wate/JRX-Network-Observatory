//! The Tauri host. Command handlers and the event bus.
//!
//! ARCHITECTURE.md §5: the WebView is treated as untrusted. Commands take
//! typed arguments only — never a raw path, shell string, or arbitrary host —
//! and the renderer cannot widen the collection surface.

use std::time::Instant;

use jrx_collector::registry::ALL_PROBES;
use jrx_core::capability::{CapabilityMatrix, PermissionSet};
use jrx_core::declaration::Platform;
use jrx_core::network::NetworkIdentity;
use serde::Serialize;

/// The Visibility Panel's contents for this platform and permission state.
///
/// M1 reports permissions as unknown, so every permissioned probe resolves to
/// `Available`. Live permission detection lands in M2.
#[tauri::command]
fn get_capabilities() -> Result<CapabilityMatrix, String> {
    let platform = Platform::current().ok_or("unsupported platform")?;
    Ok(CapabilityMatrix::build(
        ALL_PROBES,
        platform,
        &PermissionSet::new(),
    ))
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_capabilities,
            get_network_identity
        ])
        .run(tauri::generate_context!())
        .expect("error while running JRX Observatory");
}
