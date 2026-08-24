//! The thin, untestable shell around macOS command-line tools.
//!
//! Absolute paths only: resolving these through `PATH` would let a modified
//! environment substitute a binary, which is not acceptable in a tool that
//! presents itself as a security product.
//!
//! Everything here returns raw text. All interpretation happens in the pure,
//! fixture-tested functions in `super::parse`.

use std::process::Command;
use std::time::Duration;

use crate::probe::ProbeError;

const NETSTAT: &str = "/usr/sbin/netstat";
const IFCONFIG: &str = "/sbin/ifconfig";
const NETWORKSETUP: &str = "/usr/sbin/networksetup";
const SCUTIL: &str = "/usr/sbin/scutil";
const SYSTEM_PROFILER: &str = "/usr/sbin/system_profiler";

fn run(program: &str, args: &[&str]) -> Result<String, ProbeError> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| ProbeError::Failed(format!("{program}: {e}")))?;

    if !output.status.success() {
        return Err(ProbeError::Failed(format!(
            "{program} exited with {}",
            output.status
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn routing_table() -> Result<String, ProbeError> {
    run(NETSTAT, &["-rn", "-f", "inet"])
}

pub fn interfaces() -> Result<String, ProbeError> {
    run(IFCONFIG, &["-a"])
}

pub fn hardware_ports() -> Result<String, ProbeError> {
    run(NETWORKSETUP, &["-listallhardwareports"])
}

pub fn dns_configuration() -> Result<String, ProbeError> {
    run(SCUTIL, &["--dns"])
}

/// Wi-Fi details. Costs roughly 300 ms on macOS 26, which is why it runs as
/// enrichment after the identity card has already painted, never in the
/// cold-start critical path (ARCHITECTURE.md §7.1).
pub fn airport_json() -> Result<String, ProbeError> {
    run(SYSTEM_PROFILER, &["SPAirPortDataType", "-json"])
}

/// Budget for the fast-path probes, from the cold-start latency budget.
pub const FAST_PATH_BUDGET: Duration = Duration::from_millis(400);
