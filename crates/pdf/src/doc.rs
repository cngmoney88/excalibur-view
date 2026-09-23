//! A whole document: the bytes, the cross reference, and the page tree.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use crate::filters;
use crate::object::{Dict, Object, Ref};
use crate::parse::Reader;
use crate::xref::{self, Slot, XRef};

pub struct Document {
    pub bytes: Vec<u8>,
    pub xref: XRef,
    /// True when the file declares an /Encrypt dictionary. Until [`unlock`]
    /// has been given the right password, every string and stream in it is
    /// ciphertext, and Hyperview says so rather than showing rubbish.
    ///
    /// [`unlock`]: Document::unlock
    pub encrypted: bool,
    /// Set once a password has opened the file. From then on every object is
    /// put back the way it was written as it is read.
    crypt: Option<crate::opening::Opened>,
    /// Which object holds `/Encrypt`. That one is never itself encrypted --
    /// it is what tells a reader how to decrypt everything else -- so it is
    /// the one object that has to be left alone.
    encrypt_object: Option<u32>,
    objects: RefCell<HashMap<u32, Rc<Object>>>,
    bundles: RefCell<HashMap<u32, Rc<Bundle>>>,
    pages: RefCell<Option<Rc<Vec<Ref>>>>,
}

/// A decoded object stream and the offsets of the objects packed into it.
struct Bundle {
    data: Vec<u8>,
    entries: Vec<(u32, usize)>,
}

