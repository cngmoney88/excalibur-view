//! Finding words across a whole drawing set.
//!
//! What a person searching a drawing set is usually doing is not reading — it
//! is locating. "Which sheet has the shear wall schedule on it", "where does
//! W12x26 appear", "is there an HSS5x5 anywhere in this package". So the list
//! is grouped by sheet and each answer is shown with the words around it, and
//! clicking one goes there and points at it.

use std::collections::BTreeMap;

use crate::render::Hit;

/// Everything about the search now running.
#[derive(Default)]
pub struct Search {
    pub needle: String,
    pub whole_words: bool,
    pub match_case: bool,
    /// Rises with each search, so answers from one the user has moved on from
    /// are recognised and thrown away.
    pub job: u64,
    /// Which sheets have been looked at, and what was on them.
    pub hits: BTreeMap<u32, Vec<Hit>>,
    pub looked_at: u32,
    pub sheets: u32,
    pub running: bool,
    /// The one being pointed at, as (sheet, which on that sheet).
    pub showing: Option<(u32, usize)>,
}

impl Search {
    pub fn total(&self) -> usize {
        self.hits.values().map(|h| h.len()).sum()
    }

    pub fn sheets_with_something(&self) -> usize {
        self.hits.len()
    }

    /// Takes in what one sheet turned up. Answers from an older search are
    /// dropped: somebody who has typed another letter is not waiting on them.
    pub fn accept(&mut self, job: u64, page: u32, hits: Vec<Hit>, done: bool) {
        if job != self.job {
            return;
        }
        self.looked_at = self.looked_at.max(page + 1);
        if !hits.is_empty() {
            self.hits.insert(page, hits);
        }
        if done {
            self.running = false;
        }
    }

    pub fn begin(&mut self, sheets: u32) {
        self.job += 1;
        self.hits.clear();
        self.looked_at = 0;
        self.sheets = sheets;
        self.showing = None;
        self.running = !self.needle.trim().is_empty();
    }

    pub fn clear(&mut self) {
        self.job += 1;
        self.hits.clear();
        self.looked_at = 0;
        self.running = false;
        self.showing = None;
    }

    /// Every answer in order, so Next and Previous can walk them.
    pub fn in_order(&self) -> Vec<(u32, usize)> {
        self.hits
            .iter()
            .flat_map(|(page, hits)| (0..hits.len()).map(move |i| (*page, i)))
            .collect()
    }

    /// The one after the one being shown, wrapping round.
    pub fn next(&self) -> Option<(u32, usize)> {
        let all = self.in_order();
        if all.is_empty() {
            return None;
        }
        Some(match self.showing {
            None => all[0],
            Some(now) => {
                let at = all.iter().position(|p| *p == now).unwrap_or(0);
                all[(at + 1) % all.len()]
            }
        })
    }

    pub fn previous(&self) -> Option<(u32, usize)> {
        let all = self.in_order();
        if all.is_empty() {
            return None;
        }
        Some(match self.showing {
            None => all[all.len() - 1],
            Some(now) => {
                let at = all.iter().position(|p| *p == now).unwrap_or(0);
                all[(at + all.len() - 1) % all.len()]
            }
        })
    }

    pub fn at(&self, where_: (u32, usize)) -> Option<&Hit> {
        self.hits.get(&where_.0).and_then(|h| h.get(where_.1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_hit(page: u32, word: &str) -> Hit {
        Hit {
            page,
            context: word.to_string(),
            at: 0..word.len(),
            area: [0.0, 0.0, 10.0, 10.0],
        }
    }

    #[test]
    fn answers_from_a_search_already_moved_on_from_are_thrown_away() {
        let mut search = Search::default();
        search.needle = "W12x26".into();
        search.begin(12);
        let stale = search.job;
        // The user types another letter.
        search.needle = "W12x260".into();
        search.begin(12);

        search.accept(stale, 3, vec![a_hit(3, "W12x26")], false);
        assert_eq!(search.total(), 0, "the old search's answers do not appear");

        search.accept(search.job, 3, vec![a_hit(3, "W12x260")], false);
        assert_eq!(search.total(), 1);
    }

    #[test]
    fn walking_the_answers_wraps_round_in_both_directions() {
        let mut search = Search::default();
        search.needle = "x".into();
        search.begin(12);
        let job = search.job;
        search.accept(job, 2, vec![a_hit(2, "a"), a_hit(2, "b")], false);
        search.accept(job, 7, vec![a_hit(7, "c")], true);

        assert_eq!(search.total(), 3);
        assert_eq!(search.sheets_with_something(), 2);

        search.showing = search.next();
        assert_eq!(search.showing, Some((2, 0)));
        search.showing = search.next();
        assert_eq!(search.showing, Some((2, 1)));
        search.showing = search.next();
        assert_eq!(search.showing, Some((7, 0)));
        search.showing = search.next();
        assert_eq!(search.showing, Some((2, 0)), "and round again");

        search.showing = search.previous();
        assert_eq!(search.showing, Some((7, 0)), "backwards wraps too");
    }

    #[test]
    fn a_search_that_finds_nothing_has_nothing_to_walk() {
        let mut search = Search::default();
        search.needle = "nothing here".into();
        search.begin(12);
        search.accept(search.job, 0, Vec::new(), true);
        assert!(!search.running);
        assert_eq!(search.next(), None);
        assert_eq!(search.previous(), None);
    }

    #[test]
    fn an_empty_search_never_starts() {
        let mut search = Search::default();
        search.needle = "   ".into();
        search.begin(12);
        assert!(!search.running, "there is nothing to look for");
    }

    #[test]
    fn progress_counts_sheets_looked_at_rather_than_sheets_with_answers() {
        let mut search = Search::default();
        search.needle = "beam".into();
        search.begin(12);
        let job = search.job;
        for page in 0..5 {
            search.accept(job, page, Vec::new(), false);
        }
        assert_eq!(search.looked_at, 5);
        assert_eq!(search.total(), 0, "looked at five, found none");
        assert!(search.running, "and it is still going");
    }
}
