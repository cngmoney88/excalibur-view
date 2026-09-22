//! Cross reference tables and streams: finding where each object lives.
//!
//! This is the part of a PDF most likely to be wrong in a file that has been
//! through several programs. When it is, the whole file is scanned for objects
//! instead, which is slower but always works.

use std::collections::HashMap;

use crate::filters;
use crate::object::{Dict, Object, Ref};
use crate::parse::{find, is_space, rfind, Reader};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Free,
    /// A plain object at a byte offset.
    InFile { offset: usize, generation: u16 },
    /// An object packed inside an object stream.
    InStream { stream: u32, index: u32 },
}

#[derive(Default, Debug)]
pub struct XRef {
    pub slots: HashMap<u32, Slot>,
    pub trailer: Dict,
    /// Byte offset of the newest cross reference section, needed when writing
    /// an incremental update.
    pub start: usize,
    /// True when the newest section was a cross reference stream, so an update
    /// has to be written in the same style.
    pub stream_style: bool,
    /// Set when the tables could not be trusted and the file was scanned.
    pub rebuilt: bool,
}

impl XRef {
    fn remember(&mut self, number: u32, slot: Slot) {
        // Sections are read newest first, so the first answer for an object
        // number is the one that counts.
        self.slots.entry(number).or_insert(slot);
    }

    fn absorb_trailer(&mut self, dict: &Dict) {
        for (key, value) in dict.iter() {
            if !self.trailer.has(key.as_str()) {
                self.trailer.set(key.clone(), value.clone());
            }
        }
    }
}

/// Reads the cross reference chain. Falls back to scanning if anything is
/// missing, and also scans when the tables are present but do not lead to a
/// usable catalog.
pub fn load(bytes: &[u8]) -> XRef {
    let mut xref = match start_offset(bytes) {
        Some(start) => {
            let mut xref = XRef {
                start,
                ..Default::default()
            };
            let mut seen = Vec::new();
            follow(bytes, start, &mut xref, &mut seen, 0);
            xref
        }
        None => XRef::default(),
    };

    if xref.slots.is_empty() || !xref.trailer.has("Root") {
        rebuild(bytes, &mut xref);
    }
    xref
}

fn start_offset(bytes: &[u8]) -> Option<usize> {
    // `startxref` sits in the last few hundred bytes of a well formed file, so
    // look there first. Scanning twenty megabytes backwards to find it costs
    // more than opening the whole document should.
    let tail_from = bytes.len().saturating_sub(4096);
    let at = rfind(&bytes[tail_from..], b"startxref", bytes.len() - tail_from)
        .map(|p| p + tail_from)
        .or_else(|| rfind(bytes, b"startxref", bytes.len()))?;
    let mut r = Reader::at(bytes, at + b"startxref".len());
    let offset = r.integer().ok()?;
    (offset > 0 && (offset as usize) < bytes.len()).then_some(offset as usize)
}

fn follow(bytes: &[u8], at: usize, xref: &mut XRef, seen: &mut Vec<usize>, depth: usize) {
    if depth > 64 || seen.contains(&at) || at >= bytes.len() {
        return;
    }
    seen.push(at);

    let mut r = Reader::at(bytes, at);
    r.skip_space();

    let trailer = if r.eat("xref") {
        if depth == 0 {
            xref.stream_style = false;
        }
        read_table(bytes, &mut r, xref)
    } else {
        if depth == 0 {
            xref.stream_style = true;
        }
        read_stream(bytes, at, xref)
    };

    let Some(trailer) = trailer else { return };
    xref.absorb_trailer(&trailer);

    // A hybrid file keeps newer entries in a stream alongside the old table.
    if let Some(hybrid) = trailer.get("XRefStm").and_then(|o| o.as_i64()) {
        follow(bytes, hybrid as usize, xref, seen, depth + 1);
    }
    if let Some(previous) = trailer.get("Prev").and_then(|o| o.as_i64()) {
        if previous > 0 {
            follow(bytes, previous as usize, xref, seen, depth + 1);
        }
    }
}

