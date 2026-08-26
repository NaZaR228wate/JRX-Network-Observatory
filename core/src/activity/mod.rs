//! This Mac's own live network activity.
//!
//! Domain models and session accounting. Pure: every rule here is testable
//! against fixtures, and nothing in this module touches the operating system.
//!
//! The product boundary is fixed and narrow (PRODUCT_BOUNDARIES.md): JRX shows
//! which of *this machine's* programs are talking, how much, and to whose
//! network. It does not name websites, and there is deliberately no field in
//! which a website name could be stored.

pub mod owner;
pub mod session;

use std::net::IpAddr;
use std::time::Duration;

use serde::Serialize;

/// Transport protocol, as the OS reported it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Tcp,
    Udp,
}

impl Protocol {
    pub fn label(self) -> &'static str {
        match self {
            Protocol::Tcp => "TCP",
            Protocol::Udp => "UDP",
        }
    }
}

/// One socket, as seen in a single observation.
///
/// Byte counts are cumulative for the life of that socket, which is why the
/// session accounting has to remember them: a socket that closes vanishes from
/// the next observation entirely.
#[derive(Debug, Clone, PartialEq)]
pub struct SocketObservation {
    pub protocol: Protocol,
    pub local_address: IpAddr,
    pub local_port: u16,
    pub remote_address: Option<IpAddr>,
    pub remote_port: Option<u16>,
    pub state: Option<String>,
    pub rtt_ms: Option<f64>,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub pid: u32,
    /// As the tool reported it, possibly truncated.
    pub reported_name: String,
    /// Resolved from the PID. `None` when the process could not be resolved —
    /// never reconstructed by guessing.
    pub executable_path: Option<String>,
}

impl SocketObservation {
    /// Identifies one socket across observations.
    ///
    /// The five-tuple plus the owning PID. A closed socket whose five-tuple is
    /// later reused by a different connection will collide, which is why the
    /// accounting also treats a counter that went backwards as a new socket.
    pub fn key(&self) -> SocketKey {
        SocketKey {
            protocol: self.protocol,
            local_port: self.local_port,
            remote_address: self.remote_address,
            remote_port: self.remote_port,
            pid: self.pid,
        }
    }
}

/// Identity of a socket across observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SocketKey {
    pub protocol: Protocol,
    pub local_port: u16,
    pub remote_address: Option<IpAddr>,
    pub remote_port: Option<u16>,
    pub pid: u32,
}

/// One read of an interface's cumulative counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CounterSample {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// Identity of a running program across observations.
///
/// A PID alone is not an identity: the OS reuses them. Pairing the PID with
/// what the process actually is means a reused PID becomes a different program
/// rather than inheriting the previous one's traffic.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessKey {
    pub pid: u32,
    /// The executable path when it could be resolved, otherwise the reported
    /// name. Never a guess.
    pub identity: String,
}

/// What a program has been observed doing during this session.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProcessActivity {
    pub pid: u32,
    /// The executable's own name.
    pub process_name: String,
    /// The full path, when it resolved.
    pub executable_path: Option<String>,
    /// The application this executable belongs to, when the path proves it.
    /// `None` for anything not inside an application bundle.
    pub application: Option<String>,
    /// True when the reported name was cut short and could not be repaired.
    pub name_is_truncated: bool,

    /// Observed by JRX since monitoring began. Never includes traffic that
    /// happened before JRX was watching.
    pub session_bytes_in: u64,
    pub session_bytes_out: u64,
    /// From the most recent interval.
    pub rate_in: u64,
    pub rate_out: u64,
    pub active_connections: usize,
    /// Samples since this program last moved any bytes.
    pub idle_samples: u32,
    pub connections: Vec<ConnectionActivity>,
}

impl ProcessActivity {
    /// What to show. Prefers the application name, then the executable name.
    pub fn display_name(&self) -> &str {
        self.application.as_deref().unwrap_or(&self.process_name)
    }

    pub fn session_total(&self) -> u64 {
        self.session_bytes_in.saturating_add(self.session_bytes_out)
    }
}

/// One connection belonging to a program.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConnectionActivity {
    pub protocol: Protocol,
    pub remote_address: Option<IpAddr>,
    pub remote_port: Option<u16>,
    pub state: Option<String>,
    pub rtt_ms: Option<f64>,
    /// The organisation that owns the address range, where published
    /// allocation data says so.
    ///
    /// This is not a website, a domain, or a service. One Cloudflare address
    /// fronts millions of sites, and JRX has no way to know which — see
    /// TECH_DECISIONS.md ADR-019.
    pub network_owner: Option<&'static str>,
    pub session_bytes_in: u64,
    pub session_bytes_out: u64,
    /// True while the socket is still present in the latest observation.
    pub is_open: bool,
}

/// How much of the activity picture is currently available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ActivityHealth {
    /// Interface counters and per-program detail both working.
    Full,
    /// Interface counters working; per-program detail is not.
    ///
    /// `reason` is for a diagnostic view, not for the headline: a user should
    /// not be shown a parser error.
    Limited { reason: String },
    /// The per-program provider is still starting up.
    Initializing,
    /// No usable network connection.
    NoNetwork,
}

/// Everything the Activity screen renders.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActivitySnapshot {
    pub interface: String,
    pub health: ActivityHealth,

    /// Bytes JRX has watched move since monitoring began.
    pub session_bytes_in: u64,
    pub session_bytes_out: u64,
    /// From the most recent interval.
    pub rate_in: u64,
    pub rate_out: u64,

    /// What the OS reports the interface has carried since it started
    /// counting. Kept separate from the session totals, because they answer
    /// different questions and conflating them would overstate what JRX saw.
    pub interface_total_in: u64,
    pub interface_total_out: u64,

    pub active_connections: usize,
    pub programs: Vec<ProcessActivity>,

    pub session_duration: Duration,
    pub sample_interval: Duration,
}
