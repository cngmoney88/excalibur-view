//! Takes tools out of the user's own Tool Chest, puts them on a real sheet,
//! saves, and reopens — the whole round trip in one go.

use annot::markup::Markup;
use annot::measure::Measure;
use annot::measure::imperial;
use annot::place::{read_page, Placer};
use annot::viewport;
use chest::Profile;
use pdf::Document;

fn main() {
    let mut args = std::env::args().skip(1);
    let drawing = args.next().expect("drawing");
    let profile = args.next().expect("profile");
    let page: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);
    let out = args.next().unwrap_or_else(|| "/tmp/placed.pdf".into());

    let doc = Document::open(&drawing).expect("open drawing");
    let p = Profile::open(&profile).expect("open profile");
    let sheet = doc.page(page).expect("page");
    let area = doc.page_box(&sheet);
    println!(
        "{} page {} is {:.0} x {:.0} pt, box starts at {:.0},{:.0}",
        drawing.rsplit('/').next().unwrap(),
        page + 1,
        area[2] - area[0],
        area[3] - area[1],
        area[0],
        area[1]
    );

    let find = |want: &str| {
        p.sets
            .iter()
            .flat_map(|s| s.tools.iter())
            .find(|t| t.subject == want)
            .unwrap_or_else(|| panic!("no tool {want}"))
    };

    // The sheet's own scale, set once, is what every markup measures against.
    let mut setting = pdf::Update::new(&doc);
    viewport::set_scale(&doc, &mut setting, page, &imperial(48.0, "1/4\" = 1'-0\"", 16));
    let doc = pdf::Document::from_bytes(setting.apply(&doc));
    println!(
        "  sheet scale set to {}",
        viewport::scale_of(&doc, &doc.page(page).unwrap()).unwrap().ratio
    );

    let mut placer = Placer::new(&doc).by("Creede");
    let (ox, oy) = (area[0] + 300.0, area[1] + 260.0);

    for (i, name) in ["W12x26", "HSS6x6x1/4", "L4x4x1/2"].iter().enumerate() {
        let tool = find(name);
        let mut m = Markup::from_tool(&tool.annotation);
        let y = oy + i as f64 * 120.0;
        let (a, b) = ([ox, y], [ox + 720.0, y]);
        m.set_line(a, b);
        let run = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let sheet_scale = viewport::scale_of(&doc, &doc.page(page).unwrap()).unwrap();
        let caption = sheet_scale.length(run);
        m.set_contents(&caption);
        placer.add(page, &mut m).expect("placed");

        let feet = run / 72.0 * 4.0;
        println!(
            "  {name:<12} {caption:<12} over {run:.0} pt = {feet:.2} ft{}",
            tool.pounds_per_foot()
                .map(|w| format!(", {:.0} lb ({:.3} ton)", w * feet, w * feet / 2000.0))
                .unwrap_or_default()
        );
    }

    let plate = find("PL1/2");
    let mut m = Markup::from_tool(&plate.annotation);
    let x = ox + 820.0;
    let y = oy;
    m.set_vertices(&[[x, y], [x + 300.0, y], [x + 300.0, y + 220.0], [x, y + 220.0]]);
    let square_points = 300.0 * 220.0;
    let measure = viewport::scale_of(&doc, &doc.page(page).unwrap()).unwrap();
    let caption = measure.area(square_points);
    m.set_contents(&caption);
    placer.add(page, &mut m).unwrap();
    let square_feet = square_points * (4.0f64 / 72.0).powi(2);
    println!(
        "  {:<12} {caption:<12} = {square_feet:.2} sf{}",
        "PL1/2",
        plate
            .pounds_per_square_foot()
            .map(|w| format!(", {:.0} lb ({:.3} ton)", w * square_feet, w * square_feet / 2000.0))
            .unwrap_or_default()
    );

    let conn = find("Shear Conn");
    for i in 0..6 {
        let mut m = Markup::from_tool(&conn.annotation);
        let at = [ox + 120.0 * i as f64, oy + 400.0];
        let symbol: Vec<[f64; 2]> = (0..24)
            .map(|k| {
                let a = k as f64 / 24.0 * std::f64::consts::TAU;
                [at[0] + 9.0 * a.cos(), at[1] + 9.0 * a.sin()]
            })
            .collect();
        m.set_vertices(&symbol);
        m.set_contents("1");
        placer.add(page, &mut m).unwrap();
    }
    println!("  {:<12} 6 placed", "Shear Conn");

    let update = placer.finish();
    let saved = update.apply(&doc);
    std::fs::write(&out, &saved).unwrap();
    println!(
        "\n  wrote {out}: {} -> {} bytes, original untouched: {}",
        doc.bytes.len(),
        saved.len(),
        saved.starts_with(&doc.bytes)
    );

    let again = Document::open(&out).unwrap();
    let found = read_page(&again, page);
    println!("  reopened: {} markups on page {}", found.len(), page + 1);
    let mut tonnage = 0.0;
    for (_, m) in &found {
        let subject = m.subject();
        let contents = m.contents();
        let columns: Vec<String> = m
            .dict
            .get("BSIColumnData")
            .and_then(|o| o.as_array())
            .map(|a| a.iter().filter_map(|o| o.as_text()).collect())
            .unwrap_or_default();
        if let Some(weight) = columns.first().and_then(|c| c.parse::<f64>().ok()) {
            if let Some(measure) = m
                .dict
                .get("Measure")
                .and_then(|o| o.as_dict())
                .and_then(Measure::read)
            {
                let points = m
                    .points()
                    .windows(2)
                    .map(|p| ((p[1][0] - p[0][0]).powi(2) + (p[1][1] - p[0][1]).powi(2)).sqrt())
                    .sum::<f64>();
                tonnage += weight * measure.length_value(points) / 2000.0;
            }
        }
        println!("    {subject:<14} {contents:<12} columns {columns:?}");
    }
    println!("  linear steel on this sheet: {tonnage:.3} ton");
}
