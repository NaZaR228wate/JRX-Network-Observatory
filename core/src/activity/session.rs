//! Turning a stream of observations into what JRX has actually watched happen.
//!
//! Two things make this less obvious than summing counters.
//!
//! The counters are cumulative and belong to the OS, so the first sample says
//! nothing about JRX: it only establishes where the counting starts. Claiming
//! it would mean reporting traffic that happened before anyone was looking.
//!
//! A closed socket disappears from the next observation entirely. Session
//! totals therefore cannot be derived from whatever sockets happen to be
//! visible; they have to be accumulated as deltas while the socket exists, and
//! survive it closing.

use std::collections::HashMap;
use std::time::Duration;

use crate::activity::owner;
use crate::activity::{
    ActivityHealth, ActivitySnapshot, ConnectionActivity, CounterSample, ProcessActivity,
    ProcessKey, Protocol, SocketKey, SocketObservation,
};

/// What was last seen for one socket, so the next observation can be turned
/// into a delta.
#[derive(Debug, Clone)]
struct TrackedSocket {
    last_in: u64,
    last_out: u64,
    session_in: u64,
    session_out: u64,
    protocol: Protocol,
    remote_address: Option<std::net::IpAddr>,
    remote_port: Option<u16>,
    state: Option<String>,
    rtt_ms: Option<f64>,
    is_open: bool,
}

/// What has been observed of one program.
#[derive(Debug, Clone)]
struct TrackedProcess {
    pid: u32,
    process_name: String,
    executable_path: Option<String>,
    reported_name: String,
    session_in: u64,
    session_out: u64,
    rate_in: u64,
    rate_out: u64,
    idle_samples: u32,
    sockets: HashMap<SocketKey, TrackedSocket>,
}

/// One run of activity monitoring.
#[derive(Debug)]
pub struct ActivitySession {
    interface: String,
    last_counters: Option<CounterSample>,
    session_in: u64,
    session_out: u64,
    rate_in: u64,
    rate_out: u64,
    interface_total_in: u64,
    interface_total_out: u64,
    elapsed: Duration,
    processes: HashMap<ProcessKey, TrackedProcess>,
    health: ActivityHealth,
}

impl ActivitySession {
    /// How many consecutive silent samples before a program is dropped.
    ///
    /// Bounds memory over a long session without discarding a program that has
    /// merely paused. At one sample a second this is a few minutes.
    pub const FORGET_AFTER_IDLE_SAMPLES: u32 = 300;

    pub fn new(interface: &str) -> ActivitySession {
        ActivitySession {
            interface: interface.to_string(),
            last_counters: None,
            session_in: 0,
            session_out: 0,
            rate_in: 0,
            rate_out: 0,
            interface_total_in: 0,
            interface_total_out: 0,
            elapsed: Duration::ZERO,
            processes: HashMap::new(),
            health: ActivityHealth::Initializing,
        }
    }

    pub fn interface(&self) -> &str {
        &self.interface
    }

    pub fn session_bytes_in(&self) -> u64 {
        self.session_in
    }

    pub fn session_bytes_out(&self) -> u64 {
        self.session_out
    }

    pub fn rate_in(&self) -> u64 {
        self.rate_in
    }

    pub fn rate_out(&self) -> u64 {
        self.rate_out
    }

    pub fn set_health(&mut self, health: ActivityHealth) {
        self.health = health;
    }

    /// Move to a different interface.
    ///
    /// The new interface's counters have nothing to do with the old ones, so
    /// the baseline is dropped. Traffic already observed is kept: it happened.
    pub fn switch_interface(&mut self, interface: &str) {
        if self.interface != interface {
            self.interface = interface.to_string();
            self.last_counters = None;
        }
    }

    /// Fold in one read of the interface counters.
    pub fn observe_counters(&mut self, sample: CounterSample, interval: Duration) {
        self.elapsed += interval;
        self.interface_total_in = sample.rx_bytes;
        self.interface_total_out = sample.tx_bytes;

        let Some(previous) = self.last_counters else {
            // First read of this interface: it fixes where counting starts and
            // says nothing about what JRX has seen.
            self.last_counters = Some(sample);
            self.rate_in = 0;
            self.rate_out = 0;
            return;
        };

        let (delta_in, delta_out) = counter_delta(previous, sample);
        self.session_in = self.session_in.saturating_add(delta_in);
        self.session_out = self.session_out.saturating_add(delta_out);
        self.rate_in = per_second(delta_in, interval);
        self.rate_out = per_second(delta_out, interval);
        self.last_counters = Some(sample);
    }

