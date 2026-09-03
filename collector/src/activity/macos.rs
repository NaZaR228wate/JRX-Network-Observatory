//! The macOS activity provider.
//!
//! Two sources with very different reliability, kept apart on purpose:
//!
//! - `netstat -ib` reads the interface counters. Cheap, stable, and the thing
//!   that must keep working.
//! - `nettop` reports per-socket bytes with the owning process. Richer, and a
//!   *tool* rather than an API — its output format is not contractual, so
//!   everything here treats a change in it as a degradation and not a crash
//!   (TECH_DECISIONS.md ADR-018).

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jrx_core::activity::{CounterSample, SocketObservation};

use crate::activity::nettop;
use crate::activity::provider::{
    InterfaceActivityProvider, ProcessConnectionProvider, ProviderError,
};

const NETSTAT: &str = "/usr/sbin/netstat";
const NETTOP: &str = "/usr/bin/nettop";

/// How long `nettop` is given before the sample is abandoned.
///
/// The first call after boot was measured at 4.2 s while it initialises;
/// afterwards it settles at ~27 ms. The generous ceiling only matters for that
/// first call, which `warm()` makes off the critical path.
const NETTOP_TIMEOUT: Duration = Duration::from_secs(8);

// ---- interface counters ----

pub struct NetstatInterfaceProvider;

impl InterfaceActivityProvider for NetstatInterfaceProvider {
    fn counters(&self, interface: &str) -> Result<CounterSample, ProviderError> {
        let output = capture(NETSTAT, &["-ib", "-I", interface], NETTOP_TIMEOUT)?;
        parse_counters(&output, interface)
            .ok_or_else(|| ProviderError::Unreadable(format!("netstat -ib -I {interface}")))
    }
}

/// Parse `netstat -ib -I <interface>`.
///
/// An interface appears once per address family; only the `<Link#n>` row
/// carries byte totals.
pub fn parse_counters(output: &str, interface: &str) -> Option<CounterSample> {
    output.lines().find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 11 || fields[0] != interface || !fields[2].starts_with("<Link") {
            return None;
        }
        Some(CounterSample {
            rx_bytes: fields[6].parse().ok()?,
            tx_bytes: fields[9].parse().ok()?,
        })
    })
}

// ---- per-process sockets ----

/// Per-socket bytes, from one `nettop` sample at a time.
///
/// The choice of invocation was made twice, and the second time on better
/// evidence (TECH_DECISIONS.md ADR-020).
///
/// Continuous logging mode (`-L 0`) gives stable latency but pegs a core:
/// measured at 128% CPU sustained on this Mac, at any `-s` interval, with the
/// output being drained. That is not something to leave running.
///
/// A single sample (`-L 1`) costs ~27 ms once warm, with occasional 70 ms
/// outliers — and **4.2 s on the very first call after boot**, while the tool
/// establishes its connection to the statistics source. That first call is
/// paid by `warm()`, off the critical path.
pub struct NettopConnectionProvider {
    state: Arc<StreamState>,
    warmed: AtomicBool,
}

struct StreamState {
    /// PID to executable path.
    ///
    /// A process's path does not change while it lives, so it is resolved
    /// once. Entries for PIDs that stop appearing are dropped, so a PID the OS
    /// later reuses is resolved afresh rather than inheriting an identity.
    paths: Mutex<HashMap<u32, Option<String>>>,
}

impl Default for NettopConnectionProvider {
    fn default() -> Self {
        NettopConnectionProvider {
            state: Arc::new(StreamState {
                paths: Mutex::new(HashMap::new()),
            }),
            warmed: AtomicBool::new(false),
        }
    }
}

impl StreamState {
    fn cached_path(&self, pid: u32) -> Option<String> {
        if let Ok(cache) = self.paths.lock()
            && let Some(known) = cache.get(&pid)
        {
            return known.clone();
        }
        let resolved = executable_path(pid);
        if let Ok(mut cache) = self.paths.lock() {
            cache.insert(pid, resolved.clone());
        }
        resolved
    }

    fn retain_seen(&self, seen: &[u32]) {
        if let Ok(mut cache) = self.paths.lock() {
            cache.retain(|pid, _| seen.contains(pid));
        }
    }
}

