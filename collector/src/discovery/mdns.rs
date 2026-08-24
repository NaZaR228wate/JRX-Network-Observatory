//! mDNS / DNS-SD discovery.
//!
//! The highest-value source in the product: it is what turns a list of IP
//! addresses into "Living Room Apple TV" and "HP LaserJet"
//! (ARCHITECTURE.md §6.3). The queries are multicast announcements
//! indistinguishable from what macOS, Windows and every phone on the network
//! already send continuously.
//!
//! Note: on macOS 15+ a denied Local Network permission makes this return
//! nothing at all, silently. That is why an empty result is never presented as
//! "no devices" on its own (ARCHITECTURE.md §12).

use std::net::IpAddr;
use std::time::{Duration, Instant};

use mdns_sd::{ServiceDaemon, ServiceEvent};

use crate::probe::ProbeError;

/// Service types worth asking for directly.
///
/// The meta-query below enumerates whatever else exists, but it can be slow to
/// answer. Asking for the common types up front means the map populates in the
/// first second rather than the fifth.
const COMMON_TYPES: &[&str] = &[
    "_airplay._tcp.local.",
    "_raop._tcp.local.",
    "_googlecast._tcp.local.",
    "_hap._tcp.local.",
    "_ipp._tcp.local.",
    "_ipps._tcp.local.",
    "_printer._tcp.local.",
    "_pdl-datastream._tcp.local.",
    "_smb._tcp.local.",
    "_ssh._tcp.local.",
    "_sftp-ssh._tcp.local.",
    "_afpovertcp._tcp.local.",
    "_rfb._tcp.local.",
    "_workstation._tcp.local.",
    "_device-info._tcp.local.",
    "_companion-link._tcp.local.",
    "_apple-mobdev2._tcp.local.",
    "_spotify-connect._tcp.local.",
    "_http._tcp.local.",
    "_adisk._tcp.local.",
];

/// The DNS-SD meta-query: "what service types exist here?"
const META_QUERY: &str = "_services._dns-sd._udp.local.";

/// One resolved service instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdnsService {
    pub address: IpAddr,
    pub hostname: Option<String>,
    /// e.g. `_airplay._tcp`
    pub service_type: String,
}

