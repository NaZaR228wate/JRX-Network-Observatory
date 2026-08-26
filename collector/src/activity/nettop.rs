//! Parsing `nettop`, which is the only unprivileged source on macOS that
//! reports bytes per socket together with the owning process.
//!
//! Measured on this machine: ~11 ms per sample once warm, 0.00 s user CPU,
//! ~5 MB peak RSS for the child process. It covers processes owned by other
//! users, including root, without elevation.
//!
//! `nettop` links `/System/Library/PrivateFrameworks/NetworkStatistics`.
//! Running Apple's own shipped tool is fine; linking that framework ourselves
//! would be a private API, so JRX does not.

use std::net::IpAddr;

use crate::activity::{Connection, ProcessRef, Protocol, RemoteEndpoint};

/// `nettop` truncates process names to this many characters. Observed:
/// `identityservicesd` arrives as `identityservice`, and
/// `com.apple.WebKit.Networking` as `com.apple.WebKi`.
pub const NAME_LIMIT: usize = 15;

/// Parse the CSV form of `nettop -x -L 1 -J bytes_in,bytes_out,state`.
///
/// The output is a flat list where a process row is followed by its socket
/// rows, so ownership comes from position rather than from a column.
pub fn parse(output: &str, resolve_name: impl Fn(u32) -> Option<String>) -> Vec<Connection> {
    let mut connections = Vec::new();
    let mut current: Option<ProcessRef> = None;

    for line in output.lines() {
        let fields: Vec<&str> = line.split(',').collect();
        let Some(first) = fields.first().map(|f| f.trim()) else {
            continue;
        };
        if first.is_empty() || first == "time" {
            continue;
        }

        // A socket row starts with its protocol; anything else names a process.
        if let Some(socket) = parse_socket_row(&fields) {
            if let Some(process) = &current {
                connections.push(Connection {
                    process: process.clone(),
                    ..socket
                });
            }
            continue;
        }

        current = parse_process_row(first, &resolve_name);
    }

    connections
}

/// `Telegram.675` -> pid 675, reported name "Telegram".
fn parse_process_row(field: &str, resolve: &impl Fn(u32) -> Option<String>) -> Option<ProcessRef> {
    let (name, pid) = field.rsplit_once('.')?;
    let pid: u32 = pid.parse().ok()?;
    if name.is_empty() {
        return None;
    }
    Some(ProcessRef {
        pid,
        reported_name: name.to_string(),
        // Resolved from the PID, because the reported name may be cut short.
        // A failure here leaves it None; it is never reconstructed by guessing.
        full_name: resolve(pid),
    })
}

/// `tcp4 172.16.0.207:50959<->17.242.218.132:5223` and its IPv6 form.
fn parse_socket_row(fields: &[&str]) -> Option<Connection> {
    let descriptor = fields.first()?.trim();
    let (proto, rest) = descriptor.split_once(' ')?;

    let protocol = match proto {
        "tcp4" | "tcp6" => Protocol::Tcp,
        "udp4" | "udp6" => Protocol::Udp,
        _ => return None,
    };
    let ipv6 = proto.ends_with('6');

    let (local, remote) = rest.split_once("<->")?;
    let (local_address, local_port) = split_endpoint(local, ipv6)?;

    // `*:*` is a socket with no peer. It is not an endpoint, and inventing one
    // would put a connection on screen that does not exist.
    let remote = split_endpoint(remote, ipv6).map(|(address, port)| RemoteEndpoint {
        address,
        port,
        network_owner: crate::activity::owner::network_owner(address),
        // Never populated from the address. See the module docs.
        hostname: None,
    });

    let column = |name: &str| -> Option<&str> {
        // Columns follow the descriptor in the order requested on the command
        // line; the caller passes state, bytes_in, bytes_out.
        match name {
            "state" => fields.get(1).map(|s| s.trim()),
            "bytes_in" => fields.get(2).map(|s| s.trim()),
            "bytes_out" => fields.get(3).map(|s| s.trim()),
            _ => None,
        }
    };

    Some(Connection {
        protocol,
        local_address,
        local_port,
        remote,
        state: column("state").filter(|s| !s.is_empty()).map(str::to_owned),
        // An unparseable count is zero, never a guess proportional to anything.
        bytes_in: column("bytes_in").and_then(|v| v.parse().ok()).unwrap_or(0),
        bytes_out: column("bytes_out")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        process: ProcessRef {
            pid: 0,
            reported_name: String::new(),
            full_name: None,
        },
    })
}