impl ProcessConnectionProvider for NettopConnectionProvider {
    fn observe(&self) -> Result<Vec<SocketObservation>, ProviderError> {
        let output = capture(
            NETTOP,
            &["-x", "-L", "1", "-J", "state,bytes_in,bytes_out"],
            NETTOP_TIMEOUT,
        )?;

        let observations = nettop::parse(&output, |pid| self.state.cached_path(pid));

        // Output that parses to nothing means the format moved, or the tool
        // printed usage. Either way it is not "no connections", and reporting
        // it as such would be a fabricated zero.
        if observations.is_empty() && !has_sample_header(&output) {
            return Err(ProviderError::Unreadable(NETTOP.to_string()));
        }

        let seen: Vec<u32> = observations.iter().map(|o| o.pid).collect();
        self.state.retain_seen(&seen);

        Ok(observations)
    }

    /// Pay the first call's cost where nobody is waiting.
    fn warm(&self) {
        if self.warmed.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = capture(NETTOP, &["-x", "-L", "1", "-J", "bytes_in"], NETTOP_TIMEOUT);
    }

    fn describe(&self) -> &'static str {
        "nettop"
    }
}

/// A genuine sample carries the column header, even when it lists nothing.
fn has_sample_header(output: &str) -> bool {
    output.lines().any(is_header)
}

fn is_header(line: &str) -> bool {
    line.starts_with("time,") || line.starts_with(",state,") || line.starts_with(",bytes_in,")
}

/// The executable path for a PID.
///
/// Native, so it costs no process spawn. `nettop` truncates names at 15
/// characters and this is how the full one is recovered; a process that has
/// already exited yields `None` rather than a reconstructed guess.
#[cfg(target_os = "macos")]
pub fn executable_path(pid: u32) -> Option<String> {
    // From `<libproc.h>`, part of libSystem. Documented, and unprivileged.
    #[allow(unsafe_code)]
    unsafe extern "C" {
        fn proc_pidpath(
            pid: i32,
            buffer: *mut core::ffi::c_void,
            buffersize: u32,
        ) -> core::ffi::c_int;
    }

    const MAX_PATH: usize = 4096;
    let mut buffer = vec![0u8; MAX_PATH];

    // SAFETY: the buffer is owned here, is MAX_PATH bytes long, and that
    // length is passed truthfully. The call writes only within it and returns
    // how many bytes it wrote.
    #[allow(unsafe_code)]
    let written = unsafe { proc_pidpath(pid as i32, buffer.as_mut_ptr().cast(), MAX_PATH as u32) };

    if written <= 0 {
        return None;
    }
    buffer.truncate(written as usize);
    String::from_utf8(buffer).ok()
}

#[cfg(not(target_os = "macos"))]
pub fn executable_path(_pid: u32) -> Option<String> {
    None
}

