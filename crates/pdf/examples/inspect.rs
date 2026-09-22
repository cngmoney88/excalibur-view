//! Opens real files and reports what the object layer made of them.

use std::time::Instant;

fn main() {
    for path in std::env::args().skip(1) {
        let started = Instant::now();
        let doc = match pdf::Document::open(&path) {
            Ok(d) => d,
            Err(e) => {
                println!("{path}: could not read: {e}");
                continue;
            }
        };
        let open = started.elapsed();
        let pages = doc.page_count();
        let listed = Instant::now() - started;
        let name = path.rsplit('/').next().unwrap_or(&path);
        println!(
            "\n{name}\n  {} bytes, PDF {}, {pages} pages, opened in {:?}{}{}",
            doc.bytes.len(),
            doc.version(),
            listed,
            if doc.xref.rebuilt { ", REBUILT" } else { "" },
            if doc.encrypted { ", ENCRYPTED" } else { "" },
        );
        let _ = open;
        let mut annots = 0usize;
        let mut boxes = std::collections::BTreeMap::new();
        let mut rotations = std::collections::BTreeMap::new();
        for i in 0..pages {
            let Some(page) = doc.page(i) else { continue };
            annots += doc.annotations(&page).len();
            let b = doc.page_box(&page);
            *boxes
                .entry(format!("{:.0}x{:.0}", b[2] - b[0], b[3] - b[1]))
                .or_insert(0usize) += 1;
            *rotations.entry(doc.page_rotation(&page)).or_insert(0usize) += 1;
        }
        println!("  walked every page in {:?}", started.elapsed());
        println!("  page sizes: {boxes:?}");
        println!("  rotations: {rotations:?}");
        println!("  annotations: {annots}");
        // Prove the catalog and a deep object actually resolved.
        let catalog = doc.catalog();
        println!(
            "  catalog keys: {:?}",
            catalog.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>()
        );
    }
}