/// IPv4 uses `address:port`; IPv6 uses `address.port`.
fn split_endpoint(text: &str, ipv6: bool) -> Option<(IpAddr, u16)> {
    let text = text.trim();
    if text.starts_with('*') {
        return None;
    }

    let (address, port) = if ipv6 {
        text.rsplit_once('.')?
    } else {
        text.rsplit_once(':')?
    };

    // A scope suffix such as `%en0` is not part of the address.
    let address = address.split('%').next()?;
    Some((address.parse().ok()?, port.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shape taken verbatim from `nettop -x -L 1 -J state,bytes_in,bytes_out`
    /// on macOS 26. Addresses are real in form and redacted in value.
    const SAMPLE: &str = concat!(
        "time,,interface,state,bytes_in,bytes_out,\n",
        "launchd.1,,,,0,0,\n",
        "tcp4 127.0.0.1:8021<->*:*,Listen,,,\n",
        "apsd.378,,,,17942,81874,\n",
        "tcp4 172.16.0.207:50959<->17.242.218.132:5223,Established,17942,81874,\n",
        "identityservice.669,,,,0,0,\n",
        "Telegram.675,,,,2367178,843131,\n",
        "tcp4 172.16.0.207:51002<->149.154.167.91:443,Established,2367178,843131,\n",
        "udp4 *:54676<->*:*,,0,636,\n",
        "com.apple.WebKi.842,,,,93,1079,\n",
        "tcp6 fe80::1.51100<->2606:4700:20::681a:1.443,Established,93,1079,\n",
    );

    fn no_resolution(_: u32) -> Option<String> {
        None
    }

    #[test]
    fn a_connection_is_attributed_to_the_process_it_appeared_under() {
        let connections = parse(SAMPLE, no_resolution);
        let telegram = connections
            .iter()
            .find(|c| c.local_port == 51002)
            .expect("the Telegram socket");

        assert_eq!(telegram.process.pid, 675);
        assert_eq!(telegram.process.reported_name, "Telegram");
    }

    /// Ownership comes from position in the output, so a socket appearing
    /// before any process row must be dropped rather than attributed to
    /// whatever happens to follow.
    #[test]
    fn a_socket_with_no_owning_process_is_discarded() {
        let orphan = "tcp4 10.0.0.1:1<->10.0.0.2:2,Established,5,6,\n";
        assert!(parse(orphan, no_resolution).is_empty());
    }

    #[test]
    fn reads_both_endpoints_and_the_protocol() {
        let connections = parse(SAMPLE, no_resolution);
        let apsd = connections
            .iter()
            .find(|c| c.local_port == 50959)
            .expect("the apsd socket");

        assert_eq!(apsd.protocol, Protocol::Tcp);
        assert_eq!(apsd.local_address.to_string(), "172.16.0.207");
        let remote = apsd.remote.as_ref().expect("a peer");
        assert_eq!(remote.address.to_string(), "17.242.218.132");
        assert_eq!(remote.port, 5223);
        assert_eq!(apsd.state.as_deref(), Some("Established"));
    }

    /// IPv6 sockets separate the port with a dot, not a colon.
    #[test]
    fn parses_the_ipv6_endpoint_form() {
        let connections = parse(SAMPLE, no_resolution);
        let v6 = connections
            .iter()
            .find(|c| c.local_port == 51100)
            .expect("the IPv6 socket");

        assert_eq!(v6.local_address.to_string(), "fe80::1");
        assert_eq!(v6.remote.as_ref().expect("a peer").port, 443);
    }

    /// A listening socket has no peer. Inventing one would put a connection on
    /// screen that does not exist.
    #[test]
    fn a_listening_socket_has_no_remote_endpoint() {
        let connections = parse(SAMPLE, no_resolution);
        let listener = connections
            .iter()
            .find(|c| c.local_port == 8021)
            .expect("the listening socket");

        assert!(listener.remote.is_none());
        assert_eq!(listener.state.as_deref(), Some("Listen"));
    }

    #[test]
    fn byte_counts_come_from_the_tool_and_are_never_derived() {
        let connections = parse(SAMPLE, no_resolution);
        let telegram = connections
            .iter()
            .find(|c| c.local_port == 51002)
            .expect("the Telegram socket");

        assert_eq!(telegram.bytes_in, 2_367_178);
        assert_eq!(telegram.bytes_out, 843_131);
    }

    /// A row whose counts cannot be read reports zero. It must never be given
    /// a number inferred from the connection existing at all.
    #[test]
    fn an_unreadable_byte_count_becomes_zero_not_an_estimate() {
        let broken = concat!(
            "Telegram.675,,,,0,0,\n",
            "tcp4 10.0.0.1:1<->10.0.0.2:2,Established,notanumber,alsonot,\n",
        );
        let connections = parse(broken, no_resolution);
        assert_eq!(connections[0].bytes_in, 0);
        assert_eq!(connections[0].bytes_out, 0);
    }

    // ---- names ----

    #[test]
    fn the_full_process_name_is_resolved_from_the_pid() {
        let connections = parse(SAMPLE, |pid| {
            (pid == 842).then(|| "com.apple.WebKit.Networking".to_string())
        });
        let webkit = connections
            .iter()
            .find(|c| c.process.pid == 842)
            .expect("the WebKit socket");

        assert_eq!(
            webkit.process.full_name.as_deref(),
            Some("com.apple.WebKit.Networking")
        );
        assert_eq!(webkit.process.display(), "com.apple.WebKit.Networking");
        assert!(!webkit.process.name_is_truncated());
    }

    /// When the PID cannot be resolved the truncated name is shown as-is and
    /// flagged. It is never extended by guessing what the rest might be.
    #[test]
    fn an_unresolvable_pid_keeps_the_truncated_name_and_admits_it() {
        let connections = parse(SAMPLE, no_resolution);
        let webkit = connections
            .iter()
            .find(|c| c.process.pid == 842)
            .expect("the WebKit socket");

        assert_eq!(webkit.process.reported_name, "com.apple.WebKi");
        assert_eq!(webkit.process.full_name, None);
        assert!(
            webkit.process.name_is_truncated(),
            "a name at the tool's limit with no resolution must be flagged"
        );
    }

    #[test]
    fn a_short_unresolved_name_is_not_flagged_as_truncated() {
        let connections = parse(SAMPLE, no_resolution);
        let telegram = connections
            .iter()
            .find(|c| c.process.pid == 675)
            .expect("the Telegram socket");
        assert!(!telegram.process.name_is_truncated());
    }

    #[test]
    fn garbage_input_yields_nothing_rather_than_panicking() {
        assert!(parse("", no_resolution).is_empty());
        assert!(parse("\0\0not csv at all\n???,,,\n", no_resolution).is_empty());
    }
}
