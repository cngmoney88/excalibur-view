//! Imports a real profile and reports what came across.

use chest::Profile;
use std::time::Instant;

fn main() {
    for path in std::env::args().skip(1) {
        let started = Instant::now();
        let Some(p) = Profile::open(&path) else {
            println!("{path}: not a profile");
            continue;
        };
        println!(
            "\n=== {} ({} tools in {} sets, read in {:?})",
            p.name,
            p.tool_count(),
            p.sets.len(),
            started.elapsed()
        );
        println!(
            "  left panel {:.0}px, active {:?}, tabs: {:?}",
            p.panels.left.size, p.panels.left.active, p.panels.left.tabs
        );
        println!(
            "  bottom: {:?} (collapsed {})",
            p.panels.bottom.tabs, p.panels.bottom.collapsed
        );
        println!(
            "  {} toolbars, {} visible; {} distinct commands",
            p.toolbars.len(),
            p.toolbars.iter().filter(|t| t.visible).count(),
            p.commands().len()
        );
        if let Some(m) = p.toolbar("toolStripMeasure") {
            println!("  measure bar: {:?}", m.shown());
        }
        let visible: Vec<&str> = p
            .markup_columns
            .iter()
            .filter(|c| c.visible)
            .map(|c| c.key.as_str())
            .collect();
        println!(
            "  markup columns: {} defined, showing {:?}",
            p.markup_columns.len(),
            visible
        );

        let mut weighted = 0;
        let mut kinds = std::collections::BTreeMap::new();
        for set in &p.sets {
            weighted += set.weighted();
            for tool in &set.tools {
                *kinds.entry(tool.kind.name()).or_insert(0usize) += 1;
            }
        }
        println!("  kinds: {kinds:?}");
        println!("  tools carrying a weight: {weighted}");

        for set in &p.sets {
            if set.tools.len() < 8 {
                continue;
            }
            println!(
                "   {:<24} {:>4} tools, {:>4} weighted",
                set.title,
                set.tools.len(),
                set.weighted()
            );
        }

        // Spot check the ones that matter.
        for want in ["W12x26", "HSS6x6x1/4", "PL1/2", "Bar Grate 1x3/16", "Shear Conn", "L4x4x1/2"] {
            let found = p
                .sets
                .iter()
                .flat_map(|s| s.tools.iter())
                .find(|t| t.subject == want);
            match found {
                Some(t) => {
                    let measure = t.annotation.get("Measure").and_then(|o| o.as_dict()).is_some();
                    println!(
                        "   {:<18} {:<9} lb/ft {:>8} psf {:>8}  scale dict {}  colour {:?}",
                        t.subject,
                        t.kind.name(),
                        t.pounds_per_foot().map(|v| format!("{v:.2}")).unwrap_or("-".into()),
                        t.pounds_per_square_foot().map(|v| format!("{v:.2}")).unwrap_or("-".into()),
                        if measure { "resolved" } else { "none" },
                        t.colour.map(|c| (c * 100.0).round() / 100.0),
                    );
                }
                None => println!("   {want:<18} NOT FOUND"),
            }
        }
        // And a worked tonnage, to prove the numbers are usable as they stand.
        if let Some(beam) = p
            .sets
            .iter()
            .flat_map(|s| s.tools.iter())
            .find(|t| t.subject == "W12x26")
        {
            if let Some(per_foot) = beam.pounds_per_foot() {
                let run_feet = 40.0;
                println!(
                    "   worked example: 40'-0\" of W12x26 = {:.0} lb = {:.3} ton",
                    per_foot * run_feet,
                    per_foot * run_feet / 2000.0
                );
            }
        }
    }
}
