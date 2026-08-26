//! Devices observed on the local network, and the evidence behind every
//! conclusion drawn about them.
//!
//! ARCHITECTURE.md §8. Devices are *derived* from evidence, never asserted, so
//! the Device Inspector can always show why JRX believes what it believes.

use std::fmt;
use std::net::IpAddr;

use serde::{Deserialize, Serialize};

/// A hardware address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(into = "String")]
pub struct MacAddress([u8; 6]);

impl MacAddress {
    /// Parse `aa:bb:cc:dd:ee:ff`, `AA-BB-CC-DD-EE-FF`, or the short form
    /// macOS `arp` emits (`a4:83:e7:1:2:3`).
    pub fn parse(text: &str) -> Option<MacAddress> {
        let mut octets = [0u8; 6];
        let mut seen = 0;

        for part in text.split([':', '-']) {
            if seen == 6 || part.is_empty() || part.len() > 2 {
                return None;
            }
            octets[seen] = u8::from_str_radix(part, 16).ok()?;
            seen += 1;
        }

        (seen == 6).then_some(MacAddress(octets))
    }

    /// The 24-bit vendor prefix.
    pub fn oui(self) -> [u8; 3] {
        [self.0[0], self.0[1], self.0[2]]
    }

    /// True when the locally-administered bit is set.
    ///
    /// Modern phones set this and rotate the address per network, deliberately
    /// preventing tracking. JRX reports that as a device protecting itself
    /// rather than guessing who it is (TECH_DECISIONS.md ADR-008).
    pub fn is_randomised(self) -> bool {
        self.0[0] & 0b0000_0010 != 0
    }

    /// True for the all-ones broadcast address.
    pub fn is_broadcast(self) -> bool {
        self.0 == [0xff; 6]
    }

    /// True when the group bit is set. Multicast addresses are destinations,
    /// not devices, and must never become entries in the device list.
    pub fn is_multicast(self) -> bool {
        self.0[0] & 0b0000_0001 != 0
    }

    /// True if this address could belong to a real, identifiable device.
    pub fn is_device_address(self) -> bool {
        !self.is_broadcast() && !self.is_multicast()
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let o = self.0;
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            o[0], o[1], o[2], o[3], o[4], o[5]
        )
    }
}

impl From<MacAddress> for String {
    fn from(mac: MacAddress) -> String {
        mac.to_string()
    }
}

#[cfg(test)]
mod mac_tests {
    use super::*;

    #[test]
    fn parses_colon_separated_lowercase() {
        let mac = MacAddress::parse("9c:69:d3:6c:38:28").expect("valid");
        assert_eq!(mac.to_string(), "9c:69:d3:6c:38:28");
        assert_eq!(mac.oui(), [0x9c, 0x69, 0xd3]);
    }

    #[test]
    fn accepts_uppercase_and_hyphens() {
        let mac = MacAddress::parse("A4-83-E7-11-22-33").expect("valid");
        assert_eq!(mac.to_string(), "a4:83:e7:11:22:33");
    }

    /// macOS `arp -an` prints short octets: `a4:83:e7:1:2:3`.
    #[test]
    fn accepts_single_digit_octets_from_arp_output() {
        let mac = MacAddress::parse("a4:83:e7:1:2:3").expect("valid");
        assert_eq!(mac.to_string(), "a4:83:e7:01:02:03");
    }

    #[test]
    fn rejects_malformed_input() {
        for bad in [
            "",
            "not a mac",
            "aa:bb:cc:dd:ee",
            "aa:bb:cc:dd:ee:ff:00",
            "zz:bb:cc:dd:ee:ff",
        ] {
            assert!(MacAddress::parse(bad).is_none(), "{bad:?} should not parse");
        }
    }

    /// The locally-administered bit is the second-least-significant bit of the
    /// first octet. Modern phones set it and rotate the address per network.
    #[test]
    fn detects_randomised_address() {
        // 0x9c = 1001_1100 -> LA bit clear -> a real vendor address
        assert!(
            !MacAddress::parse("9c:69:d3:6c:38:28")
                .unwrap()
                .is_randomised()
        );
        // 0x9e = 1001_1110 -> LA bit set -> randomised
        assert!(
            MacAddress::parse("9e:69:d3:6c:38:28")
                .unwrap()
                .is_randomised()
        );
        assert!(
            MacAddress::parse("02:00:00:00:00:01")
                .unwrap()
                .is_randomised()
        );
    }

    /// The broadcast address and multicast addresses are not devices.
    #[test]
    fn recognises_non_device_addresses() {
        assert!(
            MacAddress::parse("ff:ff:ff:ff:ff:ff")
                .unwrap()
                .is_broadcast()
        );
        assert!(
            MacAddress::parse("01:00:5e:00:00:fb")
                .unwrap()
                .is_multicast()
        );
        assert!(
            !MacAddress::parse("9c:69:d3:6c:38:28")
                .unwrap()
                .is_multicast()
        );
    }
}

/// How a fact about a device was learned. Shown to the user verbatim, because
/// "we saw it announce itself" and "we asked it directly" are different
/// things and the difference matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryMethod {
    /// Read from the neighbour cache the OS had already built. Nothing sent.
    ArpCache,
    /// The device announced itself over mDNS.
    Mdns,
    /// The device answered a UPnP search.
    Ssdp,
    /// This machine, from its own interfaces.
    SelfInterface,
    /// Holds the default route.
    DefaultRoute,
}

impl DiscoveryMethod {
    pub fn label(self) -> &'static str {
        match self {
            DiscoveryMethod::ArpCache => "already known to this Mac (nothing was sent)",
            DiscoveryMethod::Mdns => "announced itself over mDNS",
            DiscoveryMethod::Ssdp => "answered a UPnP search",
            DiscoveryMethod::SelfInterface => "this device",
            DiscoveryMethod::DefaultRoute => "holds the default route",
        }
    }
}

/// What kind of fact a piece of evidence is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    MacAddress,
    Hostname,
    /// A DNS-SD service type such as `_airplay._tcp`.
    ServiceType,
    /// A UPnP device type URN.
    UpnpDeviceType,
    /// Vendor resolved from the MAC prefix. Never classifies on its own.
    Vendor,
    GatewayRole,
    SelfRole,
}

/// One observation about one device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Evidence {
    pub kind: EvidenceKind,
    pub value: String,
    pub method: DiscoveryMethod,
}

impl Evidence {
    pub fn new(kind: EvidenceKind, value: impl Into<String>, method: DiscoveryMethod) -> Evidence {
        Evidence {
            kind,
            value: value.into(),
            method,
        }
    }
}

/// The five categories. Unknown is a legitimate, informative outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Computers,
    Phones,
    SmartHome,
    Infrastructure,
    Unknown,
}

impl Category {
    /// Display order for the topology rings.
    pub const ORDER: [Category; 5] = [
        Category::Computers,
        Category::Phones,
        Category::SmartHome,
        Category::Infrastructure,
        Category::Unknown,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Computers => "Computers",
            Category::Phones => "Phones",
            Category::SmartHome => "Smart home",
            Category::Infrastructure => "Infrastructure",
            // Not "Unknown": a device JRX chose not to guess at is a valid
            // result, and the word should not read like a missing value.
            Category::Unknown => "Unidentified",
        }
    }
}

/// How sure we are of a category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Definitive evidence: a role we verified, or a service only one kind of
    /// device advertises.
    High,
    /// A suggestive name, corroborated by something else.
    Medium,
    /// Not classified. Paired with `Category::Unknown`.
    None,
}

