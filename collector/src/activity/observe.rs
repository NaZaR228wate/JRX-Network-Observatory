//! Running the sources and assembling a snapshot.
//!
//! The thin, untestable layer. All interpretation lives in the pure parsers
//! beside it.

use std::process::Command;
use std::time::{Duration, Instant};

use crate::activity::{ActivitySnapshot, nettop, throughput};
use crate::probe::ProbeError;

const NETTOP: &str = "/usr/bin/nettop";
const NETSTAT: &str = "/usr/sbin/netstat";

/// Resolve a PID to its executable path.
///
/// Native, so it costs no process spawn. `nettop` truncates names at 15
/// characters, and this is how the full one is recovered — a process that has
/// already exited simply yields `None` rather than a reconstructed guess.
#[cfg(target_os = "macos")]
pub fn process_name(pid: u32) -> Option<String> {
    // From `<libproc.h>`, part of libSystem. Documented and unprivileged for
    // the caller's own processes; returns 0 for others, which is handled.
    #[allow(unsafe_code)]
    unsafe extern "C" {
        fn proc_pidpath(
            pid: i32,
            buffer: *mut core::ffi::c_void,
            buffersize: u32,
        ) -> core::ffi::c_int;
    }

    const PROC_PIDPATHINFO_MAXSIZE: usize = 4096;
    let mut buffer = vec![0u8; PROC_PIDPATHINFO_MAXSIZE];

    // SAFETY: the buffer is owned here, is at least PROC_PIDPATHINFO_MAXSIZE
    // bytes, and its length is passed truthfully. The call only writes within
    // that length and returns the number of bytes written.
    #[allow(unsafe_code)]
    let written = unsafe {
        proc_pidpath(
            pid as i32,
            buffer.as_mut_ptr().cast(),
            PROC_PIDPATHINFO_MAXSIZE as u32,
        )
    };

    if written <= 0 {
        return None;
    }
    buffer.truncate(written as usize);
    let path = String::from_utf8(buffer).ok()?;

    // The last path component is the executable name.
    Some(path.rsplit('/').next()?.to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn process_name(_pid: u32) -> Option<String> {
    None
}

fn run(program: &str, args: &[&str]) -> Result<String, ProbeError> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| ProbeError::Failed(format!("{program}: {e}")))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// One connection listing, with bytes and owning process.
pub fn connections() -> Result<String, ProbeError> {
    run(NETTOP, &["-x", "-L", "1", "-J", "state,bytes_in,bytes_out"])
}

/// One read of an interface's counters.
pub fn counters(interface: &str) -> Result<String, ProbeError> {
    run(NETSTAT, &["-ib", "-I", interface])
}

/// Take a snapshot, measuring the rate over `window`.
///
/// Two counter reads are needed for a rate, so this blocks for `window`. A
/// real implementation would keep the previous sample instead of sleeping.
pub fn snapshot(interface: &str, window: Duration) -> Result<ActivitySnapshot, ProbeError> {
    let first = throughput::parse_counters(&counters(interface)?, interface)
        .ok_or_else(|| ProbeError::Failed(format!("no counters for {interface}")))?;

    let started = Instant::now();
    std::thread::sleep(window);
    let elapsed = started.elapsed();

    let second = throughput::parse_counters(&counters(interface)?, interface)
        .ok_or_else(|| ProbeError::Failed(format!("no counters for {interface}")))?;

    let (down, up) = throughput::rate(first, second, elapsed.as_secs_f64());
    let connections = nettop::parse(&connections()?, process_name);

    Ok(ActivitySnapshot {
        interface: interface.to_string(),
        down_bytes_per_sec: down,
        up_bytes_per_sec: up,
        total_rx: second.rx_bytes,
        total_tx: second.tx_bytes,
        connections,
        sampled_over: elapsed,
    })
}
