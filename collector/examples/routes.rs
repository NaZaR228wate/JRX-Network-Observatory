//! Development harness: what does the real routing table parse to?
fn main() {
    let raw = jrx_collector::macos::exec::routing_table().expect("routes");
    let routes = jrx_collector::macos::parse::parse_routes(&raw);
    println!(
        "{} routes parsed from {} lines\n",
        routes.len(),
        raw.lines().count()
    );

    let mut by_iface: std::collections::BTreeMap<&str, usize> = Default::default();
    for r in &routes {
        *by_iface.entry(&r.interface).or_default() += 1;
    }
    println!("routes per interface:");
    for (iface, count) in &by_iface {
        println!("  {iface:>8}: {count}");
    }
    println!("\ndefault route(s):");
    for r in routes.iter().filter(|r| r.destination == "default") {
        println!("  via {:?} on {}", r.gateway, r.interface);
    }
}