/// Services that only one kind of device advertises.
///
/// The bar here is deliberately high: a service qualifies only if a
/// general-purpose computer would not advertise it. Media services do not
/// qualify — this was found the hard way on a real network, where macOS's
/// built-in AirPlay Receiver caused several MacBooks to be labelled "Smart
/// home" with High confidence. See `media_capable_category` below.
fn definitive_service_category(service: &str) -> Option<Category> {
    let s = service.to_ascii_lowercase();
    let has = |needle: &str| s.contains(needle);

    // A pairing service only phones and tablets run.
    if has("_apple-mobdev") || has("_rdlink") {
        return Some(Category::Phones);
    }
    // Interactive login, file sharing and screen sharing mean a computer.
    if has("_smb.")
        || has("_ssh.")
        || has("_sftp-ssh.")
        || has("_rfb.")
        || has("_workstation.")
        || has("_afpovertcp.")
    {
        return Some(Category::Computers);
    }
    // Appliances: home-automation accessories, printers, and cast receivers.
    // A laptop does not become a Chromecast by having Chrome installed.
    if has("_hap.")
        || has("_matter")
        || has("_hue.")
        || has("_googlecast.")
        || has("_ipp.")
        || has("_ipps.")
        || has("_printer.")
        || has("_pdl-datastream.")
    {
        return Some(Category::SmartHome);
    }
    // Network plumbing.
    if has("_nas.") || has("_adisk.") || has("_smb-router") {
        return Some(Category::Infrastructure);
    }
    None
}

/// A UPnP device-type URN we are sure about.
fn definitive_upnp_category(urn: &str) -> Option<Category> {
    let u = urn.to_ascii_lowercase();
    if u.contains("internetgatewaydevice") || u.contains("wlanaccesspoint") {
        return Some(Category::Infrastructure);
    }
    if u.contains("mediarenderer") || u.contains("mediaserver") || u.contains("printer") {
        return Some(Category::SmartHome);
    }
    None
}

/// A category suggested by a device's name.
///
/// Weaker than an advertised service: names are chosen by people and are
/// frequently wrong or ambiguous.
fn hostname_hint(name: &str) -> Option<Category> {
    let n = name.to_ascii_lowercase();
    let matches = |needles: &[&str]| needles.iter().any(|w| n.contains(w));

    let mut found: Option<Category> = None;
    let mut set = |category: Category, hit: bool| {
        if hit {
            match found {
                // Two different categories in one name means the name is not
                // usable evidence. Do not pick a winner.
                Some(existing) if existing != category => found = Some(Category::Unknown),
                Some(_) => {}
                None => found = Some(category),
            }
        }
    };

    set(
        Category::Phones,
        matches(&["iphone", "ipad", "android", "galaxy", "pixel"]),
    );
    set(
        Category::Computers,
        matches(&[
            "macbook", "imac", "mac-mini", "macmini", "laptop", "desktop", "-pc", "thinkpad",
        ]),
    );
    set(
        Category::SmartHome,
        matches(&[
            "tv",
            "printer",
            "camera",
            "echo",
            "alexa",
            "nest",
            "hue",
            "sonos",
            "chromecast",
            "roku",
        ]),
    );
    set(
        Category::Infrastructure,
        matches(&[
            "router",
            "gateway",
            "switch",
            "access-point",
            "unifi",
            "nas",
            "synology",
        ]),
    );

    match found {
        Some(Category::Unknown) | None => None,
        other => other,
    }
}

/// A finer statement than a category: what kind of thing this is.
///
/// Only ever set from definitive evidence. A name is enough to say "some kind
/// of computer" but not enough to say "a laptop", so Medium-confidence
/// categories carry no family (TECH_DECISIONS.md ADR-008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceFamily {
    Router,
    AccessPoint,
    NetworkStorage,
    Printer,
    MediaReceiver,
    HomeAccessory,
    Workstation,
    Handheld,
}

impl DeviceFamily {
    pub fn label(self) -> &'static str {
        match self {
            DeviceFamily::Router => "Router",
            DeviceFamily::AccessPoint => "Access point",
            DeviceFamily::NetworkStorage => "Network storage",
            DeviceFamily::Printer => "Printer",
            DeviceFamily::MediaReceiver => "Media receiver",
            DeviceFamily::HomeAccessory => "Home accessory",
            DeviceFamily::Workstation => "Workstation",
            DeviceFamily::Handheld => "Phone or tablet",
        }
    }
}

/// One recorded change of category, and what caused it.
///
/// A device must never change what it is without leaving a reason behind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CategoryChange {
    pub from: Category,
    pub to: Category,
    pub confidence: Confidence,
    pub reason: &'static str,
    /// The single observation that tipped the conclusion.
    pub triggered_by: Evidence,
}

/// What JRX concluded about a device, and why.
///
/// Kept separate from `ObservedFacts` on purpose: a fact is something we saw,
/// an inference is something we decided. Mixing them is how a guess ends up
/// presented as a measurement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CategoryInference {
    pub category: Category,
    pub confidence: Confidence,
    pub family: Option<DeviceFamily>,
    /// Why, in words the user can read.
    pub rationale: &'static str,
    /// The evidence that produced this conclusion — never the whole evidence
    /// list, and never a vendor.
    pub supporting: Vec<Evidence>,
    /// Every category change, in the order the evidence was considered.
    pub history: Vec<CategoryChange>,
}