/// Browse the local network for advertised services.
///
/// `window` bounds the whole operation, including the second round of browsing
/// for types the meta-query discovers.
pub fn discover(window: Duration) -> Result<Vec<MdnsService>, ProbeError> {
    let daemon =
        ServiceDaemon::new().map_err(|e| ProbeError::Failed(format!("mdns daemon: {e}")))?;

    let deadline = Instant::now() + window;
    let mut receivers = Vec::new();

    // Ask for the common types and enumerate everything else at the same time.
    for service_type in COMMON_TYPES {
        if let Ok(receiver) = daemon.browse(service_type) {
            receivers.push(receiver);
        }
    }
    let meta = daemon.browse(META_QUERY).ok();

    let mut found: Vec<MdnsService> = Vec::new();
    let mut extra_types: Vec<String> = Vec::new();

    // First pass: collect resolutions, and note any service types we did not
    // already ask about.
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let slice = remaining.min(Duration::from_millis(150));
        let mut progressed = false;

        if let Some(meta) = &meta
            && let Ok(event) = meta.recv_timeout(slice)
        {
            progressed = true;
            if let ServiceEvent::ServiceFound(_, full_name) = event {
                let discovered = normalise_type(&full_name);
                if !COMMON_TYPES.contains(&discovered.as_str())
                    && !extra_types.contains(&discovered)
                    && discovered.starts_with('_')
                {
                    extra_types.push(discovered);
                }
            }
        }

        for receiver in &receivers {
            while let Ok(event) = receiver.recv_timeout(Duration::from_millis(1)) {
                progressed = true;
                collect(event, &mut found);
            }
        }

        if !progressed {
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    // Second pass: browse the types we learned about, within whatever time is
    // left. Deliberately bounded — a slow network must not stall the UI.
    let second_pass = Duration::from_millis(800);
    let extra_deadline = Instant::now() + second_pass;
    let extra: Vec<_> = extra_types
        .iter()
        .filter_map(|t| daemon.browse(t).ok())
        .collect();

    while Instant::now() < extra_deadline {
        for receiver in &extra {
            while let Ok(event) = receiver.recv_timeout(Duration::from_millis(1)) {
                collect(event, &mut found);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    let _ = daemon.shutdown();
    Ok(found)
}

fn collect(event: ServiceEvent, found: &mut Vec<MdnsService>) {
    let ServiceEvent::ServiceResolved(info) = event else {
        return;
    };

    let service_type = type_from_fullname(info.get_fullname());
    let hostname = clean_hostname(info.get_hostname());

    for address in info.get_addresses_v4() {
        let entry = MdnsService {
            address: IpAddr::V4(address),
            hostname: hostname.clone(),
            service_type: service_type.clone(),
        };
        if !found.contains(&entry) {
            found.push(entry);
        }
    }
}

/// `Apple TV._airplay._tcp.local.` -> `_airplay._tcp`
///
/// The resolved-service record carries only the full instance name, so the
/// service type is the last two labels once the domain is removed. Taking the
/// last two rather than splitting on the first `._` keeps instance names that
/// themselves contain dots intact.
fn type_from_fullname(fullname: &str) -> String {
    let without_domain = normalise_type(fullname);
    let labels: Vec<&str> = without_domain.split('.').collect();
    match labels.as_slice() {
        [.., protocol_owner, protocol] => format!("{protocol_owner}.{protocol}"),
        _ => without_domain,
    }
}

/// `_airplay._tcp.local.` -> `_airplay._tcp`
fn normalise_type(raw: &str) -> String {
    raw.trim_end_matches('.')
        .trim_end_matches(".local")
        .trim_end_matches('.')
        .to_string()
}

/// `Apple-TV.local.` -> `Apple-TV`
fn clean_hostname(raw: &str) -> Option<String> {
    let name = raw
        .trim_end_matches('.')
        .trim_end_matches(".local")
        .trim_end_matches('.');
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_types_are_normalised_for_display_and_matching() {
        assert_eq!(normalise_type("_airplay._tcp.local."), "_airplay._tcp");
        assert_eq!(normalise_type("_hap._tcp.local"), "_hap._tcp");
        assert_eq!(normalise_type("_ssh._tcp"), "_ssh._tcp");
    }

    #[test]
    fn the_service_type_is_recovered_from_the_instance_name() {
        assert_eq!(
            type_from_fullname("Apple TV._airplay._tcp.local."),
            "_airplay._tcp"
        );
        assert_eq!(
            type_from_fullname("HP LaserJet._ipp._tcp.local."),
            "_ipp._tcp"
        );
    }

    /// Instance names may contain dots. Splitting on the first one would
    /// mangle the type and silently lose the device's category.
    #[test]
    fn an_instance_name_containing_dots_still_yields_the_right_type() {
        assert_eq!(
            type_from_fullname("Nazar's Mac. Office._ssh._tcp.local."),
            "_ssh._tcp"
        );
    }

    #[test]
    fn hostnames_lose_the_mdns_suffix() {
        assert_eq!(clean_hostname("Apple-TV.local."), Some("Apple-TV".into()));
        assert_eq!(clean_hostname("nas."), Some("nas".into()));
        assert_eq!(clean_hostname(""), None);
        assert_eq!(clean_hostname("."), None);
    }

    /// The curated list must use fully-qualified names or the daemon silently
    /// browses nothing and mDNS appears to find no devices.
    #[test]
    fn every_common_type_is_fully_qualified() {
        for t in COMMON_TYPES {
            assert!(t.ends_with(".local."), "{t} is not fully qualified");
            assert!(t.starts_with('_'), "{t} is not a service type");
        }
    }

    /// The normalised form of each browsed type must match what classification
    /// looks for, or a discovered printer would never be categorised.
    #[test]
    fn normalised_common_types_match_the_classifier() {
        use jrx_core::device::{Category, DiscoveryMethod, Evidence, EvidenceKind, classify};

        let expect = |raw: &str, want: Category| {
            let evidence = vec![Evidence::new(
                EvidenceKind::ServiceType,
                normalise_type(raw),
                DiscoveryMethod::Mdns,
            )];
            assert_eq!(classify(&evidence).0, want, "{raw}");
        };

        expect("_ipp._tcp.local.", Category::SmartHome);
        expect("_hap._tcp.local.", Category::SmartHome);
        expect("_ssh._tcp.local.", Category::Computers);
        expect("_apple-mobdev2._tcp.local.", Category::Phones);
    }
}
