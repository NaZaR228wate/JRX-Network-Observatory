//! The Tauri host. Command handlers and the event bus.
//!
//! ARCHITECTURE.md §5: the WebView is treated as untrusted. Commands take
//! typed arguments only — never a raw path, shell string, or arbitrary host —
//! and the renderer cannot widen the collection surface.

use jrx_collector::registry::ALL_PROBES;
use jrx_core::capability::{CapabilityMatrix, PermissionSet};
use jrx_core::declaration::Platform;

/// The Visibility Panel's contents for this platform and permission state.
///
/// M0 reports permissions as unknown, so every permissioned probe resolves to
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_capabilities])
        .run(tauri::generate_context!())
        .expect("error while running JRX Observatory");
}