/// The family implied by one definitive piece of evidence, if any.
fn definitive_family(evidence: &Evidence) -> Option<DeviceFamily> {
    match evidence.kind {
        EvidenceKind::GatewayRole => Some(DeviceFamily::Router),
        EvidenceKind::ServiceType => {
            let s = evidence.value.to_ascii_lowercase();
            if s.contains("_ipp.")
                || s.contains("_ipps.")
                || s.contains("_printer.")
                || s.contains("_pdl-datastream.")
            {
                Some(DeviceFamily::Printer)
            } else if s.contains("_googlecast.") {
                Some(DeviceFamily::MediaReceiver)
            } else if s.contains("_hap.") || s.contains("_matter") || s.contains("_hue.") {
                Some(DeviceFamily::HomeAccessory)
            } else if s.contains("_smb.")
                || s.contains("_ssh.")
                || s.contains("_sftp-ssh.")
                || s.contains("_rfb.")
                || s.contains("_workstation.")
                || s.contains("_afpovertcp.")
            {
                Some(DeviceFamily::Workstation)
            } else if s.contains("_apple-mobdev") || s.contains("_rdlink") {
                Some(DeviceFamily::Handheld)
            } else if s.contains("_nas.") || s.contains("_adisk.") {
                Some(DeviceFamily::NetworkStorage)
            } else {
                None
            }
        }
        EvidenceKind::UpnpDeviceType => {
            let u = evidence.value.to_ascii_lowercase();
            if u.contains("internetgatewaydevice") {
                Some(DeviceFamily::Router)
            } else if u.contains("wlanaccesspoint") {
                Some(DeviceFamily::AccessPoint)
            } else if u.contains("printer") {
                Some(DeviceFamily::Printer)
            } else if u.contains("mediarenderer") || u.contains("mediaserver") {
                Some(DeviceFamily::MediaReceiver)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Derive a category, the evidence behind it, and the trail of how it was
/// reached.
///
/// The trail is produced by replaying the evidence in derivation order and
/// recording every point at which the conclusion moved. That is what makes
/// "why do you think this is a computer?" answerable with a specific
/// observation rather than a shrug.
pub fn infer(evidence: &[Evidence]) -> CategoryInference {
    let mut history: Vec<CategoryChange> = Vec::new();
    let mut standing = (Category::Unknown, Confidence::None);

    for step in 1..=evidence.len() {
        let (category, confidence) = classify(&evidence[..step]);
        if (category, confidence) == standing {
            continue;
        }
        history.push(CategoryChange {
            from: standing.0,
            to: category,
            confidence,
            reason: rationale_for(category, confidence),
            triggered_by: evidence[step - 1].clone(),
        });
        standing = (category, confidence);
    }

    let (category, confidence) = standing;
    let supporting = supporting_evidence(evidence, category, confidence);
    let family = (confidence == Confidence::High)
        .then(|| supporting.iter().find_map(definitive_family))
        .flatten();

    CategoryInference {
        category,
        confidence,
        family,
        rationale: rationale_for(category, confidence),
        supporting,
        history,
    }
}

/// The evidence that actually decided the category.
///
/// Deliberately excludes vendor: a manufacturer is an observed fact and never
/// support for a category (TECH_DECISIONS.md ADR-008), so citing it here would
/// misrepresent the reasoning.
fn supporting_evidence(
    evidence: &[Evidence],
    category: Category,
    confidence: Confidence,
) -> Vec<Evidence> {
    if category == Category::Unknown {
        return Vec::new();
    }

    evidence
        .iter()
        .filter(|e| match e.kind {
            EvidenceKind::GatewayRole | EvidenceKind::SelfRole => true,
            EvidenceKind::ServiceType => definitive_service_category(&e.value) == Some(category),
            EvidenceKind::UpnpDeviceType => definitive_upnp_category(&e.value) == Some(category),
            EvidenceKind::Hostname => {
                confidence == Confidence::Medium && hostname_hint(&e.value) == Some(category)
            }
            EvidenceKind::Vendor | EvidenceKind::MacAddress => false,
        })
        .cloned()
        .collect()
}

fn rationale_for(category: Category, confidence: Confidence) -> &'static str {
    match (category, confidence) {
        (Category::Unknown, _) => {
            "Not identified. Nothing this device revealed says what kind of device it is."
        }
        (Category::Infrastructure, Confidence::High) => "Verified role on this network.",
        (_, Confidence::High) => "Advertises a service only this kind of device provides.",
        (_, Confidence::Medium) => {
            "Its name suggests this, and another observation agrees. Names are chosen by people, so this is likely rather than certain."
        }
        (_, Confidence::None) => "Not identified.",
    }
}

/// Derive a category from evidence.
///
/// The rule, from TECH_DECISIONS.md ADR-008: a category is assigned only on
/// High confidence, or on Medium confidence supported by at least one non-OUI
/// signal. A vendor match alone never classifies. Everything else stays
/// Unknown, which is an informative answer rather than a failure.
pub fn classify(evidence: &[Evidence]) -> (Category, Confidence) {
    // 1. Roles we verified ourselves.
    if evidence.iter().any(|e| e.kind == EvidenceKind::GatewayRole) {
        return (Category::Infrastructure, Confidence::High);
    }
    if evidence.iter().any(|e| e.kind == EvidenceKind::SelfRole) {
        return (Category::Computers, Confidence::High);
    }

    // 2. Services and device types only one kind of device advertises.
    let definitive: Vec<Category> = evidence
        .iter()
        .filter_map(|e| match e.kind {
            EvidenceKind::ServiceType => definitive_service_category(&e.value),
            EvidenceKind::UpnpDeviceType => definitive_upnp_category(&e.value),
            _ => None,
        })
        .collect();

    if let Some(first) = definitive.first() {
        // Where definitive signals disagree, prefer the most specific rather
        // than silently taking whichever arrived first. A device that both
        // shares files and casts media is a computer running media software.
        let category = if definitive.contains(&Category::Computers) {
            Category::Computers
        } else {
            *first
        };
        return (category, Confidence::High);
    }

    // 3. Names, which require corroboration and can never stand alone.
    let hints: Vec<Category> = evidence
        .iter()
        .filter(|e| e.kind == EvidenceKind::Hostname)
        .filter_map(|e| hostname_hint(&e.value))
        .collect();

    let all_agree = hints
        .first()
        .is_some_and(|first| hints.iter().all(|c| c == first));
    let corroborated = evidence.iter().any(|e| e.kind != EvidenceKind::Hostname);

    if all_agree && corroborated {
        return (hints[0], Confidence::Medium);
    }

    // 4. Vendor-only, address-only, or contradictory: say so.
    (Category::Unknown, Confidence::None)
}

#[cfg(test)]
mod classify_tests {
    use super::*;

    fn service(name: &str) -> Evidence {
        Evidence::new(EvidenceKind::ServiceType, name, DiscoveryMethod::Mdns)
    }
    fn hostname(name: &str) -> Evidence {
        Evidence::new(EvidenceKind::Hostname, name, DiscoveryMethod::Mdns)
    }
    fn vendor(name: &str) -> Evidence {
        Evidence::new(EvidenceKind::Vendor, name, DiscoveryMethod::ArpCache)
    }
    fn mac(addr: &str) -> Evidence {
        Evidence::new(EvidenceKind::MacAddress, addr, DiscoveryMethod::ArpCache)
    }

    // ---- the rule that matters most ----

    /// "Apple, Inc." cannot distinguish a MacBook from an iPhone from an Apple
    /// TV. A vendor match is an observed fact, never a category
    /// (TECH_DECISIONS.md ADR-008).
    #[test]
    fn vendor_alone_never_classifies() {
        let (category, confidence) = classify(&[mac("a4:83:e7:11:22:33"), vendor("Apple")]);
        assert_eq!(category, Category::Unknown);
        assert_eq!(confidence, Confidence::None);
    }

    #[test]
    fn vendor_plus_more_vendor_evidence_still_never_classifies() {
        let (category, _) = classify(&[
            mac("a4:83:e7:11:22:33"),
            vendor("Apple"),
            Evidence::new(EvidenceKind::Vendor, "Apple", DiscoveryMethod::Mdns),
        ]);
        assert_eq!(category, Category::Unknown);
    }

    // ---- the requested cases ----

    #[test]
    fn macbook_with_file_and_shell_services_is_a_computer() {
        let (category, confidence) = classify(&[
            hostname("Nazars-MacBook-Pro.local"),
            service("_smb._tcp"),
            service("_ssh._tcp"),
            vendor("Apple"),
        ]);
        assert_eq!(category, Category::Computers);
        assert_eq!(confidence, Confidence::High);
    }

    #[test]
    fn iphone_advertising_the_ios_pairing_service_is_a_phone() {
        let (category, confidence) = classify(&[service("_apple-mobdev2._tcp"), vendor("Apple")]);
        assert_eq!(category, Category::Phones);
        assert_eq!(confidence, Confidence::High);
    }

    /// A name is real evidence, but weaker than an advertised service, so it
    /// yields Medium and only with corroboration.
    #[test]
    fn iphone_known_only_by_its_name_is_medium_confidence() {
        let (category, confidence) = classify(&[hostname("Nazars-iPhone.local"), vendor("Apple")]);
        assert_eq!(category, Category::Phones);
        assert_eq!(confidence, Confidence::Medium);
    }

    /// Holding the default route is the one thing we know for certain.
    #[test]
    fn the_default_gateway_is_infrastructure() {
        let (category, confidence) = classify(&[Evidence::new(
            EvidenceKind::GatewayRole,
            "default route",
            DiscoveryMethod::DefaultRoute,
        )]);
        assert_eq!(category, Category::Infrastructure);
        assert_eq!(confidence, Confidence::High);
    }

    #[test]
    fn printer_advertising_ipp_is_smart_home() {
        let (category, confidence) =
            classify(&[service("_ipp._tcp"), hostname("HP-LaserJet.local")]);
        assert_eq!(category, Category::SmartHome);
        assert_eq!(confidence, Confidence::High);
    }

    #[test]
    fn homekit_accessory_is_smart_home() {
        let (category, _) = classify(&[service("_hap._tcp")]);
        assert_eq!(category, Category::SmartHome);
    }

    #[test]
    fn a_device_known_only_by_its_address_stays_unknown() {
        let (category, confidence) = classify(&[]);
        assert_eq!(category, Category::Unknown);
        assert_eq!(confidence, Confidence::None);
    }

    /// A randomised address carries no vendor and no category. Guessing here
    /// would defeat the privacy feature the device is using.
    #[test]
    fn randomised_mac_with_nothing_else_stays_unknown() {
        let (category, confidence) = classify(&[mac("9e:69:d3:6c:38:28")]);
        assert_eq!(category, Category::Unknown);
        assert_eq!(confidence, Confidence::None);
    }

    // ---- guarding against confident wrong answers ----

    /// Contradictory hints must not be resolved by picking one.
    #[test]
    fn conflicting_hints_stay_unknown_rather_than_choosing() {
        let (category, _) = classify(&[hostname("living-room-tv-macbook"), vendor("Apple")]);
        assert_eq!(category, Category::Unknown);
    }

    /// A definitive service outranks a misleading name.
    #[test]
    fn advertised_service_outranks_a_misleading_hostname() {
        let (category, confidence) = classify(&[hostname("printer-room-pc"), service("_hap._tcp")]);
        assert_eq!(category, Category::SmartHome);
        assert_eq!(confidence, Confidence::High);
    }

    // ---- regressions found on a real network ----

    /// Observed live: several MacBooks were labelled "Smart home" with High
    /// confidence because macOS ships an AirPlay Receiver and advertises
    /// `_airplay._tcp` and `_raop._tcp`. A media service says a device can
    /// play media, which computers, TVs and speakers all can.
    #[test]
    fn a_macbook_advertising_airplay_is_not_a_smart_home_device() {
        let (category, _) = classify(&[
            hostname("MacBook-Pro-Aris"),
            service("_companion-link._tcp"),
            service("_airplay._tcp"),
            service("_raop._tcp"),
            vendor("Apple"),
        ]);
        assert_eq!(category, Category::Computers);
    }

    /// Observed live: a Windows laptop running the Spotify desktop app.
    #[test]
    fn a_laptop_running_spotify_is_not_a_speaker() {
        let (category, _) = classify(&[
            hostname("LAPTOP-FSPI6LK4"),
            service("_spotify-connect._tcp"),
            vendor("Intel Corporate"),
        ]);
        assert_eq!(category, Category::Computers);
    }

    /// With no name to corroborate it, a media service is not enough to say
    /// what a device is. Unknown is the honest answer.
    #[test]
    fn a_media_service_alone_does_not_classify() {
        let (category, confidence) = classify(&[service("_airplay._tcp")]);
        assert_eq!(category, Category::Unknown);
        assert_eq!(confidence, Confidence::None);
    }

    /// Chromecast receivers are a different matter: no general-purpose
    /// computer advertises the cast receiver protocol.
    #[test]
    fn a_cast_receiver_is_still_definitively_smart_home() {
        let (category, confidence) = classify(&[service("_googlecast._tcp")]);
        assert_eq!(category, Category::SmartHome);
        assert_eq!(confidence, Confidence::High);
    }

    /// Nor is `_companion-link._tcp` a phone signal: Macs, iPads and iPhones
    /// all advertise it for Continuity.
    #[test]
    fn apple_continuity_alone_does_not_make_a_device_a_phone() {
        let (category, _) = classify(&[service("_companion-link._tcp"), vendor("Apple")]);
        assert_eq!(category, Category::Unknown);
    }

    #[test]
    fn hostname_hint_without_corroboration_is_not_enough() {
        let (category, _) = classify(&[hostname("somebodys-iphone")]);
        assert_eq!(category, Category::Unknown);
    }
}

/// The addressing facts used to decide whether two sightings are one device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub addresses: Vec<IpAddr>,
    pub mac: Option<MacAddress>,
}

/// Whether two sightings describe the same device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeDecision {
    /// Same stable hardware address. Survives a DHCP lease change.
    SameHardware,
    /// Same network address, with nothing contradicting it.
    SameAddress,
    /// Different devices.
    Distinct,
}

/// Decide whether an incoming sighting belongs to a known device.
///
/// The rules, in order of strength:
///
/// 1. Two stable hardware addresses that match are one device, whatever their
///    network addresses — this is a DHCP lease change.
/// 2. Two stable hardware addresses that differ are two devices, whatever
///    their network addresses — this is a reused lease, and merging them would
///    fuse two machines into one entry with a merged history.
/// 3. A randomised hardware address is never a reason to merge. It is
///    deliberately unstable: one phone changes it per network, and two phones
///    can present the same one.
/// 4. Otherwise a shared network address merges, which is the ordinary case of
///    mDNS enriching an ARP entry.
///
/// Names are absent from this list on purpose. Hostnames are not unique.
pub fn merge_decision(existing: &Identity, incoming: &Identity) -> MergeDecision {
    let stable = |mac: Option<MacAddress>| mac.filter(|m| !m.is_randomised());
    let (left, right) = (stable(existing.mac), stable(incoming.mac));

    if let (Some(left), Some(right)) = (left, right) {
        return if left == right {
            MergeDecision::SameHardware
        } else {
            MergeDecision::Distinct
        };
    }

    let shares_address = incoming
        .addresses
        .iter()
        .any(|a| existing.addresses.contains(a));

    if shares_address {
        MergeDecision::SameAddress
    } else {
        MergeDecision::Distinct
    }
}

/// One sighting of one device, from one source.
#[derive(Debug, Clone)]
pub struct Observation {
    pub address: IpAddr,
    pub mac: Option<MacAddress>,
    pub hostname: Option<String>,
    pub services: Vec<String>,
    pub upnp_types: Vec<String>,
    pub method: DiscoveryMethod,
}

impl Observation {
    pub fn new(address: IpAddr, method: DiscoveryMethod) -> Observation {
        Observation {
            address,
            mac: None,
            hostname: None,
            services: Vec::new(),
            upnp_types: Vec::new(),
            method,
        }
    }

    #[must_use]
    pub fn with_mac(mut self, mac: Option<MacAddress>) -> Self {
        self.mac = mac;
        self
    }

    #[must_use]
    pub fn with_hostname(mut self, hostname: Option<String>) -> Self {
        self.hostname = hostname;
        self
    }

    #[must_use]
    pub fn with_service(mut self, service: impl Into<String>) -> Self {
        self.services.push(service.into());
        self
    }

    #[must_use]
    pub fn with_upnp_type(mut self, urn: impl Into<String>) -> Self {
        self.upnp_types.push(urn.into());
        self
    }
}

/// What was actually observed about a device. No conclusions.
///
/// Held separately from `CategoryInference` on purpose: a fact is something we
/// saw, an inference is something we decided. Mixing them is how a guess ends
/// up presented as a measurement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObservedFacts {
    pub addresses: Vec<IpAddr>,
    pub mac: Option<MacAddress>,
    pub hostname: Option<String>,
    /// Resolved from the hardware address prefix. An observed fact about the
    /// manufacturer — never a statement about what the device is.
    pub vendor: Option<String>,
    pub services: Vec<String>,
    pub upnp_types: Vec<String>,
    /// Which sources saw this device.
    pub sources: Vec<DiscoveryMethod>,
    /// The device is deliberately rotating its hardware address.
    pub mac_randomised: bool,
}

