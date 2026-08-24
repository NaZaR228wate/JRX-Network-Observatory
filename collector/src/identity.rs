//! Observing the current network identity.
//!
//! The only glue layer: it calls `exec`, hands the raw text to `parse`, and
//! passes the parsed data to `jrx_core`. It contains no interpretation of its
//! own, which is what keeps every rule fixture-testable.

use jrx_core::network::{NetworkIdentity, WifiStatus};

use crate::probe::ProbeError;

#[cfg(target_os = "macos")]
use crate::macos::{exec, parse};

/// Read the current network identity.
///
/// The five OS reads run concurrently. Four are ~5-20 ms; the Wi-Fi read costs
/// ~300 ms on macOS 26, so running it in parallel is what keeps the whole
/// observation inside the 400 ms cold-start budget (ARCHITECTURE.md §7.1)
/// rather than serialising to ~345 ms plus overhead.
#[cfg(target_os = "macos")]
pub fn observe() -> Result<NetworkIdentity, ProbeError> {
    let (routes, ifaces, ports, dns, wifi) = std::thread::scope(|scope| {
        let routes = scope.spawn(exec::routing_table);
        let ifaces = scope.spawn(exec::interfaces);
        let ports = scope.spawn(exec::hardware_ports);
        let dns = scope.spawn(exec::dns_configuration);
        let wifi = scope.spawn(exec::airport_json);

        (
            routes.join(),
            ifaces.join(),
            ports.join(),
            dns.join(),
            wifi.join(),
        )
    });

    let panicked = || ProbeError::Failed("probe thread panicked".into());
    let routes = routes.map_err(|_| panicked())??;
    let ifaces = ifaces.map_err(|_| panicked())??;

    // The routing table and interface list are load-bearing: without them
    // there is no identity to report. The rest degrade individually.
    let ports = ports
        .map_err(|_| panicked())
        .and_then(|r| r)
        .unwrap_or_default();
    let dns = dns
        .map_err(|_| panicked())
        .and_then(|r| r)
        .unwrap_or_default();

    let wifi = match wifi.map_err(|_| panicked()).and_then(|r| r) {
        Ok(json) => parse::parse_airport(&json),
        // A failed read is reported as a failed read, never as absent
        // hardware (ARCHITECTURE.md §12).
        Err(e) => WifiStatus::Unavailable {
            reason: e.to_string(),
        },
    };

    Ok(NetworkIdentity::assemble(
        parse::parse_default_route(&routes),
        &parse::parse_interfaces(&ifaces),
        &parse::parse_hardware_ports(&ports),
        parse::parse_dns_servers(&dns),
        wifi,
    ))
}

#[cfg(not(target_os = "macos"))]
pub fn observe() -> Result<NetworkIdentity, ProbeError> {
    // Windows lands in M7 (MVP_ROADMAP.md).
    Err(ProbeError::Unsupported)
}