fn read_table(bytes: &[u8], r: &mut Reader, xref: &mut XRef) -> Option<Dict> {
    loop {
        r.skip_space();
        if r.eat("trailer") {
            r.skip_space();
            return match r.object() {
                Ok(Object::Dict(d)) => Some(d),
                _ => None,
            };
        }
        if r.done() {
            return None;
        }
        let Ok(first) = r.integer() else { return None };
        let Ok(count) = r.integer() else { return None };
        if count < 0 || count > 50_000_000 {
            return None;
        }
        for i in 0..count {
            r.skip_space();
            let Ok(offset) = r.integer() else { return None };
            let Ok(generation) = r.integer() else { return None };
            r.skip_space();
            let kind = r.peek().unwrap_or(b'n');
            r.at += 1;
            let number = (first + i).max(0) as u32;
            if kind == b'f' {
                xref.remember(number, Slot::Free);
            } else if offset > 0 && (offset as usize) < bytes.len() {
                xref.remember(
                    number,
                    Slot::InFile {
                        offset: offset as usize,
                        generation: generation.clamp(0, 65535) as u16,
                    },
                );
            }
        }
    }
}

fn read_stream(bytes: &[u8], at: usize, xref: &mut XRef) -> Option<Dict> {
    let (_, object) = Reader::at(bytes, at).indirect().ok()?;
    let stream = object.as_stream()?;
    let dict = stream.dict.clone();
    let data = filters::decode(stream).ok()?;

    let widths: Vec<usize> = dict
        .get("W")?
        .as_array()?
        .iter()
        .map(|o| o.as_i64().unwrap_or(0).max(0) as usize)
        .collect();
    if widths.len() < 3 {
        return None;
    }
    let size = dict.get("Size").and_then(|o| o.as_i64()).unwrap_or(0).max(0) as u32;
    let index: Vec<i64> = match dict.get("Index").and_then(|o| o.as_array()) {
        Some(a) => a.iter().map(|o| o.as_i64().unwrap_or(0)).collect(),
        None => vec![0, size as i64],
    };

    let row = widths.iter().sum::<usize>();
    if row == 0 {
        return None;
    }
    let mut cursor = 0usize;
    for pair in index.chunks(2) {
        let (first, count) = (pair[0], *pair.get(1).unwrap_or(&0));
        for i in 0..count {
            if cursor + row > data.len() {
                break;
            }
            let mut field = [0u64; 3];
            let mut at = cursor;
            for (f, width) in widths.iter().enumerate().take(3) {
                let mut value = 0u64;
                for _ in 0..*width {
                    value = (value << 8) | data[at] as u64;
                    at += 1;
                }
                field[f] = value;
            }
            cursor += row;
            // A zero width type field means type 1, the common case.
            let kind = if widths[0] == 0 { 1 } else { field[0] };
            let number = (first + i).max(0) as u32;
            match kind {
                0 => xref.remember(number, Slot::Free),
                1 => {
                    let offset = field[1] as usize;
                    if offset > 0 && offset < bytes.len() {
                        xref.remember(
                            number,
                            Slot::InFile {
                                offset,
                                generation: field[2].min(65535) as u16,
                            },
                        );
                    }
                }
                2 => xref.remember(
                    number,
                    Slot::InStream {
                        stream: field[1] as u32,
                        index: field[2] as u32,
                    },
                ),
                _ => {}
            }
        }
    }
    Some(dict)
}