/// A device on the local network, derived entirely from evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Device {
    /// Stable within one run; used to reference a device from the topology.
    pub id: String,
    /// What we saw.
    pub facts: ObservedFacts,
    /// What we concluded, and why.
    pub inference: CategoryInference,
    /// Everything considered, in derivation order.
    pub evidence: Vec<Evidence>,
    pub is_self: bool,
    pub is_gateway: bool,
}

impl Device {
    pub fn category(&self) -> Category {
        self.inference.category
    }

    pub fn confidence(&self) -> Confidence {
        self.inference.confidence
    }
}

impl Device {
    /// What to show as the device's name, without inventing one.
    pub fn display_name(&self) -> String {
        if let Some(host) = &self.facts.hostname {
            return host
                .trim_end_matches(".local")
                .trim_end_matches('.')
                .to_string();
        }
        if let Some(vendor) = &self.facts.vendor {
            // A vendor is not a name, so it is presented as a description.
            return format!("{vendor} device");
        }
        self.facts
            .addresses
            .first()
            .map_or_else(|| "Unidentified device".to_string(), |a| a.to_string())
    }
}

/// Accumulates observations and merges them into devices.
#[derive(Debug, Default)]
pub struct DeviceTable {
    entries: Vec<Entry>,
}

#[derive(Debug, Default)]
struct Entry {
    addresses: Vec<IpAddr>,
    mac: Option<MacAddress>,
    hostname: Option<String>,
    services: Vec<String>,
    upnp_types: Vec<String>,
    methods: Vec<DiscoveryMethod>,
    is_self: bool,
    is_gateway: bool,
}

