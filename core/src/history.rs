//! Recognition — "have we seen this network, or this device, before?"
//!
//! Pure logic. This module never touches a database, a clock, or the OS: it
//! turns a live observation into a stable **key**, and turns a key plus a
//! yes/no answer from storage into an honest **recognition**. The storage that
//! remembers keys between runs lives in the platform layer (ADR-021); this is
//! the brain, and it is reused unchanged by a future mobile client
//! (ARCHITECTURE.md §17).
//!
//! Two honesty rules shape everything here:
//!
//! 1. **A key is only as trustworthy as the evidence it came from.** A match on
//!    an access point's own address is confident; a match on "same subnet and
//!    resolvers" is merely likely, because many networks share `192.168.1.0/24`.
//!    The key carries that distinction so the UI can, too.
//! 2. **A randomised hardware address is never recognised.** Modern phones
//!    rotate their MAC per network specifically to defeat tracking. Treating a
//!    new random address as a "new device" would cry wolf on every phone that
//!    walks past. Such a device is *undeterminable*, not new — the same stance
//!    `device::merge_decision` already takes (ADR-008).

use serde::Serialize;

use crate::device::{Identity, MacAddress};
use crate::network::{NetworkIdentity, WifiStatus};

/// A stable, privacy-preserving identifier for a network.
///
/// It is a one-way digest of the strongest stable signal available, so storing
/// it recognises a network on return **without** storing its name, its access
/// point's address, or your local addressing. The digest is a fingerprint, not
/// a cryptographic commitment — it identifies a network to JRX's own local
/// store, and is never transmitted (ADR-021).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct NetworkKey {
    /// 16 hex characters. The stored, comparable identity.
    pub digest: String,
    /// What the digest was derived from, and therefore how far to trust a match.
    pub strength: KeyStrength,
}

/// How defensible a network match is, by what its key was derived from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyStrength {
    /// A hardware address unique to this network — the Wi-Fi access point's
    /// BSSID, or the gateway's own MAC. A match is confident.
    Hardware,
    /// Only the addressing layout (subnet plus resolvers) was available. Many
    /// distinct networks share one; a match is possible, not certain.
    Addressing,
}

/// The answer to "have I been on this network before?", carrying the honesty of
/// the evidence with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkRecognition {
    /// No stored key matched. JRX has not seen this network before.
    FirstTime,
    /// A stored key matched on a hardware identity. This is the same network.
    Returning,
    /// A stored key matched, but only on addressing. A network *like* this one
    /// was seen before; JRX cannot be certain it is the same one.
    ReturningLikely,
}

/// Where a device stands relative to what JRX has seen on this network before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStanding {
    /// A stable hardware identity not seen on this network before.
    New,
    /// A stable hardware identity seen here before.
    Known,
    /// No stable identity to judge by — a randomised or absent MAC. Reported as
    /// undeterminable rather than guessed either way.
    CannotDetermine,
}

/// Derive the stable key for the current network, strongest signal first.
///
/// `gateway_mac` is the router's hardware address if discovery resolved it; it
/// is what makes a wired network (no BSSID) recognisable. Returns `None` when
/// nothing stable is available — e.g. no network at all — in which case there is
/// simply nothing to remember, which is the honest result.
pub fn network_key(
    identity: &NetworkIdentity,
    gateway_mac: Option<MacAddress>,
) -> Option<NetworkKey> {
    // 1. The access point's own address, when Wi-Fi is associated and macOS
    //    let us read the BSSID. Normalised through MacAddress so casing and the
    //    short `arp` form collapse to one canonical value.
    if let WifiStatus::Associated(wifi) = &identity.wifi
        && let Some(bssid) = wifi.bssid.as_deref().and_then(MacAddress::parse)
    {
        return Some(NetworkKey {
            digest: digest(&format!("n:bssid:{bssid}")),
            strength: KeyStrength::Hardware,
        });
    }

    // 2. The gateway's own hardware address — the wired-network equivalent.
    if let Some(mac) = gateway_mac {
        return Some(NetworkKey {
            digest: digest(&format!("n:gwmac:{mac}")),
            strength: KeyStrength::Hardware,
        });
    }

    // 3. Addressing only. Weaker, and labelled so. Resolvers are sorted so the
    //    order the OS happened to report them in does not change the key.
    if let Some(subnet) = &identity.subnet {
        let mut dns: Vec<String> = identity.dns_servers.iter().map(|d| d.to_string()).collect();
        dns.sort();
        let canonical = format!(
            "n:addr:{}/{};dns={}",
            subnet.network,
            subnet.prefix_len,
            dns.join(",")
        );
        return Some(NetworkKey {
            digest: digest(&canonical),
            strength: KeyStrength::Addressing,
        });
    }

    None
}

