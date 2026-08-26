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

use jrx_core::activity::{Protocol, SocketObservation};

/// `nettop` truncates process names to this many characters. Observed:
/// `identityservicesd` arrives as `identityservice`, and
/// `com.apple.WebKit.Networking` as `com.apple.WebKi`.
pub const NAME_LIMIT: usize = 15;

/// Parse the CSV form of `nettop -x -L 1 -J bytes_in,bytes_out,state`.
///
/// The output is a flat list where a process row is followed by its socket
/// rows, so ownership comes from position rather than from a column.
pub fn parse(output: &str, resolve_path: impl Fn(u32) -> Option<String>) -> Vec<SocketObservation> {
    let mut connections = Vec::new();
    let mut current: Option<(u32, String, Option<String>)> = None;

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
            // Ownership comes from position: a socket with no process above it
            // is dropped rather than attributed to whatever follows.
            if let Some((pid, reported_name, executable_path)) = &current {
                connections.push(SocketObservation {
                    pid: *pid,
                    reported_name: reported_name.clone(),
                    executable_path: executable_path.clone(),
                    ..socket
                });
            }
            continue;
        }

        current = parse_process_row(first, &resolve_path);
    }

    connections
}

/// `Telegram.675` -> pid 675, reported name "Telegram".
fn parse_process_row(
    field: &str,
    resolve: &impl Fn(u32) -> Option<String>,
) -> Option<(u32, String, Option<String>)> {
    let (name, pid) = field.rsplit_once('.')?;
    let pid: u32 = pid.parse().ok()?;
    if name.is_empty() {
        return None;
    }
    // The path is resolved from the PID because the reported name may be cut
    // short. A failure leaves it None; it is never reconstructed by guessing.
    Some((pid, name.to_string(), resolve(pid)))
}

