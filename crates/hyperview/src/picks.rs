//! Sheets picked in the Thumbnails panel, to print or save just those.
//!
//! It works the way picking files in Explorer does, because that is the habit
//! everybody already has. A plain click goes to a sheet. Ctrl-click (Cmd on a
//! Mac) adds a sheet or takes it back off, Shift-click picks the run from the
//! last sheet clicked, and Ctrl-Shift-click adds a run to what is already
//! picked. Picking never moves the drawing: you can keep looking at one sheet
//! while you choose five others.
//!
//! Nothing picked means "the sheet on screen", so every action that works on
//! the picked sheets works on the current one when there are none.

use std::collections::BTreeSet;

/// How a thumbnail was clicked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Click {
    /// Go to it, and forget any picks.
    Plain,
    /// Ctrl-click: add it, or take it back off.
    Toggle,
    /// Shift-click: the run from the last sheet clicked to this one.
    Run,
    /// Ctrl-Shift-click: that run, added to what is already picked.
    AddRun,
}

impl Click {
    pub fn from(modifiers: egui::Modifiers) -> Click {
        match (modifiers.command, modifiers.shift) {
            (true, true) => Click::AddRun,
            (true, false) => Click::Toggle,
            (false, true) => Click::Run,
            (false, false) => Click::Plain,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Picks {
    sheets: BTreeSet<u32>,
    /// Where a Shift-click run starts from: the last sheet clicked.
    anchor: Option<u32>,
}

impl Picks {
    /// A click on `page`. `current` is the sheet on screen and `shown` the
    /// list as it stands (a search can hide some), in order. Says whether the
    /// click should go to the sheet, which only a plain click does.
    pub fn click(&mut self, page: u32, how: Click, current: u32, shown: &[u32]) -> bool {
        match how {
            Click::Plain => {
                self.sheets.clear();
                self.anchor = Some(page);
                true
            }
            Click::Toggle => {
                // The sheet on screen counts as picked until something else
                // is, the same as the one file you last clicked in Explorer.
                if self.sheets.is_empty() {
                    self.sheets.insert(current);
                }
                if !self.sheets.remove(&page) {
                    self.sheets.insert(page);
                }
                self.anchor = Some(page);
                false
            }
            Click::Run | Click::AddRun => {
                let from = self.anchor.unwrap_or(current);
                let (low, high) = (from.min(page), from.max(page));
                if how == Click::Run {
                    self.sheets.clear();
                }
                // Only what the list is showing: a run across a search for
                // "S-" should not quietly pick the architectural sheets
                // hidden between them.
                self.sheets
                    .extend(shown.iter().copied().filter(|p| (low..=high).contains(p)));
                false
            }
        }
    }

    /// A right-click. Right-clicking a sheet that isn't picked picks it on its
    /// own, so the menu that opens is about the sheet under the pointer.
    pub fn right_click(&mut self, page: u32, current: u32) {
        if !self.covers(page, current) {
            self.sheets.clear();
            self.sheets.insert(page);
            self.anchor = Some(page);
        }
    }

    /// Whether an action on "the picked sheets" would include `page`.
    pub fn covers(&self, page: u32, current: u32) -> bool {
        if self.sheets.is_empty() {
            page == current
        } else {
            self.sheets.contains(&page)
        }
    }

    pub fn contains(&self, page: u32) -> bool {
        self.sheets.contains(&page)
    }

    pub fn is_empty(&self) -> bool {
        self.sheets.is_empty()
    }

    pub fn count(&self) -> usize {
        self.sheets.len()
    }

    pub fn clear(&mut self) {
        self.sheets.clear();
    }

    /// Every sheet the list is showing.
    pub fn all(&mut self, shown: &[u32]) {
        self.sheets = shown.iter().copied().collect();
    }

    /// What an action on the picked sheets acts on, in drawing order: the
    /// picks, or the sheet on screen when there are none.
    pub fn sheets(&self, current: u32) -> Vec<u32> {
        if self.sheets.is_empty() {
            vec![current]
        } else {
            self.sheets.iter().copied().collect()
        }
    }

    /// The picks, when they are worth offering as a choice of their own: more
    /// than the sheet on screen, which already has a choice of its own.
    pub fn offered(&self, current: u32) -> Option<Vec<u32>> {
        match self.sheets.len() {
            0 => None,
            1 if self.sheets.contains(&current) => None,
            _ => Some(self.sheets.iter().copied().collect()),
        }
    }

    /// Drops picks past the end, for when a drawing has fewer sheets than it did.
    pub fn keep_within(&mut self, count: usize) {
        self.sheets.retain(|p| (*p as usize) < count);
        if self.anchor.is_some_and(|a| a as usize >= count) {
            self.anchor = None;
        }
    }
}

/// Sheets (counted from 0) the way somebody types them in a sheet box, counted
/// from 1 with runs joined up: `[0, 1, 2, 6]` is "1-3, 7".
pub fn as_range(pages: &[u32]) -> String {
    let mut sorted: Vec<u32> = pages.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i];
        let mut end = start;
        while i + 1 < sorted.len() && sorted[i + 1] == end + 1 {
            i += 1;
            end = sorted[i];
        }
        parts.push(if end == start {
            format!("{}", start + 1)
        } else {
            format!("{}-{}", start + 1, end + 1)
        });
        i += 1;
    }
    parts.join(", ")
}

/// The name a saved copy of some sheets is offered under. One sheet with a
/// number is named for it; otherwise it says how many there are.
pub fn file_name(stem: &str, numbers: &[String]) -> String {
    let tidy = |s: &str| -> String {
        s.chars()
            .map(|c| if "\\/:*?\"<>|".contains(c) || c.is_control() { '-' } else { c })
            .collect::<String>()
            .trim()
            .to_string()
    };
    match numbers {
        [one] if !tidy(one).is_empty() => format!("{stem} - {}.pdf", tidy(one)),
        [_] => format!("{stem} - 1 sheet.pdf"),
        many => format!("{stem} - {} sheets.pdf", many.len()),
    }
}

/// A few sheet numbers for a sentence: "S-101, S-102 and S-104", or the first
/// few and how many more.
pub fn in_words(numbers: &[String]) -> String {
    let named: Vec<&str> = numbers.iter().map(|n| n.trim()).filter(|n| !n.is_empty()).collect();
    if named.len() != numbers.len() || named.is_empty() {
        return String::new();
    }
    match named.len() {
        1 => named[0].to_string(),
        2..=4 => format!("{} and {}", named[..named.len() - 1].join(", "), named[named.len() - 1]),
        n => format!("{}, {} and {} more", named[0], named[1], n - 2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: &[u32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9];

    #[test]
    fn a_plain_click_goes_there_and_forgets_the_picks() {
        let mut picks = Picks::default();
        picks.click(3, Click::Toggle, 0, ALL);
        assert!(picks.click(5, Click::Plain, 0, ALL));
        assert!(picks.is_empty());
        assert_eq!(picks.sheets(5), vec![5]);
    }

    #[test]
    fn ctrl_click_adds_to_the_sheet_on_screen_and_never_moves() {
        let mut picks = Picks::default();
        // Looking at sheet 2; Ctrl-click 5 and 7.
        assert!(!picks.click(5, Click::Toggle, 2, ALL));
        assert!(!picks.click(7, Click::Toggle, 2, ALL));
        assert_eq!(picks.sheets(2), vec![2, 5, 7]);
        // And again takes one back off.
        picks.click(5, Click::Toggle, 2, ALL);
        assert_eq!(picks.sheets(2), vec![2, 7]);
    }

    #[test]
    fn ctrl_clicking_the_sheet_on_screen_leaves_nothing_picked() {
        let mut picks = Picks::default();
        picks.click(4, Click::Toggle, 4, ALL);
        assert!(picks.is_empty());
    }

    #[test]
    fn shift_click_picks_the_run_from_the_last_click() {
        let mut picks = Picks::default();
        picks.click(2, Click::Plain, 0, ALL);
        picks.click(5, Click::Run, 2, ALL);
        assert_eq!(picks.sheets(2), vec![2, 3, 4, 5]);
        // Backwards works the same.
        picks.click(0, Click::Run, 2, ALL);
        assert_eq!(picks.sheets(2), vec![0, 1, 2]);
    }

    #[test]
    fn ctrl_shift_click_adds_a_second_run() {
        let mut picks = Picks::default();
        picks.click(1, Click::Plain, 0, ALL);
        picks.click(2, Click::Run, 1, ALL);
        picks.click(6, Click::Toggle, 1, ALL);
        picks.click(8, Click::AddRun, 1, ALL);
        assert_eq!(picks.sheets(1), vec![1, 2, 6, 7, 8]);
    }

    #[test]
    fn a_run_only_picks_what_the_list_is_showing() {
        let mut picks = Picks::default();
        let found = [1, 4, 6, 9];
        picks.click(1, Click::Plain, 0, &found);
        picks.click(9, Click::Run, 1, &found);
        assert_eq!(picks.sheets(1), vec![1, 4, 6, 9]);
    }

    #[test]
    fn a_right_click_off_the_picks_is_about_that_sheet_alone() {
        let mut picks = Picks::default();
        picks.click(3, Click::Toggle, 0, ALL);
        picks.right_click(7, 0);
        assert_eq!(picks.sheets(0), vec![7]);
        // On one that is picked, the picks stay.
        picks.click(8, Click::Toggle, 0, ALL);
        picks.right_click(7, 0);
        assert_eq!(picks.sheets(0), vec![7, 8]);
        // With nothing picked, the sheet on screen is what the menu is about.
        let mut none = Picks::default();
        none.right_click(4, 4);
        assert!(none.is_empty());
    }

    #[test]
    fn only_more_than_the_sheet_on_screen_is_offered_as_its_own_choice() {
        let mut picks = Picks::default();
        assert_eq!(picks.offered(2), None);
        picks.click(2, Click::Toggle, 5, ALL); // picks 5 and 2
        assert_eq!(picks.offered(5), Some(vec![2, 5]));
        picks.click(2, Click::Toggle, 5, ALL); // back to 5 alone
        assert_eq!(picks.offered(5), None);
    }

    #[test]
    fn picks_past_the_end_go_when_the_drawing_gets_shorter() {
        let mut picks = Picks::default();
        picks.click(8, Click::Toggle, 1, ALL);
        picks.keep_within(5);
        assert_eq!(picks.sheets(1), vec![1]);
    }

    #[test]
    fn ranges_are_written_the_way_people_type_them() {
        assert_eq!(as_range(&[0, 1, 2, 6]), "1-3, 7");
        assert_eq!(as_range(&[4]), "5");
        assert_eq!(as_range(&[9, 3, 4]), "4-5, 10");
        assert_eq!(as_range(&[]), "");
        // And they read back as the same sheets.
        assert_eq!(crate::print::read_range(&as_range(&[0, 1, 2, 6]), 20), vec![0, 1, 2, 6]);
    }

    #[test]
    fn a_saved_copy_is_named_for_its_sheet_or_its_count() {
        let n = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(file_name("Northgate", &n(&["S-101"])), "Northgate - S-101.pdf");
        assert_eq!(file_name("Northgate", &n(&["S/101"])), "Northgate - S-101.pdf");
        assert_eq!(file_name("Northgate", &n(&[""])), "Northgate - 1 sheet.pdf");
        assert_eq!(file_name("Northgate", &n(&["S-101", "S-102", "S-104"])), "Northgate - 3 sheets.pdf");
    }

    #[test]
    fn sheet_numbers_read_as_a_sentence() {
        let n = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(in_words(&n(&["S-101"])), "S-101");
        assert_eq!(in_words(&n(&["S-101", "S-102", "S-104"])), "S-101, S-102 and S-104");
        assert_eq!(in_words(&n(&["A", "B", "C", "D", "E", "F"])), "A, B and 4 more");
        // A sheet with no number read off it: don't name some and not others.
        assert_eq!(in_words(&n(&["S-101", ""])), "");
    }
}
