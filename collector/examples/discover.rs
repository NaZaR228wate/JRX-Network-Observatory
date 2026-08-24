//! Development harness: discover devices on this network.
//!
//! Run with: cargo run -p jrx-collector --example discover

fn main() {
    let identity = jrx_collector::identity::observe().expect("identity");
    println!(
        "network: {:?} on {}",
        identity.connection, identity.interface
    );
    println!("subnet:  {:?}\n", identity.subnet);

    let report = jrx_collector::discovery::observe(&identity).expect("discovery");

    for source in &report.quality.sources {
        println!(
            "{:>12?}: {:?} (names: {}, service types: {})",
            source.method, source.status, source.names_resolved, source.services_seen
        );
    }
    println!("\nverdict: {:?}", report.quality.verdict);
    println!("  {}", report.quality.explanation);
    println!("local network access: {:?}", report.quality.local_network);
    println!(
        "\n{} devices in {} ms",
        report.summary.total, report.took_ms
    );
    println!("unidentified: {}", report.summary.unidentified);
    println!("isolation: {:?}", report.summary.isolation);
    println!("\nby category:");
    for (category, count) in &report.summary.by_category {
        println!("  {:>14}: {}", category.label(), count);
    }

    let randomised = report
        .devices
        .iter()
        .filter(|d| d.facts.mac_randomised)
        .count();
    let with_vendor = report
        .devices
        .iter()
        .filter(|d| d.facts.vendor.is_some())
        .count();
    let named = report
        .devices
        .iter()
        .filter(|d| d.facts.hostname.is_some())
        .count();
    println!("\nwhy so many are unidentified:");
    println!("  randomised MAC (protecting identity): {randomised}");
    println!("  vendor known but type not:            {with_vendor}");
    println!("  announced a name:                     {named}");

    println!("\nidentified devices — facts vs. inference:");
    for device in report
        .devices
        .iter()
        .filter(|d| d.category() != jrx_core::device::Category::Unknown)
    {
        println!(
            "\n  {} [{:?} / {:?}{}]",
            device.display_name(),
            device.category(),
            device.confidence(),
            device
                .inference
                .family
                .map(|f| format!(" / {}", f.label()))
                .unwrap_or_default(),
        );
        println!("    why:  {}", device.inference.rationale);

        println!("    KNOWN (observed):");
        if let Some(mac) = device.facts.mac {
            println!("      hardware address  {mac}");
        }
        if let Some(vendor) = &device.facts.vendor {
            println!("      manufacturer      {vendor}");
        }
        if let Some(host) = &device.facts.hostname {
            println!("      announced name    {host}");
        }
        for service in &device.facts.services {
            println!("      service           {service}");
        }

        println!("    CONCLUDED (supported by):");
        if device.inference.supporting.is_empty() {
            println!("      nothing");
        }
        for evidence in &device.inference.supporting {
            println!("      {:?} = {}", evidence.kind, evidence.value);
        }

        println!("    HOW IT GOT HERE:");
        for change in &device.inference.history {
            println!(
                "      {:?} -> {:?}/{:?} on {}",
                change.from, change.to, change.confidence, change.triggered_by.value
            );
        }
    }
}
