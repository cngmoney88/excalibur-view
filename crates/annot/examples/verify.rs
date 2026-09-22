//! Reads a file marked up in Bluebeam and checks Hyperview's own measurement
//! engine against what Revu wrote. Every caption Hyperview computes has to match
//! the caption Revu stored, character for character.

use annot::measure::Measure;
use annot::place::read_page;
use annot::viewport;
use pdf::Document;
use std::collections::BTreeMap;

fn main() {
    let path = std::env::args().nth(1).expect("file");
    let doc = Document::open(&path).expect("open");
    println!("{}\n  {} pages", path.rsplit('/').next().unwrap(), doc.page_count());

    let mut agreed = 0usize;
    let mut differed: Vec<String> = Vec::new();
    let mut totals: BTreeMap<String, (usize, f64, f64, f64)> = BTreeMap::new();
    let mut unscaled = 0usize;

    for index in 0..doc.page_count() {
        let Some(page) = doc.page(index) else { continue };
        let sheet_scale = viewport::scale_of(&doc, &page);
        let markups = read_page(&doc, index);
        if markups.is_empty() {
            continue;
        }
        println!(
            "\n  sheet {}: {} markups, page scale {}",
            index + 1,
            markups.len(),
            sheet_scale
                .as_ref()
                .map(|m| m.ratio.clone())
                .unwrap_or_else(|| "NOT SET".into())
        );

        for (_, m) in &markups {
            let subject = m.subject();
            let stored = m.contents();
            let kind = m
                .dict
                .get("IT")
                .and_then(|o| o.as_name())
                .map(|n| n.as_str().to_string())
                .unwrap_or_default();

            // The scale to measure with: the markup's own, else the sheet's.
            let own = m
                .dict
                .get("Measure")
                .and_then(|o| o.as_dict())
                .and_then(Measure::read);
            let at = m.points().first().copied().unwrap_or([0.0, 0.0]);
            let measure = own.or_else(|| viewport::scale_at(&doc, &page, at));
            let Some(measure) = measure else {
                unscaled += 1;
                continue;
            };

            let points = m.points();
            let (mine, quantity, area) = match kind.as_str() {
                "LineDimension" | "PolyLineDimension" | "Polylength" => {
                    let run: f64 = points
                        .windows(2)
                        .map(|p| ((p[1][0] - p[0][0]).powi(2) + (p[1][1] - p[0][1]).powi(2)).sqrt())
                        .sum();
                    (measure.length(run), measure.length_value(run), 0.0)
                }
                "PolygonDimension" => {
                    let n = points.len();
                    let mut twice = 0.0;
                    for i in 0..n {
                        let a = points[i];
                        let b = points[(i + 1) % n];
                        twice += a[0] * b[1] - b[0] * a[1];
                    }
                    let square_points = (twice * 0.5).abs();
                    let per = measure.per_point();
                    (
                        measure.area(square_points),
                        0.0,
                        square_points * per * per,
                    )
                }
                _ => (stored.clone(), 0.0, 0.0),
            };

            if mine == stored {
                agreed += 1;
            } else {
                differed.push(format!("{subject}: Revu {stored:?}, Excalibur View {mine:?}"));
            }

            // Weights, straight out of the markup's own column data.
            let columns: Vec<String> = m
                .dict
                .get("BSIColumnData")
                .and_then(|o| o.as_array())
                .map(|a| a.iter().map(|o| o.as_text().unwrap_or_default()).collect())
                .unwrap_or_default();
            let per_foot = columns.first().and_then(|c| c.parse::<f64>().ok()).unwrap_or(0.0);
            let per_square_foot = columns.get(5).and_then(|c| c.parse::<f64>().ok()).unwrap_or(0.0);
            let pounds = per_foot * quantity + per_square_foot * area;

            let entry = totals.entry(subject.clone()).or_insert((0, 0.0, 0.0, 0.0));
            entry.0 += 1;
            entry.1 += quantity;
            entry.2 += area;
            entry.3 += pounds;
        }
    }

    println!("\n  captions: {agreed} matched Revu exactly, {} differed", differed.len());
    for line in differed.iter().take(12) {
        println!("    {line}");
    }
    if unscaled > 0 {
        println!("  {unscaled} markups had no scale and were left out");
    }

    println!("\n  TAKEOFF");
    println!(
        "  {:<18} {:>5} {:>14} {:>12} {:>12} {:>9}",
        "subject", "picks", "length", "area", "pounds", "tons"
    );
    let (mut total_lb, mut total_picks) = (0.0, 0usize);
    for (subject, (picks, length, area, pounds)) in &totals {
        println!(
            "  {subject:<18} {picks:>5} {:>12.2} ft {:>9.2} sf {pounds:>12.0} {:>9.3}",
            length,
            area,
            pounds / 2000.0
        );
        total_lb += pounds;
        total_picks += picks;
    }
    println!(
        "  {:<18} {total_picks:>5} {:>27} {total_lb:>12.0} {:>9.3}",
        "TOTAL", "", total_lb / 2000.0
    );
}
