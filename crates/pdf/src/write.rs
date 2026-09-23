//! Writing PDF syntax, and saving changes as an incremental update.
//!
//! When Hyperview saves markups it appends: the new and changed objects, then a new
//! cross reference section pointing back at the old one. Not one byte of the
//! original file is rewritten. If the machine dies mid-save the file still
//! opens, at its previous state, because the old cross reference is still
//! intact and still the one a reader finds first if the new tail is incomplete.

use std::collections::BTreeMap;

use crate::doc::Document;
use crate::object::{Dict, Name, Object, Ref, Stream, StringKind};
use crate::xref::Slot;

/// Formats a number the way PDF wants it: no exponent, no trailing zeros.
pub fn number(value: f64) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    if value == value.trunc() && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    let mut text = format!("{value:.6}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    // Rounding can leave "-0" or "-", neither of which is a number.
    if text.is_empty() || text == "-" || text == "-0" || text == "0" {
        "0".into()
    } else {
        text
    }
}

pub fn write_name(name: &Name, out: &mut Vec<u8>) {
    out.push(b'/');
    for b in &name.0 {
        if crate::parse::is_regular(*b) && *b != b'#' && *b > 0x20 && *b < 0x7f {
            out.push(*b);
        } else {
            out.extend_from_slice(format!("#{b:02X}").as_bytes());
        }
    }
}

fn write_literal(bytes: &[u8], out: &mut Vec<u8>) {
    out.push(b'(');
    for b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                out.push(b'\\');
                out.push(*b);
            }
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\n' => out.extend_from_slice(b"\\n"),
            _ => out.push(*b),
        }
    }
    out.push(b')');
}

fn write_hex(bytes: &[u8], out: &mut Vec<u8>) {
    out.push(b'<');
    for b in bytes {
        out.extend_from_slice(format!("{b:02X}").as_bytes());
    }
    out.push(b'>');
}

pub fn write_object(object: &Object, out: &mut Vec<u8>) {
    match object {
        Object::Null => out.extend_from_slice(b"null"),
        Object::Bool(true) => out.extend_from_slice(b"true"),
        Object::Bool(false) => out.extend_from_slice(b"false"),
        Object::Int(i) => out.extend_from_slice(i.to_string().as_bytes()),
        Object::Real(r) => out.extend_from_slice(number(*r).as_bytes()),
        Object::String(s, StringKind::Hex) => write_hex(s, out),
        Object::String(s, StringKind::Literal) => write_literal(s, out),
        Object::Name(n) => write_name(n, out),
        Object::Ref(r) => {
            out.extend_from_slice(format!("{} {} R", r.number, r.generation).as_bytes())
        }
        Object::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                write_object(item, out);
            }
            out.push(b']');
        }
        Object::Dict(d) => write_dict(d, out),
        Object::Stream(s) => write_stream(s, out),
    }
}

pub fn write_dict(dict: &Dict, out: &mut Vec<u8>) {
    out.extend_from_slice(b"<<");
    for (key, value) in dict.iter() {
        write_name(key, out);
        // A name or a number needs a separator after the key; brackets do not.
        if matches!(
            value,
            Object::Name(_) | Object::Int(_) | Object::Real(_) | Object::Bool(_) | Object::Null | Object::Ref(_)
        ) {
            out.push(b' ');
        }
        write_object(value, out);
    }
    out.extend_from_slice(b">>");
}

fn write_stream(stream: &Stream, out: &mut Vec<u8>) {
    let mut dict = stream.dict.clone();
    // The declared length must match what is actually written, whatever the
    // file we read it from claimed.
    dict.set(Name::new("Length"), Object::Int(stream.data.len() as i64));
    write_dict(&dict, out);
    out.extend_from_slice(b"\nstream\n");
    out.extend_from_slice(&stream.data);
    out.extend_from_slice(b"\nendstream");
}

pub fn write_indirect(reference: Ref, object: &Object, out: &mut Vec<u8>) {
    out.extend_from_slice(format!("{} {} obj\n", reference.number, reference.generation).as_bytes());
    write_object(object, out);
    out.extend_from_slice(b"\nendobj\n");
}

/// A set of additions and replacements, saved as an incremental update.
pub struct Update {
    changes: BTreeMap<u32, (u16, Object)>,
    next: u32,
    /// Extra entries for the trailer, such as a changed /Root.
    trailer: Dict,
}

