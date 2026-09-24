//! Writes every icon onto one PDF page and renders it, so the whole set can be
//! looked at side by side. Icons that do not look like siblings are far easier
//! to spot together than one at a time on a toolbar.

use std::path::PathBuf;

fn main() {
    let out = PathBuf::from("/tmp/claude-0/iconsheet");
    std::fs::create_dir_all(&out).unwrap();

    let all: &[(&str, ui::icon::Glyph)] = ui::icon::CONTACT_SHEET;
    let across = 12usize;
    let cell = 56.0f64;
    let label = 12.0f64;
    let down = all.len().div_ceil(across);
    let width = across as f64 * cell + 24.0;
    let height = down as f64 * (cell + label) + 24.0;

    let mut body = String::new();
    body.push_str("1 J 1 j 0.12 0.12 0.12 rg 0 0 ");
    body.push_str(&format!("{width} {height} re f\n"));
    for (i, (name, glyph)) in all.iter().enumerate() {
        let col = (i % across) as f64;
        let row = (i / across) as f64;
        let x = 12.0 + col * cell;
        let y = height - 12.0 - (row + 1.0) * (cell + label) + label;
        let scale = (cell - 14.0) / 100.0;

        body.push_str("q 0.91 0.91 0.91 RG 0.91 0.91 0.91 rg 4 w\n");
        // The icon box is 100 by 100 with y down; a PDF has y up.
        body.push_str(&format!(
            "1 0 0 1 {:.3} {:.3} cm {:.5} 0 0 {:.5} 0 0 cm 1 0 0 -1 0 100 cm\n",
            x + 7.0,
            y,
            scale,
            scale
        ));
        for (d, filled) in *glyph {
            body.push_str(&to_pdf_path(d));
            body.push_str(if *filled { "f\n" } else { "S\n" });
        }
        body.push_str("Q\n");
        body.push_str(&format!(
            "q 0.62 0.62 0.62 rg BT /F1 5 Tf 1 0 0 1 {:.2} {:.2} Tm ({}) Tj ET Q\n",
            x + 3.0,
            y - 8.0,
            name.replace('(', "").replace(')', "")
        ));
    }

    let sheet = out.join("icons.pdf");
    write_one_page(&sheet, width, height, &body);
    let png = out.join("icons.png");
    hyperview::render::picture_of(library(), &sheet, 0, 2000, 2600, &png).unwrap();
    println!("{} icons -> {}", all.len(), png.display());
}

/// The icon path language is a subset of SVG's, and every one of its commands
/// has a PDF operator that means the same thing.
fn to_pdf_path(d: &str) -> String {
    let mut out = String::new();
    let mut command = ' ';
    let mut numbers: Vec<f64> = Vec::new();
    let flush = |command: char, numbers: &mut Vec<f64>, out: &mut String| {
        match command {
            'M' => {
                if numbers.len() >= 2 {
                    out.push_str(&format!("{} {} m\n", numbers[0], numbers[1]));
                }
            }
            'L' => {
                for pair in numbers.chunks(2) {
                    if pair.len() == 2 {
                        out.push_str(&format!("{} {} l\n", pair[0], pair[1]));
                    }
                }
            }
            'C' => {
                for six in numbers.chunks(6) {
                    if six.len() == 6 {
                        out.push_str(&format!(
                            "{} {} {} {} {} {} c\n",
                            six[0], six[1], six[2], six[3], six[4], six[5]
                        ));
                    }
                }
            }
            'Z' => out.push_str("h\n"),
            _ => {}
        }
        numbers.clear();
    };
    for token in d.split_whitespace() {
        match token {
            "M" | "L" | "C" | "Z" => {
                flush(command, &mut numbers, &mut out);
                command = token.chars().next().unwrap();
                if command == 'Z' {
                    flush('Z', &mut numbers, &mut out);
                    command = ' ';
                }
            }
            other => {
                if let Ok(v) = other.parse::<f64>() {
                    numbers.push(v);
                }
            }
        }
    }
    flush(command, &mut numbers, &mut out);
    out
}

fn write_one_page(to: &std::path::Path, width: f64, height: f64, body: &str) {
    let objects = [
        "<</Type/Catalog/Pages 2 0 R>>".to_string(),
        "<</Type/Pages/Kids[3 0 R]/Count 1>>".to_string(),
        format!(
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 {width} {height}]\
             /Resources<</Font<</F1 <</Type/Font/Subtype/Type1/BaseFont/Helvetica>> >> >>\
             /Contents 4 0 R>>"
        ),
        format!("<</Length {}>>\nstream\n{body}endstream", body.len()),
    ];
    let mut out: Vec<u8> = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for o in &offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<</Size {}/Root 1 0 R>>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    std::fs::write(to, out).unwrap();
}

fn library() -> Option<PathBuf> {
    for guess in [
        "third_party/pdfium/linux-x64/lib",
        "../third_party/pdfium/linux-x64/lib",
    ] {
        let path = PathBuf::from(guess);
        if path.join("libpdfium.so").exists() {
            return Some(path);
        }
    }
    None
}
