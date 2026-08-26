//! How much to trust this run of discovery.
//!
//! The question this module answers is the one that decides whether an empty
//! screen is honest or broken:
//!
//! > "No devices found because the network blocks discovery"
//! > versus
//! > "No devices found because there are no devices"
//!
//! Those are different statements and JRX must not conflate them
//! (ARCHITECTURE.md §12).

use jrx_core::device::DiscoveryMethod;
use serde::Serialize;

pub use super::SourceStatus;

/// What one source produced, in detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceQuality {
    pub method: DiscoveryMethod,
    pub label: &'static str,
    pub status: SourceStatus,
    pub observations: usize,
    /// mDNS: distinct hostnames resolved.
    pub names_resolved: usize,
    /// mDNS: distinct service types seen.
    pub services_seen: usize,
}

impl SourceQuality {
    fn ran(&self) -> bool {
        matches!(self.status, SourceStatus::Ok { .. })
    }

    fn failure(&self) -> Option<&str> {
        match &self.status {
            SourceStatus::Failed { reason } | SourceStatus::Refused { reason } => Some(reason),
            SourceStatus::Ok { .. } => None,
        }
    }
}

/// The overall standing of a discovery run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryVerdict {
    /// Devices found, every source working.
    Healthy,
    /// Devices found, but a source failed — the picture may be incomplete.
    Degraded,
    /// Nothing found, and a source failed. The emptiness is ours, not the
    /// network's, and must never be reported as "no devices".
    DiscoveryBlocked,
    /// Nothing found, and every source ran. The network really does look
    /// empty, or it isolates its clients.
    NetworkAppearsEmpty,
}

/// Whether macOS Local Network access appears to be working.
///
/// macOS provides no API to ask (TECH_DECISIONS.md ADR-002 discussion in M2),
/// and a denial is silent. This is the behavioural answer M2 promised: it is
/// an inference from evidence, labelled as one, never a reported status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalNetworkInference {
    /// mDNS returned something, so access demonstrably works.
    Working,
    /// Neighbours are demonstrably present, yet mDNS heard nothing at all.
    /// Devices that exist are not announcing themselves to us.
    LikelyBlocked,
    /// Not enough evidence to say either way.
    Undetermined,
}

/// The full quality picture for one discovery run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscoveryQuality {
    pub verdict: DiscoveryVerdict,
    /// Written for a person, naming any specific failure.
    pub explanation: String,
    pub sources: Vec<SourceQuality>,
    pub local_network: LocalNetworkInference,
}

/// Neighbours below this count prove nothing about mDNS silence: a quiet
/// network is quiet for ordinary reasons.
const CROWD_THRESHOLD: usize = 3;

/// Judge a discovery run.
pub fn assess(sources: &[SourceQuality], devices_found: usize) -> DiscoveryQuality {
    let failures: Vec<&str> = sources.iter().filter_map(SourceQuality::failure).collect();

    let verdict = match (devices_found > 0, failures.is_empty()) {
        (true, true) => DiscoveryVerdict::Healthy,
        (true, false) => DiscoveryVerdict::Degraded,
        (false, false) => DiscoveryVerdict::DiscoveryBlocked,
        (false, true) => DiscoveryVerdict::NetworkAppearsEmpty,
    };

    let explanation = match verdict {
        DiscoveryVerdict::Healthy => format!(
            "{devices_found} other {} observed. Every discovery source worked.",
            if devices_found == 1 { "device" } else { "devices" }
        ),
        DiscoveryVerdict::Degraded => format!(
            "{devices_found} other {} observed, but the picture may be \
             incomplete: {}",
            if devices_found == 1 { "device" } else { "devices" },
            failures.join("; ")
        ),
        DiscoveryVerdict::DiscoveryBlocked => format!(
            "No devices could be found, and that is a fault on our side rather              than an empty network: {}",
            failures.join("; ")
        ),
        DiscoveryVerdict::NetworkAppearsEmpty => {
            "No other devices answered. Every discovery source ran without error,              so either this network keeps its devices apart from each other, or              there is genuinely nothing else here."
                .to_string()
        }
    };

    DiscoveryQuality {
        verdict,
        explanation,
        sources: sources.to_vec(),
        local_network: infer_local_network(sources),
    }
}