/// Turn a derived key plus storage's yes/no into an honest recognition.
///
/// `is_known` is the caller's lookup of `key.digest` in the local store; keeping
/// it a plain boolean is what lets this stay pure and fully testable.
pub fn recognise_network(key: &NetworkKey, is_known: bool) -> NetworkRecognition {
    match (is_known, key.strength) {
        (false, _) => NetworkRecognition::FirstTime,
        (true, KeyStrength::Hardware) => NetworkRecognition::Returning,
        (true, KeyStrength::Addressing) => NetworkRecognition::ReturningLikely,
    }
}

/// The stable key for a device, or `None` when it has no stable identity.
///
/// Only a real, globally-administered MAC qualifies. A randomised MAC is
/// deliberately excluded: it is not a lasting identity, and remembering it would
/// be remembering noise.
pub fn device_key(identity: &Identity) -> Option<String> {
    identity
        .mac
        .filter(|mac| !mac.is_randomised() && mac.is_device_address())
        .map(|mac| digest(&format!("d:mac:{mac}")))
}

/// Where a device stands on this network, given storage's lookup of its key.
///
/// `is_known` is called with the device's key only when one exists; a device
/// with no stable key is `CannotDetermine` without consulting storage at all.
pub fn device_standing(identity: &Identity, is_known: impl FnOnce(&str) -> bool) -> DeviceStanding {
    match device_key(identity) {
        Some(key) if is_known(&key) => DeviceStanding::Known,
        Some(_) => DeviceStanding::New,
        None => DeviceStanding::CannotDetermine,
    }
}

