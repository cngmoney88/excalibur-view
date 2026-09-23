fn main() {
    let path = std::env::args().nth(1).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let p = chest::Profile::read(&bytes).unwrap();
    let tools: usize = p.sets.iter().map(|s| s.tools.len()).sum();
    let mut subjects: Vec<&str> = p.sets.iter().flat_map(|s| s.tools.iter().map(|t| t.subject.as_str())).collect();
    subjects.sort();
    let all = subjects.len();
    subjects.dedup();
    println!("sets: {}", p.sets.len());
    println!("tools: {tools}");
    println!("distinct subjects: {}", subjects.len());
    println!("(duplicate subjects across sets: {})", all - subjects.len());
    println!("\nsets, largest first:");
    let mut sets: Vec<(&str, usize)> = p.sets.iter().map(|s| (s.title.as_str(), s.tools.len())).collect();
    sets.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (title, n) in &sets { println!("  {n:>5}  {title}"); }
}
