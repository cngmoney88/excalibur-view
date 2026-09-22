//! What Revu's own appearance streams do for a measurement caption: is there a
//! background behind the words, and where does a polyline put them?
use annot::place::read_page;
use pdf::Document;

fn main() {
    let doc = Document::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut shown = 0;
    for index in 0..doc.page_count() {
        for (_, m) in read_page(&doc, index) {
            let it = m
                .dict
                .get("IT")
                .and_then(|o| o.as_name())
                .map(|n| n.as_str().to_string())
                .unwrap_or_default();
            if m.contents().is_empty() {
                continue;
            }
            println!("candidate: {} [{}]", m.subject(), it);
            let ap = doc.at(&m.dict, "AP");
            let Some(ap) = ap.as_dict() else { continue };
            let n = doc.at(ap, "N");
            let Some(stream) = n.as_stream() else { continue };
            let Ok(bytes) = pdf::filters::decode(stream) else { continue };
            let text = String::from_utf8_lossy(&bytes);
            println!("=== {} [{}] {} pts, caption {:?}", m.subject(), it, m.points().len(), m.contents());
            println!("--- stream ({} bytes)", bytes.len());
            for line in text.lines().take(60) {
                println!("    {line}");
            }
            shown += 1;
            if shown >= 3 {
                return;
            }
        }
    }
}
