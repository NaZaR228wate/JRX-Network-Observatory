//! How much data this Mac is moving, from the interface's own counters.
//!
//! Measured on this machine: `netstat -ib` costs ~11 ms, and the counters are
//! cumulative, so two reads a known interval apart give a rate. This is the
//! same mechanism the OS uses for its own reporting, and it needs no
//! privileges.
//!
//! Interface totals are *not* the sum of per-process counts: they include
//! traffic the socket layer never attributes to a process.

/// One read of an interface's cumulative counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CounterSample {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// Parse `netstat -ib -I <interface>`.
///
/// The link-level row is the one that counts: an interface appears several
/// times, once per address family, and only the `<Link#n>` row carries the
/// byte totals.
pub fn parse_counters(output: &str, interface: &str) -> Option<CounterSample> {
    output.lines().find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 11 || fields[0] != interface {
            return None;
        }
        // Ibytes and Obytes sit at fixed offsets in the link row.
        if !fields[2].starts_with("<Link") {
            return None;
        }
        Some(CounterSample {
            rx_bytes: fields[6].parse().ok()?,
            tx_bytes: fields[9].parse().ok()?,
        })
    })
}

/// Bytes per second between two reads.
///
/// Counters can go backwards when an interface resets. That is not negative
/// traffic, so the rate is reported as zero rather than as a wrapped-around
/// number.
pub fn rate(earlier: CounterSample, later: CounterSample, seconds: f64) -> (u64, u64) {
    if seconds <= 0.0 {
        return (0, 0);
    }
    let down = later.rx_bytes.saturating_sub(earlier.rx_bytes);
    let up = later.tx_bytes.saturating_sub(earlier.tx_bytes);
    ((down as f64 / seconds) as u64, (up as f64 / seconds) as u64)
}
