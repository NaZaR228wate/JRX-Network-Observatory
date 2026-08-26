//! SSDP / UPnP discovery.
//!
//! Sends the standard `M-SEARCH` to the UPnP multicast group and collects
//! replies for a bounded window. This is ordinary participation in LAN service
//! discovery — the same request every media player and smart speaker makes —
//! not a scan of the address space.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use crate::probe::ProbeError;

const MULTICAST: &str = "239.255.255.250:1900";

/// One device's reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsdpResponse {
    /// Where the reply came from, filled in from the socket.
    pub address: Option<IpAddr>,
    /// The UPnP device type URN.
    pub device_type: Option<String>,
    pub location: Option<String>,
    /// Stable across reboots where advertised; a strong identity anchor.
    pub uuid: Option<String>,
    pub server: Option<String>,
}

impl SsdpResponse {
    /// The host named in the LOCATION URL.
    pub fn location_host(&self) -> Option<IpAddr> {
        let location = self.location.as_deref()?;
        let after_scheme = location.split("//").nth(1)?;
        let authority = after_scheme.split('/').next()?;
        // Strip the port; IPv6 literals are bracketed.
        let host = authority
            .rsplit_once(':')
            .map_or(authority, |(host, _)| host)
            .trim_matches(['[', ']']);
        host.parse().ok()
    }
}

/// Turn a send failure into something a person can act on.
///
/// macOS reports a denied Local Network permission as `EHOSTUNREACH` on the
/// multicast send — the same error a genuine routing fault produces. Passing
/// that through as "No route to host" sends the reader looking for a network
/// problem that is not there. macOS offers no API to confirm which it is, so
/// this is stated as what it appears to be rather than as a fact
/// (TECH_DECISIONS.md ADR-008).
fn classify_send_failure(error: &std::io::Error) -> ProbeError {
    if error.kind() == std::io::ErrorKind::HostUnreachable {
        return ProbeError::Refused(
            "local network access appears to be blocked for JRX by macOS \
             (the multicast request was refused before it reached the network)"
                .to_string(),
        );
    }
    ProbeError::Failed(format!("ssdp send: {error}"))
}

/// Parse one SSDP reply or advertisement.
///
/// Returns `None` for anything that is not a device announcing its presence —
/// including `ssdp:byebye`, which announces the opposite.
pub fn parse_response(raw: &str) -> Option<SsdpResponse> {
    let mut response = SsdpResponse {
        address: None,
        device_type: None,
        location: None,
        uuid: None,
        server: None,
    };

    for line in raw.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }

        match name.trim().to_ascii_uppercase().as_str() {
            // ST on a search reply, NT on an unsolicited advertisement.
            "ST" | "NT" => response.device_type = Some(value.to_string()),
            "LOCATION" => response.location = Some(value.to_string()),
            "SERVER" => response.server = Some(value.to_string()),
            "USN" => response.uuid = parse_uuid(value),
            // A device announcing that it is leaving is not a discovery.
            "NTS" if value.eq_ignore_ascii_case("ssdp:byebye") => return None,
            _ => {}
        }
    }

    // A reply that identifies nothing is not evidence of anything.
    (response.device_type.is_some() || response.location.is_some()).then_some(response)
}

/// `uuid:1111-2222::urn:...` -> `1111-2222`
fn parse_uuid(usn: &str) -> Option<String> {
    let rest = usn
        .strip_prefix("uuid:")
        .or_else(|| usn.strip_prefix("UUID:"))?;
    Some(rest.split("::").next()?.trim().to_string())
}

