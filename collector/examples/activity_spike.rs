//! M5 phase 0 spike: what could an activity view truthfully show for this Mac?
//!
//! cargo run -p jrx-collector --features activity-spike --example activity_spike
//!
//! Prints only fields that were actually proven. Remote addresses are masked
//! by default; pass --raw to see them (they are not written anywhere).

#[cfg(feature = "activity-spike")]
fn main() {
    use jrx_collector::activity::{observe, owner};
    use std::time::Duration;

    let raw = std::env::args().any(|a| a == "--raw");
    let identity = jrx_collector::identity::observe().expect("identity");
    let interface = identity.interface.clone();

    let snapshot = observe::snapshot(&interface, Duration::from_secs(2)).expect("snapshot");

    let human = |bytes: u64| {
        if bytes >= 1_048_576 {
            format!("{:.1} MB/s", bytes as f64 / 1_048_576.0)
        } else if bytes >= 1024 {
            format!("{:.0} KB/s", bytes as f64 / 1024.0)
        } else {
            format!("{bytes} B/s")
        }
    };
    let mask = |a: std::net::IpAddr| {
        if raw {
            a.to_string()
        } else {
            match a {
                std::net::IpAddr::V4(v4) => {
                    let o = v4.octets();
                    format!("{}.{}.x.x", o[0], o[1])
                }
                std::net::IpAddr::V6(_) => "[v6 redacted]".into(),
            }
        }
    };

    println!("THIS MAC\n");
    println!("Interface:\n  {}\n", snapshot.interface);
    println!("Traffic (measured over {:?}):", snapshot.sampled_over);
    println!("  down {}", human(snapshot.down_bytes_per_sec));
    println!("  up   {}", human(snapshot.up_bytes_per_sec));
    println!(
        "  since the OS started counting: {:.2} GB in, {:.2} GB out",
        snapshot.total_rx as f64 / 1e9,
        snapshot.total_tx as f64 / 1e9
    );

    let live: Vec<_> = snapshot
        .established()
        .filter(|c| {
            c.remote
                .as_ref()
                .is_some_and(|r| !owner::is_local(r.address))
        })
        .collect();
    println!("\nActive connections: {}", snapshot.connections.len());
    println!("  to the internet:  {}", live.len());

    println!("\nObserved endpoints (busiest first):\n");
    let mut sorted = live.clone();
    sorted.sort_by_key(|c| std::cmp::Reverse(c.bytes_in + c.bytes_out));

    for c in sorted.iter().take(12) {
        let r = c.remote.as_ref().expect("filtered above");
        println!("  process:      {}", c.process.display());
        if c.process.name_is_truncated() {
            println!(
                "                (name truncated by nettop; PID {} exited)",
                c.process.pid
            );
        }
        println!("  remote:       {}", mask(r.address));
        println!("  protocol:     {}/{}", c.protocol.label(), r.port);
        println!("  bytes:        {} in / {} out", c.bytes_in, c.bytes_out);
        match r.network_owner {
            Some(o) => println!("  network owner: {o}  (confidence: network-owner only)"),
            None => println!("  network owner: unavailable"),
        }
        println!("  domain:       unavailable  (see the feasibility report)");
        println!();
    }

    let identified = live
        .iter()
        .filter(|c| c.remote.as_ref().is_some_and(|r| r.network_owner.is_some()))
        .count();
    println!(
        "network owner resolved for {}/{} internet connections",
        identified,
        live.len()
    );
}

#[cfg(not(feature = "activity-spike"))]
fn main() {
    eprintln!("build with --features activity-spike");
    std::process::exit(1);
}