impl DeviceTable {
    pub fn new() -> DeviceTable {
        DeviceTable::default()
    }

    /// Record a sighting.
    ///
    /// Addresses that cannot belong to a device — broadcast and multicast —
    /// are dropped here rather than becoming phantom entries in the device
    /// list. macOS keeps them in the ARP cache.
    pub fn observe(&mut self, observation: Observation) {
        if observation.mac.is_some_and(|m| !m.is_device_address()) {
            return;
        }

        let index = self.index_for(observation.address, observation.mac);
        let entry = &mut self.entries[index];

        if !entry.addresses.contains(&observation.address) {
            entry.addresses.push(observation.address);
        }
        if entry.mac.is_none() {
            entry.mac = observation.mac;
        }
        if entry.hostname.is_none() {
            entry.hostname = observation.hostname;
        }
        for service in observation.services {
            if !entry.services.contains(&service) {
                entry.services.push(service);
            }
        }
        for urn in observation.upnp_types {
            if !entry.upnp_types.contains(&urn) {
                entry.upnp_types.push(urn);
            }
        }
        if !entry.methods.contains(&observation.method) {
            entry.methods.push(observation.method);
        }
    }

    /// Find the entry this observation belongs to, creating one if needed.
    ///
    /// Merging on a hardware address is only sound when that address is
    /// stable. A randomised address is deliberately not, so two devices using
    /// randomised addresses must never be collapsed into one.
    fn index_for(&mut self, address: IpAddr, mac: Option<MacAddress>) -> usize {
        if let Some(index) = self
            .entries
            .iter()
            .position(|e| e.addresses.contains(&address))
        {
            return index;
        }
        if let Some(mac) = mac.filter(|m| !m.is_randomised())
            && let Some(index) = self.entries.iter().position(|e| e.mac == Some(mac))
        {
            return index;
        }

        self.entries.push(Entry {
            addresses: vec![address],
            mac,
            ..Entry::default()
        });
        self.entries.len() - 1
    }

    /// Mark the device holding the default route. Creates it if unseen: the
    /// router always belongs on the map.
    pub fn mark_gateway(&mut self, address: IpAddr) {
        let index = self.index_for(address, None);
        self.entries[index].is_gateway = true;
        self.push_method(index, DiscoveryMethod::DefaultRoute);
    }

    /// Mark this machine. Creates it if unseen.
    pub fn mark_self(&mut self, address: IpAddr) {
        let index = self.index_for(address, None);
        self.entries[index].is_self = true;
        self.push_method(index, DiscoveryMethod::SelfInterface);
    }

    fn push_method(&mut self, index: usize, method: DiscoveryMethod) {
        let methods = &mut self.entries[index].methods;
        if !methods.contains(&method) {
            methods.push(method);
        }
    }

    /// Resolve vendors, classify, and produce the device list.
    ///
    /// `vendor_of` is injected so this crate stays free of the OUI database
    /// and of any I/O (ARCHITECTURE.md §4).
    pub fn finish(&self, vendor_of: impl Fn(MacAddress) -> Option<&'static str>) -> Vec<Device> {
        self.entries
            .iter()
            .map(|entry| {
                let randomised = entry.mac.is_some_and(MacAddress::is_randomised);

                // A rotating address identifies no manufacturer. Looking one up
                // would attach a confident, wrong label to a device that is
                // deliberately protecting itself.
                let vendor = entry
                    .mac
                    .filter(|_| !randomised)
                    .and_then(&vendor_of)
                    .map(str::to_owned);

                let evidence = Self::evidence_for(entry, vendor.as_deref());
                let inference = infer(&evidence);

                Device {
                    id: entry
                        .addresses
                        .first()
                        .map_or_else(String::new, ToString::to_string),
                    facts: ObservedFacts {
                        addresses: entry.addresses.clone(),
                        mac: entry.mac,
                        hostname: entry.hostname.clone(),
                        vendor,
                        services: entry.services.clone(),
                        upnp_types: entry.upnp_types.clone(),
                        sources: entry.methods.clone(),
                        mac_randomised: randomised,
                    },
                    inference,
                    evidence,
                    is_self: entry.is_self,
                    is_gateway: entry.is_gateway,
                }
            })
            .collect()
    }

    fn evidence_for(entry: &Entry, vendor: Option<&str>) -> Vec<Evidence> {
        let primary = entry
            .methods
            .first()
            .copied()
            .unwrap_or(DiscoveryMethod::ArpCache);
        let mut evidence = Vec::new();

        if entry.is_gateway {
            evidence.push(Evidence::new(
                EvidenceKind::GatewayRole,
                "holds the default route",
                DiscoveryMethod::DefaultRoute,
            ));
        }
        if entry.is_self {
            evidence.push(Evidence::new(
                EvidenceKind::SelfRole,
                "this machine",
                DiscoveryMethod::SelfInterface,
            ));
        }
        if let Some(mac) = entry.mac {
            evidence.push(Evidence::new(
                EvidenceKind::MacAddress,
                mac.to_string(),
                primary,
            ));
        }
        if let Some(vendor) = vendor {
            evidence.push(Evidence::new(EvidenceKind::Vendor, vendor, primary));
        }
        if let Some(host) = &entry.hostname {
            evidence.push(Evidence::new(
                EvidenceKind::Hostname,
                host,
                DiscoveryMethod::Mdns,
            ));
        }
        for service in &entry.services {
            evidence.push(Evidence::new(
                EvidenceKind::ServiceType,
                service,
                DiscoveryMethod::Mdns,
            ));
        }
        for urn in &entry.upnp_types {
            evidence.push(Evidence::new(
                EvidenceKind::UpnpDeviceType,
                urn,
                DiscoveryMethod::Ssdp,
            ));
        }

        evidence
    }
}

