//! Moving sheets about in a drawing set, bringing sheets in from other PDFs,
//! adding a blank sheet, turning sheets and taking them out — what the
//! Thumbnails panel does when a sheet is dragged, a PDF is dropped on it, or
//! its menu is used.
//!
//! All of it changes the drawing itself, in place, and all of it goes back
//! with Undo. The file is only ever added to (see `pdf::pages`), so the
//! markups on a sheet travel with it, bookmarks and layers are left as they
//! were, and undoing is pointing the file at the old order again.
//!
//! Everything the window keeps about a sheet by its place in the set — its
//! picture in the list, its sheet number, the words read off it — is carried
//! to the sheet's new place rather than read again, so a set of eighty
//! sheets can be put in order without eighty thumbnails redrawing.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use pdf::pages::Slot;
use pdf::{Object, Ref};

use crate::sheet::{stamp, write_into_place, Doc, Step, REMEMBERED_STEPS};

/// What somebody asked to happen to the sheets. Sheets are counted from 0,
/// in the set's order as it stands; `before` is the sheet the new or moved
/// ones go in front of, and one past the last sheet means at the end.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    Move { pages: Vec<u32>, before: u32 },
    Insert { files: Vec<PathBuf>, before: u32 },
    Blank { before: u32 },
    Remove { pages: Vec<u32> },
    Turn { pages: Vec<u32>, quarter_turns: i32 },
}

/// A change to the sheets, as the Undo History keeps it: the objects to
/// write to go back, and to go forward again.
#[derive(Clone, Debug)]
pub struct Sheets {
    pub before: Rc<Vec<(Ref, Object)>>,
    pub after: Rc<Vec<(Ref, Object)>>,
    /// Sheets whose own drawing changed, so their pictures are drawn again.
    pub turned: Rc<Vec<Ref>>,
}

/// Sheets being dragged in the Thumbnails list, from the press to the let-go.
pub struct Dragging {
    pub doc: u64,
    pub pages: Vec<u32>,
}

/// Where the Thumbnails list's rows were last drawn, for a PDF let go over it
/// from outside: the drop is dealt with before the list is drawn again.
pub struct Rows {
    pub doc: u64,
    /// The part of the list on screen.
    pub list: egui::Rect,
    /// Each row on screen, with its place in the list.
    pub rows: Vec<(usize, egui::Rect)>,
    /// The sheets the list shows, in order: fewer than all of them while a
    /// search is typed in.
    pub shown: Vec<u32>,
    pub count: usize,
}

impl Rows {
    /// The sheet a PDF let go at this point would go in front of, when the
    /// point is over the list.
    pub fn before_at(&self, at: egui::Pos2) -> Option<u32> {
        if !self.list.contains(at) {
            return None;
        }
        let gap = gap(&self.rows, at.y).unwrap_or(self.shown.len());
        Some(before_gap(&self.shown, gap, self.count))
    }
}

