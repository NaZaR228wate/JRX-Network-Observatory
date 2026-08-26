//! The Tauri host. Command handlers and the event bus.
//!
//! ARCHITECTURE.md §5: the WebView is treated as untrusted. Commands take
//! typed arguments only — never a raw path, shell string, or arbitrary host —
//! and the renderer cannot widen the collection surface.

use std::time::Instant;

// Defence in depth: jrx-collector refuses to compile a release build with
// fixtures, and so does the host. A binary that fabricates a network must not
// be shippable through either crate.
#[cfg(all(feature = "fixtures", not(debug_assertions)))]
compile_error!(
    "the `fixtures` feature must never be enabled in a release build: it would \
     ship an application that fabricates network data"
);

use jrx_collector::registry::ALL_PROBES;
use jrx_core::capability::{CapabilityMatrix, PermissionSet};
use jrx_core::declaration::{Permission, Platform};
use jrx_core::network::NetworkIdentity;
use serde::Serialize;
use tauri::{Emitter, Manager};

/// Everything JRX can and cannot see, here and now.
///
/// Generated from the probe declarations plus the live permission state, so it
/// cannot drift from what the collector actually does (ARCHITECTURE.md §9).
#[tauri::command]
fn get_capabilities() -> Result<CapabilityMatrix, String> {
    #[cfg(feature = "fixtures")]
    if let Some(fixture) = jrx_collector::fixtures::Fixture::from_env() {
        return Ok(jrx_collector::fixtures::capabilities(fixture));
    }

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

    #[cfg(feature = "fixtures")]
    if let Some(fixture) = jrx_collector::fixtures::Fixture::from_env() {
        return Ok(NetworkIdentityReport {
            identity: fixture.identity(),
            observed_in_ms: 214,
        });
    }

    let identity = jrx_collector::identity::observe().map_err(|e| e.to_string())?;

    Ok(NetworkIdentityReport {
        identity,
        observed_in_ms: started.elapsed().as_millis() as u64,
    })
}

/// Discover devices on the current network, reporting progress as it happens.
///
/// Passive only: the ARP cache is read with nothing sent, and mDNS/SSDP emit
/// the same multicast queries every device on the network already sends. No
/// subnet sweep runs here (TECH_DECISIONS.md ADR-009).
///
/// Returns immediately and streams stages over `discovery://stage`, finishing
/// with `discovery://complete`. The multicast sources listen for three
/// seconds; blocking the UI for that long would be the blank loading screen
/// this design exists to avoid.
#[tauri::command]
fn start_discovery(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        #[cfg(feature = "fixtures")]
        if let Some(fixture) = jrx_collector::fixtures::Fixture::from_env() {
            let report = fixture.report();
            // Staged the same way a real run is, so the first-seconds
            // experience being validated is the real one.
            for source in &report.quality.sources {
                let _ = app.emit(
                    "discovery://stage",
                    jrx_collector::discovery::DiscoveryStage::SourceFinished {
                        source: source.clone(),
                    },
                );
            }
            if let Ok(mut held) = app.state::<LastDiscovery>().0.lock() {
                *held = Some(report.clone());
            }
            let _ = app.emit("discovery://complete", report);
            return;
        }

        let identity = match jrx_collector::identity::observe() {
            Ok(identity) => identity,
            Err(e) => {
                let _ = app.emit("discovery://failed", e.to_string());
                return;
            }
        };

        let emit = |stage: jrx_collector::discovery::DiscoveryStage| {
            let _ = app.emit("discovery://stage", stage);
        };

        match jrx_collector::discovery::observe_with_progress(&identity, &emit) {
            Ok(report) => {
                if let Ok(mut held) = app.state::<LastDiscovery>().0.lock() {
                    *held = Some(report.clone());
                }
                let _ = app.emit("discovery://complete", report);
            }
            Err(e) => {
                let _ = app.emit("discovery://failed", e.to_string());
            }
        }
    });
}

/// The most recent discovery result, held by the host.
///
/// The renderer is untrusted (ARCHITECTURE.md §5), so it never sends device
/// data back for the host to act on. It asks for a view by typed category and
/// page number, and the host derives it from what it already holds.
#[derive(Default)]
struct LastDiscovery(std::sync::Mutex<Option<jrx_collector::discovery::DiscoveryReport>>);