/// Whether the network appears to stop devices from seeing each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Isolation {
    /// Only the router answered, on a network with room for many hosts. Guest
    /// and corporate Wi-Fi commonly keep clients apart. Stated as "likely"
    /// because it is an inference from absence, not something the network told
    /// us.
    LikelyIsolated,
    /// No other devices, on a network too small for that to be surprising — a
    /// phone hotspot, for instance. Nothing here suggests interference.
    NoPeersObserved,
    Normal,
}

/// Assess whether the network is isolating its clients.
/// The largest prefix still roomy enough that an empty result is worth
/// remarking on. A /24 holds 254 hosts; a /28 holds 14.
const ROOMY_PREFIX: u8 = 24;

/// Assess whether the network is keeping its clients apart.
///
/// The size of the network is the evidence. Finding nobody on a /20 is odd;
/// finding nobody on a hotspot's /28 is the normal shape of a hotspot, and
/// calling that isolation would be inventing a finding. Without a known
/// subnet there is no basis to choose, so the weaker claim is made.
pub fn assess_isolation(devices: &[Device], subnet: Option<crate::network::Subnet>) -> Isolation {
    let others = devices
        .iter()
        .filter(|d| !d.is_self && !d.is_gateway)
        .count();
    if others > 0 {
        return Isolation::Normal;
    }

    match subnet {
        Some(subnet) if subnet.prefix_len <= ROOMY_PREFIX => Isolation::LikelyIsolated,
        _ => Isolation::NoPeersObserved,
    }
}

#[cfg(test)]
mod table_tests {
    use super::*;

    fn ip(s: &str) -> std::net::IpAddr {
        s.parse().unwrap()
    }
    fn no_vendor(_: MacAddress) -> Option<&'static str> {
        None
    }
    fn apple_vendor(mac: MacAddress) -> Option<&'static str> {
        (mac.oui() == [0xa4, 0x83, 0xe7]).then_some("Apple")
    }

    /// Answers for *any* address, so a `None` result can only mean the lookup
    /// was deliberately skipped. A resolver that happens not to know the test
    /// address would make the assertion below unfalsifiable.
    fn vendor_for_anything(_: MacAddress) -> Option<&'static str> {
        Some("Test Vendor")
    }

    fn arp(addr: &str, mac: &str) -> Observation {
        Observation::new(ip(addr), DiscoveryMethod::ArpCache).with_mac(MacAddress::parse(mac))
    }

    #[test]
    fn a_single_arp_entry_becomes_one_device() {
        let mut table = DeviceTable::new();
        table.observe(arp("192.168.1.10", "a4:83:e7:11:22:33"));

        let devices = table.finish(no_vendor);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].category(), Category::Unknown);
        assert_eq!(devices[0].facts.sources, vec![DiscoveryMethod::ArpCache]);
    }

    /// The same device seen by ARP and by mDNS is one device, and the record
    /// must say it was found both ways.
    #[test]
    fn observations_of_the_same_address_merge_into_one_device() {
        let mut table = DeviceTable::new();
        table.observe(arp("192.168.1.10", "a4:83:e7:11:22:33"));
        table.observe(
            Observation::new(ip("192.168.1.10"), DiscoveryMethod::Mdns)
                .with_hostname(Some("Nazars-MacBook-Pro.local".into()))
                .with_service("_ssh._tcp"),
        );

        let devices = table.finish(no_vendor);
        assert_eq!(devices.len(), 1, "same address must not appear twice");
        assert_eq!(devices[0].category(), Category::Computers);
        assert_eq!(
            devices[0].facts.sources,
            vec![DiscoveryMethod::ArpCache, DiscoveryMethod::Mdns]
        );
    }

    /// One device holding two addresses (IPv4 and IPv6, or a changed lease) is
    /// still one device when the hardware address matches.
    #[test]
    fn two_addresses_sharing_a_hardware_address_are_one_device() {
        let mut table = DeviceTable::new();
        table.observe(arp("192.168.1.10", "a4:83:e7:11:22:33"));
        table.observe(arp("192.168.1.55", "a4:83:e7:11:22:33"));

        let devices = table.finish(no_vendor);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].facts.addresses.len(), 2);
    }

    /// A randomised address is deliberately not stable, so two devices using
    /// randomised addresses must never be merged on that basis.
    #[test]
    fn randomised_addresses_are_never_used_to_merge_devices() {
        let mut table = DeviceTable::new();
        table.observe(arp("192.168.1.10", "9e:aa:bb:cc:dd:ee"));
        table.observe(arp("192.168.1.11", "9e:aa:bb:cc:dd:ee"));

        assert_eq!(table.finish(no_vendor).len(), 2);
    }

    #[test]
    fn randomised_mac_is_flagged_and_gets_no_vendor() {
        let mut table = DeviceTable::new();
        table.observe(arp("192.168.1.10", "a6:83:e7:11:22:33"));

        let devices = table.finish(vendor_for_anything);
        assert!(devices[0].facts.mac_randomised);
        assert_eq!(
            devices[0].facts.vendor, None,
            "a randomised address does not identify a manufacturer"
        );
        assert_eq!(devices[0].category(), Category::Unknown);
    }

    #[test]
    fn real_hardware_address_resolves_a_vendor() {
        let mut table = DeviceTable::new();
        table.observe(arp("192.168.1.10", "a4:83:e7:11:22:33"));

        let devices = table.finish(apple_vendor);
        assert_eq!(devices[0].facts.vendor.as_deref(), Some("Apple"));
        // ...and still does not classify it.
        assert_eq!(devices[0].category(), Category::Unknown);
    }

    /// Broadcast and multicast addresses are destinations, not devices. macOS
    /// keeps them in the ARP cache.
    #[test]
    fn multicast_and_broadcast_entries_are_not_devices() {
        let mut table = DeviceTable::new();
        table.observe(arp("192.168.1.255", "ff:ff:ff:ff:ff:ff"));
        table.observe(arp("224.0.0.251", "01:00:5e:00:00:fb"));
        table.observe(arp("192.168.1.10", "a4:83:e7:11:22:33"));

        assert_eq!(table.finish(no_vendor).len(), 1);
    }

    #[test]
    fn gateway_and_self_are_marked_and_classified() {
        let mut table = DeviceTable::new();
        table.observe(arp("192.168.1.1", "aa:bb:cc:00:00:01"));
        table.observe(arp("192.168.1.10", "a4:83:e7:11:22:33"));
        table.mark_gateway(ip("192.168.1.1"));
        table.mark_self(ip("192.168.1.10"));

        let devices = table.finish(no_vendor);
        let gateway = devices
            .iter()
            .find(|d| d.is_gateway)
            .expect("gateway present");
        let me = devices.iter().find(|d| d.is_self).expect("self present");

        assert_eq!(gateway.category(), Category::Infrastructure);
        assert_eq!(gateway.confidence(), Confidence::High);
        assert_eq!(me.category(), Category::Computers);
    }

    /// Marking a device we have not otherwise seen must still create it: this
    /// machine and its router always belong on the map.
    #[test]
    fn marking_an_unseen_gateway_still_creates_it() {
        let mut table = DeviceTable::new();
        table.mark_gateway(ip("192.168.1.1"));

        let devices = table.finish(no_vendor);
        assert_eq!(devices.len(), 1);
        assert!(devices[0].is_gateway);
    }

    #[test]
    fn every_conclusion_keeps_the_evidence_behind_it() {
        let mut table = DeviceTable::new();
        table.observe(
            Observation::new(ip("192.168.1.30"), DiscoveryMethod::Mdns)
                .with_hostname(Some("HP-LaserJet.local".into()))
                .with_service("_ipp._tcp"),
        );

        let devices = table.finish(no_vendor);
        let kinds: Vec<_> = devices[0].evidence.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&EvidenceKind::Hostname));
        assert!(kinds.contains(&EvidenceKind::ServiceType));
        assert!(
            devices[0]
                .evidence
                .iter()
                .all(|e| e.method == DiscoveryMethod::Mdns)
        );
    }

    // ---- client isolation ----

    /// Guest and corporate Wi-Fi often stop devices seeing each other. Seeing
    /// only ourselves and the router is the signature, but it is an inference
    /// and must be labelled as one — the same discipline the Visibility Panel
    /// applies to permissions.
    #[test]
    fn seeing_only_self_and_router_reads_as_likely_isolation() {
        let mut table = DeviceTable::new();
        table.observe(arp("192.168.1.1", "aa:bb:cc:00:00:01"));
        table.observe(arp("192.168.1.10", "a4:83:e7:11:22:33"));
        table.mark_gateway(ip("192.168.1.1"));
        table.mark_self(ip("192.168.1.10"));

        let devices = table.finish(no_vendor);
        assert_eq!(
            assess_isolation(
                &devices,
                Some(crate::network::Subnet {
                    network: "192.168.1.0".parse().unwrap(),
                    prefix_len: 24,
                })
            ),
            Isolation::LikelyIsolated
        );
    }

    #[test]
    fn a_populated_network_is_not_reported_as_isolated() {
        let mut table = DeviceTable::new();
        table.observe(arp("192.168.1.1", "aa:bb:cc:00:00:01"));
        table.observe(arp("192.168.1.10", "a4:83:e7:11:22:33"));
        table.observe(arp("192.168.1.20", "aa:bb:cc:00:00:02"));
        table.mark_gateway(ip("192.168.1.1"));
        table.mark_self(ip("192.168.1.10"));

        let devices = table.finish(no_vendor);
        assert_eq!(
            assess_isolation(
                &devices,
                Some(crate::network::Subnet {
                    network: "192.168.1.0".parse().unwrap(),
                    prefix_len: 24,
                })
            ),
            Isolation::Normal
        );
    }
}

