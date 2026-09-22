fn main(){
    let p = chest::Profile::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut all = p.commands();
    if let Some(second) = std::env::args().nth(2) {
        if let Some(q) = chest::Profile::open(second) {
            for c in q.commands() { if !all.contains(&c) { all.push(c); } }
        }
    }
    all.sort();
    println!("{} commands", all.len());
    let mut by_group: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for c in &all {
        let g = c.split('.').next().unwrap_or("").to_string();
        by_group.entry(g).or_default().push(c.clone());
    }
    for (g, items) in by_group {
        println!("\n{g} ({}):", items.len());
        for chunk in items.chunks(6) { println!("   {}", chunk.join(", ")); }
    }
}