    /// Fold in one observation of the socket table.
    pub fn observe_sockets(&mut self, observations: Vec<SocketObservation>, interval: Duration) {
        // Everything is closed until this observation says otherwise. A socket
        // that has gone keeps the bytes already counted for it.
        for process in self.processes.values_mut() {
            process.rate_in = 0;
            process.rate_out = 0;
            for socket in process.sockets.values_mut() {
                socket.is_open = false;
            }
        }

        let mut touched: Vec<ProcessKey> = Vec::new();

        for observation in observations {
            let key = process_key(&observation);
            let socket_key = observation.key();

            let process = self
                .processes
                .entry(key.clone())
                .or_insert_with(|| TrackedProcess {
                    pid: observation.pid,
                    process_name: executable_name(&observation),
                    executable_path: observation.executable_path.clone(),
                    reported_name: observation.reported_name.clone(),
                    session_in: 0,
                    session_out: 0,
                    rate_in: 0,
                    rate_out: 0,
                    idle_samples: 0,
                    sockets: HashMap::new(),
                });

            let (delta_in, delta_out) = match process.sockets.get(&socket_key) {
                // A counter that went backwards means the tuple was reused by a
                // new connection. Its own bytes count; the difference does not.
                Some(previous)
                    if observation.bytes_in < previous.last_in
                        || observation.bytes_out < previous.last_out =>
                {
                    (observation.bytes_in, observation.bytes_out)
                }
                Some(previous) => (
                    observation.bytes_in - previous.last_in,
                    observation.bytes_out - previous.last_out,
                ),
                // Newly seen. Whatever it had already carried happened before
                // JRX was watching and is not ours to report.
                None => (0, 0),
            };

            let entry = process
                .sockets
                .entry(socket_key)
                .or_insert_with(|| TrackedSocket {
                    last_in: observation.bytes_in,
                    last_out: observation.bytes_out,
                    session_in: 0,
                    session_out: 0,
                    protocol: observation.protocol,
                    remote_address: observation.remote_address,
                    remote_port: observation.remote_port,
                    state: observation.state.clone(),
                    rtt_ms: observation.rtt_ms,
                    is_open: true,
                });

            entry.last_in = observation.bytes_in;
            entry.last_out = observation.bytes_out;
            entry.session_in = entry.session_in.saturating_add(delta_in);
            entry.session_out = entry.session_out.saturating_add(delta_out);
            entry.state = observation.state.clone();
            entry.rtt_ms = observation.rtt_ms;
            entry.is_open = true;

            process.session_in = process.session_in.saturating_add(delta_in);
            process.session_out = process.session_out.saturating_add(delta_out);
            process.rate_in = process
                .rate_in
                .saturating_add(per_second(delta_in, interval));
            process.rate_out = process
                .rate_out
                .saturating_add(per_second(delta_out, interval));

            // A resolved path can arrive later than the first sighting.
            if process.executable_path.is_none() && observation.executable_path.is_some() {
                process.executable_path = observation.executable_path.clone();
                process.process_name = executable_name(&observation);
            }

            if !touched.contains(&key) {
                touched.push(key);
            }
        }

        for (key, process) in &mut self.processes {
            if touched.contains(key) && (process.rate_in > 0 || process.rate_out > 0) {
                process.idle_samples = 0;
            } else {
                process.idle_samples = process.idle_samples.saturating_add(1);
            }
        }

        // Bound memory: a program silent for long enough is dropped.
        self.processes
            .retain(|_, p| p.idle_samples <= Self::FORGET_AFTER_IDLE_SAMPLES);
    }