impl Update {
    pub fn new(doc: &Document) -> Update {
        Update {
            changes: BTreeMap::new(),
            next: doc.highest_object() + 1,
            trailer: Dict::new(),
        }
    }

    /// Adds a brand new object and hands back its reference.
    pub fn add(&mut self, object: Object) -> Ref {
        let reference = Ref::new(self.next, 0);
        self.next += 1;
        self.changes.insert(reference.number, (0, object));
        reference
    }

    /// Replaces an existing object. The generation stays as it was, because a
    /// reference elsewhere in the file still names it.
    pub fn replace(&mut self, reference: Ref, object: Object) {
        self.changes
            .insert(reference.number, (reference.generation, object));
    }

    pub fn set_trailer(&mut self, key: &str, value: Object) {
        self.trailer.set(Name::new(key), value);
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.changes.len()
    }

    /// Produces the whole file: the original bytes with the update appended.
    pub fn apply(&self, doc: &Document) -> Vec<u8> {
        let mut out = doc.bytes.clone();
        self.append(doc, &mut out);
        out
    }

    /// Appends just the update to bytes that already hold the original file.
    pub fn append(&self, doc: &Document, out: &mut Vec<u8>) {
        if self.changes.is_empty() {
            return;
        }
        // An update must start on its own line.
        if !out.ends_with(b"\n") {
            out.push(b'\n');
        }

        let mut offsets: BTreeMap<u32, (u16, usize)> = BTreeMap::new();
        for (number, (generation, object)) in &self.changes {
            offsets.insert(*number, (*generation, out.len()));
            // An update appended to a locked file has to be locked the same
            // way, or the file says everything in it is encrypted and some of
            // it is not -- which is a drawing set no reader opens. The object
            // held here stays as it is; what goes into the file is a sealed
            // copy of it.
            match doc.sealing() {
                Some(crypt) => {
                    let mut sealed = object.clone();
                    crypt.seal(*number, *generation, &mut sealed);
                    write_indirect(Ref::new(*number, *generation), &sealed, out);
                }
                None => write_indirect(Ref::new(*number, *generation), object, out),
            }
        }

        // A file whose cross reference had to be rebuilt cannot be chained to,
        // so this update writes a complete table and repairs it instead.
        let repairing = doc.xref.rebuilt;
        if repairing {
            for (number, slot) in &doc.xref.slots {
                if offsets.contains_key(number) {
                    continue;
                }
                if let Slot::InFile { offset, generation } = slot {
                    offsets.insert(*number, (*generation, *offset));
                }
            }
        }

        let size = offsets.keys().copied().max().unwrap_or(0) + 1;
        let start = out.len();
        if doc.xref.stream_style && !repairing {
            self.write_xref_stream(doc, &mut offsets, start, size, out);
        } else {
            self.write_xref_table(doc, &offsets, size, repairing, out);
        }
        out.extend_from_slice(format!("startxref\n{start}\n%%EOF\n").as_bytes());
    }

    fn trailer_for(&self, doc: &Document, size: u32, chain: bool) -> Dict {
        let mut trailer = Dict::new();
        trailer.set(Name::new("Size"), Object::Int(size as i64));
        for key in ["Root", "Info", "Encrypt", "ID"] {
            if let Some(value) = doc.xref.trailer.get(key) {
                trailer.set(Name::new(key), value.clone());
            }
        }
        for (key, value) in self.trailer.iter() {
            trailer.set(key.clone(), value.clone());
        }
        if chain {
            trailer.set(Name::new("Prev"), Object::Int(doc.xref.start as i64));
        }
        trailer
    }

    fn write_xref_table(
        &self,
        doc: &Document,
        offsets: &BTreeMap<u32, (u16, usize)>,
        size: u32,
        repairing: bool,
        out: &mut Vec<u8>,
    ) {
        out.extend_from_slice(b"xref\n");
        // Object zero is only listed when the section starts at zero, which it
        // does whenever this update is repairing the whole file.
        let mut runs: Vec<Vec<u32>> = Vec::new();
        for number in offsets.keys().copied() {
            match runs.last_mut() {
                Some(run) if *run.last().unwrap() + 1 == number => run.push(number),
                _ => runs.push(vec![number]),
            }
        }
        if repairing {
            out.extend_from_slice(b"0 1\n0000000000 65535 f \n");
        }
        for run in runs {
            out.extend_from_slice(format!("{} {}\n", run[0], run.len()).as_bytes());
            for number in run {
                let (generation, offset) = offsets[&number];
                out.extend_from_slice(format!("{offset:010} {generation:05} n \n").as_bytes());
            }
        }
        out.extend_from_slice(b"trailer\n");
        let trailer = self.trailer_for(doc, size, !repairing);
        write_dict(&trailer, out);
        out.push(b'\n');
    }

