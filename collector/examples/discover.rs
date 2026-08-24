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

    for source in &report.sources {
        println!("{:>12?}: {:?}", source.method, source.status);
    }
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

    let randomised = report.devices.iter().filter(|d| d.mac_randomised).count();
    let with_vendor = report.devices.iter().filter(|d| d.vendor.is_some()).count();
    let named = report
        .devices
        .iter()
        .filter(|d| d.hostname.is_some())
        .count();
    println!("\nwhy so many are unidentified:");
    println!("  randomised MAC (protecting identity): {randomised}");
    println!("  vendor known but type not:            {with_vendor}");
    println!("  announced a name:                     {named}");

    println!("\nidentified devices, with the evidence behind each:");
    for device in report
        .devices
        .iter()
        .filter(|d| d.category != jrx_core::device::Category::Unknown)
    {
        println!(
            "  [{:?}/{:?}] {}",
            device.category,
            device.confidence,
            device.display_name()
        );
        for evidence in &device.evidence {
            println!(
                "      {:?} = {} ({})",
                evidence.kind,
                evidence.value,
                evidence.method.label()
            );
        }
    }
}
