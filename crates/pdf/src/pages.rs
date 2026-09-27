//! Putting a drawing set's sheets in a new order, adding sheets from another
//! file, turning sheets and taking them out — all as one incremental update,
//! so nothing already in the file is rewritten or lost.
//!
//! The page tree is flattened as it is rewritten: every sheet ends up
//! directly under the tree's root, in the order asked for. A sheet that used
//! to sit lower down, and took its paper size or its resources from the node
//! above it, has them written onto itself first, so it looks exactly as it
//! did. The old nodes stay in the file where nothing points at them, which is
//! how an incremental save always leaves what it replaces — and why going
//! back is only a matter of pointing the root at the old list again.
//!
//! Sheets from another file are copied in whole: their drawing, fonts,
//! pictures and markups, each object once however many sheets share it. A
//! link from one of them to a sheet of that file that was not brought in
//! points at nothing afterwards, rather than dragging that sheet in unseen.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::doc::Document;
use crate::object::{Dict, Object, Ref, Stream};
use crate::write::Update;

/// Attributes a page may take from the nodes above it.
const INHERITED: [&str; 4] = ["Resources", "MediaBox", "CropBox", "Rotate"];

/// One place in the new order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Slot {
    /// A sheet already in this file, by its index before the change.
    Here(usize),
    /// A sheet from another file: which file, and which of its sheets.
    From(usize, usize),
    /// A new, empty sheet of this size, in points.
    Blank([f64; 2]),
}

/// What an arrangement changes, both ways round.
pub struct Arranged {
    /// The update that makes the change, ready to append to the file.
    pub update: Update,
    /// Every object the update replaces, as it was: writing these back is
    /// undoing the change.
    pub before: Vec<(Ref, Object)>,
    /// The same objects as the update leaves them: writing these again is
    /// doing it again, without copying anything a second time.
    pub after: Vec<(Ref, Object)>,
    /// The sheets of this file whose own drawing changed — turned — as
    /// opposed to only moved.
    pub turned: Vec<Ref>,
}

/// Works out the update that gives `host` the sheets in `order`, turning the
/// ones in `turn` by that many quarter turns clockwise.
///
/// A sheet of this file left out of `order` is taken out of the set. Each of
/// its sheets may appear once at most: a page belongs to one place in a tree.
pub fn arrange(
    host: &Document,
    guests: &[&Document],
    order: &[Slot],
    turn: &BTreeMap<usize, i32>,
) -> Result<Arranged, String> {
    if order.is_empty() {
        return Err("that would leave no sheets at all".into());
    }
    let catalog = host.catalog();
    let root = catalog
        .get("Pages")
        .and_then(|o| o.as_ref())
        .ok_or("the file has no page tree to change")?;
    let root_object = host.get(root);
    let root_dict = root_object.as_dict().cloned().ok_or("the file's page tree is not readable")?;
    let pages = host.pages();

    let mut seen = HashSet::new();
    for slot in order {
        match slot {
            Slot::Here(i) => {
                if *i >= pages.len() {
                    return Err(format!("there is no sheet {} in this file", i + 1));
                }
                if !seen.insert(*i) {
                    return Err(format!("sheet {} is in the new order twice", i + 1));
                }
            }
            Slot::From(g, p) => {
                let guest = guests.get(*g).ok_or("a file to bring sheets from is missing")?;
                if *p >= guest.page_count() {
                    return Err(format!("that file has no sheet {}", p + 1));
                }
            }
            Slot::Blank(size) => {
                if !(size[0] > 0.0 && size[1] > 0.0) {
                    return Err("a blank sheet needs a size".into());
                }
            }
        }
    }

    let mut update = Update::new(host);
    let mut before = vec![(root, (*root_object).clone())];
    let mut after = Vec::new();
    let mut turned = Vec::new();
    let mut kids = Vec::with_capacity(order.len());

    // Sheets coming from other files: every sheet of one file copied with the
    // same copier, so what they share is copied once.
    let mut copiers: HashMap<usize, Copier> = HashMap::new();
    let mut guest_pages: HashMap<(usize, usize), Ref> = HashMap::new();
    for slot in order {
        if let Slot::From(g, p) = slot {
            let page = guests[*g].pages()[*p];
            let reserved = update.reserve();
            copiers.entry(*g).or_default().pages.insert(page.number, reserved);
            guest_pages.insert((*g, *p), reserved);
        }
    }

    for slot in order {
        match *slot {
            Slot::Here(i) => {
                let page = pages[i];
                let object = host.get(page);
                let Some(dict) = object.as_dict() else {
                    return Err(format!("sheet {} is not readable", i + 1));
                };
                let quarter = turn.get(&i).copied().unwrap_or(0).rem_euclid(4);
                let parent = dict.get("Parent").and_then(|o| o.as_ref());
                if parent != Some(root) || quarter != 0 {
                    let mut changed = settled(host, dict);
                    changed.set("Parent", Object::Ref(root));
                    if quarter != 0 {
                        let now = changed.get("Rotate").and_then(|o| o.as_i64()).unwrap_or(0);
                        changed.set("Rotate", Object::Int((now + 90 * quarter as i64).rem_euclid(360)));
                        turned.push(page);
                    }
                    before.push((page, (*object).clone()));
                    after.push((page, Object::Dict(changed.clone())));
                    update.replace(page, Object::Dict(changed));
                }
                kids.push(Object::Ref(page));
            }
            Slot::From(g, p) => {
                let guest = guests[g];
                let source = guest.pages()[p];
                let reserved = guest_pages[&(g, p)];
                let copier = copiers.get_mut(&g).expect("made above");
                let object = guest.get(source);
                let dict = object.as_dict().cloned().unwrap_or_default();
                let mut page = settled(guest, &dict);
                // Everything a copied sheet needs is its own, so it takes
                // nothing from this file's tree by accident.
                if !page.has("Resources") {
                    page.set("Resources", Object::Dict(Dict::new()));
                }
                if !page.has("MediaBox") {
                    page.set("MediaBox", rect([0.0, 0.0, 612.0, 792.0]));
                }
                if !page.has("CropBox") {
                    let media = page.get("MediaBox").cloned().unwrap_or(Object::Null);
                    page.set("CropBox", media);
                }
                if !page.has("Rotate") {
                    page.set("Rotate", Object::Int(0));
                }
                for key in ["Parent", "B", "StructParents", "Thumb"] {
                    page.remove(key);
                }
                let mut copied = copier.dict(guest, &mut update, &page);
                copied.set("Parent", Object::Ref(root));
                update.replace(reserved, Object::Dict(copied));
                kids.push(Object::Ref(reserved));
            }
            Slot::Blank(size) => {
                let mut page = Dict::new();
                page.set("Type", Object::name("Page"));
                page.set("Parent", Object::Ref(root));
                page.set("MediaBox", rect([0.0, 0.0, size[0], size[1]]));
                page.set("CropBox", rect([0.0, 0.0, size[0], size[1]]));
                page.set("Rotate", Object::Int(0));
                page.set("Resources", Object::Dict(Dict::new()));
                kids.push(Object::Ref(update.add(Object::Dict(page))));
            }
        }
    }

    let mut new_root = root_dict;
    new_root.set("Kids", Object::Array(kids));
    new_root.set("Count", Object::Int(order.len() as i64));
    after.insert(0, (root, Object::Dict(new_root.clone())));
    update.replace(root, Object::Dict(new_root));
    Ok(Arranged { update, before, after, turned })
}