    fn write_xref_stream(
        &self,
        doc: &Document,
        offsets: &mut BTreeMap<u32, (u16, usize)>,
        start: usize,
        size: u32,
        out: &mut Vec<u8>,
    ) {
        // The cross reference stream is itself an object and has to appear in
        // its own table.
        let own = self.next;
        offsets.insert(own, (0, start));
        let size = size.max(own + 1);

        let mut index: Vec<Object> = Vec::new();
        let mut rows: Vec<u8> = Vec::new();
        let mut runs: Vec<Vec<u32>> = Vec::new();
        for number in offsets.keys().copied() {
            match runs.last_mut() {
                Some(run) if *run.last().unwrap() + 1 == number => run.push(number),
                _ => runs.push(vec![number]),
            }
        }
        for run in &runs {
            index.push(Object::Int(run[0] as i64));
            index.push(Object::Int(run.len() as i64));
            for number in run {
                let (generation, offset) = offsets[number];
                rows.push(1);
                rows.extend_from_slice(&(offset as u32).to_be_bytes());
                rows.extend_from_slice(&generation.to_be_bytes());
            }
        }

        let mut dict = self.trailer_for(doc, size, true);
        dict.set(Name::new("Type"), Object::name("XRef"));
        dict.set(
            Name::new("W"),
            Object::Array(vec![Object::Int(1), Object::Int(4), Object::Int(2)]),
        );
        dict.set(Name::new("Index"), Object::Array(index));
        dict.set(Name::new("Filter"), Object::name("FlateDecode"));
        let data = crate::filters::deflate(&rows);
        let stream = Object::Stream(Box::new(Stream { dict, data }));
        write_indirect(Ref::new(own, 0), &stream, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict;

    fn roundtrip(object: Object) -> Object {
        let mut bytes = Vec::new();
        write_object(&object, &mut bytes);
        crate::Reader::new(&bytes).object().unwrap()
    }

    #[test]
    fn numbers_are_written_without_exponents_or_trailing_zeros() {
        assert_eq!(number(4.0), "4");
        assert_eq!(number(0.5), "0.5");
        assert_eq!(number(-0.0000001), "0");
        assert_eq!(number(1234.5678), "1234.5678");
        assert_eq!(number(0.05555556), "0.055556");
        assert!(!number(1e-9).contains('e'));
    }

    #[test]
    fn every_object_type_survives_being_written_and_read_back() {
        for object in [
            Object::Null,
            Object::Bool(true),
            Object::Int(-42),
            Object::Real(3.5),
            Object::name("Type"),
            Object::Ref(Ref::new(9, 1)),
            Object::Array(vec![Object::Int(1), Object::name("A")]),
            Object::Dict(dict! {"Subj" => Object::text("W12x26")}),
        ] {
            assert_eq!(roundtrip(object.clone()), object, "{object:?}");
        }
    }

    #[test]
    fn a_name_with_awkward_bytes_is_escaped_and_comes_back_whole() {
        let name = Object::name("Adobe Green#1");
        assert_eq!(roundtrip(name.clone()), name);
    }

    #[test]
    fn a_string_with_parentheses_and_newlines_survives() {
        let text = Object::String(b"L3x2x1/4 (typ)\r\nsee detail".to_vec(), StringKind::Literal);
        assert_eq!(roundtrip(text.clone()), text);
    }

    #[test]
    fn a_stream_is_written_with_the_length_it_actually_has() {
        let stream = Object::Stream(Box::new(Stream {
            dict: dict! {"Length" => Object::Int(9999)},
            data: b"12345".to_vec(),
        }));
        let mut bytes = Vec::new();
        write_object(&stream, &mut bytes);
        let back = crate::Reader::new(&bytes).object().unwrap();
        let back = back.as_stream().unwrap();
        assert_eq!(back.data, b"12345");
        assert_eq!(back.dict.get("Length").unwrap().as_i64(), Some(5));
    }

    fn sample() -> Vec<u8> {
        let bodies = [
            "<</Type/Catalog/Pages 2 0 R>>",
            "<</Type/Pages/Kids[3 0 R]/Count 1>>",
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>",
        ];
        let mut file = b"%PDF-1.5\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in bodies.iter().enumerate() {
            offsets.push(file.len());
            file.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
        }
        let table = file.len();
        file.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
        for offset in &offsets {
            file.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        file.extend_from_slice(
            format!("trailer\n<</Size 4/Root 1 0 R>>\nstartxref\n{table}\n%%EOF\n").as_bytes(),
        );
        file
    }

    #[test]
    fn an_update_leaves_every_original_byte_where_it_was() {
        let original = sample();
        let doc = Document::from_bytes(original.clone());
        let mut update = Update::new(&doc);
        update.add(Object::Dict(dict! {"Subtype" => Object::name("Square")}));
        let saved = update.apply(&doc);
        assert!(
            saved.starts_with(&original),
            "an incremental update appends and never rewrites"
        );
        assert!(saved.len() > original.len());
    }

    #[test]
    fn a_new_object_can_be_read_back_out_of_the_saved_file() {
        let doc = Document::from_bytes(sample());
        let mut update = Update::new(&doc);
        let added = update.add(Object::Dict(dict! {"Subj" => Object::text("W12x26")}));
        let saved = update.apply(&doc);

        let reopened = Document::from_bytes(saved);
        let object = reopened.get(added);
        assert_eq!(
            object.as_dict().unwrap().get("Subj").unwrap().as_string(),
            Some(&b"W12x26"[..])
        );
        // And the original objects are still reachable through the chain.
        assert_eq!(reopened.page_count(), 1);
    }

    #[test]
    fn replacing_a_page_to_add_annotations_reads_back_changed() {
        let doc = Document::from_bytes(sample());
        let page_ref = doc.pages()[0];
        let mut page = doc.page(0).unwrap();

        let mut update = Update::new(&doc);
        let annot = update.add(Object::Dict(dict! {
            "Type" => Object::name("Annot"),
            "Subtype" => Object::name("Line"),
            "Subj" => Object::text("Length Measurement"),
        }));
        page.set(Name::new("Annots"), Object::Array(vec![Object::Ref(annot)]));
        update.replace(page_ref, Object::Dict(page));
        let saved = update.apply(&doc);

        let reopened = Document::from_bytes(saved);
        let page = reopened.page(0).unwrap();
        let annots = reopened.annotations(&page);
        assert_eq!(annots.len(), 1);
        let got = reopened.get(annots[0]);
        assert!(got.as_dict().unwrap().get("Subtype").unwrap().is_name("Line"));
    }

    #[test]
    fn two_updates_in_a_row_both_survive() {
        let doc = Document::from_bytes(sample());
        let mut first = Update::new(&doc);
        let a = first.add(Object::text("first"));
        let once = first.apply(&doc);

        let doc = Document::from_bytes(once);
        let mut second = Update::new(&doc);
        let b = second.add(Object::text("second"));
        let twice = second.apply(&doc);

        let reopened = Document::from_bytes(twice);
        assert_eq!(reopened.get(a).as_string(), Some(&b"first"[..]));
        assert_eq!(reopened.get(b).as_string(), Some(&b"second"[..]));
        assert_ne!(a, b, "the second update must not reuse the first's number");
    }

    #[test]
    fn a_file_that_had_to_be_repaired_is_written_out_whole_rather_than_chained() {
        let broken = String::from_utf8_lossy(&sample())
            .replacen("startxref", "startxrefXX", 1)
            .into_bytes();
        let doc = Document::from_bytes(broken);
        assert!(doc.xref.rebuilt);

        let mut update = Update::new(&doc);
        let added = update.add(Object::text("repaired"));
        let saved = update.apply(&doc);

        let reopened = Document::from_bytes(saved);
        assert!(
            !reopened.xref.rebuilt,
            "the saved file should now have a table a reader can trust"
        );
        assert_eq!(reopened.page_count(), 1);
        assert_eq!(reopened.get(added).as_string(), Some(&b"repaired"[..]));
    }
}
