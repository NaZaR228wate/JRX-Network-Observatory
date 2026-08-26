//! Dump a fixture as the exact payload the UI receives.
//!
//! Used by the development-only preview page, so visual review runs on the
//! real pipeline's output rather than hand-written data.
//!
//! cargo run -p jrx-collector --features fixtures --example fixture_json -- home_wifi

#[cfg(feature = "fixtures")]
fn main() {
    use jrx_collector::fixtures::{Fixture, capabilities};

    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "home_wifi".into());
    let fixture = Fixture::parse(&name).unwrap_or_else(|| panic!("unknown fixture {name}"));

    let report = fixture.report();

    // Every group page, produced by the real GroupView. The preview looks these
    // up rather than re-deriving them in TypeScript, so what is reviewed
    // visually is what production computes.
    let mut group_pages = serde_json::Map::new();
    for category in jrx_core::device::Category::ORDER {
        let first = jrx_core::topology::GroupView::build(&report.devices, category, 0);
        let pages: Vec<_> = (0..first.page_count)
            .map(|page| jrx_core::topology::GroupView::build(&report.devices, category, page))
            .collect();
        group_pages.insert(
            serde_json::to_value(category)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            serde_json::to_value(pages).unwrap(),
        );
    }

    let payload = serde_json::json!({
        "fixture": fixture.name(),
        "identity": { "identity": fixture.identity(), "observed_in_ms": 214 },
        "capabilities": capabilities(fixture),
        "report": report,
        "group_pages": group_pages,
    });
    println!("{}", serde_json::to_string(&payload).unwrap());
}

#[cfg(not(feature = "fixtures"))]
fn main() {
    eprintln!("build with --features fixtures");
    std::process::exit(1);
}