impl Document {
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Document> {
        let bytes = std::fs::read(path)?;
        Ok(Document::from_bytes(bytes))
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Document {
        let xref = xref::load(&bytes);
        let encrypted = xref.trailer.has("Encrypt");
        let encrypt_object = xref.trailer.get("Encrypt").and_then(|o| o.as_ref()).map(|r| r.number);
        Document {
            bytes,
            xref,
            encrypted,
            crypt: None,
            encrypt_object,
            objects: RefCell::new(HashMap::new()),
            bundles: RefCell::new(HashMap::new()),
            pages: RefCell::new(None),
        }
    }

    /// Opens a locked file with a password, or says it is the wrong one.
    ///
    /// Both the user's password and the owner's are tried. Either opens the
    /// file; the difference between them is only what the file *asks* a reader
    /// to allow, and that was never enforcement -- see [`crate::crypt`].
    ///
    /// Anything already read was read as ciphertext, so it is all thrown away
    /// and read again.
    pub fn unlock(&mut self, password: &str) -> bool {
        if !self.encrypted {
            // Nothing to open. Saying yes is right: the caller asked for a
            // readable document and has one.
            return true;
        }
        if self.crypt.is_some() {
            return true;
        }
        let Some(encrypt) = self.xref.trailer.get("Encrypt").cloned() else {
            return false;
        };
        let encrypt = match self.follow(&encrypt).as_dict() {
            Some(dict) => dict.clone(),
            None => return false,
        };
        // The first half of the trailer's /ID goes into the key for every
        // handler before revision 5. It is not encrypted and never was.
        let id = self
            .xref
            .trailer
            .get("ID")
            .and_then(|o| o.as_array())
            .and_then(|a| a.first())
            .and_then(|o| o.as_bytes())
            .map(|b| b.to_vec())
            .unwrap_or_default();
        let Some(opened) = crate::opening::open(&encrypt, &id, password) else {
            return false;
        };
        self.crypt = Some(opened);
        self.objects.borrow_mut().clear();
        self.bundles.borrow_mut().clear();
        *self.pages.borrow_mut() = None;
        true
    }

    /// Which lock the file uses, once a password has opened it.
    ///
    /// [`crate::opening::Lock::weak`] is the part worth putting on screen: most
    /// locked drawings in circulation are held shut with something that has
    /// not been real protection since the nineties.
    pub fn lock(&self) -> Option<crate::opening::Lock> {
        self.crypt.as_ref().map(|c| c.lock)
    }

    /// True when the file is locked and no password has opened it yet. Every
    /// string and stream read in this state is ciphertext.
    pub fn still_locked(&self) -> bool {
        self.encrypted && self.crypt.is_none()
    }

    /// What the file asks a reader to allow, once opened. A request, not a
    /// lock.
    pub fn allowed(&self) -> Option<crate::crypt::Allowed> {
        self.crypt.as_ref().map(|c| c.allowed)
    }

    /// The version from the header, and from the catalog if it overrides it.
    pub fn version(&self) -> String {
        let header = self
            .bytes
            .get(5..8)
            .map(|b| String::from_utf8_lossy(b).to_string())
            .unwrap_or_else(|| "1.4".into());
        match self.catalog().get("Version").and_then(|o| o.as_name()) {
            Some(n) => n.as_str().to_string(),
            None => header,
        }
    }

    /// Fetches an object by reference. A missing or unreadable object comes
    /// back as null rather than an error, which is what every PDF reader does
    /// and what keeps one bad object from closing a drawing set.
    pub fn get(&self, reference: Ref) -> Rc<Object> {
        if let Some(found) = self.objects.borrow().get(&reference.number) {
            return found.clone();
        }
        let object = Rc::new(self.load(reference));
        self.objects
            .borrow_mut()
            .insert(reference.number, object.clone());
        object
    }

    fn load(&self, reference: Ref) -> Object {
        match self.xref.slots.get(&reference.number).copied() {
            Some(Slot::InFile { offset, .. }) => {
                let object = match Reader::at(&self.bytes, offset).indirect() {
                    // The object at that offset must be the one asked for;
                    // a file with shifted offsets otherwise returns a stranger.
                    Ok((found, object)) if found.number == reference.number => object,
                    _ => self.hunt(reference),
                };
                self.unscramble(reference, object)
            }
            // Objects packed into an object stream are *not* decrypted here.
            // The stream that holds them was decrypted as a whole when it was
            // read, so what comes out of it is already plain; running it
            // through again would turn readable text into rubbish.
            Some(Slot::InStream { stream, index }) => self.from_bundle(stream, index, reference),
            _ => Object::Null,
        }
    }

    /// One object as it was before the file was locked, when a password has
    /// opened it, and untouched otherwise.
    fn unscramble(&self, reference: Ref, mut object: Object) -> Object {
        let Some(crypt) = &self.crypt else { return object };
        if Some(reference.number) == self.encrypt_object {
            return object;
        }
        crypt.object(reference.number, reference.generation, &mut object);
        object
    }

    /// Last resort for one object whose offset is wrong: scan for its header.
    fn hunt(&self, reference: Ref) -> Object {
        let needle = format!("{} {} obj", reference.number, reference.generation);
        let mut at = 0usize;
        while let Some(found) = crate::parse::find(&self.bytes, needle.as_bytes(), at) {
            at = found + needle.len();
            let starts_cleanly = found == 0
                || !self.bytes[found - 1].is_ascii_digit();
            if !starts_cleanly {
                continue;
            }
            if let Ok((got, object)) = Reader::at(&self.bytes, found).indirect() {
                if got.number == reference.number {
                    return object;
                }
            }
        }
        Object::Null
    }

    fn from_bundle(&self, stream: u32, index: u32, want: Ref) -> Object {
        let bundle = match self.bundle(stream) {
            Some(b) => b,
            None => return Object::Null,
        };
        // Trust the object number over the index: some writers get it wrong.
        let entry = bundle
            .entries
            .iter()
            .find(|(number, _)| *number == want.number)
            .or_else(|| bundle.entries.get(index as usize));
        let Some((_, offset)) = entry.copied() else {
            return Object::Null;
        };
        if offset >= bundle.data.len() {
            return Object::Null;
        }
        Reader::at(&bundle.data, offset)
            .object()
            .unwrap_or(Object::Null)
    }

    fn bundle(&self, number: u32) -> Option<Rc<Bundle>> {
        if let Some(found) = self.bundles.borrow().get(&number) {
            return Some(found.clone());
        }
        // Object streams never contain object streams, so this cannot recurse.
        let Some(Slot::InFile { offset, .. }) = self.xref.slots.get(&number).copied() else {
            return None;
        };
        let (found, object) = Reader::at(&self.bytes, offset).indirect().ok()?;
        // Read from the bytes rather than through `get`, so nothing has
        // decrypted it yet -- and it has to be, before the filters see it.
        let object = self.unscramble(found, object);
        let stream = object.as_stream()?;
        let data = filters::decode(stream).ok()?;
        let count = stream.dict.get("N").and_then(|o| o.as_i64()).unwrap_or(0).max(0) as usize;
        let first = stream
            .dict
            .get("First")
            .and_then(|o| o.as_i64())
            .unwrap_or(0)
            .max(0) as usize;

        let mut header = Reader::new(&data);
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let Ok(number) = header.integer() else { break };
            let Ok(offset) = header.integer() else { break };
            entries.push((number.max(0) as u32, first + offset.max(0) as usize));
        }
        let bundle = Rc::new(Bundle { data, entries });
        self.bundles.borrow_mut().insert(number, bundle.clone());
        Some(bundle)
    }

    /// Resolves a value that might be an indirect reference.
    pub fn follow(&self, object: &Object) -> Rc<Object> {
        match object {
            Object::Ref(r) => self.get(*r),
            other => Rc::new(other.clone()),
        }
    }

    /// Looks a key up in a dictionary and resolves it in one step, which is
    /// what nearly every caller wants.
    pub fn at(&self, dict: &Dict, key: &str) -> Rc<Object> {
        match dict.get(key) {
            Some(value) => self.follow(value),
            None => Rc::new(Object::Null),
        }
    }

