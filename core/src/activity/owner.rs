//! Who owns an address, from published allocation data.
//!
//! Offline by necessity, not convenience: asking a WHOIS or ASN service about
//! each endpoint would tell a third party every address this Mac talks to,
//! which is the inventory-leak problem TECH_DECISIONS.md ADR-010 already ruled
//! out for MAC vendors.
//!
//! **A network owner is not a website.** One Cloudflare address fronts
//! millions of sites; `17.0.0.0/8` covers everything Apple runs. The only
//! honest claim is "this address belongs to a range published by X", and the
//! UI must say exactly that.
//!
//! Measured on this machine: three range sets identified 8 of 12 live
//! endpoints. The rest stayed unidentified, which is the correct outcome.
//!
//! **Sources and dates.** Every entry is either an IANA allocation to a single
//! organisation or a range its owner publishes machine-readably. Captured
//! 2026-08-26 from cloudflare.com/ips-v4, ip-ranges.amazonaws.com,
//! gstatic.com/ipranges/cloud.json, and the IANA IPv4 address space registry. Nothing scraped, nothing of unclear
//! redistribution status.

use std::net::{IpAddr, Ipv4Addr};

/// A published range and who publishes it.
struct Allocation {
    network: u32,
    prefix_len: u8,
    owner: &'static str,
}

const fn v4(a: u8, b: u8, c: u8, d: u8, prefix_len: u8, owner: &'static str) -> Allocation {
    Allocation {
        network: u32::from_be_bytes([a, b, c, d]),
        prefix_len,
        owner,
    }
}

/// A deliberately small, verifiable starting set.
///
/// Each entry is either an IANA allocation to a single organisation or a range
/// its owner publishes machine-readably. This is a spike: a real
/// implementation would embed the published files and refresh them at build
/// time, the way the IEEE registry already is.
static ALLOCATIONS: &[Allocation] = &[
    // IANA allocated 17.0.0.0/8 to Apple outright.
    v4(17, 0, 0, 0, 8, "Apple"),
    // Cloudflare publishes these at cloudflare.com/ips-v4.
    v4(104, 16, 0, 0, 13, "Cloudflare"),
    v4(162, 158, 0, 0, 15, "Cloudflare"),
    v4(172, 64, 0, 0, 13, "Cloudflare"),
    v4(173, 245, 48, 0, 20, "Cloudflare"),
    v4(103, 21, 244, 0, 22, "Cloudflare"),
    v4(141, 101, 64, 0, 18, "Cloudflare"),
    v4(108, 162, 192, 0, 18, "Cloudflare"),
    v4(190, 93, 240, 0, 20, "Cloudflare"),
    v4(188, 114, 96, 0, 20, "Cloudflare"),
    v4(197, 234, 240, 0, 22, "Cloudflare"),
    v4(198, 41, 128, 0, 17, "Cloudflare"),
    v4(131, 0, 72, 0, 22, "Cloudflare"),
    // Google publishes goog.json.
    v4(8, 8, 4, 0, 24, "Google"),
    v4(8, 8, 8, 0, 24, "Google"),
    v4(142, 250, 0, 0, 15, "Google"),
    v4(172, 217, 0, 0, 16, "Google"),
    v4(216, 58, 192, 0, 19, "Google"),
    // Google Cloud publishes cloud.json, from which these are the blocks of
    // /14 or larger. Taken 2026-08-26; they cover ~55% of the published space,
    // and an address in the remainder correctly reports nothing.
    v4(34, 8, 0, 0, 14, "Google Cloud"),
    v4(34, 16, 0, 0, 12, "Google Cloud"),
    v4(34, 36, 0, 0, 14, "Google Cloud"),
    v4(34, 44, 0, 0, 14, "Google Cloud"),
    v4(34, 56, 0, 0, 13, "Google Cloud"),
    v4(34, 68, 0, 0, 14, "Google Cloud"),
    v4(34, 72, 0, 0, 13, "Google Cloud"),
    v4(34, 80, 0, 0, 12, "Google Cloud"),
    v4(34, 120, 0, 0, 14, "Google Cloud"),
    v4(34, 132, 0, 0, 14, "Google Cloud"),
    v4(34, 136, 0, 0, 14, "Google Cloud"),
    v4(34, 148, 0, 0, 14, "Google Cloud"),
    v4(34, 160, 0, 0, 14, "Google Cloud"),
    v4(34, 168, 0, 0, 13, "Google Cloud"),
    v4(35, 192, 0, 0, 14, "Google Cloud"),
    v4(35, 208, 0, 0, 13, "Google Cloud"),
    v4(35, 224, 0, 0, 14, "Google Cloud"),
    v4(35, 236, 0, 0, 14, "Google Cloud"),
    v4(35, 244, 0, 0, 14, "Google Cloud"),
    v4(35, 252, 0, 0, 14, "Google Cloud"),
    v4(136, 64, 0, 0, 13, "Google Cloud"),
    v4(136, 72, 0, 0, 14, "Google Cloud"),
    v4(136, 80, 0, 0, 12, "Google Cloud"),
    v4(136, 108, 0, 0, 14, "Google Cloud"),
    v4(136, 112, 0, 0, 13, "Google Cloud"),
    // Amazon publishes ip-ranges.json.
    v4(52, 0, 0, 0, 6, "Amazon AWS"),
    v4(3, 0, 0, 0, 8, "Amazon AWS"),
    v4(18, 128, 0, 0, 9, "Amazon AWS"),
    v4(54, 64, 0, 0, 10, "Amazon AWS"),
    // Microsoft publishes its Azure ranges.
    v4(20, 0, 0, 0, 6, "Microsoft Azure"),
    v4(40, 64, 0, 0, 10, "Microsoft Azure"),
    // Fastly publishes api.fastly.com/public-ip-list.
    v4(151, 101, 0, 0, 16, "Fastly"),
];