#[cfg(test)]
mod merge_rule_tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }
    fn mac(s: &str) -> MacAddress {
        MacAddress::parse(s).unwrap()
    }
    fn known(addresses: &[&str], hardware: Option<&str>) -> Identity {
        Identity {
            addresses: addresses.iter().map(|a| ip(a)).collect(),
            mac: hardware.map(mac),
        }
    }

    /// A stable hardware address is the strongest identity we have: the same
    /// device keeps it across a DHCP lease change.
    #[test]
    fn dhcp_address_change_keeps_one_device() {
        let decision = merge_decision(
            &known(&["192.168.1.50"], Some("a4:83:e7:11:22:33")),
            &known(&["192.168.1.77"], Some("a4:83:e7:11:22:33")),
        );
        assert_eq!(decision, MergeDecision::SameHardware);
    }

    /// A randomised address is deliberately not stable. Two phones can present
    /// the same one, and one phone changes it per network, so it can never be
    /// the reason to merge.
    #[test]
    fn randomised_addresses_never_merge_even_when_identical() {
        let decision = merge_decision(
            &known(&["192.168.1.50"], Some("9e:aa:bb:cc:dd:ee")),
            &known(&["192.168.1.51"], Some("9e:aa:bb:cc:dd:ee")),
        );
        assert_eq!(decision, MergeDecision::Distinct);
    }

    /// A lease handed to a different machine must not fuse two devices into a
    /// single entry with a merged history.
    #[test]
    fn a_reused_address_with_different_hardware_stays_two_devices() {
        let decision = merge_decision(
            &known(&["192.168.1.50"], Some("a4:83:e7:11:22:33")),
            &known(&["192.168.1.50"], Some("b8:27:eb:00:11:22")),
        );
        assert_eq!(
            decision,
            MergeDecision::Distinct,
            "the same address held by different hardware is two devices"
        );
    }

    /// An address alone merges only when nothing contradicts it — which is the
    /// mDNS-enriches-ARP case, where one source has no hardware address.
    #[test]
    fn an_address_match_merges_when_one_side_has_no_hardware_address() {
        let decision = merge_decision(
            &known(&["192.168.1.50"], Some("a4:83:e7:11:22:33")),
            &known(&["192.168.1.50"], None),
        );
        assert_eq!(decision, MergeDecision::SameAddress);
    }

    #[test]
    fn unrelated_devices_do_not_merge() {
        let decision = merge_decision(
            &known(&["192.168.1.50"], Some("a4:83:e7:11:22:33")),
            &known(&["192.168.1.60"], Some("b8:27:eb:00:11:22")),
        );
        assert_eq!(decision, MergeDecision::Distinct);
    }

    /// A randomised address on one side must not block an address match, or a
    /// phone seen by both ARP and mDNS would appear twice.
    #[test]
    fn a_randomised_device_still_merges_on_a_shared_address() {
        let decision = merge_decision(
            &known(&["192.168.1.50"], Some("9e:aa:bb:cc:dd:ee")),
            &known(&["192.168.1.50"], None),
        );
        assert_eq!(decision, MergeDecision::SameAddress);
    }
}

#[cfg(test)]
mod hostname_merge_tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    /// Names are not unique. Two machines imaged from the same template,
    /// or two phones with the default name, share a hostname and are still
    /// two devices.
    #[test]
    fn two_devices_sharing_a_hostname_are_not_merged() {
        let mut table = DeviceTable::new();
        table.observe(
            Observation::new(ip("192.168.1.10"), DiscoveryMethod::Mdns)
                .with_mac(MacAddress::parse("a4:83:e7:11:22:33"))
                .with_hostname(Some("MacBook-Pro".into())),
        );
        table.observe(
            Observation::new(ip("192.168.1.11"), DiscoveryMethod::Mdns)
                .with_mac(MacAddress::parse("b8:27:eb:00:11:22"))
                .with_hostname(Some("MacBook-Pro".into())),
        );

        assert_eq!(table.finish(|_| None).len(), 2);
    }
}

#[cfg(test)]
mod inference_tests {
    use super::*;

    fn service(name: &str) -> Evidence {
        Evidence::new(EvidenceKind::ServiceType, name, DiscoveryMethod::Mdns)
    }
    fn hostname(name: &str) -> Evidence {
        Evidence::new(EvidenceKind::Hostname, name, DiscoveryMethod::Mdns)
    }
    fn vendor(name: &str) -> Evidence {
        Evidence::new(EvidenceKind::Vendor, name, DiscoveryMethod::ArpCache)
    }
    fn mac(addr: &str) -> Evidence {
        Evidence::new(EvidenceKind::MacAddress, addr, DiscoveryMethod::ArpCache)
    }

    /// An inference must name the evidence that produced it. A category with
    /// no cited evidence is an assertion, not a conclusion.
    #[test]
    fn an_inference_cites_the_evidence_that_produced_it() {
        let inference = infer(&[
            hostname("Nazars-MacBook-Pro"),
            service("_ssh._tcp"),
            vendor("Apple"),
        ]);

        assert_eq!(inference.category, Category::Computers);
        assert!(
            inference.supporting.iter().any(|e| e.value == "_ssh._tcp"),
            "the service that decided it must be cited"
        );
        assert!(!inference.rationale.is_empty());
    }

