//! Rebuilding a file whose cross-reference table no longer matches it.
//!
//! A PDF says where each of its objects is in a table at the end. When that
//! table is wrong — a set written by a program that got it wrong, a download
//! that was cut short, a file assembled by concatenation — readers do one of
//! two things: refuse to open it, or open it with sheets missing. Both are
//! how a drawing set goes out short without anybody noticing.
//!
//! The repair is to stop believing the table and read the file instead. Every
//! `N G obj` in the bytes is an object; the last one with a given number wins,
//! because that is what an incremental update means. From those, a fresh table
//! and trailer are written, and the original bytes are left exactly as they
//! were underneath — so a repair never loses anything that was there, and a
//! file that was already fine comes back the same.

use std::collections::BTreeMap;

use crate::object::{Dict, Object, Ref};
use crate::parse::Reader;

pub struct Rebuilt {
    pub bytes: Vec<u8>,
    /// How many objects were found in the file itself.
    pub found: usize,
    /// How many looked like objects but could not be read.
    pub unreadable: usize,
}

/// Finds every object in the bytes and writes a file with a table that matches.
pub fn rebuild(bytes: &[u8]) -> Rebuilt {
    let mut at: BTreeMap<u32, (u16, usize)> = BTreeMap::new();
    let mut unreadable = 0usize;

    for start in object_starts(bytes) {
        match Reader::at(bytes, start).indirect() {
            Ok((reference, _)) => {
                // The last one wins: that is what an incremental update means,
                // and a repair that preferred the first would undo every
                // change made since the file was first written.
                at.insert(reference.number, (reference.generation, start));
            }
            Err(_) => unreadable += 1,
        }
    }

    let found = at.len();
    let root = find_root(bytes, &at);

    // The original bytes, then a fresh table for everything in them. Nothing
    // is rewritten, so a repair can never damage what it read.
    let mut out = bytes.to_vec();
    if !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    let xref_at = out.len();
    let highest = at.keys().copied().max().unwrap_or(0);
    out.extend_from_slice(format!("xref\n0 {}\n", highest + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for number in 1..=highest {
        match at.get(&number) {
            Some((generation, offset)) => {
                out.extend_from_slice(
                    format!("{offset:010} {generation:05} n \n").as_bytes(),
                );
            }
            // A number nothing was found for is free rather than a dangling
            // offset into the middle of something else.
            None => out.extend_from_slice(b"0000000000 65535 f \n"),
        }
    }
    let mut trailer = Dict::new();
    trailer.set("Size", Object::Int(highest as i64 + 1));
    if let Some(root) = root {
        trailer.set("Root", Object::Ref(root));
    }
    out.extend_from_slice(b"trailer\n");
    crate::write::write_object(&Object::Dict(trailer), &mut out);
    out.extend_from_slice(format!("\nstartxref\n{xref_at}\n%%EOF\n").as_bytes());

    Rebuilt {
        bytes: out,
        found,
        unreadable,
    }
}

/// Every place in the bytes that looks like the start of an object.
fn object_starts(bytes: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 3 < bytes.len() {
        let Some(found) = find(bytes, b"obj", at) else { break };
        at = found + 3;
        // Walk back over " 0 " and the object number to where the object
        // really starts.
        let mut back = found;
        if back == 0 {
            continue;
        }
        back -= 1;
        while back > 0 && bytes[back].is_ascii_whitespace() {
            back -= 1;
        }
        if !bytes[back].is_ascii_digit() {
            continue;
        }
        while back > 0 && bytes[back].is_ascii_digit() {
            back -= 1;
        }
        while back > 0 && bytes[back].is_ascii_whitespace() {
            back -= 1;
        }
        if !bytes[back].is_ascii_digit() {
            continue;
        }
        while back > 0 && bytes[back - 1].is_ascii_digit() {
            back -= 1;
        }
        out.push(back);
    }
    out
}

/// Which object is the catalog.
///
/// From the last trailer that names one where possible, because that is what
/// the file itself says; otherwise by looking for the object that says it is a
/// catalog, which is what a file with no readable trailer leaves to go on.
fn find_root(bytes: &[u8], at: &BTreeMap<u32, (u16, usize)>) -> Option<Ref> {
    // A trailer that says /Root is only worth believing if what it points at
    // really is a catalog. A wrecked table takes the trailer down with it, and
    // a /Root pointing at a page content stream produces a file that opens
    // with no sheets in it at all — which is worse than one that will not open.
    let mut best: Option<Ref> = None;
    let mut from = 0usize;
    while let Some(found) = find(bytes, b"/Root", from) {
        from = found + 5;
        let mut reader = Reader::at(bytes, from);
        if let Ok(Object::Ref(reference)) = reader.object() {
            if at
                .get(&reference.number)
                .map(|(_, offset)| is_catalog_at(bytes, *offset))
                .unwrap_or(false)
            {
                best = Some(reference);
            }
        }
    }
    if best.is_some() {
        return best;
    }
    for (number, (generation, offset)) in at {
        if is_catalog_at(bytes, *offset) {
            return Some(Ref {
                number: *number,
                generation: *generation,
            });
        }
    }
    None
}

/// Whether the object at this offset is the document catalog.
fn is_catalog_at(bytes: &[u8], offset: usize) -> bool {
    let Ok((_, object)) = Reader::at(bytes, offset).indirect() else {
        return false;
    };
    let Some(dict) = object.as_dict() else {
        return false;
    };
    let says_so = dict
        .get("Type")
        .and_then(|o| o.as_name())
        .map(|n| n.as_str() == "Catalog")
        .unwrap_or(false);
    // A catalog without /Type is rare but legal, and a page tree is the thing
    // that makes it useful either way.
    says_so || (dict.has("Pages") && !dict.has("Contents") && !dict.has("Kids"))
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|at| at + from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny but complete one page file.
    fn a_file() -> Vec<u8> {
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
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
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
        out
    }

    #[test]
    fn every_object_in_the_file_is_found() {
        let rebuilt = rebuild(&a_file());
        assert_eq!(rebuilt.found, 3);
        assert_eq!(rebuilt.unreadable, 0);
    }

    #[test]
    fn a_file_with_a_wrecked_table_comes_back_readable() {
        // Offsets shifted by a program that got them wrong: every reader
        // either refuses this or opens it with the sheets missing.
        let mut broken = a_file();
        let at = broken
            .windows(4)
            .position(|w| w == b"xref")
            .expect("there is a table");
        for byte in broken[at..].iter_mut() {
            if byte.is_ascii_digit() {
                *byte = b'9';
            }
        }
        let rebuilt = rebuild(&broken);
        let doc = crate::Document::from_bytes(rebuilt.bytes);
        assert_eq!(doc.page_count(), 1, "the sheet should come back");
    }

    #[test]
    fn the_original_bytes_are_left_exactly_where_they_were() {
        // A repair appends a table rather than rewriting the file, so nothing
        // that was in it can be lost on the way through.
        let original = a_file();
        let rebuilt = rebuild(&original);
        assert!(rebuilt.bytes.starts_with(&original));
    }

    #[test]
    fn a_later_version_of_an_object_wins_over_an_earlier_one() {
        // That is what an incremental update means; preferring the first
        // would undo every change made since the file was written.
        let mut bytes = a_file();
        bytes.extend_from_slice(
            b"3 0 obj\n<</Type/Page/Parent 2 0 R/MediaBox[0 0 1224 792]>>\nendobj\n",
        );
        let rebuilt = rebuild(&bytes);
        let doc = crate::Document::from_bytes(rebuilt.bytes);
        let page = doc.page(0).expect("a sheet");
        assert_eq!(doc.page_box(&page), [0.0, 0.0, 1224.0, 792.0]);
    }

    #[test]
    fn a_file_with_no_trailer_at_all_still_finds_its_catalog() {
        let whole = a_file();
        let cut = whole
            .windows(7)
            .position(|w| w == b"trailer")
            .expect("there is one");
        let rebuilt = rebuild(&whole[..cut]);
        let doc = crate::Document::from_bytes(rebuilt.bytes);
        assert_eq!(doc.page_count(), 1);
    }
}
