//! Reads the markups already in a real file, then writes one of our own back in
//! and proves the file still opens with everything intact.

use pdf::{Document, Name, Object, Ref, Update};
use std::collections::BTreeMap;

fn show(doc: &Document, page: usize, reference: Ref) -> Option<String> {
    let object = doc.get(reference);
    let d = object.as_dict()?;
    let sub = d.get("Subtype").and_then(|o| o.as_name())?.as_str().to_string();
    let it = doc.at(d, "IT").as_name().map(|n| n.as_str().to_string()).unwrap_or_default();
    let subj = doc
        .at(d, "Subj")
        .as_string()
        .map(|s| String::from_utf8_lossy(s).to_string())
        .unwrap_or_default();
    let contents = doc
        .at(d, "Contents")
        .as_string()
        .map(|s| String::from_utf8_lossy(s).to_string())
        .unwrap_or_default();
    let mt = doc.at(d, "MeasurementTypes").as_i64().unwrap_or(0);
    let cols = doc.at(d, "BSIColumnData");
    let cols = cols
        .as_array()
        .map(|a| {
            a.iter()
                .map(|o| String::from_utf8_lossy(o.as_string().unwrap_or(b"")).to_string())
                .collect::<Vec<_>>()
                .join("|")
        })
        .unwrap_or_default();
    let measure = doc.at(d, "Measure");
    let scale = measure
        .as_dict()
        .and_then(|m| m.get("R"))
        .and_then(|o| o.as_string())
        .map(|s| String::from_utf8_lossy(s).to_string())
        .unwrap_or_default();
    Some(format!(
        "p{page:<4} {sub:<10} {it:<20} mt={mt:<5} {subj:<28} {contents:<18} [{cols}] {scale}"
    ))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = &args[0];
    let doc = Document::open(path).expect("open");
    println!("{path}\n  {} pages", doc.page_count());

    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut subjects: BTreeMap<String, usize> = BTreeMap::new();
    let mut shown = 0;
    let mut total = 0;
    for i in 0..doc.page_count() {
        let Some(page) = doc.page(i) else { continue };
        for reference in doc.annotations(&page) {
            total += 1;
            let object = doc.get(reference);
            let Some(d) = object.as_dict() else { continue };
            let sub = d
                .get("Subtype")
                .and_then(|o| o.as_name())
                .map(|n| n.as_str().to_string())
                .unwrap_or_default();
            *kinds.entry(sub).or_default() += 1;
            if let Some(s) = doc.at(d, "Subj").as_string() {
                *subjects
                    .entry(String::from_utf8_lossy(s).to_string())
                    .or_default() += 1;
            }
            if shown < 12 {
                if let Some(line) = show(&doc, i, reference) {
                    println!("  {line}");
                    shown += 1;
                }
            }
        }
    }
    println!("  {total} annotations");
    println!("  subtypes: {kinds:?}");
    let mut top: Vec<(&String, &usize)> = subjects.iter().collect();
    top.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    println!(
        "  subjects: {:?}",
        top.iter().take(12).map(|(s, n)| format!("{s} x{n}")).collect::<Vec<_>>()
    );

    // --- now write one of ours in, on the first page, and reopen ---
    let page_ref = doc.pages()[0];
    let mut page = doc.page(0).unwrap();
    let before: Vec<Ref> = doc.annotations(&page);

    let mut update = Update::new(&doc);
    let mut annot = pdf::Dict::new();
    annot.set(Name::new("Type"), Object::name("Annot"));
    annot.set(Name::new("Subtype"), Object::name("Line"));
    annot.set(Name::new("IT"), Object::name("LineDimension"));
    annot.set(Name::new("Subj"), Object::text("W12x26"));
    annot.set(Name::new("Contents"), Object::text("20'-0\""));
    annot.set(Name::new("MeasurementTypes"), Object::Int(130));
    annot.set(
        Name::new("BSIColumnData"),
        Object::Array(vec![Object::text("26.00"), Object::text("520.00")]),
    );
    annot.set(
        Name::new("L"),
        Object::Array(vec![
            Object::real(100.0),
            Object::real(100.0),
            Object::real(460.0),
            Object::real(100.0),
        ]),
    );
    annot.set(
        Name::new("Rect"),
        Object::Array(vec![
            Object::real(95.0),
            Object::real(95.0),
            Object::real(465.0),
            Object::real(115.0),
        ]),
    );
    annot.set(Name::new("F"), Object::Int(4));
    annot.set(
        Name::new("C"),
        Object::Array(vec![Object::real(1.0), Object::real(0.0), Object::real(0.0)]),
    );
    let added = update.add(Object::Dict(annot));

    let mut list: Vec<Object> = before.iter().map(|r| Object::Ref(*r)).collect();
    list.push(Object::Ref(added));
    page.set(Name::new("Annots"), Object::Array(list));
    update.replace(page_ref, Object::Dict(page));

    let saved = update.apply(&doc);
    let out = format!("/tmp/marked-{}", path.rsplit('/').next().unwrap());
    std::fs::write(&out, &saved).unwrap();
    println!(
        "\n  wrote {out}\n  original {} bytes -> {} bytes (+{}), original bytes untouched: {}",
        doc.bytes.len(),
        saved.len(),
        saved.len() - doc.bytes.len(),
        saved.starts_with(&doc.bytes)
    );

    let again = Document::open(&out).expect("reopen");
    let page = again.page(0).unwrap();
    let after = again.annotations(&page);
    println!(
        "  reopened: {} pages, page 1 had {} annotations and now has {}",
        again.page_count(),
        before.len(),
        after.len()
    );
    let ours = again.get(added);
    let d = ours.as_dict().expect("our annotation came back");
    println!(
        "  ours reads back: Subj={:?} Contents={:?} cols={:?}",
        String::from_utf8_lossy(again.at(d, "Subj").as_string().unwrap_or(b"")),
        String::from_utf8_lossy(again.at(d, "Contents").as_string().unwrap_or(b"")),
        again
            .at(d, "BSIColumnData")
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0)
    );
}
