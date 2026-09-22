//! Keeping the markups that a flatten turned into paint.
//!
//! Flattening is the one operation that genuinely destroys something:
//! a markup becomes marks on the page, and marks on a page cannot be read back
//! as a markup by looking at them. Every program that flattens has this
//! problem, and most answer it with "keep the original file".
//!
//! Hyperview keeps the original file too — nothing is ever done in place — but
//! it also writes the markups it flattened into the flattened file, under a
//! key of its own in the catalog. They are not annotations there: they are a
//! record of what the paint used to be. Unflatten reads them back and puts
//! them on as annotations again.
//!
//! A set flattened by Revu, by Acrobat or by anything else carries no such
//! record, and Hyperview says so plainly rather than pretending it can undo
//! what it never did.

use pdf::{Dict, Object};

/// The catalog key the record lives under. Named for this program because it
/// is this program's record: no other reader knows or needs to know about it,
/// and a reader that ignores it loses nothing.
pub const KEY: &str = "ExcaliburFlattened";

/// Writes a record of the markups about to be flattened.
///
/// `marks` are the page each markup was on and the annotation dictionary it
/// was, exactly as it stood before the flatten.
pub fn write(update: &mut pdf::write::Update, file: &pdf::Document, marks: &[(u32, Dict)]) {
    if marks.is_empty() {
        return;
    }
    let Some(catalog_ref) = file.xref.trailer.get("Root").and_then(|o| o.as_ref()) else {
        return;
    };
    let mut kept: Vec<Object> = Vec::with_capacity(marks.len() * 2);
    for (page, dict) in marks {
        let mut one = dict.clone();
        // The appearance stream belonged to the file as it was. It is the
        // paint's business now, and a stale reference would point at nothing.
        one.remove("AP");
        one.remove("Popup");
        kept.push(Object::Int(*page as i64));
        kept.push(Object::Dict(one));
    }
    let list = update.add(Object::Array(kept));
    let mut catalog = file.catalog();
    catalog.set(KEY, Object::Ref(list));
    update.replace(catalog_ref, Object::Dict(catalog));
}

/// Reads back what a flatten kept, as the page and the annotation it was.
pub fn read(file: &pdf::Document) -> Vec<(u32, Dict)> {
    let catalog = file.catalog();
    let Some(list) = catalog.get(KEY) else {
        return Vec::new();
    };
    let list = file.follow(list);
    let Some(items) = list.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 1 < items.len() {
        let page = file.follow(&items[at]).as_i64().unwrap_or(-1);
        let dict = file.follow(&items[at + 1]).as_dict().cloned();
        at += 2;
        if let (true, Some(dict)) = (page >= 0, dict) {
            out.push((page as u32, dict));
        }
    }
    out
}

/// Takes the record out, once the markups have been put back.
///
/// Leaving it would mean a file that says it still holds flattened markups
/// after they have been lifted, and a second Unflatten would put a second copy
/// of every one of them on the sheet.
pub fn clear(file: &pdf::Document, update: &mut pdf::write::Update) {
    let Some(catalog_ref) = file.xref.trailer.get("Root").and_then(|o| o.as_ref()) else {
        return;
    };
    let mut catalog = file.catalog();
    if !catalog.has(KEY) {
        return;
    }
    catalog.remove(KEY);
    update.replace(catalog_ref, Object::Dict(catalog));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_file() -> pdf::Document {
        let objects = [
            "<</Type/Catalog/Pages 2 0 R>>",
            "<</Type/Pages/Kids[3 0 R]/Count 1>>",
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>",
        ];
        let mut out = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
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
        pdf::Document::from_bytes(out)
    }

    fn a_markup(subject: &str) -> Dict {
        let mut dict = Dict::new();
        dict.set("Type", Object::name("Annot"));
        dict.set("Subtype", Object::name("Square"));
        dict.set("Subj", Object::text(subject));
        dict.set("Rect", Object::Array(vec![
            Object::Int(10),
            Object::Int(10),
            Object::Int(100),
            Object::Int(60),
        ]));
        dict
    }

    #[test]
    fn a_file_nobody_flattened_has_nothing_to_lift() {
        assert!(read(&a_file()).is_empty());
    }

    #[test]
    fn what_was_kept_comes_back_the_way_it_went_in() {
        let file = a_file();
        let mut update = pdf::write::Update::new(&file);
        write(&mut update, &file, &[(0, a_markup("W12x26")), (2, a_markup("SLAB"))]);
        let bytes = update.apply(&file);

        let again = pdf::Document::from_bytes(bytes);
        let kept = read(&again);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].0, 0);
        assert_eq!(kept[1].0, 2);
        assert_eq!(
            kept[0].1.get("Subj").and_then(|o| o.as_text()),
            Some("W12x26".to_string())
        );
    }

    #[test]
    fn lifting_them_out_takes_the_record_with_them() {
        // Otherwise a second Unflatten would put a second copy of every markup
        // on the sheet.
        let file = a_file();
        let mut update = pdf::write::Update::new(&file);
        write(&mut update, &file, &[(0, a_markup("W12x26"))]);
        let once = pdf::Document::from_bytes(update.apply(&file));
        assert_eq!(read(&once).len(), 1);

        let mut update = pdf::write::Update::new(&once);
        clear(&once, &mut update);
        let twice = pdf::Document::from_bytes(update.apply(&once));
        assert!(read(&twice).is_empty());
    }

    #[test]
    fn nothing_is_written_when_there_was_nothing_to_flatten() {
        let file = a_file();
        let mut update = pdf::write::Update::new(&file);
        write(&mut update, &file, &[]);
        let after = pdf::Document::from_bytes(update.apply(&file));
        assert!(read(&after).is_empty());
    }
}