/// An update that writes these objects as they are: how an arrangement is
/// undone, or done again.
pub fn restore(host: &Document, objects: &[(Ref, Object)]) -> Update {
    let mut update = Update::new(host);
    for (reference, object) in objects {
        update.replace(*reference, object.clone());
    }
    update
}

/// A page's own dictionary with whatever it inherits written onto it, so it
/// can move anywhere in a tree and look the same.
fn settled(doc: &Document, page: &Dict) -> Dict {
    let mut out = page.clone();
    for key in INHERITED {
        if out.has(key) {
            continue;
        }
        if let Some(found) = inherited(doc, page, key) {
            out.set(key, found);
        }
    }
    out
}

/// The value a page takes from the nodes above it, as it is written there —
/// a reference stays a reference, so nothing is duplicated.
fn inherited(doc: &Document, page: &Dict, key: &str) -> Option<Object> {
    let mut at = page.get("Parent").and_then(|o| o.as_ref());
    let mut steps = 0;
    while let Some(node) = at {
        steps += 1;
        if steps > 64 {
            break;
        }
        let object = doc.get(node);
        let dict = object.as_dict()?;
        if let Some(value) = dict.get(key) {
            return Some(value.clone());
        }
        at = dict.get("Parent").and_then(|o| o.as_ref());
    }
    None
}

fn rect(r: [f64; 4]) -> Object {
    Object::Array(r.iter().map(|v| Object::real(*v)).collect())
}

/// Copies objects from another file into this one, each once.
#[derive(Default)]
struct Copier {
    /// Where each object already copied went.
    done: HashMap<u32, Ref>,
    /// The sheets being brought in, and where each is going: a link between
    /// two of them still works afterwards.
    pages: HashMap<u32, Ref>,
    /// One null object, for links to anything that is not coming along.
    nothing: Option<Ref>,
}

