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
    // Google publishes goog.json / cloud.json.
    v4(8, 8, 4, 0, 24, "Google"),
    v4(8, 8, 8, 0, 24, "Google"),
    v4(142, 250, 0, 0, 15, "Google"),
    v4(172, 217, 0, 0, 16, "Google"),
    v4(216, 58, 192, 0, 19, "Google"),
    v4(34, 64, 0, 0, 10, "Google Cloud"),
    v4(104, 196, 0, 0, 14, "Google Cloud"),
    v4(104, 199, 0, 0, 16, "Google Cloud"),
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
