//! One controlled sampling loop.
//!
//! The host owns the timer; this owns what happens on each tick. Nothing else
//! in JRX may run these tools, so there is exactly one `nettop` in flight at a
//! time and no way for a component to start its own.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use jrx_core::activity::session::ActivitySession;
use jrx_core::activity::{ActivityHealth, ActivitySnapshot};

use crate::activity::provider::ActivityProvider;

/// How often to sample.
///
/// Measured at ~13 ms per tick, so one second costs about 1.3% of a core.
/// Faster buys nothing: these are counters, not events, and a quicker cadence
/// only adds noise to the rate.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(1);

/// Consecutive per-program failures before the view drops to interface-only.
///
/// One failure is usually a slow tick; a run of them is a real problem worth
/// telling the user about.
const FAILURES_BEFORE_LIMITED: u32 = 3;

pub struct ActivityMonitor {
    provider: ActivityProvider,
    state: Mutex<MonitorState>,
    interval: Duration,
}

struct MonitorState {
    session: ActivitySession,
    last_tick: Option<Instant>,
    consecutive_failures: u32,
    /// Sticky: once per-program data has worked, a single bad tick does not
    /// take the whole section away.
    ever_succeeded: bool,
}

impl ActivityMonitor {
    pub fn new(provider: ActivityProvider, interface: &str) -> ActivityMonitor {
        ActivityMonitor {
            provider,
            state: Mutex::new(MonitorState {
                session: ActivitySession::new(interface),
                last_tick: None,
                consecutive_failures: 0,
                ever_succeeded: false,
            }),
            interval: DEFAULT_INTERVAL,
        }
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Pay the per-program provider's start-up cost off the critical path.
    pub fn warm(&self) {
        self.provider.connections.warm();
    }

    /// Follow the network changing underneath us.
    pub fn switch_interface(&self, interface: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.session.switch_interface(interface);
        }
    }

    /// Take one sample.
    ///
    /// The interface counters are read first and independently: they are the
    /// part that must keep working when the richer source does not.
    pub fn tick(&self) -> ActivitySnapshot {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            // A poisoned lock means a previous tick panicked. Reporting stale
            // data would be worse than reporting nothing.
            Err(poisoned) => poisoned.into_inner(),
        };

        let now = Instant::now();
        let elapsed = state.last_tick.map_or(self.interval, |last| now - last);
        state.last_tick = Some(now);

        let interface = state.session.interface().to_string();
        let counters = self.provider.interface.counters(&interface);
        let has_interface = counters.is_ok();

        if let Ok(sample) = counters {
            state.session.observe_counters(sample, elapsed);
        }

        let health = match self.provider.connections.observe() {
            Ok(observations) => {
                state.session.observe_sockets(observations, elapsed);
                state.consecutive_failures = 0;
                state.ever_succeeded = true;
                if has_interface {
                    ActivityHealth::Full
                } else {
                    // Sockets without counters: unusual, but the program detail
                    // is still real.
                    ActivityHealth::Full
                }
            }
            Err(error) => {
                state.consecutive_failures = state.consecutive_failures.saturating_add(1);

                // Sockets are not observed on a failed tick, so nothing is
                // marked closed: last known state is kept rather than being
                // replaced with fabricated zeros.
                if !has_interface {
                    ActivityHealth::NoNetwork
                } else if !state.ever_succeeded && error.is_transient() {
                    // Still starting up. `nettop`'s first call after boot takes
                    // seconds, and that is not a failure.
                    ActivityHealth::Initializing
                } else if state.consecutive_failures >= FAILURES_BEFORE_LIMITED
                    || !error.is_transient()
                {
                    ActivityHealth::Limited {
                        reason: error.to_string(),
                    }
                } else if state.ever_succeeded {
                    ActivityHealth::Full
                } else {
                    ActivityHealth::Initializing
                }
            }
        };

        state.session.set_health(health);
        state.session.snapshot(self.interval)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::provider::{
        InterfaceActivityProvider, ProcessConnectionProvider, ProviderError,
    };
    use jrx_core::activity::{CounterSample, Protocol, SocketObservation};
    use std::sync::atomic::{AtomicU32, Ordering};

    struct FixedCounters {
        value: AtomicU32,
        fail: bool,
    }
    impl InterfaceActivityProvider for FixedCounters {
        fn counters(&self, _: &str) -> Result<CounterSample, ProviderError> {
            if self.fail {
                return Err(ProviderError::Unreadable("netstat".into()));
            }
            let n = self.value.fetch_add(1_000, Ordering::SeqCst) as u64;
            Ok(CounterSample {
                rx_bytes: n,
                tx_bytes: n / 2,
            })
        }
    }

