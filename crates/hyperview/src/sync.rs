//! Keeping a drawing set on a server and the copy on this machine in step.
//!
//! The rule the whole thing hangs on: **the PDF on disk is the file of record.**
//! The server is not the truth and this is not a distributed database. What
//! goes up is a copy of markups that are already saved into the file here, and
//! what comes down is written into that same file. Turn the server off and
//! nothing is lost; turn it on again and it catches up.
//!
//! Which makes merging simple enough to be trustworthy. A markup is identified
//! by its `/NM`, unique across machines, and markups do not change under each
//! other — somebody editing one produces a replacement that carries the same
//! name, and a removal travels as a record rather than as an absence. So the
//! merge is: anything with a name we have not got, take; anything marked
//! removed, remove. There is no case where two seats' work has to be
//! reconciled into one markup, because no two seats ever own the same one.

use std::collections::HashSet;

use annot::Markup;

/// What a drawing set knows about where it came from.
#[derive(Clone, Debug, Default)]
pub struct Attached {
    /// The drawing set on the server.
    pub set: String,
    /// The last revision this machine has seen everything up to.
    pub revision: u64,
    /// Names already sent, so a save does not send them again.
    pub sent: HashSet<String>,
}

/// What a merge did, so it can be said out loud rather than happening quietly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Merged {
    /// Markups from other people, added to this file.
    pub taken: usize,
    /// Markups other people removed, taken out of this file.
    pub removed: usize,
    /// Ones that came back from the server that this machine already had.
    pub already_had: usize,
}

impl Merged {
    pub fn nothing(&self) -> bool {
        self.taken == 0 && self.removed == 0
    }

    /// A sentence, or nothing when nothing happened.
    pub fn says(&self) -> Option<String> {
        if self.nothing() {
            return None;
        }
        let mut parts = Vec::new();
        if self.taken > 0 {
            parts.push(format!(
                "{} markup{} from the office",
                self.taken,
                if self.taken == 1 { "" } else { "s" }
            ));
        }
        if self.removed > 0 {
            parts.push(format!(
                "{} removed elsewhere",
                self.removed
            ));
        }
        Some(format!("Brought in {}.", parts.join(", ")))
    }
}

/// Which of these markups the server has not been told about.
///
/// By name rather than by anything about their contents: a markup that has been
/// sent is sent, and one somebody has since changed carries a new name because
/// a change is a replacement.
pub fn to_send<'a>(
    marks: impl Iterator<Item = &'a mut crate::sheet::Mark>,
    attached: &Attached,
) -> Vec<(u32, Markup)> {
    let mut out = Vec::new();
    for mark in marks {
        if mark.gone {
            continue;
        }
        let name = mark.markup.named();
        if attached.sent.contains(&name) {
            continue;
        }
        out.push((mark.page, mark.markup.clone()));
    }
    out
}