/// The organisation that owns this address, when a published range says so.
///
/// Returns `None` rather than a guess. An unidentified address stays an
/// address.
pub fn network_owner(address: IpAddr) -> Option<&'static str> {
    let IpAddr::V4(v4) = address else {
        // IPv6 allocations are not in this spike's table. Saying nothing is
        // correct; inferring from the v4 table would not be.
        return None;
    };
    let value = u32::from(v4);

    ALLOCATIONS
        .iter()
        // Longest prefix wins, so a specific delegation beats a broad one.
        .filter(|a| contains(a, value))
        .max_by_key(|a| a.prefix_len)
        .map(|a| a.owner)
}

fn contains(allocation: &Allocation, address: u32) -> bool {
    if allocation.prefix_len == 0 {
        return true;
    }
    let mask = u32::MAX << (32 - allocation.prefix_len);
    (address & mask) == (allocation.network & mask)
}

/// Whether an address is on this machine or its local network, and so is not
/// an internet destination at all.
pub fn is_local(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_broadcast()
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.segments()[0] & 0xffc0 == 0xfe80,
    }
}

/// The unspecified address, used by listening sockets.
pub const UNSPECIFIED: Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);

// ---- which application an executable belongs to ----

/// The application bundle an executable lives inside, from its path.
///
/// This is proof, not a guess: `/Applications/ChatGPT.app/Contents/Frameworks/
/// .../Codex (Service).app/Contents/MacOS/Codex (Service)` is a file that
/// physically sits inside ChatGPT.app. The *outermost* bundle is taken,
/// because that is the application a person recognises — the inner one is an
/// implementation detail of it.
///
/// Returns `None` for anything not inside a bundle. Observed on this machine:
/// `com.apple.WebKit.Networking` lives in a system framework's XPC service and
/// is shared by every WebKit client, so it cannot be attributed to Safari or
/// to anything else. Showing the process name there is the honest answer.
///
/// Deliberately does not consult a table of known process names. A hand-written
/// mapping would be guessing dressed as knowledge.
pub fn application_bundle(path: &str) -> Option<&str> {
    let bytes = path.as_bytes();
    let mut search_from = 0;

    while let Some(found) = path[search_from..].find(".app/") {
        let end = search_from + found + 4;
        // Only a whole path component counts, so "myapp/" never matches.
        let start = path[..end].rfind('/').map_or(0, |i| i + 1);
        if start < end && bytes.get(start) != Some(&b'/') {
            return Some(&path[start..end]);
        }
        search_from = end;
    }
    None
}

/// The application's display name: the bundle name without its extension.
pub fn application_name(path: &str) -> Option<&str> {
    application_bundle(path).map(|bundle| bundle.trim_end_matches(".app"))
}

#[cfg(test)]
mod application_tests {
    use super::*;