    /// Vendor is an observed fact, never support for a category.
    #[test]
    fn vendor_is_never_cited_as_support_for_a_category() {
        let inference = infer(&[
            hostname("Nazars-iPhone"),
            vendor("Apple"),
            mac("a4:83:e7:11:22:33"),
        ]);

        assert_eq!(inference.category, Category::Phones);
        assert!(
            !inference
                .supporting
                .iter()
                .any(|e| e.kind == EvidenceKind::Vendor),
            "a vendor must not appear as support for a category"
        );
    }

    #[test]
    fn an_unclassified_device_cites_nothing_and_says_why() {
        let inference = infer(&[mac("9e:aa:bb:cc:dd:ee")]);

        assert_eq!(inference.category, Category::Unknown);
        assert!(inference.supporting.is_empty());
        assert!(
            inference.rationale.to_lowercase().contains("not"),
            "an Unknown device must state what was missing, got: {}",
            inference.rationale
        );
    }

    // ---- the timeline ----

    /// Every category change records the observation that caused it. A device
    /// must never change what it is without leaving a reason behind.
    #[test]
    fn every_category_change_records_the_evidence_that_caused_it() {
        let inference = infer(&[
            mac("a4:83:e7:11:22:33"),
            vendor("Apple"),
            hostname("Nazars-MacBook-Pro"),
            service("_ssh._tcp"),
        ]);

        assert!(!inference.history.is_empty(), "no timeline recorded");

        // A confidence upgrade within the same category is a real move and
        // worth recording: "we became more sure, and here is what did it."
        for change in &inference.history {
            assert!(!change.reason.is_empty());
            assert!(
                !change.triggered_by.value.is_empty(),
                "a change must name the observation that caused it"
            );
        }
        for pair in inference.history.windows(2) {
            assert!(
                (pair[0].to, pair[0].confidence) != (pair[1].to, pair[1].confidence),
                "consecutive entries must represent a genuine move, not padding"
            );
        }

        let final_change = inference.history.last().unwrap();
        assert_eq!(final_change.to, Category::Computers);
        assert_eq!(
            final_change.triggered_by.value, "_ssh._tcp",
            "the deciding observation must be named"
        );
    }

    /// Evidence that changes nothing must not manufacture a timeline entry.
    #[test]
    fn evidence_that_changes_nothing_leaves_no_timeline_entry() {
        let inference = infer(&[mac("a4:83:e7:11:22:33"), vendor("Apple")]);
        assert!(
            inference.history.is_empty(),
            "nothing was concluded, so nothing should be recorded"
        );
    }

    /// The timeline ends where the inference stands.
    #[test]
    fn the_timeline_agrees_with_the_final_conclusion() {
        let inference = infer(&[hostname("HP-LaserJet"), service("_ipp._tcp")]);
        assert_eq!(
            inference.history.last().map(|c| c.to),
            Some(inference.category)
        );
    }

    // ---- device family ----

    /// Family is a finer statement than category and needs definitive
    /// evidence; it is never guessed from a vendor or a bare address.
    #[test]
    fn a_printer_service_yields_the_printer_family() {
        let inference = infer(&[service("_ipp._tcp")]);
        assert_eq!(inference.family, Some(DeviceFamily::Printer));
    }

    #[test]
    fn a_gateway_role_yields_the_router_family() {
        let inference = infer(&[Evidence::new(
            EvidenceKind::GatewayRole,
            "holds the default route",
            DiscoveryMethod::DefaultRoute,
        )]);
        assert_eq!(inference.family, Some(DeviceFamily::Router));
    }

    #[test]
    fn a_device_we_cannot_place_has_no_family() {
        assert_eq!(infer(&[vendor("Apple")]).family, None);
        assert_eq!(infer(&[]).family, None);
    }

    /// A category without definitive evidence must not acquire a family by
    /// implication.
    #[test]
    fn a_medium_confidence_category_does_not_invent_a_family() {
        let inference = infer(&[hostname("some-desktop"), vendor("Intel Corporate")]);
        assert_eq!(inference.category, Category::Computers);
        assert_eq!(inference.confidence, Confidence::Medium);
        assert_eq!(
            inference.family, None,
            "a name is not enough to say which kind of computer"
        );
    }
}

#[cfg(test)]
mod isolation_tests {
    use super::*;
    use crate::network::Subnet;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }
    fn subnet(net: &str, prefix: u8) -> Subnet {
        Subnet {
            network: net.parse().unwrap(),
            prefix_len: prefix,
        }
    }

    fn just_us(network: &str) -> Vec<Device> {
        let mut t = DeviceTable::new();
        t.observe(
            Observation::new(ip(&format!("{network}.1")), DiscoveryMethod::ArpCache)
                .with_mac(MacAddress::parse("00:0c:42:00:00:01")),
        );
        t.mark_gateway(ip(&format!("{network}.1")));
        t.mark_self(ip(&format!("{network}.44")));
        t.finish(|_| None)
    }

    /// A network with room for hundreds of hosts, where only the router
    /// answered, is behaving like guest Wi-Fi that keeps clients apart.
    #[test]
    fn a_large_network_with_no_peers_reads_as_likely_isolation() {
        let devices = just_us("10.7.3");
        assert_eq!(
            assess_isolation(&devices, Some(subnet("10.7.0.0", 20))),
            Isolation::LikelyIsolated
        );
    }

    /// A phone hotspot hands out a handful of addresses. Having no peers there
    /// is unremarkable, and calling it isolation would be inventing a finding.
    #[test]
    fn a_tiny_network_with_no_peers_is_not_called_isolation() {
        let devices = just_us("172.20.10");
        assert_eq!(
            assess_isolation(&devices, Some(subnet("172.20.10.0", 28))),
            Isolation::NoPeersObserved
        );
    }

    /// Without knowing how large the network is, there is no basis for
    /// choosing between the two.
    #[test]
    fn an_unknown_subnet_size_yields_the_weaker_claim() {
        let devices = just_us("10.7.3");
        assert_eq!(assess_isolation(&devices, None), Isolation::NoPeersObserved);
    }

    #[test]
    fn seeing_other_devices_is_normal_whatever_the_subnet() {
        let mut t = DeviceTable::new();
        t.observe(
            Observation::new(ip("10.7.0.1"), DiscoveryMethod::ArpCache)
                .with_mac(MacAddress::parse("00:0c:42:00:00:01")),
        );
        t.observe(
            Observation::new(ip("10.7.0.9"), DiscoveryMethod::ArpCache)
                .with_mac(MacAddress::parse("3c:aa:bb:00:00:02")),
        );
        t.mark_gateway(ip("10.7.0.1"));
        t.mark_self(ip("10.7.3.44"));
        let devices = t.finish(|_| None);

        assert_eq!(
            assess_isolation(&devices, Some(subnet("10.7.0.0", 20))),
            Isolation::Normal
        );
    }

    /// A /24 is the ordinary home case: large enough that an empty result is
    /// worth remarking on.
    #[test]
    fn an_ordinary_home_subnet_with_no_peers_reads_as_likely_isolation() {
        let devices = just_us("192.168.1");
        assert_eq!(
            assess_isolation(&devices, Some(subnet("192.168.1.0", 24))),
            Isolation::LikelyIsolated
        );
    }
}