    /// Programs, busiest first.
    ///
    /// Ranked by session total, which only grows, so a quiet interval cannot
    /// demote a program and make the rows swap places.
    pub fn programs(&self) -> impl Iterator<Item = ProcessActivity> + '_ {
        let mut out: Vec<ProcessActivity> = self.processes.values().map(render).collect();
        out.sort_by(|a, b| {
            b.session_total()
                .cmp(&a.session_total())
                // Stable tie-break, so equal programs keep a fixed order.
                .then_with(|| a.pid.cmp(&b.pid))
        });
        out.into_iter()
    }

    pub fn snapshot(&self, sample_interval: Duration) -> ActivitySnapshot {
        let programs: Vec<ProcessActivity> = self.programs().collect();
        ActivitySnapshot {
            interface: self.interface.clone(),
            health: self.health.clone(),
            session_bytes_in: self.session_in,
            session_bytes_out: self.session_out,
            rate_in: self.rate_in,
            rate_out: self.rate_out,
            interface_total_in: self.interface_total_in,
            interface_total_out: self.interface_total_out,
            active_connections: programs.iter().map(|p| p.active_connections).sum(),
            programs,
            session_duration: self.elapsed,
            sample_interval,
        }
    }
}

/// A cumulative counter that went backwards has reset. That is not negative
/// traffic, and the bytes between the last read and the reset are simply lost.
fn counter_delta(previous: CounterSample, current: CounterSample) -> (u64, u64) {
    if current.rx_bytes < previous.rx_bytes || current.tx_bytes < previous.tx_bytes {
        return (0, 0);
    }
    (
        current.rx_bytes - previous.rx_bytes,
        current.tx_bytes - previous.tx_bytes,
    )
}

fn per_second(bytes: u64, interval: Duration) -> u64 {
    let seconds = interval.as_secs_f64();
    if seconds <= 0.0 {
        return 0;
    }
    (bytes as f64 / seconds) as u64
}

/// A PID alone is not an identity, because the OS reuses them. Pairing it with
/// what the process actually is means a reused number becomes a different
/// program rather than inheriting the previous one's traffic.
fn process_key(observation: &SocketObservation) -> ProcessKey {
    ProcessKey {
        pid: observation.pid,
        identity: observation
            .executable_path
            .clone()
            .unwrap_or_else(|| observation.reported_name.clone()),
    }
}

/// The executable's own name, from its path when there is one.
fn executable_name(observation: &SocketObservation) -> String {
    observation
        .executable_path
        .as_deref()
        .and_then(|path| path.rsplit('/').next())
        .unwrap_or(&observation.reported_name)
        .to_string()
}

