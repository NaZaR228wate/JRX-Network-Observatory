//! Record what JRX actually observes on the current network.
//!
//! cargo run -p jrx-collector --example live_report

use jrx_core::device::Category;
use jrx_core::topology::{GroupView, TopologyOverview};

fn main() {
    let identity = jrx_collector::identity::observe().expect("identity");

    println!("== ENVIRONMENT ==");
    println!("  connection      {:?}", identity.connection);
    println!(
        "  interface       {} ({:?})",
        identity.interface, identity.interface_label
    );
    println!("  local address   {:?}", identity.local_ip);
    println!("  subnet          {:?}", identity.subnet);
    println!("  gateway         {:?}", identity.gateway);
    println!("  wifi            {:?}", identity.wifi);
    println!("  tunnel          {:?}", identity.tunnel);
    println!("  other active    {:?}", identity.other_active.len());

    let report = jrx_collector::discovery::observe(&identity).expect("discovery");
    let overview = TopologyOverview::build(&report.devices);

    println!("\n== DISCOVERY ==");
    for s in &report.quality.sources {
        println!("  {:>10?}  {:?}", s.method, s.status);
    }
    println!("  verdict         {:?}", report.quality.verdict);
    println!("  local network   {:?}", report.quality.local_network);
    println!("  isolation       {:?}", report.summary.isolation);
    println!("  took            {} ms", report.took_ms);

    println!("\n== LEVEL 1 ==");
    let drawn = overview.groups.len()
        + usize::from(overview.center.is_some())
        + usize::from(overview.self_node.is_some());
    println!("  total devices   {}", overview.total);
    println!("  nodes drawn     {drawn}");
    println!(
        "  centre          {:?}",
        overview.center.as_ref().map(|c| &c.display_name)
    );
    println!(
        "  this Mac        {:?}",
        overview.self_node.as_ref().map(|c| &c.display_name)
    );
    for g in &overview.groups {
        println!(
            "    {:>14}  {:>4}{}",
            g.label,
            g.count,
            if g.collapsed_by_default {
                "  [collapsed]"
            } else {
                ""
            }
        );
        for f in &g.facts {
            println!("        {:>4} {}", f.count, f.description);
        }
    }

    println!("\n== LEVEL 2 (Unidentified) ==");
    let page = GroupView::build(&report.devices, Category::Unknown, 0);
    println!(
        "  members {} · pages {} · nodes on screen {}",
        page.total,
        page.page_count,
        page.devices.len()
    );
}
