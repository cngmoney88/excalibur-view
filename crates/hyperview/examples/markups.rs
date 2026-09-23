//! Draws one of every markup through pdfium, so the whole set can be looked at
//! before it goes out. A markup that is drawn wrong still passes every test
//! that only checks for the right operators being present.

use std::path::PathBuf;

fn main() {
    let out = PathBuf::from("/tmp/claude-0/markshot");
    std::fs::create_dir_all(&out).unwrap();
    let sheet = out.join("markups.pdf");
    blank(&sheet, 1224.0, 1584.0);

    let doc = pdf::Document::open(&sheet).unwrap();
    let mut placer = annot::place::Placer::new(&doc).by("Creede");

    let mut row = 0usize;
    let mut col = 0usize;
    let cell = [380.0f64, 240.0];
    let mut next = || {
        let at = [
            40.0 + col as f64 * (cell[0] + 20.0),
            1584.0 - 60.0 - (row as f64 + 1.0) * (cell[1] + 40.0),
        ];
        col += 1;
        if col == 3 {
            col = 0;
            row += 1;
        }
        [at[0], at[1], at[0] + cell[0], at[1] + cell[1]]
    };

    let put = |placer: &mut annot::place::Placer, mut m: annot::Markup, label: &str, area: [f64; 4]| {
        placer.add(0, &mut m);
        let mut tag = annot::Markup::new(annot::Subtype::FreeText);
        tag.set_box([area[0], area[1] - 26.0, area[2], area[1] - 4.0]);
        tag.set_contents(label);
        tag.set_width(0.0);
        annot::text::Setting {
            size: 11.0,
            colour: [0.35, 0.35, 0.35],
            ..Default::default()
        }
        .onto(&mut tag);
        placer.add(0, &mut tag);
    };

    // 1. A cloud.
    let area = next();
    let mut m = annot::Markup::new(annot::Subtype::Square);
    m.set_colour([0.85, 0.15, 0.12]).set_width(2.0);
    m.set_box(area);
    cloudy(&mut m, 2.0);
    put(&mut placer, m, "Cloud", area);

    // 2. A polygon cloud.
    let area = next();
    let mut m = annot::Markup::new(annot::Subtype::Polygon);
    m.set_colour([0.85, 0.15, 0.12]).set_width(2.0);
    m.set_vertices(&[
        [area[0] + 20.0, area[1] + 20.0],
        [area[2] - 40.0, area[1] + 10.0],
        [area[2] - 10.0, area[3] - 40.0],
        [area[0] + 120.0, area[3] - 10.0],
    ]);
    cloudy(&mut m, 2.0);
    put(&mut placer, m, "Polygon cloud", area);

    // 3. An arc.
    let area = next();
    let mut m = annot::Markup::new(annot::Subtype::PolyLine);
    m.set_colour([0.12, 0.45, 0.85]).set_width(3.0);
    let points = hyperview::sheet::arc_through(
        [area[0] + 20.0, area[1] + 30.0],
        [area[2] - 20.0, area[1] + 30.0],
        [(area[0] + area[2]) * 0.5, area[3] - 20.0],
    );
    m.set_vertices(&points);
    put(&mut placer, m, "Arc", area);

    // 4. A dimension line.
    let area = next();
    let mut m = annot::Markup::new(annot::Subtype::Line);
    m.set_colour([0.15, 0.15, 0.15]).set_width(2.0);
    m.set_line([area[0] + 20.0, (area[1] + area[3]) * 0.5], [area[2] - 20.0, (area[1] + area[3]) * 0.5]);
    m.set("LE", pdf::Object::Array(vec![
        pdf::Object::name("OpenArrow"),
        pdf::Object::name("OpenArrow"),
    ]));
    m.set_contents("24'-6\"");
    put(&mut placer, m, "Dimension", area);

    // 5. A callout.
    let area = next();
    let mut m = annot::Markup::new(annot::Subtype::FreeText);
    m.set_colour([0.15, 0.15, 0.15]).set_width(1.5);
    m.set_box([area[0] + 120.0, area[1] + 90.0, area[2] - 10.0, area[3] - 20.0]);
    m.set_contents("FIELD VERIFY BEFORE FABRICATION");
    m.set("CL", pdf::Object::Array(
        [area[0] + 20.0, area[1] + 20.0, area[0] + 120.0, area[1] + 90.0]
            .iter().map(|v| pdf::Object::real(*v)).collect()));
    put(&mut placer, m, "Callout", area);

    // 6. A hatched area.
    let area = next();
    let mut m = annot::Markup::new(annot::Subtype::Polygon);
    m.set_colour([0.15, 0.55, 0.25]).set_width(2.0);
    m.set_vertices(&[
        [area[0] + 20.0, area[1] + 20.0],
        [area[2] - 20.0, area[1] + 20.0],
        [area[2] - 20.0, area[3] - 20.0],
        [area[0] + 160.0, area[3] - 20.0],
        [area[0] + 160.0, area[1] + 120.0],
        [area[0] + 20.0, area[1] + 120.0],
    ]);
    m.dict.set("BSIFillType", pdf::Object::name("Diagonal"));
    put(&mut placer, m, "Hatched area (concave)", area);

    // 7. Cross hatch in a circle.
    let area = next();
    let mut m = annot::Markup::new(annot::Subtype::Circle);
    m.set_colour([0.55, 0.15, 0.65]).set_width(2.0);
    m.set_box([area[0] + 20.0, area[1] + 20.0, area[2] - 20.0, area[3] - 20.0]);
    m.dict.set("BSIFillType", pdf::Object::name("Cross"));
    put(&mut placer, m, "Cross hatch", area);

    // 8. A text box with a font.
    let area = next();
    let mut m = annot::Markup::new(annot::Subtype::FreeText);
    m.set_colour([0.15, 0.15, 0.15]).set_width(1.0);
    m.set_box([area[0] + 10.0, area[1] + 10.0, area[2] - 10.0, area[3] - 10.0]);
    m.set_contents("ALL STRUCTURAL STEEL TO BE ASTM A992 GRADE 50 UNLESS NOTED OTHERWISE ON THE DRAWINGS");
    annot::text::Setting {
        family: annot::text::Family::Times,
        size: 16.0,
        bold: true,
        across: annot::text::Across::Middle,
        ..Default::default()
    }.onto(&mut m);
    put(&mut placer, m, "Text box, Times bold, centred", area);

    // 9. A dashed line with endings.
    let area = next();
    let mut m = annot::Markup::new(annot::Subtype::Line);
    m.set_colour([0.85, 0.45, 0.05]).set_width(3.0);
    m.set_line([area[0] + 20.0, area[1] + 40.0], [area[2] - 20.0, area[3] - 40.0]);
    let mut border = pdf::Dict::new();
    border.set("S", pdf::Object::name("D"));
    border.set("D", pdf::Object::Array(vec![pdf::Object::real(9.0), pdf::Object::real(6.0)]));
    m.dict.set("BS", pdf::Object::Dict(border));
    m.set("LE", pdf::Object::Array(vec![
        pdf::Object::name("Circle"),
        pdf::Object::name("ClosedArrow"),
    ]));
    put(&mut placer, m, "Dashed, circle to arrow", area);

    let updated = placer.finish().apply(&doc);
    std::fs::write(&sheet, updated).unwrap();

    let png = out.join("markups.png");
    hyperview::render::picture_of(library(), &sheet, 0, 1400, 1800, &png).unwrap();
    println!("{}", png.display());
}

fn cloudy(m: &mut annot::Markup, intensity: f64) {
    let mut effect = pdf::Dict::new();
    effect.set("S", pdf::Object::name("C"));
    effect.set("I", pdf::Object::real(intensity));
    m.dict.set("BE", pdf::Object::Dict(effect));
}

fn blank(to: &std::path::Path, width: f64, height: f64) {
    let objects = [
        "<</Type/Catalog/Pages 2 0 R>>".to_string(),
        "<</Type/Pages/Kids[3 0 R]/Count 1>>".to_string(),
        format!("<</Type/Page/Parent 2 0 R/MediaBox[0 0 {width} {height}]>>"),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for o in &offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!(
        "trailer\n<</Size {}/Root 1 0 R>>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    std::fs::write(to, out).unwrap();
}

fn library() -> Option<PathBuf> {
    for guess in ["third_party/pdfium/linux-x64/lib", "../third_party/pdfium/linux-x64/lib"] {
        let path = PathBuf::from(guess);
        if path.join("libpdfium.so").exists() {
            return Some(path);
        }
    }
    None
}
