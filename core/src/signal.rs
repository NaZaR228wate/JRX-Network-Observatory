//! Observations. Append-only evidence from which devices are derived.
//!
//! ARCHITECTURE.md §8.2: `Device` records are derived from signals, never the
//! other way round, so every conclusion stays explainable in the Device
//! Inspector.

use crate::declaration::ProbeId;
use serde::Serialize;

/// What kind of fact a signal carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    /// A hardware address observed for a host.
    MacAddress,
    /// An IP address observed for a host.
    IpAddress,
    /// A hostname advertised over mDNS.
    Hostname,
    /// A DNS-SD service type, e.g. `_airplay._tcp`.
    ServiceType,
    /// A UPnP device type or UUID.
    UpnpDevice,
    /// The host answered an ICMP echo.
    Liveness,
}

/// One observation, attributed to the probe that made it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Signal {
    /// Host this signal is about, as observed (usually an IP or MAC).
    pub subject: String,
    pub kind: SignalKind,
    pub value: String,
    pub source: ProbeId,
    /// Milliseconds since the Unix epoch.
    pub observed_at: u64,
    /// Evidence weight used by classification (ARCHITECTURE.md §8.3).
    pub weight: u8,
}
