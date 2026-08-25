//! Live M4 validation: what would the topology actually show right now?

use jrx_core::device::Category;
use jrx_core::topology::{GroupView, TopologyOverview};

fn main() {
    let identity = jrx_collector::identity::observe().expect("identity");
    let report = jrx_collector::discovery::observe(&identity).expect("discovery");

    let overview = TopologyOverview::build(&report.devices);
    println!("LEVEL 1 — overview ({} devices total)", overview.total);
    println!(
        "  centre:   {}",
        overview
            .center
            .as_ref()
            .map_or("none".into(), |c| c.display_name.clone())
    );
    println!(
        "  this Mac: {}",
        overview
            .self_node
            .as_ref()
            .map_or("none".into(), |c| c.display_name.clone())
    );
    let drawn = overview.groups.len()
        + usize::from(overview.center.is_some())
        + usize::from(overview.self_node.is_some());
    println!("  nodes drawn at level 1: {drawn}");
    for group in &overview.groups {
        println!(
            "    {:>14}  {:>4}{}",
            group.label,
            group.count,
            if group.collapsed_by_default {
                "  [collapsed]"
            } else {
                ""
            }
        );
        for fact in &group.facts {
            println!("        {:>4} {}", fact.count, fact.description);
        }
    }

    println!("\nLEVEL 2 — opening Unknown");
    let view = GroupView::build(&report.devices, Category::Unknown, 0);
    println!(
        "  {} members, page 1 of {}, {} nodes handed to the renderer",
        view.total,
        view.page_count,
        view.devices.len()
    );
    for node in view.devices.iter().take(4) {
        println!(
            "    {:<26} {}",
            node.display_name,
            node.vendor.clone().unwrap_or_else(|| "no vendor".into())
        );
    }

    println!(
        "\nquality: {:?} / local network {:?}",
        report.quality.verdict, report.quality.local_network
    );
    println!("  {}", report.quality.explanation);
    println!("\ntook {} ms", report.took_ms);
}
