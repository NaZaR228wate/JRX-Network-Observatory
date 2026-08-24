//! Development harness: print the Visibility Panel as the UI receives it.
//!
//! Run with: cargo run -p jrx-collector --example visibility

use jrx_collector::registry::ALL_PROBES;
use jrx_core::capability::CapabilityMatrix;
use jrx_core::declaration::Platform;

fn main() {
    #[cfg(target_os = "macos")]
    let perms = jrx_collector::macos::permissions::observe();
    #[cfg(not(target_os = "macos"))]
    let perms = jrx_core::capability::PermissionSet::new();

    let m = CapabilityMatrix::build(
        ALL_PROBES,
        Platform::current().expect("supported platform"),
        &perms,
    );

    println!("== OBSERVED ==");
    for r in m.rows() {
        if let jrx_core::capability::CapabilityState::Observed { mechanism } = r.state {
            println!("  {} — {}", r.describes, mechanism);
        }
    }
    println!("\n== AVAILABLE ==");
    for r in m.rows() {
        if let jrx_core::capability::CapabilityState::Available { missing, certainty } = r.state {
            println!(
                "  {} — needs {} [{:?}]",
                r.describes,
                missing.label(),
                certainty
            );
        }
    }
    println!("\n== NOT POSSIBLE ==");
    for l in m.limitations() {
        println!("  {}", l.describes);
    }
    println!("\n== REFUSED BY DESIGN ==");
    for r in m.refused() {
        println!("  {:?}", r.class);
    }
    println!("\nsummary: {:?}", m.summary());

    if std::env::args().any(|a| a == "--json") {
        println!("---JSON---");
        println!("{}", serde_json::to_string(&m).unwrap());
    }
}