/// Read Local Network access from behaviour.
fn infer_local_network(sources: &[SourceQuality]) -> LocalNetworkInference {
    // An outright refusal is the strongest evidence there is: the request was
    // rejected before it reached the network. It outranks another source that
    // appeared to work.
    if sources
        .iter()
        .any(|s| matches!(s.status, SourceStatus::Refused { .. }))
    {
        return LocalNetworkInference::LikelyBlocked;
    }

    let find = |method: DiscoveryMethod| sources.iter().find(|s| s.method == method);

    let Some(mdns) = find(DiscoveryMethod::Mdns) else {
        return LocalNetworkInference::Undetermined;
    };

    // A probe that errored tells us about our own code, not about permission.
    if !mdns.ran() {
        return LocalNetworkInference::Undetermined;
    }
    if mdns.observations > 0 {
        return LocalNetworkInference::Working;
    }

    // Silence only means something when we can prove others are present.
    let neighbours = find(DiscoveryMethod::ArpCache)
        .filter(|arp| arp.ran())
        .map_or(0, |arp| arp.observations);

    if neighbours >= CROWD_THRESHOLD {
        LocalNetworkInference::LikelyBlocked
    } else {
        LocalNetworkInference::Undetermined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(method: DiscoveryMethod, observations: usize) -> SourceQuality {
        SourceQuality {
            method,
            label: method.label(),
            status: SourceStatus::Ok { observations },
            observations,
            names_resolved: 0,
            services_seen: 0,
        }
    }
    fn failed(method: DiscoveryMethod, why: &str) -> SourceQuality {
        SourceQuality {
            method,
            label: method.label(),
            status: SourceStatus::Failed { reason: why.into() },
            observations: 0,
            names_resolved: 0,
            services_seen: 0,
        }
    }

    /// The distinction the whole module exists for: an empty result means
    /// something different depending on whether our sources worked.
    #[test]
    fn nothing_found_with_working_sources_means_the_network_looks_empty() {
        let quality = assess(
            &[
                source(DiscoveryMethod::ArpCache, 0),
                source(DiscoveryMethod::Mdns, 0),
                source(DiscoveryMethod::Ssdp, 0),
            ],
            0,
        );
        assert_eq!(quality.verdict, DiscoveryVerdict::NetworkAppearsEmpty);
    }

    #[test]
    fn nothing_found_with_a_failed_source_is_not_blamed_on_the_network() {
        let quality = assess(
            &[
                failed(DiscoveryMethod::ArpCache, "arp: permission denied"),
                source(DiscoveryMethod::Mdns, 0),
                source(DiscoveryMethod::Ssdp, 0),
            ],
            0,
        );
        assert_eq!(quality.verdict, DiscoveryVerdict::DiscoveryBlocked);
        assert!(
            quality.explanation.contains("arp: permission denied"),
            "the failure must be named, got: {}",
            quality.explanation
        );
    }

    #[test]
    fn devices_found_with_every_source_working_is_healthy() {
        let quality = assess(
            &[
                source(DiscoveryMethod::ArpCache, 40),
                source(DiscoveryMethod::Mdns, 12),
                source(DiscoveryMethod::Ssdp, 3),
            ],
            30,
        );
        assert_eq!(quality.verdict, DiscoveryVerdict::Healthy);
    }

    #[test]
    fn devices_found_but_a_source_failed_is_degraded_not_healthy() {
        let quality = assess(
            &[
                source(DiscoveryMethod::ArpCache, 40),
                failed(DiscoveryMethod::Ssdp, "ssdp send: No route to host"),
            ],
            30,
        );
        assert_eq!(quality.verdict, DiscoveryVerdict::Degraded);
        assert!(quality.explanation.contains("No route to host"));
    }

    // ---- the M2 promise, delivered behaviourally ----

    /// macOS refuses to say whether Local Network access is granted. But if
    /// the ARP cache is full of neighbours and mDNS heard absolutely nothing,
    /// that is the signature of a denial — devices that are demonstrably
    /// present are not announcing themselves to us.
    #[test]
    fn a_full_arp_cache_with_silent_mdns_reads_as_a_blocked_permission() {
        let quality = assess(
            &[
                source(DiscoveryMethod::ArpCache, 40),
                source(DiscoveryMethod::Mdns, 0),
            ],
            40,
        );
        assert_eq!(quality.local_network, LocalNetworkInference::LikelyBlocked);
    }

    #[test]
    fn any_mdns_result_proves_local_network_access_is_working() {
        let quality = assess(
            &[
                source(DiscoveryMethod::ArpCache, 40),
                source(DiscoveryMethod::Mdns, 1),
            ],
            40,
        );
        assert_eq!(quality.local_network, LocalNetworkInference::Working);
    }

    /// On a network with nothing else on it, silence from mDNS proves nothing.
    /// Claiming a blocked permission here would be inventing a finding.
    #[test]
    fn silence_on_an_empty_network_is_not_evidence_of_a_block() {
        let quality = assess(
            &[
                source(DiscoveryMethod::ArpCache, 1),
                source(DiscoveryMethod::Mdns, 0),
            ],
            1,
        );
        assert_eq!(quality.local_network, LocalNetworkInference::Undetermined);
    }

    /// A source the OS refused outright is the strongest evidence available
    /// that local network access is blocked — stronger than silence, because
    /// the refusal happened before anything reached the network.
    #[test]
    fn a_refused_source_outweighs_a_source_that_seemed_to_work() {
        let quality = assess(
            &[
                source(DiscoveryMethod::ArpCache, 139),
                // mDNS returned something, which on its own would read as
                // working...
                source(DiscoveryMethod::Mdns, 3),
                SourceQuality {
                    status: SourceStatus::Refused {
                        reason: "local network access appears to be blocked".into(),
                    },
                    ..source(DiscoveryMethod::Ssdp, 0)
                },
            ],
            139,
        );

        assert_eq!(
            quality.local_network,
            LocalNetworkInference::LikelyBlocked,
            "a refusal must not be overridden by another source appearing to work"
        );
        assert_eq!(quality.verdict, DiscoveryVerdict::Degraded);
        assert!(quality.explanation.contains("blocked"));
    }

    #[test]
    fn a_failed_mdns_probe_is_not_reported_as_a_permission_problem() {
        let quality = assess(
            &[
                source(DiscoveryMethod::ArpCache, 40),
                failed(DiscoveryMethod::Mdns, "mdns daemon: address in use"),
            ],
            40,
        );
        assert_eq!(quality.local_network, LocalNetworkInference::Undetermined);
    }

    #[test]
    fn every_verdict_carries_an_explanation_a_person_can_read() {
        for quality in [
            assess(&[source(DiscoveryMethod::ArpCache, 0)], 0),
            assess(&[source(DiscoveryMethod::ArpCache, 40)], 30),
            assess(&[failed(DiscoveryMethod::ArpCache, "boom")], 0),
        ] {
            assert!(!quality.explanation.is_empty(), "{:?}", quality.verdict);
        }
    }
}
