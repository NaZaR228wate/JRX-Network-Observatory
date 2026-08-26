//! What a live activity view would actually cost.
//!
//! cargo run --release -p jrx-collector --features activity-spike --example activity_perf

#[cfg(feature = "activity-spike")]
fn main() {
    use jrx_collector::activity::{nettop, observe, throughput};
    use std::time::Instant;

    let interface = jrx_collector::identity::observe()
        .map(|i| i.interface)
        .unwrap_or_else(|_| "en0".into());

    // Warm the tools: the very first nettop call costs seconds while it sets
    // up, and reporting that as the steady-state cost would be wrong.
    let _ = observe::connections();
    let _ = observe::counters(&interface);

    let mut spawn_count = 0usize;
    let mut connection_ms = Vec::new();
    let mut counter_ms = Vec::new();
    let mut parse_us = Vec::new();
    let mut connections_seen = 0usize;

    for _ in 0..20 {
        let t = Instant::now();
        let raw = observe::connections().expect("nettop");
        connection_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        spawn_count += 1;

        let t = Instant::now();
        let parsed = nettop::parse(&raw, observe::process_name);
        parse_us.push(t.elapsed().as_secs_f64() * 1_000_000.0);
        connections_seen = parsed.len();

        let t = Instant::now();
        let counters = observe::counters(&interface).expect("netstat");
        counter_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        spawn_count += 1;
        let _ = throughput::parse_counters(&counters, &interface);
    }

    let stat = |v: &[f64]| {
        let mut s = v.to_vec();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (s[0], s[s.len() / 2], s[s.len() - 1])
    };

    let (cmin, cmed, cmax) = stat(&connection_ms);
    let (nmin, nmed, nmax) = stat(&counter_ms);
    let (pmin, pmed, pmax) = stat(&parse_us);

    println!(
        "20 refreshes, {spawn_count} process spawns ({} per refresh)\n",
        spawn_count / 20
    );
    println!("  nettop  (connections + bytes) : min {cmin:.1} med {cmed:.1} max {cmax:.1} ms");
    println!("  netstat (interface counters)  : min {nmin:.1} med {nmed:.1} max {nmax:.1} ms");
    println!("  parsing + PID resolution      : min {pmin:.0} med {pmed:.0} max {pmax:.0} us");
    println!("\n  connections parsed per refresh: {connections_seen}");

    let per_refresh = cmed + nmed;
    println!("\n  wall time per refresh: {per_refresh:.1} ms");
    for hz in [1.0, 2.0, 4.0] {
        println!(
            "    at {hz:>3} Hz -> {:.1}% of one core",
            per_refresh * hz / 10.0
        );
    }
}

#[cfg(not(feature = "activity-spike"))]
fn main() {}