/// True for a file this can bring sheets in from.
pub fn is_pdf(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// The order after moving `moving` to just in front of `before`, as each
/// place's sheet from the old order. Moved sheets keep their order among
/// themselves.
pub fn moved(count: usize, moving: &[u32], before: u32) -> Vec<usize> {
    let moving: Vec<usize> = {
        let mut m: Vec<usize> = moving.iter().map(|p| *p as usize).filter(|p| *p < count).collect();
        m.sort_unstable();
        m.dedup();
        m
    };
    let staying: Vec<usize> = (0..count).filter(|p| !moving.contains(p)).collect();
    let at = staying.iter().filter(|p| **p < before as usize).count();
    let mut order = Vec::with_capacity(count);
    order.extend_from_slice(&staying[..at]);
    order.extend_from_slice(&moving);
    order.extend_from_slice(&staying[at..]);
    order
}

/// True when an order puts every sheet back where it was.
pub fn unchanged(order: &[usize]) -> bool {
    order.iter().enumerate().all(|(i, p)| i == *p)
}

/// Which gap in a list of rows a point is nearest: the row whose middle it
/// is above, or one past the last. `rows` are the rows on screen, each with
/// its place in the list.
pub fn gap(rows: &[(usize, egui::Rect)], y: f32) -> Option<usize> {
    let first = rows.first()?;
    for (index, rect) in rows {
        if y < rect.center().y {
            return Some(*index);
        }
    }
    let last = rows.last().unwrap_or(first);
    Some(last.0 + 1)
}

/// The sheet a gap in the list goes in front of. With a search narrowing
/// the list, a gap after the last sheet shown is in front of the sheet
/// after it.
pub fn before_gap(shown: &[u32], gap: usize, count: usize) -> u32 {
    match shown.get(gap) {
        Some(page) => *page,
        None => shown.last().map(|p| p + 1).unwrap_or(count as u32),
    }
}

/// Whether moving these sheets to that gap would change anything.
pub fn moves_anything(count: usize, moving: &[u32], before: u32) -> bool {
    !unchanged(&moved(count, moving, before))
}

/// Opens a PDF to bring sheets in from.
fn open_guest(path: &Path) -> Result<pdf::Document, String> {
    let name = crate::app::name_of(path);
    let mut doc = pdf::Document::open(path).map_err(|e| format!("{name} could not be read: {e}"))?;
    if doc.encrypted && !doc.unlock("") {
        return Err(format!(
            "{name} is locked with a password. Open it on its own, take the lock off in \
             Document → Security, and bring it in from there."
        ));
    }
    if doc.page_count() == 0 {
        return Err(format!("{name} has no sheets in it."));
    }
    Ok(doc)
}

/// Keys a map by where each sheet went. Anything whose sheet went, or whose
/// sheet is not the same any more, is dropped.
fn follow<T>(map: &mut HashMap<u32, T>, to: &HashMap<u32, u32>) {
    let old = std::mem::take(map);
    for (page, value) in old {
        if let Some(new) = to.get(&page) {
            map.insert(*new, value);
        }
    }
}

impl Doc {
    /// Why this drawing's sheets cannot be changed here, when they cannot.
    pub fn why_sheets_stay(&self) -> Option<String> {
        if self.read_only {
            return Some(self.why_read_only());
        }
        if !self.password.is_empty() {
            // The drawing is read again from the file after its sheets move,
            // and a set that needs a password to open cannot be.
            return Some(
                "This set opens with a password, and its sheets can't be moved while it does. \
                 Document → Security takes the lock off."
                    .into(),
            );
        }
        if let Some(allowed) = self.file.allowed() {
            if !allowed.assemble && !allowed.change {
                return Some(
                    "Whoever locked this set asked that its sheets not be moved, added or taken \
                     out. Ask them for the set without that lock, or for its owner's password."
                        .into(),
                );
            }
        }
        if self.attached.is_some() {
            return Some(
                "This drawing is shared through the office's server, which keeps everybody's \
                 markups sheet by sheet. Moving its sheets here would put them on the wrong \
                 sheets for everyone else. Save a copy with File → Save As to rearrange it."
                    .into(),
            );
        }
        None
    }

    /// Changes the sheets as planned, as one step Undo takes back. Says what
    /// happened, in a sentence for the status bar.
    pub fn change_sheets(&mut self, plan: &Plan, author: &str) -> Result<String, String> {
        if let Some(why) = self.why_sheets_stay() {
            return Err(why);
        }
        // Markups go into the file first, so they are on their sheets and
        // travel with them.
        self.draft = None;
        self.save(author)?;
        if stamp(&self.path).is_some() && stamp(&self.path) != self.on_disk {
            return Err(
                "Something else changed the file since it was opened here. Close it and open \
                 it again, then try once more."
                    .into(),
            );
        }
        let count = self.pages.len();
        let mut turn = BTreeMap::new();
        let mut guests: Vec<pdf::Document> = Vec::new();
        let mut show: Option<u32> = None;
        let names = |doc: &Doc, pages: &[u32]| -> String {
            match pages {
                [one] => doc.sheet_name(*one),
                many => format!("{} sheets", many.len()),
            }
        };
        let (order, what, said): (Vec<Slot>, String, String) = match plan {
            Plan::Move { pages, before } => {
                let order = moved(count, pages, *before);
                if unchanged(&order) {
                    return Ok("The sheets are already in that order.".into());
                }
                let who = names(self, pages);
                let place = if (*before as usize) >= count {
                    "to the end".to_string()
                } else {
                    format!("in front of {}", self.sheet_name(*before))
                };
                let said = format!("Moved {who} {place}. Undo puts {} back.", them(pages.len()));
                (order.into_iter().map(Slot::Here).collect(), format!("Move {who}"), said)
            }
            Plan::Insert { files, before } => {
                for path in files {
                    guests.push(open_guest(path)?);
                }
                let at = (*before as usize).min(count);
                let mut order: Vec<Slot> = (0..at).map(Slot::Here).collect();
                let mut added = 0usize;
                for (g, guest) in guests.iter().enumerate() {
                    for p in 0..guest.page_count() {
                        order.push(Slot::From(g, p));
                        added += 1;
                    }
                }
                order.extend((at..count).map(Slot::Here));
                show = Some(at as u32);
                let from = match files.as_slice() {
                    [one] => crate::app::name_of(one),
                    many => format!("{} files", many.len()),
                };
                let place = if at == 0 {
                    "at the front".to_string()
                } else {
                    format!("after {}", self.sheet_name(at as u32 - 1))
                };
                let sheets = if added == 1 { "1 sheet".to_string() } else { format!("{added} sheets") };
                (order, format!("Insert {from}"), format!("Added {sheets} from {from} {place}."))
            }
            Plan::Blank { before } => {
                let at = (*before as usize).min(count);
                // The same paper as the sheet it follows, the way it is seen.
                let beside = if at > 0 { at - 1 } else { 0 };
                let size = self
                    .pages
                    .get(beside)
                    .map(|p| [p.width as f64, p.height as f64])
                    .unwrap_or([2592.0, 1728.0]);
                let mut order: Vec<Slot> = (0..at).map(Slot::Here).collect();
                order.push(Slot::Blank(size));
                order.extend((at..count).map(Slot::Here));
                show = Some(at as u32);
                (order, "Add a blank sheet".into(), format!("Added a blank sheet as sheet {}.", at + 1))
            }
            Plan::Remove { pages } => {
                let gone: HashSet<usize> = pages.iter().map(|p| *p as usize).collect();
                let order: Vec<Slot> = (0..count).filter(|p| !gone.contains(p)).map(Slot::Here).collect();
                if order.is_empty() {
                    return Err("A drawing set has to keep at least one sheet.".into());
                }
                let who = names(self, pages);
                let said = format!("Took out {who}. Undo puts {} back.", them(pages.len()));
                (order, format!("Delete {who}"), said)
            }
            Plan::Turn { pages, quarter_turns } => {
                for page in pages {
                    turn.insert(*page as usize, *quarter_turns);
                }
                let who = names(self, pages);
                let way = if quarter_turns.rem_euclid(4) == 3 { "left" } else { "right" };
                (
                    (0..count).map(Slot::Here).collect(),
                    format!("Turn {who}"),
                    format!("Turned {who} to the {way}."),
                )
            }
        };
        let guest_refs: Vec<&pdf::Document> = guests.iter().collect();
        let arranged = pdf::pages::arrange(&self.file, &guest_refs, &order, &turn)?;
        let old: Vec<Ref> = self.file.pages().to_vec();
        let bytes = arranged.update.apply(&self.file);
        write_into_place(&self.path, &bytes)?;
        self.file = self.read_again(bytes);
        self.on_disk = stamp(&self.path);
        let sheets = Sheets {
            before: Rc::new(arranged.before),
            after: Rc::new(arranged.after),
            turned: Rc::new(arranged.turned),
        };
        self.carry_over(&old, &sheets.turned);
        if let Some(page) = show {
            self.page = page.min(self.pages.len().saturating_sub(1) as u32);
        }
        self.undo.push(Step {
            what,
            marks: self.marks.clone(),
            file: None,
            sheets: Some(sheets),
        });
        if self.undo.len() > REMEMBERED_STEPS {
            self.undo.remove(0);
        }
        self.redo.clear();
        Ok(said)
    }

    /// Puts the sheets back the way they were before a change, or the way the
    /// change left them, for Undo and Redo.
    pub fn swap_sheets(&mut self, sheets: &Sheets, forward: bool) -> Result<(), String> {
        if self.read_only {
            return Err("the drawing is read-only".into());
        }
        // Markups changed since go in first; reading the sheets back must
        // not lose them.
        let author = self.saved_by.clone();
        self.save(&author)?;
        let objects = if forward { &sheets.after } else { &sheets.before };
        let old: Vec<Ref> = self.file.pages().to_vec();
        let bytes = pdf::pages::restore(&self.file, objects).apply(&self.file);
        write_into_place(&self.path, &bytes)?;
        self.file = self.read_again(bytes);
        self.on_disk = stamp(&self.path);
        self.carry_over(&old, &sheets.turned);
        Ok(())
    }

    /// Reads the sheets again after their order changed, carrying across
    /// everything kept about each sheet to where that sheet now is.
    fn carry_over(&mut self, old: &[Ref], turned: &[Ref]) {
        let new: Vec<Ref> = self.file.pages().to_vec();
        let turned: HashSet<Ref> = turned.iter().copied().collect();
        let was_at: HashMap<Ref, u32> = old.iter().enumerate().map(|(i, r)| (*r, i as u32)).collect();
        // Where each sheet went, and where each sheet that looks the same went.
        let mut went: HashMap<u32, u32> = HashMap::new();
        let mut same: HashMap<u32, u32> = HashMap::new();
        for (i, r) in new.iter().enumerate() {
            if let Some(from) = was_at.get(r) {
                went.insert(*from, i as u32);
                if !turned.contains(r) {
                    same.insert(*from, i as u32);
                }
            }
        }
        let (pages, frames) = crate::sheet::sizes(&self.file);
        self.pages = pages;
        self.frames = frames;
        let last = self.pages.len().saturating_sub(1) as u32;

        let labels = std::mem::take(&mut self.labels);
        let mut all_known = (self.labelled as usize) >= old.len();
        self.labels = new
            .iter()
            .map(|r| match was_at.get(r).and_then(|i| labels.get(*i as usize)) {
                Some(label) => label.clone(),
                None => {
                    all_known = false;
                    Default::default()
                }
            })
            .collect();
        self.labelled = if all_known { self.pages.len() as u32 } else { 0 };
        self.relabel = !all_known;

        follow(&mut self.thumbs, &same);
        follow(&mut self.geometry, &same);
        follow(&mut self.paragraphs, &same);
        let previews = std::mem::take(&mut self.previews);
        for (page, picture) in previews {
            if let Some(to) = same.get(&page) {
                self.previews.insert(*to, picture);
            }
        }
        // Anything asked for and not yet back will come back for the old
        // order and be turned away, so it is asked for again.
        self.asked = self.thumbs.keys().copied().collect();
        self.geometry_asked = self.geometry.keys().copied().collect();
        self.paragraphs_asked = self.paragraphs.keys().copied().collect();
        self.tiles = Default::default();
        self.sent.clear();
        self.redraw.clear();
        self.redrawing.clear();

        let place = |page: u32| went.get(&page).copied().unwrap_or(page).min(last);
        self.page = place(self.page);
        if let Some(other) = self.other.as_mut() {
            other.page = place(other.page);
        }
        self.picks.follow(|page| went.get(&page).copied());
        self.scales.clear();
        self.draft = None;
        self.selected = None;
        self.also.clear();
        self.marks.clear();
        self.reload_marks();
        self.dirty = false;
        self.reshuffled = true;
        self.sheets_changed = true;
    }
}

fn them(count: usize) -> &'static str {
    if count == 1 {
        "it"
    } else {
        "them"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sheet_dragged_up_goes_in_front_of_the_one_it_was_dropped_on() {
        // S-301 (index 3) dropped in front of S-101 (index 1).
        assert_eq!(moved(5, &[3], 1), vec![0, 3, 1, 2, 4]);
        // And dragged down, to the end.
        assert_eq!(moved(5, &[1], 5), vec![0, 2, 3, 4, 1]);
    }

    #[test]
    fn several_picked_sheets_move_together_in_their_own_order() {
        assert_eq!(moved(6, &[4, 1], 0), vec![1, 4, 0, 2, 3, 5]);
        assert_eq!(moved(6, &[0, 2], 4), vec![1, 3, 0, 2, 4, 5]);
    }

    #[test]
    fn dropping_a_sheet_where_it_already_is_changes_nothing() {
        assert!(!moves_anything(4, &[2], 2));
        assert!(!moves_anything(4, &[2], 3), "just after itself is where it is");
        assert!(moves_anything(4, &[2], 1));
    }

    #[test]
    fn the_gap_is_above_whichever_row_the_pointer_is_over_the_top_half_of() {
        let row = |i: usize| (i, egui::Rect::from_min_size(egui::pos2(0.0, i as f32 * 70.0), egui::vec2(200.0, 64.0)));
        let rows: Vec<_> = (3..6).map(row).collect();
        assert_eq!(gap(&rows, 3.0 * 70.0 + 5.0), Some(3));
        assert_eq!(gap(&rows, 3.0 * 70.0 + 50.0), Some(4));
        assert_eq!(gap(&rows, 5.0 * 70.0 + 60.0), Some(6));
        assert_eq!(gap(&[], 10.0), None);
    }

    fn sample() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../mobile/samples/Northgate Warehouse - Structural Set.pdf")
    }

    fn copy_of_sample(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("excalibur-reorder-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("set.pdf");
        std::fs::copy(sample(), &path).unwrap();
        path
    }

    fn markups_on(doc: &Doc, sheet: Ref) -> usize {
        let at = doc.file.pages().iter().position(|r| *r == sheet).expect("sheet in the set") as u32;
        doc.marks.iter().filter(|m| m.page == at && !m.gone).count()
    }

    #[test]
    fn sheets_move_come_in_turn_and_go_and_every_step_goes_back() {
        let path = copy_of_sample("steps");
        let mut doc = Doc::open(path.clone()).unwrap();
        let first: Vec<Ref> = doc.file.pages().to_vec();
        let marked = first[2];
        let markups = doc.marks.len();
        assert!(markups_on(&doc, marked) > 0, "the sample has a marked-up sheet");
        let on_marked = markups_on(&doc, marked);
        let original = std::fs::read(&path).unwrap();

        // The last sheet to the front: its markups and everyone else's stay put.
        doc.change_sheets(&Plan::Move { pages: vec![5], before: 0 }, "Test").unwrap();
        assert_eq!(doc.file.pages()[0], first[5]);
        assert_eq!(doc.marks.len(), markups);
        assert_eq!(markups_on(&doc, marked), on_marked);

        // Another copy of the whole set, after the first sheet, markups and all.
        doc.change_sheets(&Plan::Insert { files: vec![sample()], before: 1 }, "Test").unwrap();
        assert_eq!(doc.pages.len(), 12);
        assert_eq!(doc.marks.len(), markups * 2);

        doc.change_sheets(&Plan::Blank { before: 12 }, "Test").unwrap();
        assert_eq!(doc.pages.len(), 13);
        assert_eq!(doc.page, 12, "the new sheet is the one shown");

        doc.change_sheets(&Plan::Remove { pages: vec![0, 12] }, "Test").unwrap();
        assert_eq!(doc.pages.len(), 11);
        assert!(!doc.file.pages().contains(&first[5]));

        let (w, h) = (doc.pages[0].width, doc.pages[0].height);
        doc.change_sheets(&Plan::Turn { pages: vec![0], quarter_turns: 1 }, "Test").unwrap();
        assert_eq!((doc.pages[0].width, doc.pages[0].height), (h, w));

        // What is on disk is what is on screen.
        let read = Doc::open(path.clone()).unwrap();
        assert_eq!(*read.file.pages(), *doc.file.pages());
        assert_eq!(read.marks.len(), doc.marks.len());
        assert!(std::fs::read(&path).unwrap().starts_with(&original), "only ever added to");

        for _ in 0..5 {
            assert!(doc.undo());
        }
        assert_eq!(*doc.file.pages(), first);
        assert_eq!(doc.marks.len(), markups);
        assert_eq!(markups_on(&doc, marked), on_marked);
        assert_eq!(*Doc::open(path.clone()).unwrap().file.pages(), first);

        for _ in 0..5 {
            assert!(doc.redo());
        }
        assert_eq!(doc.pages.len(), 11);
        assert_eq!(doc.marks.len(), read.marks.len());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn undoing_a_markup_and_then_the_move_before_it_leaves_neither() {
        let path = copy_of_sample("drawn");
        let mut doc = Doc::open(path.clone()).unwrap();
        doc.change_sheets(&Plan::Move { pages: vec![0], before: 6 }, "Test").unwrap();
        let before = doc.marks.len();
        let mut markup = doc.marks.iter().find(|m| !m.gone).unwrap().markup.clone();
        markup.set_name("drawn-after-the-move");
        doc.checkpoint_named("Add a markup");
        doc.marks.push(crate::sheet::Mark::new(5, markup));
        doc.save("Test").unwrap();
        // Undo the markup, then the move: the markup is gone and stays gone.
        assert!(doc.undo());
        assert!(doc.undo());
        assert_eq!(doc.marks.iter().filter(|m| !m.gone).count(), before);
        let read = Doc::open(path.clone()).unwrap();
        assert!(!read.marks.iter().any(|m| m.markup.name() == "drawn-after-the-move"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_markup_not_saved_yet_goes_with_its_sheet() {
        let path = copy_of_sample("unsaved");
        let mut doc = Doc::open(path.clone()).unwrap();
        let sheet = doc.file.pages()[3];
        let mut markup = doc.marks.iter().find(|m| !m.gone).unwrap().markup.clone();
        markup.set_name("not-saved-yet");
        doc.checkpoint_named("Add a markup");
        doc.marks.push(crate::sheet::Mark::new(3, markup));
        assert!(doc.dirty);
        let on_it = markups_on(&doc, sheet);
        doc.change_sheets(&Plan::Move { pages: vec![3], before: 0 }, "Test").unwrap();
        assert_eq!(markups_on(&doc, sheet), on_it);
        assert!(doc.marks.iter().any(|m| m.markup.name() == "not-saved-yet" && m.page == 0));
        assert!(doc.undo());
        assert!(doc.marks.iter().any(|m| m.markup.name() == "not-saved-yet" && m.page == 3));
        let read = Doc::open(path.clone()).unwrap();
        assert!(read.marks.iter().any(|m| m.markup.name() == "not-saved-yet" && m.page == 3));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// The sample, locked with an owner's password only, the way most sets
    /// sent out by an engineer are: anyone can open it.
    fn locked_copy_of_sample(name: &str, allowed: pdf::crypt::Allowed) -> PathBuf {
        let path = copy_of_sample(name);
        let plain = pdf::Document::open(&path).unwrap();
        let locked = pdf::crypt::lock("", "the owner", allowed, [7; 32], [9; 32]);
        std::fs::write(&path, pdf::crypt::write_locked(&plain, &locked, [3; 16])).unwrap();
        path
    }

    #[test]
    fn a_locked_set_stays_readable_after_saving_and_moving_sheets() {
        let path = locked_copy_of_sample("locked", Default::default());
        let mut doc = Doc::open(path.clone()).unwrap();
        assert!(doc.locked.is_some());
        let markups = doc.marks.len();
        for (name, sheet) in [("first-save", 1u32), ("second-save", 2)] {
            let mut markup = doc.marks.iter().find(|m| !m.gone).unwrap().markup.clone();
            markup.set_name(name);
            doc.checkpoint_named("Add a markup");
            doc.marks.push(crate::sheet::Mark::new(sheet, markup));
            doc.save("Test").unwrap();
            assert!(!doc.file.still_locked(), "read again with its password");
            assert!(doc.marks.iter().any(|m| m.markup.name() == name), "{name} reads back");
        }
        doc.change_sheets(&Plan::Move { pages: vec![2], before: 0 }, "Test").unwrap();
        assert!(!doc.file.still_locked());
        let read = Doc::open(path.clone()).unwrap();
        assert_eq!(read.marks.len(), markups + 2);
        assert!(read.marks.iter().any(|m| m.markup.name() == "second-save" && m.page == 0));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_set_locked_against_moving_sheets_is_left_as_it_is() {
        let mut allowed = pdf::crypt::Allowed::default();
        allowed.assemble = false;
        allowed.change = false;
        let path = locked_copy_of_sample("no-assembly", allowed);
        let mut doc = Doc::open(path.clone()).unwrap();
        assert!(doc.why_sheets_stay().is_some());
        assert!(doc.change_sheets(&Plan::Remove { pages: vec![0] }, "Test").is_err());
        assert_eq!(Doc::open(path.clone()).unwrap().pages.len(), 6);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_set_that_cannot_be_written_says_so_and_is_left_alone() {
        let path = copy_of_sample("readonly");
        let mut doc = Doc::open(path.clone()).unwrap();
        doc.read_only = true;
        let before = std::fs::read(&path).unwrap();
        assert!(doc.change_sheets(&Plan::Move { pages: vec![1], before: 0 }, "Test").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(doc.undo.is_empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_pdf_let_go_over_the_list_goes_in_front_of_the_sheet_under_it() {
        let rows: Vec<(usize, egui::Rect)> = (0..4)
            .map(|i| (i, egui::Rect::from_min_size(egui::pos2(40.0, 100.0 + i as f32 * 70.0), egui::vec2(300.0, 64.0))))
            .collect();
        let list = Rows {
            doc: 1,
            list: egui::Rect::from_min_max(egui::pos2(40.0, 100.0), egui::pos2(340.0, 500.0)),
            rows,
            shown: vec![0, 1, 2, 3],
            count: 4,
        };
        assert_eq!(list.before_at(egui::pos2(100.0, 110.0)), Some(0));
        assert_eq!(list.before_at(egui::pos2(100.0, 150.0)), Some(1));
        assert_eq!(list.before_at(egui::pos2(100.0, 480.0)), Some(4), "below the last: at the end");
        assert_eq!(list.before_at(egui::pos2(600.0, 150.0)), None, "off the list: opened instead");
        assert!(is_pdf(Path::new("A-101.PDF")) && !is_pdf(Path::new("tools.evtools")));
    }

    #[test]
    fn with_a_search_narrowing_the_list_a_gap_means_the_sheet_under_it() {
        let shown = [2, 5, 9];
        assert_eq!(before_gap(&shown, 1, 12), 5);
        assert_eq!(before_gap(&shown, 3, 12), 10);
        assert_eq!(before_gap(&[], 0, 12), 12);
    }
}
