//! Development harness: what does macOS actually say about our permissions?
//!
//! Run with: cargo run -p jrx-collector --example permissions

fn main() {
    #[cfg(target_os = "macos")]
    {
        use jrx_collector::macos::permissions;
        println!(
            "Location Services : {:?}",
            permissions::location_services_state()
        );
        println!(
            "Local Network     : {:?}",
            permissions::local_network_state()
        );
        println!("\nfull set: {:#?}", permissions::observe());
    }
}