/// FNV-1a over the canonical string, rendered as 16 hex characters. Stable
/// across runs and Rust versions (unlike `DefaultHasher`), which is what lets a
/// digest written today still match one derived tomorrow.
fn digest(canonical: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in canonical.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::{ConnectionType, Subnet, WifiDetails};
    use std::net::{IpAddr, Ipv4Addr};

    fn base() -> NetworkIdentity {
        NetworkIdentity {
            connection: ConnectionType::Unknown,
            interface: "en0".into(),
            interface_label: None,
            local_ip: None,
            subnet: None,
            gateway: None,
            dns_servers: vec![],
            wifi: WifiStatus::NotAssociated,
            tunnel: None,
            other_active: vec![],
        }
    }

    fn associated(bssid: Option<&str>) -> WifiStatus {
        WifiStatus::Associated(WifiDetails {
            bssid: bssid.map(str::to_owned),
            ..Default::default()
        })
    }

    fn subnet(a: u8, b: u8, c: u8) -> Subnet {
        Subnet {
            network: Ipv4Addr::new(a, b, c, 0),
            prefix_len: 24,
        }
    }

    // --- network keys ---

    /// Golden digests. These pin both the hash function and the exact canonical
    /// string a key is built from — and that string is a stored-data contract:
    /// changing it would orphan every recognition record already on disk. A
    /// deliberate change means updating these values on purpose; an accidental
    /// one (or a mutation of the hash) fails here.
    #[test]
    fn digests_are_stable_across_versions() {
        let mut wifi = base();
        wifi.wifi = associated(Some("a4:83:e7:11:22:33"));
        assert_eq!(
            network_key(&wifi, None).unwrap().digest,
            "fab253f66092e3c8"
        );
        assert_eq!(
            device_key(&device("9c:69:d3:6c:38:28")).unwrap(),
            "8db907f509ce8ee8"
        );
    }

    #[test]
    fn a_bssid_yields_a_hardware_key() {
        let mut id = base();
        id.wifi = associated(Some("a4:83:e7:11:22:33"));
        let key = network_key(&id, None).expect("a key");
        assert_eq!(key.strength, KeyStrength::Hardware);
    }

    #[test]
    fn the_same_bssid_yields_the_same_digest_every_time() {
        let mut id = base();
        id.wifi = associated(Some("a4:83:e7:11:22:33"));
        let first = network_key(&id, None).expect("a key");
        let second = network_key(&id, None).expect("a key");
        assert_eq!(first.digest, second.digest);
    }

    #[test]
    fn bssid_casing_and_form_do_not_change_the_key() {
        let mut lower = base();
        lower.wifi = associated(Some("a4:83:e7:11:22:33"));
        let mut upper = base();
        upper.wifi = associated(Some("A4-83-E7-11-22-33"));
        assert_eq!(
            network_key(&lower, None).unwrap().digest,
            network_key(&upper, None).unwrap().digest
        );
    }

    #[test]
    fn different_networks_yield_different_digests() {
        let mut a = base();
        a.wifi = associated(Some("a4:83:e7:11:22:33"));
        let mut b = base();
        b.wifi = associated(Some("a4:83:e7:44:55:66"));
        assert_ne!(
            network_key(&a, None).unwrap().digest,
            network_key(&b, None).unwrap().digest
        );
    }

    #[test]
    fn a_wired_network_is_recognised_by_the_gateway_mac() {
        let id = base(); // no Wi-Fi association
        let mac = MacAddress::parse("9c:69:d3:6c:38:28");
        let key = network_key(&id, mac).expect("a key");
        assert_eq!(key.strength, KeyStrength::Hardware);
    }

    #[test]
    fn the_bssid_is_preferred_over_the_gateway_mac() {
        let mut id = base();
        id.wifi = associated(Some("a4:83:e7:11:22:33"));
        let gw = MacAddress::parse("9c:69:d3:6c:38:28");
        let with_gw = network_key(&id, gw).unwrap();
        let without = network_key(&id, None).unwrap();
        // Both are Hardware, but the digest must come from the BSSID either way.
        assert_eq!(with_gw.digest, without.digest);
    }

    #[test]
    fn addressing_is_the_last_resort_and_is_labelled_weaker() {
        let mut id = base();
        id.subnet = Some(subnet(192, 168, 1));
        id.dns_servers = vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))];
        let key = network_key(&id, None).expect("a key");
        assert_eq!(key.strength, KeyStrength::Addressing);
    }

    #[test]
    fn resolver_order_does_not_change_the_addressing_key() {
        let one = |a: Ipv4Addr, b: Ipv4Addr| {
            let mut id = base();
            id.subnet = Some(subnet(10, 0, 0));
            id.dns_servers = vec![IpAddr::V4(a), IpAddr::V4(b)];
            network_key(&id, None).unwrap().digest
        };
        assert_eq!(
            one(Ipv4Addr::new(1, 1, 1, 1), Ipv4Addr::new(8, 8, 8, 8)),
            one(Ipv4Addr::new(8, 8, 8, 8), Ipv4Addr::new(1, 1, 1, 1))
        );
    }

    #[test]
    fn nothing_stable_yields_no_key() {
        assert!(network_key(&base(), None).is_none());
    }

    // --- recognition ---

    #[test]
    fn an_unknown_network_is_first_time_whatever_its_strength() {
        let hardware = NetworkKey {
            digest: "x".into(),
            strength: KeyStrength::Hardware,
        };
        let addressing = NetworkKey {
            digest: "y".into(),
            strength: KeyStrength::Addressing,
        };
        assert_eq!(
            recognise_network(&hardware, false),
            NetworkRecognition::FirstTime
        );
        assert_eq!(
            recognise_network(&addressing, false),
            NetworkRecognition::FirstTime
        );
    }

    #[test]
    fn a_known_hardware_key_is_a_confident_return() {
        let key = NetworkKey {
            digest: "x".into(),
            strength: KeyStrength::Hardware,
        };
        assert_eq!(recognise_network(&key, true), NetworkRecognition::Returning);
    }

    #[test]
    fn a_known_addressing_key_is_only_a_likely_return() {
        let key = NetworkKey {
            digest: "x".into(),
            strength: KeyStrength::Addressing,
        };
        assert_eq!(
            recognise_network(&key, true),
            NetworkRecognition::ReturningLikely
        );
    }

    // --- device keys and standing ---

    fn device(mac: &str) -> Identity {
        Identity {
            addresses: vec![],
            mac: MacAddress::parse(mac),
        }
    }

    #[test]
    fn a_stable_mac_has_a_device_key() {
        assert!(device_key(&device("9c:69:d3:6c:38:28")).is_some());
    }

    #[test]
    fn a_randomised_mac_has_no_device_key() {
        // The locally-administered bit is set (0x02 in the first octet).
        assert!(device_key(&device("9e:69:d3:6c:38:28")).is_none());
    }

    #[test]
    fn a_device_without_a_mac_has_no_key() {
        let id = Identity {
            addresses: vec![],
            mac: None,
        };
        assert!(device_key(&id).is_none());
    }

    #[test]
    fn the_same_mac_keys_the_same_device_every_time() {
        assert_eq!(
            device_key(&device("9c:69:d3:6c:38:28")),
            device_key(&device("9c:69:d3:6c:38:28"))
        );
    }

    #[test]
    fn a_seen_stable_device_is_known() {
        let standing = device_standing(&device("9c:69:d3:6c:38:28"), |_| true);
        assert_eq!(standing, DeviceStanding::Known);
    }

    #[test]
    fn an_unseen_stable_device_is_new() {
        let standing = device_standing(&device("9c:69:d3:6c:38:28"), |_| false);
        assert_eq!(standing, DeviceStanding::New);
    }

    /// The rule that stops JRX crying wolf: a randomised MAC must NEVER be
    /// reported as a new device, however unfamiliar it looks. Storage is not
    /// even consulted.
    #[test]
    fn a_randomised_device_is_never_new() {
        let mut consulted = false;
        let standing = device_standing(&device("9e:69:d3:6c:38:28"), |_| {
            consulted = true;
            false
        });
        assert_eq!(standing, DeviceStanding::CannotDetermine);
        assert!(
            !consulted,
            "storage must not be consulted for an unstable identity"
        );
    }
}