/// Works out what a page of markups from the server means for this file.
///
/// Returns what to add and what to take out. Deciding and doing are kept apart
/// so the decision can be tested without a file, a server or a window.
pub fn plan(theirs: &[(String, u32, Markup, bool)], ours: &HashSet<String>) -> (Vec<(u32, Markup)>, Vec<String>, Merged) {
    let mut add = Vec::new();
    let mut remove = Vec::new();
    let mut merged = Merged::default();

    for (name, page, markup, removed) in theirs {
        let have = ours.contains(name);
        match (removed, have) {
            // Gone there and here: take it out.
            (true, true) => {
                remove.push(name.clone());
                merged.removed += 1;
            }
            // Gone there and never here: nothing to do.
            (true, false) => {}
            // Here already — usually our own coming back to us.
            (false, true) => merged.already_had += 1,
            (false, false) => {
                add.push((*page, markup.clone()));
                merged.taken += 1;
            }
        }
    }
    (add, remove, merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use annot::Subtype;

    fn a_markup(name: &str) -> Markup {
        let mut m = Markup::new(Subtype::Square);
        m.set_box([0.0, 0.0, 10.0, 10.0]);
        m.set_name(name);
        m
    }

    fn ours(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn a_markup_from_somebody_else_is_taken() {
        let theirs = vec![("xhv-a".into(), 3u32, a_markup("xhv-a"), false)];
        let (add, remove, merged) = plan(&theirs, &ours(&[]));
        assert_eq!(add.len(), 1);
        assert_eq!(add[0].0, 3, "onto the sheet it was made on");
        assert!(remove.is_empty());
        assert_eq!(merged.taken, 1);
        assert_eq!(merged.says().as_deref(), Some("Brought in 1 markup from the office."));
    }

    #[test]
    fn our_own_markup_coming_back_is_not_added_twice() {
        let theirs = vec![("xhv-a".into(), 0, a_markup("xhv-a"), false)];
        let (add, remove, merged) = plan(&theirs, &ours(&["xhv-a"]));
        assert!(add.is_empty(), "it is already in the file");
        assert!(remove.is_empty());
        assert_eq!(merged.already_had, 1);
        assert!(merged.nothing());
        assert_eq!(merged.says(), None, "and nothing is said about nothing");
    }

    #[test]
    fn a_markup_somebody_else_removed_is_taken_out_here() {
        let theirs = vec![("xhv-a".into(), 0, a_markup("xhv-a"), true)];
        let (add, remove, merged) = plan(&theirs, &ours(&["xhv-a"]));
        assert!(add.is_empty());
        assert_eq!(remove, vec!["xhv-a".to_string()]);
        assert_eq!(merged.removed, 1);
        assert_eq!(merged.says().as_deref(), Some("Brought in 1 removed elsewhere."));
    }

    #[test]
    fn a_removal_of_something_we_never_had_does_nothing() {
        let theirs = vec![("xhv-gone".into(), 0, a_markup("xhv-gone"), true)];
        let (add, remove, merged) = plan(&theirs, &ours(&["xhv-a"]));
        assert!(add.is_empty() && remove.is_empty());
        assert!(merged.nothing());
    }

    #[test]
    fn a_replacement_arrives_as_a_removal_and_an_addition() {
        // Which is how the server sends an edit: the old name goes, a new one
        // comes. Nothing has to be reconciled, because no two seats ever own
        // the same markup.
        let theirs = vec![
            ("xhv-old".into(), 2, a_markup("xhv-old"), true),
            ("xhv-new".into(), 2, a_markup("xhv-new"), false),
        ];
        let (add, remove, merged) = plan(&theirs, &ours(&["xhv-old"]));
        assert_eq!(add.len(), 1);
        assert_eq!(remove, vec!["xhv-old".to_string()]);
        assert_eq!(merged.taken, 1);
        assert_eq!(merged.removed, 1);
        assert_eq!(
            merged.says().as_deref(),
            Some("Brought in 1 markup from the office, 1 removed elsewhere.")
        );
    }

    #[test]
    fn a_markup_already_sent_is_not_sent_again() {
        let mut marks = vec![
            crate::sheet::Mark::new(0, a_markup("xhv-sent")),
            crate::sheet::Mark::new(1, a_markup("xhv-fresh")),
        ];
        let mut attached = Attached::default();
        attached.sent.insert("xhv-sent".to_string());
        let sending = to_send(marks.iter_mut(), &attached);
        assert_eq!(sending.len(), 1);
        assert_eq!(sending[0].0, 1);
        assert_eq!(sending[0].1.name(), "xhv-fresh");
    }

    #[test]
    fn a_markup_taken_off_the_sheet_is_not_sent() {
        let mut marks = vec![crate::sheet::Mark::new(0, a_markup("xhv-a"))];
        marks[0].gone = true;
        let sending = to_send(marks.iter_mut(), &Attached::default());
        assert!(sending.is_empty());
    }

    #[test]
    fn a_markup_with_no_name_gets_one_before_it_goes_anywhere() {
        let mut nameless = Markup::new(Subtype::Square);
        nameless.set_box([0.0, 0.0, 1.0, 1.0]);
        assert_eq!(nameless.name(), "");
        let mut marks = vec![crate::sheet::Mark::new(0, nameless)];
        let sending = to_send(marks.iter_mut(), &Attached::default());
        assert_eq!(sending.len(), 1);
        assert!(annot::name::is_ours(&sending[0].1.name()));
        // And it is the same name in the file, not a passing one.
        assert_eq!(marks[0].markup.name(), sending[0].1.name());
    }
}
