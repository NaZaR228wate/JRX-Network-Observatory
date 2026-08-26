//! M5 phase 0: what this Mac's own live network activity looks like.
//!
//! Feasibility spike. Everything here was proven against this machine before
//! it was written; nothing is designed around an assumption. See
//! `docs/M5_PHASE0_FEASIBILITY.md` for the measurements.
//!
//! Constraints, unchanged from the rest of JRX: unprivileged, no packet
//! capture, no payload inspection, no TLS interception, nothing leaves the
//! machine.

#![cfg(feature = "activity-spike")]

pub mod nettop;
pub mod observe;
pub mod owner;
pub mod throughput;

use std::net::IpAddr;
use std::time::Duration;

use serde::Serialize;

/// Transport protocol, as the OS reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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

/// The process a connection belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcessRef {
    pub pid: u32,
    /// As `nettop` reports it. Truncated to 15 characters by the tool, which
    /// is why the full name is resolved separately.
    pub reported_name: String,
    /// Resolved from the PID. `None` when the process has already exited —
    /// never guessed from the truncated name.
    pub full_name: Option<String>,
}

impl ProcessRef {
    /// What to show. Prefers the resolved name, falls back to the truncated
    /// one, and says which it is rather than pretending the short form is
    /// complete.
    pub fn display(&self) -> &str {
        self.full_name.as_deref().unwrap_or(&self.reported_name)
    }

    pub fn name_is_truncated(&self) -> bool {
        self.full_name.is_none() && self.reported_name.chars().count() >= nettop::NAME_LIMIT
    }
}

/// What can honestly be said about the other end of a connection.
///
/// The fields are deliberately separate. A network owner is not a hostname,
/// and a hostname is not a website. Collapsing them is how a tool starts
/// claiming to know where someone has been.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteEndpoint {
    pub address: IpAddr,
    pub port: u16,
    /// The organisation that owns this address, from published allocation
    /// data. Never derived from a hostname, and never treated as a site the
    /// user visited: one Cloudflare address fronts millions of sites.
    pub network_owner: Option<&'static str>,
    /// Always `None` in this phase. Reverse DNS was measured on this machine
    /// and returned nothing for 12 of 12 real endpoints, and would not be
    /// evidence of a visited site even when it does resolve.
    pub hostname: Option<String>,
}

/// One socket, as observed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Connection {
    pub protocol: Protocol,
    pub local_address: IpAddr,
    pub local_port: u16,
    /// `None` for a listening socket with no peer.
    pub remote: Option<RemoteEndpoint>,
    /// "Established", "Listen", and so on. `None` for UDP.
    pub state: Option<String>,
    /// Bytes this socket has carried. Measured by the OS, never inferred from
    /// the fact that a connection exists.
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub process: ProcessRef,
}

/// Everything the activity view could truthfully show for this Mac.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivitySnapshot {
    pub interface: String,
    /// Rates, from the difference between two counter reads.
    pub down_bytes_per_sec: u64,
    pub up_bytes_per_sec: u64,
    /// Totals the interface has carried since the OS started counting.
    pub total_rx: u64,
    pub total_tx: u64,
    pub connections: Vec<Connection>,
    /// How long the rate was measured over. A rate without its window is not
    /// a measurement.
    pub sampled_over: Duration,
}

impl ActivitySnapshot {
    pub fn established(&self) -> impl Iterator<Item = &Connection> {
        self.connections
            .iter()
            .filter(|c| c.remote.is_some() && c.state.as_deref() != Some("Listen"))
    }
}