/// Search the local network for UPnP devices.
///
/// `window` bounds the whole operation. Devices are told to spread their
/// replies over `MX` seconds, so the window must exceed it or slower devices
/// are silently missed.
///
/// `local` is the address of the interface carrying the default route. It is
/// required, not optional: binding to 0.0.0.0 leaves the kernel to choose a
/// multicast route, and on a machine with several interfaces — a Mac with
/// Wi-Fi, Thunderbolt bridges and half a dozen tunnels is the normal case —
/// it picks one that cannot reach the LAN and the send fails outright with
/// `No route to host`.
pub fn discover(local: Ipv4Addr, window: Duration) -> Result<Vec<SsdpResponse>, ProbeError> {
    let socket = UdpSocket::bind(SocketAddr::from((local, 0)))
        .map_err(|e| ProbeError::Failed(format!("ssdp bind: {e}")))?;
    socket
        .set_multicast_ttl_v4(2)
        .map_err(|e| ProbeError::Failed(e.to_string()))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(400)))
        .map_err(|e| ProbeError::Failed(e.to_string()))?;

    let mx = window.as_secs().saturating_sub(1).clamp(1, 5);
    let request = format!(
        "M-SEARCH * HTTP/1.1\r\nHOST: {MULTICAST}\r\nMAN: \"ssdp:discover\"\r\nMX: {mx}\r\nST: ssdp:all\r\n\r\n"
    );

    let target: SocketAddr = MULTICAST
        .parse()
        .map_err(|_| ProbeError::Failed("bad multicast address".into()))?;

    // Some stacks drop the first datagram while the multicast route settles.
    for _ in 0..2 {
        if let Err(e) = socket.send_to(request.as_bytes(), target) {
            return Err(classify_send_failure(&e));
        }
    }

    let deadline = Instant::now() + window;
    let mut found: Vec<SsdpResponse> = Vec::new();
    let mut buffer = [0u8; 2048];

    while Instant::now() < deadline {
        let Ok((len, from)) = socket.recv_from(&mut buffer) else {
            continue; // read timeout; keep waiting until the deadline
        };

        let text = String::from_utf8_lossy(&buffer[..len]);
        let Some(mut response) = parse_response(&text) else {
            continue;
        };
        response.address = Some(from.ip());

        // One device answers several times, once per advertised service.
        if !found.iter().any(|existing| {
            existing.address == response.address && existing.device_type == response.device_type
        }) {
            found.push(response);
        }
    }

    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A typical router reply, verbatim in shape.
    const ROUTER: &str = "HTTP/1.1 200 OK\r\n\
CACHE-CONTROL: max-age=1800\r\n\
EXT:\r\n\
LOCATION: http://192.168.1.1:5000/rootDesc.xml\r\n\
SERVER: Linux/3.4 UPnP/1.0 MiniUPnPd/1.9\r\n\
ST: urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\
USN: uuid:11111111-2222-3333-4444-555555555555::urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\
\r\n";

    #[test]
    fn reads_the_device_type_and_location() {
        let r = parse_response(ROUTER).expect("parsed");
        assert_eq!(
            r.device_type.as_deref(),
            Some("urn:schemas-upnp-org:device:InternetGatewayDevice:1")
        );
        assert_eq!(
            r.location.as_deref(),
            Some("http://192.168.1.1:5000/rootDesc.xml")
        );
    }

    /// The address is taken from the sending socket, not from LOCATION, but
    /// LOCATION is what identifies which host the description belongs to.
    #[test]
    fn extracts_the_host_from_the_location_url() {
        let r = parse_response(ROUTER).expect("parsed");
        assert_eq!(
            r.location_host().map(|h| h.to_string()).as_deref(),
            Some("192.168.1.1")
        );
    }

    #[test]
    fn reads_the_persistent_device_uuid() {
        let r = parse_response(ROUTER).expect("parsed");
        assert_eq!(
            r.uuid.as_deref(),
            Some("11111111-2222-3333-4444-555555555555")
        );
    }

    #[test]
    fn header_names_are_case_insensitive() {
        let reply =
            "HTTP/1.1 200 OK\r\nst: upnp:rootdevice\r\nlocation: http://10.0.0.5:80/d.xml\r\n\r\n";
        let r = parse_response(reply).expect("parsed");
        assert_eq!(r.device_type.as_deref(), Some("upnp:rootdevice"));
        assert_eq!(
            r.location_host().map(|h| h.to_string()).as_deref(),
            Some("10.0.0.5")
        );
    }

    #[test]
    fn a_notify_advertisement_is_also_accepted() {
        let notify = "NOTIFY * HTTP/1.1\r\nNT: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\
LOCATION: http://10.0.0.9:8060/\r\nNTS: ssdp:alive\r\n\r\n";
        let r = parse_response(notify).expect("parsed");
        assert_eq!(
            r.device_type.as_deref(),
            Some("urn:schemas-upnp-org:device:MediaRenderer:1")
        );
    }

    #[test]
    fn a_byebye_notification_is_not_a_discovery() {
        let bye = "NOTIFY * HTTP/1.1\r\nNT: upnp:rootdevice\r\nNTS: ssdp:byebye\r\n\r\n";
        assert!(
            parse_response(bye).is_none(),
            "a device leaving is not a device found"
        );
    }

    #[test]
    fn garbage_does_not_panic() {
        assert!(parse_response("").is_none());
        assert!(parse_response("\0\0\0not http at all").is_none());
    }

    #[test]
    fn a_reply_with_no_useful_headers_is_rejected() {
        assert!(parse_response("HTTP/1.1 200 OK\r\nEXT:\r\n\r\n").is_none());
    }
}
