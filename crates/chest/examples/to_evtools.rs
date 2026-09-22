//! Converts a Revu profile or tool set into an Excalibur View tool chest.
//!
//!     cargo run -p chest --example to_evtools -- "Steel Takeoff.bpx"
//!
//! writes `Steel Takeoff.evtools` beside it and says what came across.
fn main() {
    for path in std::env::args().skip(1) {
        let path = std::path::PathBuf::from(path);
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("{}: could not be read", path.display());
            continue;
        };
        let Some(profile) = chest::Profile::read(&bytes) else {
            eprintln!("{}: not a tool chest", path.display());
            continue;
        };
        let from = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let out = path.with_extension(chest::native::EXTENSION);
        let written = chest::native::write(&profile, &format!("Revu: {from}"));
        std::fs::write(&out, &written).expect("write");
        let back = chest::native::read(&written).expect("reads back");
        let same = profile
            .sets
            .iter()
            .flat_map(|s| &s.tools)
            .zip(back.sets.iter().flat_map(|s| &s.tools))
            .filter(|(a, b)| {
                a.subject == b.subject
                    && a.kind == b.kind
                    && a.columns == b.columns
                    && a.properties_only == b.properties_only
                    && a.colour.iter().zip(b.colour.iter()).all(|(x, y)| (x - y).abs() < 0.003)
                    && a.pounds_per_foot() == b.pounds_per_foot()
                    && a.pounds_per_square_foot() == b.pounds_per_square_foot()
            })
            .count();
        println!("  {same} of {} tools identical after the round trip", profile.tool_count());
        let weighted = |p: &chest::Profile| p.sets.iter().flat_map(|s| &s.tools).filter(|t| t.carries_weight()).count();
        println!(
            "{} -> {}: {} sets, {} tools ({} with weights), {} columns; read back: {} tools ({} with weights)",
            from,
            out.display(),
            profile.sets.len(),
            profile.tool_count(),
            weighted(&profile),
            profile.custom_columns.len(),
            back.tool_count(),
            weighted(&back),
        );
    }
}