    pub fn catalog(&self) -> Dict {
        self.xref
            .trailer
            .get("Root")
            .map(|r| self.follow(r))
            .and_then(|o| o.as_dict().cloned())
            .unwrap_or_default()
    }

    pub fn info(&self) -> Dict {
        self.xref
            .trailer
            .get("Info")
            .map(|r| self.follow(r))
            .and_then(|o| o.as_dict().cloned())
            .unwrap_or_default()
    }

    /// Every page, in order. Built once and kept.
    pub fn pages(&self) -> Rc<Vec<Ref>> {
        if let Some(found) = self.pages.borrow().as_ref() {
            return found.clone();
        }
        let mut found = Vec::new();
        let catalog = self.catalog();
        if let Some(root) = catalog.get("Pages").and_then(|o| o.as_ref()) {
            let mut seen = Vec::new();
            self.walk(root, &mut found, &mut seen, 0);
        }
        if found.is_empty() {
            // A file whose page tree is broken still has page objects in it.
            let mut numbers: Vec<u32> = self.xref.slots.keys().copied().collect();
            numbers.sort_unstable();
            for number in numbers {
                let reference = Ref::new(number, 0);
                let object = self.get(reference);
                let is_page = object
                    .as_dict()
                    .and_then(|d| d.get("Type"))
                    .map(|t| t.is_name("Page"))
                    .unwrap_or(false);
                if is_page {
                    found.push(reference);
                }
            }
        }
        let pages = Rc::new(found);
        *self.pages.borrow_mut() = Some(pages.clone());
        pages
    }

    fn walk(&self, node: Ref, out: &mut Vec<Ref>, seen: &mut Vec<Ref>, depth: usize) {
        if depth > 64 || seen.contains(&node) || out.len() > 200_000 {
            return;
        }
        seen.push(node);
        let object = self.get(node);
        let Some(dict) = object.as_dict() else { return };
        let kids = self.at(dict, "Kids");
        match kids.as_array() {
            Some(kids) => {
                for kid in kids {
                    if let Some(reference) = kid.as_ref() {
                        self.walk(reference, out, seen, depth + 1);
                    }
                }
            }
            // No /Kids means this is a leaf, whatever /Type claims.
            None => out.push(node),
        }
    }

    pub fn page_count(&self) -> usize {
        self.pages().len()
    }

    pub fn page(&self, index: usize) -> Option<Dict> {
        let pages = self.pages();
        let reference = pages.get(index)?;
        self.get(*reference).as_dict().cloned()
    }

    /// Reads an attribute of a page, walking up through /Parent for the ones
    /// that are allowed to be inherited.
    pub fn page_attr(&self, page: &Dict, key: &str) -> Rc<Object> {
        let mut dict = page.clone();
        for _ in 0..64 {
            let value = self.at(&dict, key);
            if !value.is_null() {
                return value;
            }
            let Some(parent) = dict.get("Parent").and_then(|o| o.as_ref()) else {
                break;
            };
            let Some(up) = self.get(parent).as_dict().cloned() else {
                break;
            };
            dict = up;
        }
        Rc::new(Object::Null)
    }

    /// The page box Hyperview measures against: the crop box when there is one,
    /// otherwise the media box, otherwise US letter.
    pub fn page_box(&self, page: &Dict) -> [f64; 4] {
        let crop = self.page_attr(page, "CropBox").as_rect();
        let media = self.page_attr(page, "MediaBox").as_rect();
        match (crop, media) {
            (Some(crop), Some(media)) => [
                crop[0].max(media[0]),
                crop[1].max(media[1]),
                crop[2].min(media[2]),
                crop[3].min(media[3]),
            ],
            (Some(only), None) | (None, Some(only)) => only,
            (None, None) => [0.0, 0.0, 612.0, 792.0],
        }
    }

    /// Page rotation, normalised to 0, 90, 180 or 270.
    pub fn page_rotation(&self, page: &Dict) -> i32 {
        let raw = self.page_attr(page, "Rotate").as_i64().unwrap_or(0);
        (((raw % 360) + 360) % 360 / 90 * 90) as i32
    }

    /// The annotations on a page, as references so they can be edited in place.
    pub fn annotations(&self, page: &Dict) -> Vec<Ref> {
        self.at(page, "Annots")
            .as_array()
            .map(|a| a.iter().filter_map(|o| o.as_ref()).collect())
            .unwrap_or_default()
    }

