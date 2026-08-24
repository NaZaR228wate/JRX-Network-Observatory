//! Development harness: observe this machine's network identity and report
//! the cost against the 400 ms cold-start budget (ARCHITECTURE.md §7.1).
//!
//! Run with: cargo run -p jrx-collector --example observe

fn main() {
    let started = std::time::Instant::now();
    let result = jrx_collector::identity::observe();
    let elapsed = started.elapsed();

    match result {
        Ok(identity) => {
            println!("{}", serde_json::to_string_pretty(&identity).unwrap());
            println!("\nobserved in {} ms (budget: 400 ms)", elapsed.as_millis());
        }
        Err(e) => println!("observation failed: {e}"),
    }
}
