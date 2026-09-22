//! VisualSearch, as the user meets it.
//!
//! Draw a box round a symbol and it finds the others: on this sheet, or across
//! the whole set. What comes back is a list of places, drawn on the drawing,
//! and **not a count**. A count appears when somebody says these are the right
//! ones — which is the same rule the rest of this program follows, and matters
//! more here than anywhere: a symbol search that quietly turned matches into
//! quantities would be the fastest way yet to price a job on numbers nobody
//! ever looked at.

use std::collections::BTreeMap;

/// One place a symbol was found, and whether it is wanted.
#[derive(Clone, Copy, Debug)]
pub struct Sighting {
    /// In sheet coordinates.
    pub area: [f64; 4],
    /// Somebody has taken this one out of the count.
    pub dropped: bool,
}

#[derive(Default)]
pub struct Looking {
    /// The box drawn round the symbol, in sheet coordinates.
    pub symbol: Option<[f64; 4]>,
    /// Being drawn right now.
    pub drawing: Option<[f64; 4]>,
    pub from_page: u32,
    /// Look on every sheet rather than only this one.
    pub everywhere: bool,
    /// How alike a match has to be.
    pub alike: f32,
    pub darkness: f32,
    pub job: u64,
    pub running: bool,
    pub looked_at: u32,
    pub sheets: u32,
    /// What was found, by sheet.
    pub found: BTreeMap<u32, Vec<Sighting>>,
}

impl Looking {
    pub fn new() -> Looking {
        Looking {
            alike: takeoff::symbols::ALIKE,
            darkness: 0.55,
            ..Default::default()
        }
    }

    pub fn begin(&mut self, from_page: u32, sheets: u32) -> u64 {
        self.job += 1;
        self.found.clear();
        self.from_page = from_page;
        self.looked_at = 0;
        self.sheets = sheets;
        self.running = self.symbol.is_some();
        self.job
    }

    pub fn accept(&mut self, job: u64, page: u32, places: Vec<[f64; 4]>, done: bool) {
        if job != self.job {
            return;
        }
        self.looked_at += 1;
        if !places.is_empty() {
            self.found.insert(
                page,
                places
                    .into_iter()
                    .map(|area| Sighting {
                        area,
                        dropped: false,
                    })
                    .collect(),
            );
        }
        if done {
            self.running = false;
        }
    }

    /// How many are wanted: the number a count would come to if applied now.
    pub fn wanted(&self) -> usize {
        self.found
            .values()
            .flatten()
            .filter(|s| !s.dropped)
            .count()
    }

    pub fn total(&self) -> usize {
        self.found.values().map(|v| v.len()).sum()
    }

    pub fn dropped(&self) -> usize {
        self.total() - self.wanted()
    }

    /// Takes one out of the count, or puts it back.
    pub fn toggle(&mut self, page: u32, index: usize) {
        if let Some(one) = self.found.get_mut(&page).and_then(|v| v.get_mut(index)) {
            one.dropped = !one.dropped;
        }
    }

    /// The nearest sighting to a point on a sheet, for clicking one off.
    pub fn nearest(&self, page: u32, at: [f64; 2]) -> Option<usize> {
        let on = self.found.get(&page)?;
        on.iter()
            .enumerate()
            .filter(|(_, s)| {
                at[0] >= s.area[0] && at[0] <= s.area[2] && at[1] >= s.area[1] && at[1] <= s.area[3]
            })
            .map(|(i, _)| i)
            .next()
    }

    pub fn clear(&mut self) {
        self.job += 1;
        self.found.clear();
        self.running = false;
        self.symbol = None;
        self.drawing = None;
    }

    /// What to say about it.
    pub fn says(&self) -> String {
        if self.symbol.is_none() {
            return "Drag a box round a symbol to find the others.".into();
        }
        if self.running {
            return format!(
                "Looking… {} of {} sheets, {} so far",
                self.looked_at,
                self.sheets,
                self.total()
            );
        }
        let total = self.total();
        if total == 0 {
            return "Nothing else on the drawing looks like that. Try a looser match.".into();
        }
        let mut said = format!(
            "{total} found on {} sheet{}",
            self.found.len(),
            if self.found.len() == 1 { "" } else { "s" }
        );
        if self.dropped() > 0 {
            said.push_str(&format!(
                " · {} taken out, {} would be counted",
                self.dropped(),
                self.wanted()
            ));
        }
        said.push_str(". Check them, then count.");
        said
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64, y: f64) -> [f64; 4] {
        [x, y, x + 10.0, y + 10.0]
    }

    #[test]
    fn nothing_is_counted_until_somebody_says_so() {
        let mut looking = Looking::new();
        looking.symbol = Some(at(0.0, 0.0));
        let job = looking.begin(0, 2);
        looking.accept(job, 0, vec![at(20.0, 20.0), at(40.0, 40.0)], false);
        looking.accept(job, 1, vec![at(60.0, 60.0)], true);

        assert_eq!(looking.total(), 3);
        assert_eq!(looking.wanted(), 3);
        // The words never claim a count, only what would be counted.
        let said = looking.says();
        assert!(said.contains("3 found on 2 sheets"), "{said}");
        assert!(said.contains("Check them"), "{said}");
    }

    #[test]
    fn a_wrong_one_can_be_taken_out_and_the_number_follows() {
        let mut looking = Looking::new();
        looking.symbol = Some(at(0.0, 0.0));
        let job = looking.begin(0, 1);
        looking.accept(job, 0, vec![at(20.0, 20.0), at(40.0, 40.0)], true);
        looking.toggle(0, 1);
        assert_eq!(looking.wanted(), 1);
        assert_eq!(looking.dropped(), 1);
        assert!(looking.says().contains("1 taken out, 1 would be counted"));
        // And back again.
        looking.toggle(0, 1);
        assert_eq!(looking.wanted(), 2);
    }

    #[test]
    fn clicking_on_a_sighting_finds_which_one() {
        let mut looking = Looking::new();
        looking.symbol = Some(at(0.0, 0.0));
        let job = looking.begin(0, 1);
        looking.accept(job, 0, vec![at(20.0, 20.0), at(50.0, 50.0)], true);
        assert_eq!(looking.nearest(0, [25.0, 25.0]), Some(0));
        assert_eq!(looking.nearest(0, [55.0, 55.0]), Some(1));
        assert_eq!(looking.nearest(0, [100.0, 100.0]), None, "nothing there");
        assert_eq!(looking.nearest(3, [25.0, 25.0]), None, "nor on that sheet");
    }

    #[test]
    fn an_answer_from_a_search_already_moved_on_from_is_dropped() {
        let mut looking = Looking::new();
        looking.symbol = Some(at(0.0, 0.0));
        let stale = looking.begin(0, 1);
        looking.begin(0, 1);
        looking.accept(stale, 0, vec![at(20.0, 20.0)], true);
        assert_eq!(looking.total(), 0);
    }

    #[test]
    fn finding_nothing_suggests_what_to_do_rather_than_saying_nothing() {
        let mut looking = Looking::new();
        looking.symbol = Some(at(0.0, 0.0));
        let job = looking.begin(0, 1);
        looking.accept(job, 0, Vec::new(), true);
        assert!(looking.says().contains("looser match"), "{}", looking.says());
    }

    #[test]
    fn with_no_symbol_picked_it_says_what_to_do() {
        let looking = Looking::new();
        assert!(looking.says().contains("Drag a box"));
        assert!(!looking.running, "and nothing is running");
    }
}