fn render(process: &TrackedProcess) -> ProcessActivity {
    let mut connections: Vec<ConnectionActivity> = process
        .sockets
        .values()
        .map(|socket| ConnectionActivity {
            protocol: socket.protocol,
            remote_address: socket.remote_address,
            remote_port: socket.remote_port,
            state: socket.state.clone(),
            rtt_ms: socket.rtt_ms,
            network_owner: socket.remote_address.and_then(owner::network_owner),
            session_bytes_in: socket.session_in,
            session_bytes_out: socket.session_out,
            is_open: socket.is_open,
        })
        .collect();
    connections.sort_by(|a, b| {
        (b.session_bytes_in + b.session_bytes_out)
            .cmp(&(a.session_bytes_in + a.session_bytes_out))
            .then_with(|| a.remote_port.cmp(&b.remote_port))
    });

    ProcessActivity {
        pid: process.pid,
        process_name: process.process_name.clone(),
        executable_path: process.executable_path.clone(),
        application: process
            .executable_path
            .as_deref()
            .and_then(owner::application_name)
            .map(str::to_owned),
        // The name was cut short and no path was available to repair it.
        name_is_truncated: process.executable_path.is_none()
            && process.reported_name.chars().count() >= 15,
        session_bytes_in: process.session_in,
        session_bytes_out: process.session_out,
        rate_in: process.rate_in,
        rate_out: process.rate_out,
        active_connections: process.sockets.values().filter(|s| s.is_open).count(),
        idle_samples: process.idle_samples,
        connections,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{CounterSample, Protocol, SocketObservation};
    use std::time::Duration;

    const TICK: Duration = Duration::from_secs(1);

    fn socket(pid: u32, name: &str, port: u16, bin: u64, bout: u64) -> SocketObservation {
        SocketObservation {
            protocol: Protocol::Tcp,
            local_address: "192.168.1.10".parse().unwrap(),
            local_port: port,
            remote_address: Some("104.18.32.1".parse().unwrap()),
            remote_port: Some(443),
            state: Some("Established".into()),
            rtt_ms: Some(24.0),
            bytes_in: bin,
            bytes_out: bout,
            pid,
            reported_name: name.into(),
            executable_path: None,
        }
    }

    fn counters(rx: u64, tx: u64) -> CounterSample {
        CounterSample {
            rx_bytes: rx,
            tx_bytes: tx,
        }
    }

    fn program(session: &ActivitySession, name: &str) -> crate::activity::ProcessActivity {
        session
            .programs()
            .find(|p| p.process_name == name)
            .unwrap_or_else(|| panic!("{name} is not in the session"))
    }

    fn names(session: &ActivitySession) -> Vec<String> {
        session.programs().map(|p| p.process_name).collect()
    }

    // ---- A/B: interface counters ----

    #[test]
    fn the_first_sample_establishes_a_baseline_and_claims_no_traffic() {
        let mut s = ActivitySession::new("en0");
        s.observe_counters(counters(1_000_000, 500_000), TICK);

        // Everything before JRX started watching belongs to the OS, not to us.
        assert_eq!(s.session_bytes_in(), 0);
        assert_eq!(s.session_bytes_out(), 0);
        assert_eq!(s.rate_in(), 0);
    }

    #[test]
    fn subsequent_samples_accumulate_only_what_was_observed() {
        let mut s = ActivitySession::new("en0");
        s.observe_counters(counters(1_000_000, 500_000), TICK);
        s.observe_counters(counters(1_002_000, 500_500), TICK);

        assert_eq!(s.session_bytes_in(), 2_000);
        assert_eq!(s.session_bytes_out(), 500);
        assert_eq!(s.rate_in(), 2_000, "2000 bytes over one second");
    }

    /// Counters reset when an interface reinitialises after sleep or a
    /// reconnect. That is not negative traffic.
    #[test]
    fn a_counter_reset_is_not_treated_as_traffic() {
        let mut s = ActivitySession::new("en0");
        s.observe_counters(counters(1_000_000, 500_000), TICK);
        s.observe_counters(counters(1_002_000, 500_500), TICK);
        // The interface came back up and started from zero.
        s.observe_counters(counters(5_000, 1_000), TICK);

        assert_eq!(s.session_bytes_in(), 2_000, "the reset must add nothing");
        assert_eq!(s.rate_in(), 0, "a backwards counter is not a rate");

        // ...and counting resumes from the new baseline.
        s.observe_counters(counters(6_000, 1_200), TICK);
        assert_eq!(s.session_bytes_in(), 3_000);
    }

    #[test]
    fn changing_interface_starts_the_counters_again_without_losing_the_session() {
        let mut s = ActivitySession::new("en0");
        s.observe_counters(counters(1_000_000, 500_000), TICK);
        s.observe_counters(counters(1_002_000, 500_500), TICK);

        s.switch_interface("en7");
        // The new interface's counters are unrelated to the old ones.
        s.observe_counters(counters(90_000_000, 40_000_000), TICK);

        assert_eq!(s.interface(), "en7");
        assert_eq!(
            s.session_bytes_in(),
            2_000,
            "traffic already observed is kept; the new interface adds nothing yet"
        );

        s.observe_counters(counters(90_001_000, 40_000_000), TICK);
        assert_eq!(s.session_bytes_in(), 3_000);
    }

    #[test]
    fn a_rate_needs_a_positive_interval() {
        let mut s = ActivitySession::new("en0");
        s.observe_counters(counters(0, 0), TICK);
        s.observe_counters(counters(10_000, 0), Duration::ZERO);
        assert_eq!(s.rate_in(), 0, "dividing by no time is not a measurement");
        assert_eq!(
            s.session_bytes_in(),
            10_000,
            "but the bytes were still seen"
        );
    }

    // ---- C/D/E/F: socket lifetime ----

    #[test]
    fn a_newly_seen_socket_contributes_nothing_until_it_moves() {
        let mut s = ActivitySession::new("en0");
        s.observe_sockets(vec![socket(500, "Safari", 52000, 10_000_000, 200)], TICK);

        // 10 MB had already moved before JRX looked. It is not ours to claim.
        assert_eq!(program(&s, "Safari").session_bytes_in, 0);
        assert_eq!(program(&s, "Safari").active_connections, 1);
    }

    #[test]
    fn growth_on_a_known_socket_is_counted() {
        let mut s = ActivitySession::new("en0");
        s.observe_sockets(vec![socket(500, "Safari", 52000, 10_000_000, 200)], TICK);
        s.observe_sockets(vec![socket(500, "Safari", 52000, 12_000_000, 700)], TICK);

        assert_eq!(program(&s, "Safari").session_bytes_in, 2_000_000);
        assert_eq!(program(&s, "Safari").session_bytes_out, 500);
        assert_eq!(program(&s, "Safari").rate_in, 2_000_000);
    }

    /// The finding that shapes this whole module: a closed socket vanishes
    /// from the next observation. Traffic already counted must survive that.
    #[test]
    fn traffic_survives_the_socket_disappearing() {
        let mut s = ActivitySession::new("en0");
        s.observe_sockets(vec![socket(500, "Safari", 52000, 10_000_000, 0)], TICK);
        s.observe_sockets(vec![socket(500, "Safari", 52000, 12_000_000, 0)], TICK);
        // The connection closed.
        s.observe_sockets(vec![], TICK);

        let safari = program(&s, "Safari");
        assert_eq!(
            safari.session_bytes_in, 2_000_000,
            "observed traffic must not vanish with the socket"
        );
        assert_eq!(safari.active_connections, 0);
        assert_eq!(safari.rate_in, 0, "a closed socket moves nothing");
    }

    #[test]
    fn several_sockets_of_one_program_are_summed() {
        let mut s = ActivitySession::new("en0");
        s.observe_sockets(
            vec![
                socket(500, "Safari", 52000, 100, 0),
                socket(500, "Safari", 52001, 200, 0),
            ],
            TICK,
        );
        s.observe_sockets(
            vec![
                socket(500, "Safari", 52000, 1_100, 0),
                socket(500, "Safari", 52001, 2_200, 0),
            ],
            TICK,
        );

        assert_eq!(program(&s, "Safari").session_bytes_in, 3_000);
        assert_eq!(program(&s, "Safari").active_connections, 2);
    }

    /// A five-tuple can be reused by a new connection. Its counters restart,
    /// which must read as a new socket rather than as negative traffic.
    #[test]
    fn a_reused_five_tuple_restarts_counting_instead_of_going_negative() {
        let mut s = ActivitySession::new("en0");
        s.observe_sockets(vec![socket(500, "Safari", 52000, 5_000_000, 0)], TICK);
        s.observe_sockets(vec![socket(500, "Safari", 52000, 6_000_000, 0)], TICK);
        // Same tuple, but the counter went backwards: this is a new socket.
        s.observe_sockets(vec![socket(500, "Safari", 52000, 4_000, 0)], TICK);

        assert_eq!(
            program(&s, "Safari").session_bytes_in,
            1_000_000 + 4_000,
            "a restarted counter contributes its own bytes, never a negative"
        );
    }

    // ---- G/H: process identity ----

    #[test]
    fn a_program_that_exits_keeps_what_it_was_observed_moving() {
        let mut s = ActivitySession::new("en0");
        s.observe_sockets(vec![socket(500, "Safari", 52000, 0, 0)], TICK);
        s.observe_sockets(vec![socket(500, "Safari", 52000, 5_000, 0)], TICK);
        s.observe_sockets(vec![], TICK);

        assert_eq!(program(&s, "Safari").session_bytes_in, 5_000);
        assert!(program(&s, "Safari").idle_samples > 0);
    }

    /// The OS reuses PIDs. A different program on the same number must not
    /// inherit the previous one's traffic.
    #[test]
    fn a_reused_pid_becomes_a_separate_program() {
        let mut s = ActivitySession::new("en0");
        s.observe_sockets(vec![socket(500, "Safari", 52000, 0, 0)], TICK);
        s.observe_sockets(vec![socket(500, "Safari", 52000, 9_000, 0)], TICK);
        s.observe_sockets(vec![], TICK);
        // PID 500 is now something else entirely.
        s.observe_sockets(vec![socket(500, "curl", 60000, 0, 0)], TICK);
        s.observe_sockets(vec![socket(500, "curl", 60000, 40, 0)], TICK);

        assert_eq!(program(&s, "Safari").session_bytes_in, 9_000);
        assert_eq!(program(&s, "curl").session_bytes_in, 40);
        assert_eq!(s.programs().count(), 2);
    }

    /// The executable path is the stronger identity, so two processes sharing
    /// a PID number across time are told apart by it.
    #[test]
    fn the_executable_path_distinguishes_programs_before_the_pid_does() {
        let mut s = ActivitySession::new("en0");
        let mut first = socket(500, "helper", 52000, 0, 0);
        first.executable_path = Some("/Applications/A.app/Contents/MacOS/helper".into());
        let mut second = socket(500, "helper", 52000, 0, 0);
        second.executable_path = Some("/Applications/B.app/Contents/MacOS/helper".into());

        s.observe_sockets(vec![first], TICK);
        s.observe_sockets(vec![second], TICK);

        assert_eq!(s.programs().count(), 2, "same PID, different executables");
    }

    // ---- ordering ----

    /// Rows that reorder on every refresh are unreadable. Ranking is by
    /// session total, which only ever grows, so a program cannot overtake
    /// another and then fall back on the next tick.
    #[test]
    fn programs_are_ranked_by_session_total_and_do_not_oscillate() {
        let mut s = ActivitySession::new("en0");
        s.observe_sockets(
            vec![socket(1, "Small", 1, 0, 0), socket(2, "Large", 2, 0, 0)],
            TICK,
        );
        s.observe_sockets(
            vec![
                socket(1, "Small", 1, 100, 0),
                socket(2, "Large", 2, 9_000, 0),
            ],
            TICK,
        );
        let first = names(&s);

        // A quiet tick for the larger program must not demote it.
        s.observe_sockets(
            vec![
                socket(1, "Small", 1, 200, 0),
                socket(2, "Large", 2, 9_000, 0),
            ],
            TICK,
        );
        let second = names(&s);

        assert_eq!(first, vec!["Large", "Small"]);
        assert_eq!(second, first, "ranking must not change on a quiet tick");
    }

    #[test]
    fn a_snapshot_reports_the_same_totals_the_session_holds() {
        let mut s = ActivitySession::new("en0");
        s.observe_counters(counters(0, 0), TICK);
        s.observe_counters(counters(4_000, 1_000), TICK);
        s.observe_sockets(vec![socket(500, "Safari", 52000, 0, 0)], TICK);
        s.observe_sockets(vec![socket(500, "Safari", 52000, 700, 0)], TICK);

        let snapshot = s.snapshot(Duration::from_secs(1));
        assert_eq!(snapshot.session_bytes_in, 4_000);
        assert_eq!(snapshot.interface_total_in, 4_000);
        assert_eq!(snapshot.active_connections, 1);
        assert_eq!(snapshot.programs.len(), 1);
        assert_eq!(snapshot.programs[0].session_bytes_in, 700);
    }

    /// Programs that have been silent for a long time are dropped so the
    /// session cannot grow without bound.
    #[test]
    fn long_silent_programs_are_eventually_forgotten() {
        let mut s = ActivitySession::new("en0");
        s.observe_sockets(vec![socket(500, "Safari", 52000, 0, 0)], TICK);
        s.observe_sockets(vec![socket(500, "Safari", 52000, 10, 0)], TICK);

        for _ in 0..ActivitySession::FORGET_AFTER_IDLE_SAMPLES + 2 {
            s.observe_sockets(vec![], TICK);
        }
        assert_eq!(s.programs().count(), 0);
    }

    #[test]
    fn a_program_that_speaks_again_is_kept() {
        let mut s = ActivitySession::new("en0");
        s.observe_sockets(vec![socket(500, "Safari", 52000, 0, 0)], TICK);
        for _ in 0..ActivitySession::FORGET_AFTER_IDLE_SAMPLES - 1 {
            s.observe_sockets(vec![], TICK);
        }
        s.observe_sockets(vec![socket(500, "Safari", 52000, 50, 0)], TICK);
        assert_eq!(program(&s, "Safari").idle_samples, 0);
        assert_eq!(s.programs().count(), 1);
    }
}