/// Walks the whole file looking for `N G obj`, which is what to do when the
/// tables are wrong. Later definitions win, matching how a reader that trusted
/// an incremental update would behave.
pub fn rebuild(bytes: &[u8], xref: &mut XRef) {
    xref.rebuilt = true;
    let mut at = 0usize;
    while let Some(found) = find(bytes, b"obj", at) {
        at = found + 3;
        // Walk back over `generation` and `number`.
        let mut i = found;
        while i > 0 && is_space(bytes[i - 1]) {
            i -= 1;
        }
        let generation_end = i;
        while i > 0 && bytes[i - 1].is_ascii_digit() {
            i -= 1;
        }
        let generation_start = i;
        if generation_start == generation_end {
            continue;
        }
        while i > 0 && is_space(bytes[i - 1]) {
            i -= 1;
        }
        let number_end = i;
        while i > 0 && bytes[i - 1].is_ascii_digit() {
            i -= 1;
        }
        let number_start = i;
        if number_start == number_end || number_end == generation_start {
            continue;
        }
        // Whatever precedes the number must not be a digit, or we are looking
        // at the tail of a longer run of numbers.
        if number_start > 0 && bytes[number_start - 1].is_ascii_digit() {
            continue;
        }
        let number: u32 = match std::str::from_utf8(&bytes[number_start..number_end])
            .ok()
            .and_then(|s| s.parse().ok())
        {
            Some(n) => n,
            None => continue,
        };
        let generation: u16 = std::str::from_utf8(&bytes[generation_start..generation_end])
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        xref.slots.insert(
            number,
            Slot::InFile {
                offset: number_start,
                generation,
            },
        );
    }

    if !xref.trailer.has("Root") {
        // Prefer a real trailer dictionary if one survives anywhere.
        let mut at = 0usize;
        while let Some(found) = find(bytes, b"trailer", at) {
            at = found + 7;
            let mut r = Reader::at(bytes, at);
            if let Ok(Object::Dict(d)) = r.object() {
                if d.has("Root") {
                    xref.absorb_trailer(&d);
                }
            }
        }
    }
    if !xref.trailer.has("Root") {
        // Otherwise find the catalog by looking at every object.
        let numbers: Vec<u32> = xref.slots.keys().copied().collect();
        for number in numbers {
            let Some(Slot::InFile { offset, .. }) = xref.slots.get(&number).copied() else {
                continue;
            };
            let Ok((_, object)) = Reader::at(bytes, offset).indirect() else {
                continue;
            };
            let is_catalog = object
                .as_dict()
                .and_then(|d| d.get("Type"))
                .map(|t| t.is_name("Catalog"))
                .unwrap_or(false);
            if is_catalog {
                xref.trailer.set("Root", Object::Ref(Ref::new(number, 0)));
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal but genuinely correct file: three objects, a classic
    /// table with real offsets, and a trailer.
    fn tiny() -> Vec<u8> {
        let bodies: [&str; 3] = [
            "<</Type/Catalog/Pages 2 0 R>>",
            "<</Type/Pages/Kids[3 0 R]/Count 1>>",
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>",
        ];
        let mut file = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in bodies.iter().enumerate() {
            offsets.push(file.len());
            file.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
        }
        let table_at = file.len();
        file.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
        for offset in &offsets {
            file.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        file.extend_from_slice(
            format!("trailer\n<</Size 4/Root 1 0 R>>\nstartxref\n{table_at}\n%%EOF\n").as_bytes(),
        );
        file
    }

    #[test]
    fn a_classic_table_is_read() {
        let x = load(&tiny());
        assert!(!x.rebuilt, "the table was good, so nothing had to be rebuilt");
        assert!(!x.stream_style);
        assert_eq!(x.slots.get(&0), Some(&Slot::Free));
        assert!(matches!(x.slots.get(&1), Some(Slot::InFile { .. })));
        assert_eq!(x.trailer.get("Root").unwrap().as_ref(), Some(Ref::new(1, 0)));
    }

    #[test]
    fn a_file_whose_offsets_are_all_wrong_is_rebuilt_by_scanning() {
        let broken = String::from_utf8_lossy(&tiny())
            .replace("0000000009", "0000009999")
            .replace("0000000054", "0000008888")
            .replacen("startxref", "startxrefXX", 1);
        let x = load(broken.as_bytes());
        assert!(x.rebuilt);
        assert!(matches!(x.slots.get(&3), Some(Slot::InFile { .. })));
        assert_eq!(x.trailer.get("Root").unwrap().as_ref(), Some(Ref::new(1, 0)));
    }

    #[test]
    fn a_catalog_is_found_even_with_no_trailer_at_all() {
        let broken = String::from_utf8_lossy(&tiny())
            .replace("trailer\n<</Size 4/Root 1 0 R>>\n", "")
            .replacen("startxref", "startxrefXX", 1);
        let x = load(broken.as_bytes());
        assert!(x.rebuilt);
        assert_eq!(x.trailer.get("Root").unwrap().as_ref(), Some(Ref::new(1, 0)));
    }

    #[test]
    fn the_newest_section_wins_over_an_older_one() {
        let mut file = tiny();
        let first_table = String::from_utf8_lossy(&file)
            .rsplit("startxref\n")
            .next()
            .and_then(|t| t.split('\n').next().map(|s| s.to_string()))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap();

        let added_at = file.len();
        file.extend_from_slice(
            b"3 0 obj\n<</Type/Page/Parent 2 0 R/MediaBox[0 0 100 100]>>\nendobj\n",
        );
        let table_at = file.len();
        file.extend_from_slice(
            format!(
                "xref\n3 1\n{added_at:010} 00000 n \ntrailer\n<</Size 4/Root 1 0 R/Prev {first_table}>>\nstartxref\n{table_at}\n%%EOF\n"
            )
            .as_bytes(),
        );

        let x = load(&file);
        assert!(!x.rebuilt);
        assert_eq!(
            x.slots.get(&3),
            Some(&Slot::InFile { offset: added_at, generation: 0 }),
            "the update's offset has to beat the original"
        );
        assert!(
            matches!(x.slots.get(&1), Some(Slot::InFile { .. })),
            "and the older section still supplies everything it did before"
        );
    }
}
