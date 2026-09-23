//! Runs the takeoff over a real marked up drawing set.

use takeoff::{columns::Column, read_document, summarise, WeightColumns};

fn main() {
    let path = std::env::args().nth(1).expect("file");
    let doc = pdf::Document::open(&path).expect("open");
    let rows = read_document(&doc);
    let weights = WeightColumns::default();

    println!("{}\n", path.rsplit('/').next().unwrap());
    let shown = [
        Column::Page,
        Column::Subject,
        Column::Measurement,
        Column::Count,
        Column::Length,
        Column::Area,
        Column::UserDefined(0),
        Column::UserDefined(5),
        Column::Pounds,
        Column::Tons,
    ];
    let widths = [5usize, 14, 12, 6, 10, 11, 8, 8, 9, 7];
    let head: String = shown
        .iter()
        .zip(widths)
        .map(|(c, w)| format!("{:<w$} ", c.heading(), w = w))
        .collect();
    println!("  {head}");
    for row in rows.iter().take(10) {
        let line: String = shown
            .iter()
            .zip(widths)
            .map(|(c, w)| format!("{:<w$} ", c.text(row, weights), w = w))
            .collect();
        println!("  {line}");
    }
    if rows.len() > 10 {
        println!("  … {} more", rows.len() - 10);
    }

    let s = summarise(&rows, weights);
    println!(
        "\n  {:<16} {:>10} {:>6} {:>12} {:>12} {:>10} {:>8}",
        "subject", "kind", "picks", "length ft", "area sf", "pounds", "tons"
    );
    for g in &s.groups {
        println!(
            "  {:<16} {:>10} {:>6} {:>12.2} {:>12.2} {:>10.0} {:>8.3}",
            g.subject,
            g.kind.name(),
            g.picks,
            g.length,
            g.area,
            g.pounds,
            g.tons()
        );
    }
    println!(
        "  {:<16} {:>10} {:>6} {:>25} {:>10.0} {:>8.3}",
        "TOTAL", "", s.picks, "", s.pounds, s.tons()
    );
    if s.is_short() {
        println!(
            "\n  WARNING: {} measurements left out, sheets {:?}",
            s.unscaled, s.unscaled_pages
        );
    }
    std::fs::write("/tmp/takeoff.csv", takeoff::summary::summary_csv(&s)).unwrap();
    std::fs::write(
        "/tmp/markups.csv",
        takeoff::summary::to_csv(&rows, &shown, weights),
    )
    .unwrap();
    println!("\n  wrote /tmp/takeoff.csv and /tmp/markups.csv");
}