impl Copier {
    fn object(&mut self, from: &Document, update: &mut Update, object: &Object) -> Object {
        match object {
            Object::Ref(r) => Object::Ref(self.reference(from, update, *r)),
            Object::Array(items) => {
                Object::Array(items.iter().map(|o| self.object(from, update, o)).collect())
            }
            Object::Dict(d) => Object::Dict(self.dict(from, update, d)),
            Object::Stream(s) => Object::Stream(Box::new(Stream {
                dict: self.dict(from, update, &s.dict),
                data: s.data.clone(),
            })),
            other => other.clone(),
        }
    }

    fn dict(&mut self, from: &Document, update: &mut Update, dict: &Dict) -> Dict {
        let mut out = Dict::new();
        for (key, value) in dict.iter() {
            // What ties an object into the other file's structure tree
            // means nothing here.
            if key.is("StructParent") || key.is("StructParents") {
                continue;
            }
            out.set(key.clone(), self.object(from, update, value));
        }
        out
    }

    fn reference(&mut self, from: &Document, update: &mut Update, r: Ref) -> Ref {
        if let Some(to) = self.pages.get(&r.number) {
            return *to;
        }
        if let Some(to) = self.done.get(&r.number) {
            return *to;
        }
        let target = from.get(r);
        let is_tree = target
            .as_dict()
            .and_then(|d| d.get("Type"))
            .is_some_and(|t| t.is_name("Page") || t.is_name("Pages"));
        if is_tree {
            return *self.nothing.get_or_insert_with(|| update.add(Object::Null));
        }
        let to = update.reserve();
        self.done.insert(r.number, to);
        let copied = self.object(from, update, &target);
        update.replace(to, copied);
        to
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(objects: &[&str], root: u32) -> Vec<u8> {
        let mut file = b"%PDF-1.5\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in objects.iter().enumerate() {
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

    /// Three sheets: two in a node of their own that gives them their paper
    /// and their fonts, one straight under the root.
    fn host() -> Document {
        Document::from_bytes(build(
            &[
                "<</Type/Catalog/Pages 2 0 R/Outlines 9 0 R>>",
                "<</Type/Pages/Kids[3 0 R 6 0 R]/Count 3>>",
                "<</Type/Pages/Parent 2 0 R/Kids[4 0 R 5 0 R]/Count 2/MediaBox[0 0 2592 1728]/Resources 7 0 R>>",
                "<</Type/Page/Parent 3 0 R/Contents 8 0 R/Annots[10 0 R]>>",
                "<</Type/Page/Parent 3 0 R/Rotate 90>>",
                "<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>",
                "<</Font<</F1 11 0 R>>>>",
                "<</Length 5>>stream\nBT ET\nendstream",
                "<</Type/Outlines/Count 0>>",
                "<</Type/Annot/Subtype/Square/P 4 0 R/Rect[0 0 10 10]>>",
                "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>",
            ],
            1,
        ))
    }

    fn guest() -> Document {
        Document::from_bytes(build(
            &[
                "<</Type/Catalog/Pages 2 0 R>>",
                "<</Type/Pages/Kids[3 0 R 4 0 R]/Count 2/MediaBox[0 0 1224 792]>>",
                "<</Type/Page/Parent 2 0 R/Resources 5 0 R/Contents 6 0 R/Annots[7 0 R]>>",
                "<</Type/Page/Parent 2 0 R/Resources 5 0 R>>",
                "<</Font<</F1 8 0 R>>>>",
                "<</Length 10>>stream\nBT (hi) ET\nendstream",
                "<</Type/Annot/Subtype/Link/P 3 0 R/Rect[0 0 5 5]/Dest[4 0 R/Fit]>>",
                "<</Type/Font/Subtype/Type1/BaseFont/Courier>>",
            ],
            1,
        ))
    }

    fn after(host: &Document, arranged: &Arranged) -> Document {
        Document::from_bytes(arranged.update.apply(host))
    }

    #[test]
    fn sheets_come_out_in_the_order_asked_for_and_keep_their_own_look() {
        let doc = host();
        let old = doc.pages().to_vec();
        let order = [Slot::Here(2), Slot::Here(0), Slot::Here(1)];
        let arranged = arrange(&doc, &[], &order, &BTreeMap::new()).unwrap();
        let new = after(&doc, &arranged);
        assert_eq!(*new.pages(), vec![old[2], old[0], old[1]]);
        // The sheets that lived in the inner node took its paper with them.
        let first_of_old = new.page(1).unwrap();
        assert_eq!(new.page_box(&first_of_old), [0.0, 0.0, 2592.0, 1728.0]);
        assert!(new.page_attr(&first_of_old, "Resources").as_dict().is_some());
        assert_eq!(new.page_rotation(&new.page(2).unwrap()), 90);
        // Its markup is still on it, and the rest of the file is untouched.
        assert_eq!(new.annotations(&first_of_old).len(), 1);
        assert!(new.catalog().get("Outlines").is_some());
        assert!(new.bytes.starts_with(&doc.bytes), "an update only adds");
    }

    #[test]
    fn going_back_puts_the_old_order_back() {
        let doc = host();
        let old = doc.pages().to_vec();
        let arranged = arrange(&doc, &[], &[Slot::Here(1), Slot::Here(0)], &BTreeMap::new()).unwrap();
        let moved = after(&doc, &arranged);
        assert_eq!(moved.page_count(), 2, "the one left out is out");
        let back = Document::from_bytes(restore(&moved, &arranged.before).apply(&moved));
        assert_eq!(*back.pages(), old);
        let again = Document::from_bytes(restore(&back, &arranged.after).apply(&back));
        assert_eq!(*again.pages(), vec![old[1], old[0]]);
    }

    #[test]
    fn a_turned_sheet_turns_clockwise_from_where_it_was() {
        let doc = host();
        let mut turn = BTreeMap::new();
        turn.insert(1, 1);
        turn.insert(2, -1);
        let order = [Slot::Here(0), Slot::Here(1), Slot::Here(2)];
        let arranged = arrange(&doc, &[], &order, &turn).unwrap();
        let new = after(&doc, &arranged);
        assert_eq!(new.page_rotation(&new.page(0).unwrap()), 0);
        assert_eq!(new.page_rotation(&new.page(1).unwrap()), 180);
        assert_eq!(new.page_rotation(&new.page(2).unwrap()), 270);
        assert_eq!(arranged.turned.len(), 2);
    }

    #[test]
    fn sheets_from_another_file_arrive_whole_and_share_what_they_shared() {
        let doc = host();
        let other = guest();
        let order = [Slot::Here(0), Slot::From(0, 1), Slot::From(0, 0), Slot::Here(1), Slot::Here(2)];
        let arranged = arrange(&doc, &[&other], &order, &BTreeMap::new()).unwrap();
        let new = after(&doc, &arranged);
        assert_eq!(new.page_count(), 5);
        let second = new.page(1).unwrap();
        let third = new.page(2).unwrap();
        // Their paper came from their own file's tree, not this one's.
        assert_eq!(new.page_box(&second), [0.0, 0.0, 1224.0, 792.0]);
        // One copy of the fonts they share.
        assert_eq!(second.get("Resources"), third.get("Resources"));
        let resources = new.page_attr(&third, "Resources");
        let font = new.at(resources.as_dict().unwrap(), "Font");
        let f1 = new.at(font.as_dict().unwrap(), "F1");
        assert!(f1.as_dict().unwrap().get("BaseFont").unwrap().is_name("Courier"));
        // The drawing came across byte for byte.
        let contents = new.at(&third, "Contents");
        assert_eq!(contents.as_stream().unwrap().data, b"BT (hi) ET");
        // The link on it still points at its neighbour, now in this file.
        let link = new.get(new.annotations(&third)[0]);
        let dest = link.as_dict().unwrap().get("Dest").unwrap().as_array().unwrap()[0].as_ref();
        assert_eq!(dest, Some(new.pages()[1]));
        let p = link.as_dict().unwrap().get("P").unwrap().as_ref();
        assert_eq!(p, Some(new.pages()[2]));
    }

    #[test]
    fn a_link_to_a_sheet_left_behind_points_at_nothing() {
        let doc = host();
        let other = guest();
        let arranged = arrange(&doc, &[&other], &[Slot::Here(0), Slot::From(0, 0)], &BTreeMap::new()).unwrap();
        let new = after(&doc, &arranged);
        assert_eq!(new.page_count(), 2);
        let page = new.page(1).unwrap();
        let link = new.get(new.annotations(&page)[0]);
        let dest = link.as_dict().unwrap().get("Dest").unwrap().as_array().unwrap()[0].clone();
        assert!(new.follow(&dest).is_null());
    }

    #[test]
    fn a_blank_sheet_is_the_size_asked_for() {
        let doc = host();
        let arranged = arrange(&doc, &[], &[Slot::Here(0), Slot::Blank([792.0, 612.0])], &BTreeMap::new()).unwrap();
        let new = after(&doc, &arranged);
        assert_eq!(new.page_box(&new.page(1).unwrap()), [0.0, 0.0, 792.0, 612.0]);
    }

    #[test]
    fn nonsense_is_refused_rather_than_written() {
        let doc = host();
        assert!(arrange(&doc, &[], &[], &BTreeMap::new()).is_err());
        assert!(arrange(&doc, &[], &[Slot::Here(0), Slot::Here(0)], &BTreeMap::new()).is_err());
        assert!(arrange(&doc, &[], &[Slot::Here(7)], &BTreeMap::new()).is_err());
        assert!(arrange(&doc, &[], &[Slot::From(0, 0)], &BTreeMap::new()).is_err());
    }
}
