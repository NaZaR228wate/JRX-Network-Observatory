//! What a probe declares about itself.
//!
//! `ProbeDeclaration` is the machine-readable contract described in
//! ARCHITECTURE.md §6.1. It is not documentation: the Visibility Panel is
//! generated from it, and `crate::invariants` audits it. A probe that reads
//! something it did not declare fails the build.

use crate::data_class::DataClass;
use serde::{Deserialize, Serialize};

/// Stable identifier for a probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeId {
    Interfaces,
    Routes,
    Wifi,
    Arp,
    IfCounters,
    Sockets,
    Mdns,
    Ssdp,
    IcmpSweep,
}

/// Whether a probe emits anything onto the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Posture {
    /// Reads local state, or listens. Emits nothing beyond ordinary LAN
    /// participation.
    Passive,
    /// Emits probes onto the network. Never runs automatically; requires
    /// per-invocation consent and is posture-gated (ARCHITECTURE.md §6.4).
    Active,
}

/// An operating-system permission a probe depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// macOS Sequoia+ gate on local network access. Denial makes mDNS and
    /// SSDP return silently empty — risk #1 in MVP_ROADMAP.md §6.
    LocalNetwork,
    /// macOS Sonoma+ requires this merely to read the current SSID.
    LocationServices,
}

impl Permission {
    /// Human-readable name, shown inline in the Visibility Panel.
    pub fn label(self) -> &'static str {
        match self {
            Permission::LocalNetwork => "Local Network",
            Permission::LocationServices => "Location Services",
        }
    }
}

/// A platform JRX runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    MacOs,
    Windows,
}

impl Platform {
    /// The platform this binary was compiled for.
    pub const fn current() -> Option<Platform> {
        if cfg!(target_os = "macos") {
            Some(Platform::MacOs)
        } else if cfg!(target_os = "windows") {
            Some(Platform::Windows)
        } else {
            None
        }
    }
}

/// A probe's self-description. Every field is rendered or audited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProbeDeclaration {
    pub id: ProbeId,
    pub posture: Posture,
    /// What the user gains, in their words. "Device names and services".
    pub describes: &'static str,
    /// How it is obtained, named honestly. "mDNS service discovery".
    pub mechanism: &'static str,
    /// Permissions without which this probe cannot run.
    pub requires: &'static [Permission],
    /// The complete set of data classes this probe reads. Audited.
    pub reads: &'static [DataClass],
    /// Platforms where this probe is implementable at all.
    pub platforms: &'static [Platform],
}