/// Run a tool and collect its output, giving up after `timeout`.
///
/// The child is killed and reaped on timeout, so a hung tool cannot leave a
/// zombie behind or accumulate across samples.
fn capture(program: &str, args: &[&str], timeout: Duration) -> Result<String, ProviderError> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ProviderError::Unavailable(program.to_string()),
            _ => ProviderError::Failed(program.to_string(), e.to_string()),
        })?;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                // Reap it, so no zombie is left behind.
                let _ = child.wait();
                return Err(ProviderError::TimedOut(program.to_string(), timeout));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => return Err(ProviderError::Failed(program.to_string(), e.to_string())),
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|e| ProviderError::Failed(program.to_string(), e.to_string()))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NETSTAT_OUTPUT: &str = concat!(
        "Name       Mtu   Network       Address            Ipkts Ierrs     Ibytes    Opkts Oerrs     Obytes  Coll\n",
        "en7        1500  <Link#13>   9c:69:d3:6c:38:28  5661610     0 5998695733  1557229     0  991638350     0\n",
        "en7        1500  172.16.0/23   172.16.0.207       5661610     -   -        1557229     -   -           -\n",
    );

    #[test]
    fn reads_the_link_row_and_ignores_the_address_rows() {
        let sample = parse_counters(NETSTAT_OUTPUT, "en7").expect("counters");
        assert_eq!(sample.rx_bytes, 5_998_695_733);
        assert_eq!(sample.tx_bytes, 991_638_350);
    }

    #[test]
    fn an_absent_interface_yields_nothing_rather_than_zero() {
        assert!(parse_counters(NETSTAT_OUTPUT, "en0").is_none());
        assert!(parse_counters("", "en7").is_none());
    }

    /// A zero would be indistinguishable from a quiet interface, so garbage
    /// must not produce one.
    #[test]
    fn garbage_yields_nothing_rather_than_a_fabricated_zero() {
        assert!(parse_counters("total nonsense\n\n???", "en7").is_none());
    }

    /// Samples in the stream are delimited by the repeated column header, so
    /// recognising it is what separates one sample from the next.
    #[test]
    fn a_sample_boundary_is_recognised_by_the_repeated_header() {
        assert!(is_header("time,,interface,state,bytes_in,"));
        assert!(is_header(",state,bytes_in,bytes_out,"));
        assert!(!is_header("Telegram.675,,0,0,"));
        assert!(!is_header("usage: nettop [-n]..."));
        assert!(!is_header(""));
    }

    #[test]
    fn a_missing_tool_is_reported_as_unavailable_and_is_not_transient() {
        let error =
            capture("/nonexistent/tool", &[], Duration::from_millis(50)).expect_err("should fail");
        assert!(matches!(error, ProviderError::Unavailable(_)));
        assert!(!error.is_transient(), "a missing tool stays missing");
    }

    // Forces a hang with a Unix command (/bin/sleep). The timeout path in
    // `capture` is platform-agnostic, so exercising it on Unix is enough;
    // Windows has no /bin/sleep, where it would report Unavailable instead.
    #[cfg(unix)]
    #[test]
    fn a_hung_tool_times_out_and_is_transient() {
        let error = capture("/bin/sleep", &["30"], Duration::from_millis(120))
            .expect_err("should time out");
        assert!(matches!(error, ProviderError::TimedOut(..)));
        assert!(error.is_transient());
    }

    /// Every failure has to be sayable to a person without exposing a parser
    /// error as the headline.
    /// A PID the OS later hands to a different process must be looked up
    /// again, not served from the cache.
    #[test]
    fn the_path_cache_forgets_pids_that_stop_appearing() {
        let provider = NettopConnectionProvider::default();
        {
            let mut cache = provider.state.paths.lock().unwrap();
            cache.insert(500, Some("/Applications/Old.app/Contents/MacOS/Old".into()));
            cache.insert(600, Some("/usr/bin/still-here".into()));
        }

        provider.state.retain_seen(&[600]);

        let cache = provider.state.paths.lock().unwrap();
        assert!(
            !cache.contains_key(&500),
            "a departed PID must not be remembered"
        );
        assert!(cache.contains_key(&600));
    }

    #[test]
    fn the_path_cache_answers_from_memory_once_resolved() {
        let provider = NettopConnectionProvider::default();
        {
            let mut cache = provider.state.paths.lock().unwrap();
            cache.insert(1, Some("/sbin/launchd".into()));
        }
        assert_eq!(
            provider.state.cached_path(1).as_deref(),
            Some("/sbin/launchd")
        );
    }

    /// A PID that cannot be resolved is remembered as unresolvable, so the
    /// lookup is not retried every tick for a process that has exited.
    #[test]
    fn an_unresolvable_pid_is_remembered_as_such() {
        let provider = NettopConnectionProvider::default();
        assert_eq!(provider.state.cached_path(0), None);
        assert!(provider.state.paths.lock().unwrap().contains_key(&0));
    }

    #[test]
    fn every_failure_has_user_facing_wording() {
        for error in [
            ProviderError::Unavailable("x".into()),
            ProviderError::Unreadable("x".into()),
            ProviderError::TimedOut("x".into(), Duration::from_secs(1)),
            ProviderError::Failed("x".into(), "y".into()),
        ] {
            assert!(!error.user_facing().is_empty());
            assert!(
                !error.user_facing().contains("parse"),
                "the headline must not read like a parser error"
            );
        }
    }
}
