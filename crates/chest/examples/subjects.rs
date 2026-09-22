//! Lists every tool's set, subject, kind and weight in a chest or profile.
fn main() {
    for path in std::env::args().skip(1) {
        println!("\n=== {path}");
        match chest::Profile::open(&path) {
            Some(p) => {
                for s in &p.sets {
                    let subjects: Vec<String> = s
                        .tools
                        .iter()
                        .map(|t| {
                            format!(
                                "{}[{:?}]{}",
                                t.subject,
                                t.kind,
                                t.pounds_per_foot().map(|w| format!(" {w}plf")).unwrap_or_default()
                            )
                        })
                        .collect();
                    println!("  {} ({}): {}", s.title, s.tools.len(), subjects.join(", "));
                }
            }
            None => println!("  unreadable"),
        }
    }
}