/// One category's members, paginated.
///
/// Pure re-derivation from the last discovery result. Opening a group performs
/// no network work at all, so it is instant and emits nothing.
#[tauri::command]
fn group_view(
    state: tauri::State<'_, LastDiscovery>,
    category: jrx_core::device::Category,
    page: usize,
    filter: Option<jrx_core::topology::GroupFilter>,
) -> Result<jrx_core::topology::GroupView, String> {
    let held = state.0.lock().map_err(|_| "discovery state poisoned")?;
    let report = held.as_ref().ok_or("no discovery has completed yet")?;
    Ok(jrx_core::topology::GroupView::filtered(
        &report.devices,
        category,
        page,
        filter.unwrap_or_default(),
    ))
}

// ---- live activity ----

/// The one sampling loop.
///
/// The host owns collection entirely. The renderer receives typed snapshots
/// and can neither start a sample nor compute an authoritative byte delta
/// (ARCHITECTURE.md §5).
#[derive(Default)]
struct ActivityRuntime {
    running: std::sync::atomic::AtomicBool,
}

/// Begin monitoring this Mac's activity.
///
/// Returns immediately. Interface throughput is available on the first tick;
/// per-program detail arrives once `nettop` has finished starting up, which
/// takes seconds on its first call after boot and must not block the screen.
#[tauri::command]
fn start_activity(app: tauri::AppHandle, state: tauri::State<'_, ActivityRuntime>) {
    use std::sync::atomic::Ordering;

    // One loop, ever. A second call while running is a no-op rather than a
    // second stream of samples.
    if state.running.swap(true, Ordering::SeqCst) {
        return;
    }

    std::thread::spawn(move || {
        let interface = jrx_collector::identity::observe()
            .map(|identity| identity.interface)
            .unwrap_or_default();

        let monitor = jrx_collector::activity::monitor::ActivityMonitor::new(
            jrx_collector::activity::ActivityProvider {
                interface: Box::new(jrx_collector::activity::macos::NetstatInterfaceProvider),
                connections: Box::new(
                    jrx_collector::activity::macos::NettopConnectionProvider::default(),
                ),
            },
            &interface,
        );

        // Pay the start-up cost off the critical path, so the first frame
        // shows throughput instead of a spinner.
        let warming = std::sync::Arc::new(monitor);
        let to_warm = std::sync::Arc::clone(&warming);
        std::thread::spawn(move || to_warm.warm());

        let interval = warming.interval();
        loop {
            if !app
                .state::<ActivityRuntime>()
                .running
                .load(Ordering::SeqCst)
            {
                return;
            }

            let started = std::time::Instant::now();
            let snapshot = warming.tick();
            if app.emit("activity://snapshot", snapshot).is_err() {
                // The window has gone.
                return;
            }

            // A sample that overran its slot simply gives up the remainder.
            // Sleeping the full interval regardless would let slow ticks
            // accumulate into a backlog.
            let spent = started.elapsed();
            if spent < interval {
                std::thread::sleep(interval - spent);
            }
        }
    });
}

/// Stop monitoring. The loop exits at its next tick.
#[tauri::command]
fn stop_activity(state: tauri::State<'_, ActivityRuntime>) {
    state
        .running
        .store(false, std::sync::atomic::Ordering::SeqCst);
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
        .manage(LastDiscovery::default())
        .manage(ActivityRuntime::default())
        .invoke_handler(tauri::generate_handler![
            get_capabilities,
            get_network_identity,
            start_discovery,
            group_view,
            start_activity,
            stop_activity,
            open_privacy_settings
        ])
        .build(tauri::generate_context!())
        .expect("error while building JRX Observatory")
        .run(|app, event| {
            // Stop sampling when the app goes away. Each sample's child is
            // reaped by the provider, so nothing is left behind — but the loop
            // itself would otherwise keep spawning during shutdown.
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                app.state::<ActivityRuntime>()
                    .running
                    .store(false, std::sync::atomic::Ordering::SeqCst);
            }
        });
}
