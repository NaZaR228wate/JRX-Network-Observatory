//! Watch this Mac's activity from the command line.
//!
//! cargo run --release -p jrx-collector --example activity_live -- [ticks]

use std::time::Instant;

use jrx_collector::activity::ActivityProvider;
use jrx_collector::activity::macos::{NetstatInterfaceProvider, NettopConnectionProvider};
use jrx_collector::activity::monitor::ActivityMonitor;

fn human(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn mask(address: Option<std::net::IpAddr>) -> String {
    match address {
        Some(std::net::IpAddr::V4(v4)) => {
            let o = v4.octets();
            format!("{}.{}.x.x", o[0], o[1])
        }
        Some(std::net::IpAddr::V6(_)) => "[v6]".into(),
        None => "-".into(),
    }
}

fn main() {
    let ticks: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(6);

    let interface = jrx_collector::identity::observe()
        .map(|i| i.interface)
        .unwrap_or_else(|_| "en0".into());

    let monitor = ActivityMonitor::new(
        ActivityProvider {
            interface: Box::new(NetstatInterfaceProvider),
            connections: Box::new(NettopConnectionProvider::default()),
        },
        &interface,
    );

    let warm_started = Instant::now();
    monitor.warm();
    println!("stream start: {} ms", warm_started.elapsed().as_millis());
    // The stream needs a moment to emit its first sample.
    std::thread::sleep(std::time::Duration::from_millis(2500));
    println!(
        "first sample after: {} ms\n",
        warm_started.elapsed().as_millis()
    );

    let mut costs = Vec::new();
    let mut last = None;

    for i in 0..ticks {
        let started = Instant::now();
        let snapshot = monitor.tick();
        costs.push(started.elapsed().as_secs_f64() * 1000.0);

        if i + 1 == ticks {
            last = Some(snapshot);
        } else {
            std::thread::sleep(monitor.interval());
        }
    }

    let snapshot = last.expect("a snapshot");
    println!(
        "interface: {}  health: {:?}",
        snapshot.interface, snapshot.health
    );
    println!(
        "rates:     down {}/s  up {}/s",
        human(snapshot.rate_in),
        human(snapshot.rate_out)
    );
    println!(
        "observed:  down {}  up {}   (interface totals: {} / {})",
        human(snapshot.session_bytes_in),
        human(snapshot.session_bytes_out),
        human(snapshot.interface_total_in),
        human(snapshot.interface_total_out)
    );
    println!("connections: {}\n", snapshot.active_connections);

    println!("PROGRAMS (busiest first)");
    for program in snapshot.programs.iter().take(8) {
        println!(
            "  {:<26} down {:>9}  up {:>9}  {} conn",
            program
                .application
                .as_deref()
                .unwrap_or(&program.process_name),
            human(program.session_bytes_in),
            human(program.session_bytes_out),
            program.active_connections
        );
        for c in program.connections.iter().filter(|c| c.is_open).take(2) {
            println!(
                "      {:<14} {}/{:<5} {:<12} owner: {}",
                mask(c.remote_address),
                c.protocol.label(),
                c.remote_port.unwrap_or(0),
                c.state.as_deref().unwrap_or("-"),
                c.network_owner.unwrap_or("unavailable")
            );
        }
    }

    costs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p95 = costs[(costs.len() as f64 * 0.95) as usize % costs.len()];
    println!(
        "\ncollection: avg {:.1} ms  p95 {:.1} ms  ({} ticks; 1 netstat spawn each, plus one long-lived nettop)",
        costs.iter().sum::<f64>() / costs.len() as f64,
        p95,
        costs.len()
    );
}