    /// The highest object number in use, so new objects start after it.
    pub fn highest_object(&self) -> u32 {
        let from_table = self.xref.slots.keys().copied().max().unwrap_or(0);
        let declared = self
            .xref
            .trailer
            .get("Size")
            .and_then(|o| o.as_i64())
            .unwrap_or(0)
            .max(0) as u32;
        from_table.max(declared.saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(objects: &[(&str, &str)], root: u32) -> Vec<u8> {
        let mut file = b"%PDF-1.5\n".to_vec();
        let mut offsets = Vec::new();
        for (i, (_, body)) in objects.iter().enumerate() {
            offsets.push(file.len());
            file.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
        }
        let table = file.len();
        file.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for offset in &offsets {
            file.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        file.extend_from_slice(
            format!(
                "trailer\n<</Size {}/Root {root} 0 R>>\nstartxref\n{table}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        file
    }

    fn three_pages() -> Document {
        Document::from_bytes(build(
            &[
                ("catalog", "<</Type/Catalog/Pages 2 0 R>>"),
                (
                    "pages",
                    "<</Type/Pages/Kids[3 0 R 4 0 R]/Count 3/MediaBox[0 0 3024 2160]/Rotate 90>>",
                ),
                ("page", "<</Type/Page/Parent 2 0 R>>"),
                ("inner", "<</Type/Pages/Parent 2 0 R/Kids[5 0 R]/Count 1>>"),
                ("page", "<</Type/Page/Parent 4 0 R/MediaBox[0 0 612 792]/Rotate 0>>"),
            ],
            1,
        ))
    }

    #[test]
    fn the_page_tree_flattens_in_order_through_nested_nodes() {
        let doc = three_pages();
        assert_eq!(doc.page_count(), 2);
        assert_eq!(*doc.pages(), vec![Ref::new(3, 0), Ref::new(5, 0)]);
    }

    #[test]
    fn a_page_inherits_its_box_and_rotation_from_the_node_above() {
        let doc = three_pages();
        let first = doc.page(0).unwrap();
        assert_eq!(doc.page_box(&first), [0.0, 0.0, 3024.0, 2160.0]);
        assert_eq!(doc.page_rotation(&first), 90);
        // The second page states its own, which must win.
        let second = doc.page(1).unwrap();
        assert_eq!(doc.page_box(&second), [0.0, 0.0, 612.0, 792.0]);
        assert_eq!(doc.page_rotation(&second), 0);
    }

    #[test]
    fn the_crop_box_is_clipped_to_the_media_box() {
        let doc = Document::from_bytes(build(
            &[
                ("catalog", "<</Type/Catalog/Pages 2 0 R>>"),
                ("pages", "<</Type/Pages/Kids[3 0 R]/Count 1>>"),
                (
                    "page",
                    "<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]/CropBox[-50 10 1000 700]>>",
                ),
            ],
            1,
        ));
        let page = doc.page(0).unwrap();
        assert_eq!(doc.page_box(&page), [0.0, 10.0, 612.0, 700.0]);
    }

    #[test]
    fn rotation_is_normalised_however_it_was_written() {
        let doc = Document::from_bytes(build(
            &[
                ("catalog", "<</Type/Catalog/Pages 2 0 R>>"),
                ("pages", "<</Type/Pages/Kids[3 0 R]/Count 1>>"),
                ("page", "<</Type/Page/Parent 2 0 R/Rotate -90>>"),
            ],
            1,
        ));
        assert_eq!(doc.page_rotation(&doc.page(0).unwrap()), 270);
    }

    #[test]
    fn an_object_whose_offset_points_at_the_wrong_object_is_hunted_down() {
        let mut file = build(
            &[
                ("catalog", "<</Type/Catalog/Pages 2 0 R>>"),
                ("pages", "<</Type/Pages/Kids[3 0 R]/Count 1>>"),
                ("page", "<</Type/Page/Parent 2 0 R/MediaBox[0 0 99 99]>>"),
            ],
            1,
        );
        // Point object 3 at object 1's offset.
        let text = String::from_utf8_lossy(&file).to_string();
        let lines: Vec<&str> = text.split('\n').collect();
        let first = lines.iter().find(|l| l.ends_with("00000 n ")).unwrap().to_string();
        let broken = text.replacen(
            &lines.iter().filter(|l| l.ends_with("00000 n ")).nth(2).unwrap().to_string(),
            &first,
            1,
        );
        file = broken.into_bytes();
        let doc = Document::from_bytes(file);
        let page = doc.page(0).unwrap();
        assert_eq!(
            doc.page_box(&page),
            [0.0, 0.0, 99.0, 99.0],
            "the right object was found despite the wrong offset"
        );
    }

    #[test]
    fn a_page_tree_that_loops_does_not_hang() {
        let doc = Document::from_bytes(build(
            &[
                ("catalog", "<</Type/Catalog/Pages 2 0 R>>"),
                ("pages", "<</Type/Pages/Kids[2 0 R 3 0 R]/Count 1>>"),
                ("page", "<</Type/Page/Parent 2 0 R>>"),
            ],
            1,
        ));
        assert_eq!(doc.page_count(), 1);
    }
}