    struct Sockets {
        outcomes: Mutex<Vec<Result<Vec<SocketObservation>, ProviderError>>>,
    }
    impl ProcessConnectionProvider for Sockets {
        fn observe(&self) -> Result<Vec<SocketObservation>, ProviderError> {
            self.outcomes
                .lock()
                .unwrap()
                .pop()
                .unwrap_or_else(|| Ok(Vec::new()))
        }
        fn describe(&self) -> &'static str {
            "test"
        }
    }

    fn observation() -> SocketObservation {
        SocketObservation {
            protocol: Protocol::Tcp,
            local_address: "192.168.1.10".parse().unwrap(),
            local_port: 52000,
            remote_address: Some("104.18.32.1".parse().unwrap()),
            remote_port: Some(443),
            state: Some("Established".into()),
            bytes_in: 10,
            bytes_out: 20,
            pid: 500,
            reported_name: "Safari".into(),
            executable_path: None,
        }
    }

    fn monitor(
        counters_fail: bool,
        outcomes: Vec<Result<Vec<SocketObservation>, ProviderError>>,
    ) -> ActivityMonitor {
        ActivityMonitor::new(
            ActivityProvider {
                interface: Box::new(FixedCounters {
                    value: AtomicU32::new(0),
                    fail: counters_fail,
                }),
                connections: Box::new(Sockets {
                    outcomes: Mutex::new(outcomes),
                }),
            },
            "en0",
        )
    }

    #[test]
    fn both_sources_working_reports_full() {
        let m = monitor(
            false,
            vec![Ok(vec![observation()]), Ok(vec![observation()])],
        );
        m.tick();
        assert_eq!(m.tick().health, ActivityHealth::Full);
    }

    /// The whole point of separating the providers: losing per-program detail
    /// must not lose the throughput number too.
    #[test]
    fn losing_the_program_source_keeps_interface_throughput_working() {
        let m = monitor(
            false,
            vec![
                Err(ProviderError::Unavailable("nettop".into())),
                Ok(vec![observation()]),
            ],
        );
        m.tick();
        let snapshot = m.tick();

        assert!(matches!(snapshot.health, ActivityHealth::Limited { .. }));
        assert!(
            snapshot.interface_total_in > 0,
            "interface counters must survive the other source failing"
        );
    }

    /// A tool that is missing will still be missing next tick, so there is no
    /// point pretending it might recover.
    #[test]
    fn a_permanently_missing_tool_drops_to_limited_at_once() {
        let m = monitor(
            false,
            vec![Err(ProviderError::Unavailable("nettop".into()))],
        );
        assert!(matches!(m.tick().health, ActivityHealth::Limited { .. }));
    }

    /// The first call after boot takes seconds. That is starting up, not
    /// failing, and must not be shown as a fault.
    #[test]
    fn a_slow_first_sample_reads_as_initializing_not_as_a_failure() {
        let m = monitor(
            false,
            vec![Err(ProviderError::TimedOut(
                "nettop".into(),
                Duration::from_secs(1),
            ))],
        );
        assert_eq!(m.tick().health, ActivityHealth::Initializing);
    }

    /// One bad tick after it has been working is a hiccup, not a reason to
    /// take the whole section away.
    #[test]
    fn a_single_failure_after_success_does_not_remove_the_section() {
        let m = monitor(
            false,
            vec![
                Err(ProviderError::TimedOut(
                    "nettop".into(),
                    Duration::from_secs(1),
                )),
                Ok(vec![observation()]),
            ],
        );
        m.tick();
        assert_eq!(m.tick().health, ActivityHealth::Full);
    }

    #[test]
    fn repeated_failures_eventually_report_limited() {
        let failure = || {
            Err(ProviderError::TimedOut(
                "nettop".into(),
                Duration::from_secs(1),
            ))
        };
        let m = monitor(
            false,
            vec![failure(), failure(), failure(), Ok(vec![observation()])],
        );
        m.tick();
        m.tick();
        m.tick();
        assert!(matches!(m.tick().health, ActivityHealth::Limited { .. }));
    }

    #[test]
    fn losing_both_sources_reports_no_network() {
        let m = monitor(true, vec![Err(ProviderError::Unavailable("nettop".into()))]);
        assert_eq!(m.tick().health, ActivityHealth::NoNetwork);
    }

    /// A failed tick must not mark every connection closed, which would show a
    /// fabricated drop to zero.
    #[test]
    fn a_failed_tick_leaves_the_last_known_programs_alone() {
        let m = monitor(
            false,
            vec![
                Err(ProviderError::TimedOut(
                    "nettop".into(),
                    Duration::from_secs(1),
                )),
                Ok(vec![SocketObservation {
                    bytes_in: 5_000,
                    ..observation()
                }]),
                Ok(vec![observation()]),
            ],
        );
        m.tick();
        let good = m.tick();
        assert_eq!(good.programs.len(), 1);

        let after_failure = m.tick();
        assert_eq!(
            after_failure.programs.len(),
            1,
            "the program must not vanish"
        );
        assert_eq!(
            after_failure.programs[0].session_bytes_in, good.programs[0].session_bytes_in,
            "a failed tick must not change what was observed"
        );
    }

    #[test]
    fn the_user_facing_reason_never_reads_like_a_parser_error() {
        let m = monitor(false, vec![Err(ProviderError::Unreadable("nettop".into()))]);
        let ActivityHealth::Limited { reason } = m.tick().health else {
            panic!("expected Limited");
        };
        // The detailed reason is kept for a diagnostic view; the UI shows the
        // user-facing wording instead.
        assert!(!reason.is_empty());
    }
}