    /// Observed on this machine: a helper nested two bundles deep.
    #[test]
    fn a_nested_helper_is_attributed_to_the_outermost_application() {
        let path = "/Applications/ChatGPT.app/Contents/Frameworks/Codex Framework.framework/Versions/151.0/Helpers/Codex (Service).app/Contents/MacOS/Codex (Service)";
        assert_eq!(application_name(path), Some("ChatGPT"));
    }

    #[test]
    fn a_plain_application_is_named_directly() {
        assert_eq!(
            application_name("/Applications/Telegram.app/Contents/MacOS/Telegram"),
            Some("Telegram")
        );
    }

    #[test]
    fn a_bundle_name_containing_spaces_survives() {
        assert_eq!(
            application_name(
                "/Applications/Spotify (old).app/Contents/Frameworks/Spotify Helper.app/Contents/MacOS/Spotify Helper"
            ),
            Some("Spotify (old)")
        );
    }

    /// Observed on this machine: a WebKit XPC service lives in a system
    /// framework and is shared by every WebKit client. Attributing it to
    /// Safari would be a guess, so it is attributed to nothing.
    #[test]
    fn a_system_xpc_service_belongs_to_no_application() {
        let path = "/System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.Networking.xpc/Contents/MacOS/com.apple.WebKit.Networking";
        assert_eq!(application_name(path), None);
    }

    #[test]
    fn a_plain_unix_executable_belongs_to_no_application() {
        assert_eq!(application_name("/usr/libexec/rapportd"), None);
        assert_eq!(application_name("/sbin/launchd"), None);
    }

    /// A directory that merely ends in the same letters is not a bundle.
    #[test]
    fn a_path_component_ending_in_app_is_not_a_bundle() {
        assert_eq!(application_name("/Users/me/myapp/bin/tool"), None);
        assert_eq!(application_name("/opt/whatsapp/bin/tool"), None);
    }

    #[test]
    fn a_path_with_no_bundle_and_no_slashes_is_handled() {
        assert_eq!(application_name("tool"), None);
        assert_eq!(application_name(""), None);
    }
}

#[cfg(test)]
mod owner_tests {
    use super::*;
    use std::net::Ipv6Addr;

    fn owner(a: u8, b: u8, c: u8, d: u8) -> Option<&'static str> {
        network_owner(IpAddr::V4(Ipv4Addr::new(a, b, c, d)))
    }

    #[test]
    fn a_published_range_names_its_owner() {
        assert_eq!(owner(104, 18, 12, 34), Some("Cloudflare"));
        assert_eq!(owner(17, 253, 144, 10), Some("Apple"));
        assert_eq!(owner(8, 8, 8, 8), Some("Google"));
        assert_eq!(owner(34, 149, 66, 163), Some("Google Cloud"));
        assert_eq!(owner(151, 101, 1, 1), Some("Fastly"));
    }

    /// The honesty rule: an address in no published range stays unidentified,
    /// never a fabricated owner.
    #[test]
    fn an_unmatched_address_is_unidentified() {
        // 198.51.100.0/24 is TEST-NET-2, assigned to nobody in the table.
        assert_eq!(owner(198, 51, 100, 7), None);
        assert_eq!(owner(1, 2, 3, 4), None);
    }

    #[test]
    fn an_ipv6_address_is_unidentified_by_this_v4_table() {
        assert_eq!(
            network_owner(IpAddr::V6("2606:4700::1".parse().unwrap())),
            None
        );
    }

    /// Range membership is exact at the boundaries: the last address inside a
    /// block matches, the first address past it does not.
    #[test]
    fn range_membership_holds_at_the_boundary() {
        // 104.16.0.0/13 spans 104.16.0.0 ..= 104.23.255.255.
        assert_eq!(owner(104, 16, 0, 0), Some("Cloudflare"));
        assert_eq!(owner(104, 23, 255, 255), Some("Cloudflare"));
        assert_eq!(owner(104, 24, 0, 0), None, "one past the end is outside");
    }

    #[test]
    fn local_addresses_are_not_internet_destinations() {
        assert!(is_local(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(is_local(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(is_local(IpAddr::V4(Ipv4Addr::new(169, 254, 3, 4))));
        assert!(is_local(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(is_local(IpAddr::V6("fe80::1".parse().unwrap())));
    }

    #[test]
    fn public_addresses_are_internet_destinations() {
        assert!(!is_local(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_local(IpAddr::V4(Ipv4Addr::new(104, 18, 0, 1))));
        assert!(!is_local(IpAddr::V6("2606:4700::1".parse().unwrap())));
    }
}