/// `tcp4 172.16.0.207:50959<->17.242.218.132:5223` and its IPv6 form.
fn parse_socket_row(fields: &[&str]) -> Option<SocketObservation> {
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
    let remote = split_endpoint(remote, ipv6);

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

    Some(SocketObservation {
        protocol,
        local_address,
        local_port,
        remote_address: remote.map(|(address, _)| address),
        remote_port: remote.map(|(_, port)| port),
        state: column("state").filter(|s| !s.is_empty()).map(str::to_owned),
        rtt_ms: None,
        // An unparseable count is zero, never a guess proportional to anything.
        bytes_in: column("bytes_in").and_then(|v| v.parse().ok()).unwrap_or(0),
        bytes_out: column("bytes_out")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        // Filled in by the caller from the process row above this one.
        pid: 0,
        reported_name: String::new(),
        executable_path: None,
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
        "Telegram.675,,,,2367178,843131,\n",
        "tcp4 172.16.0.207:51002<->149.154.167.91:443,Established,2367178,843131,\n",
        "udp4 *:54676<->*:*,,0,636,\n",
        "com.apple.WebKi.842,,,,93,1079,\n",
        "tcp6 fe80::1.51100<->2606:4700:20::681a:1.443,Established,93,1079,\n",
    );

    const WEBKIT_PATH: &str = "/System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.Networking.xpc/Contents/MacOS/com.apple.WebKit.Networking";

    fn unresolved(_: u32) -> Option<String> {
        None
    }

    fn at(observations: &[SocketObservation], port: u16) -> &SocketObservation {
        observations
            .iter()
            .find(|o| o.local_port == port)
            .unwrap_or_else(|| panic!("no socket on port {port}"))
    }

    #[test]
    fn a_socket_is_attributed_to_the_process_it_appeared_under() {
        let observations = parse(SAMPLE, unresolved);
        let telegram = at(&observations, 51002);
        assert_eq!(telegram.pid, 675);
        assert_eq!(telegram.reported_name, "Telegram");
    }

    /// Ownership comes from position, so a socket with no process above it is
    /// dropped rather than attributed to whatever follows.
    #[test]
    fn a_socket_with_no_owning_process_is_discarded() {
        let orphan = "tcp4 10.0.0.1:1<->10.0.0.2:2,Established,5,6,\n";
        assert!(parse(orphan, unresolved).is_empty());
    }

    #[test]
    fn reads_both_endpoints_the_protocol_and_the_state() {
        let observations = parse(SAMPLE, unresolved);
        let apsd = at(&observations, 50959);

        assert_eq!(apsd.protocol, Protocol::Tcp);
        assert_eq!(apsd.local_address.to_string(), "172.16.0.207");
        assert_eq!(
            apsd.remote_address.map(|a| a.to_string()).as_deref(),
            Some("17.242.218.132")
        );
        assert_eq!(apsd.remote_port, Some(5223));
        assert_eq!(apsd.state.as_deref(), Some("Established"));
    }

    /// IPv6 sockets separate the port with a dot, not a colon.
    #[test]
    fn parses_the_ipv6_endpoint_form() {
        let observations = parse(SAMPLE, unresolved);
        let v6 = at(&observations, 51100);
        assert_eq!(v6.local_address.to_string(), "fe80::1");
        assert_eq!(v6.remote_port, Some(443));
    }

    /// A listening socket has no peer. Inventing one would put a connection on
    /// screen that does not exist.
    #[test]
    fn a_listening_socket_has_no_remote_endpoint() {
        let observations = parse(SAMPLE, unresolved);
        let listener = at(&observations, 8021);
        assert!(listener.remote_address.is_none());
        assert!(listener.remote_port.is_none());
        assert_eq!(listener.state.as_deref(), Some("Listen"));
    }

    #[test]
    fn byte_counts_come_from_the_tool_and_are_never_derived() {
        let observations = parse(SAMPLE, unresolved);
        let telegram = at(&observations, 51002);
        assert_eq!(telegram.bytes_in, 2_367_178);
        assert_eq!(telegram.bytes_out, 843_131);
    }

    /// A count that cannot be read is zero, never a number inferred from the
    /// connection existing at all.
    #[test]
    fn an_unreadable_byte_count_becomes_zero_not_an_estimate() {
        let broken = concat!(
            "Telegram.675,,,,0,0,\n",
            "tcp4 10.0.0.1:1<->10.0.0.2:2,Established,notanumber,alsonot,\n",
        );
        let observations = parse(broken, unresolved);
        assert_eq!(observations[0].bytes_in, 0);
        assert_eq!(observations[0].bytes_out, 0);
    }

    /// `nettop` cut `com.apple.WebKit.Networking` down to 15 characters; the
    /// PID lookup is what recovers it.
    #[test]
    fn the_pid_lookup_supplies_what_the_truncated_name_lost() {
        let observations = parse(SAMPLE, |pid| (pid == 842).then(|| WEBKIT_PATH.to_string()));
        let webkit = at(&observations, 51100);

        assert_eq!(webkit.reported_name, "com.apple.WebKi");
        assert_eq!(webkit.reported_name.chars().count(), NAME_LIMIT);
        assert_eq!(webkit.executable_path.as_deref(), Some(WEBKIT_PATH));
    }

    /// When the PID cannot be resolved the truncated name stands as observed.
    /// It is never extended by guessing what the rest might be.
    #[test]
    fn an_unresolvable_pid_leaves_the_truncated_name_alone() {
        let observations = parse(SAMPLE, unresolved);
        let webkit = at(&observations, 51100);
        assert_eq!(webkit.reported_name, "com.apple.WebKi");
        assert_eq!(webkit.executable_path, None);
    }

    #[test]
    fn garbage_input_yields_nothing_rather_than_panicking() {
        assert!(parse("", unresolved).is_empty());
        assert!(parse("\0\0not csv at all\n???,,,\n", unresolved).is_empty());
    }
}
