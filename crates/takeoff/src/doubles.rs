//! Finding the same thing counted twice.
//!
//! The quiet way a takeoff goes wrong is not a mistyped number — somebody
//! catches those. It is the same beam picked up on Tuesday and again on
//! Thursday, or a length drawn over one already there because the first was
//! the wrong colour and nobody deleted it. The totals go up, nothing looks
//! broken, and the bid is high by a member nobody can find.
//!
//! Revu has no answer to this. It will happily add the same line twice. This
//! looks for pairs of markups on one sheet that occupy the same place and
//! measure the same amount, and **says so**.
//!
//! What it deliberately does not do:
//!
//! * It does not delete anything.
//! * It does not adjust a total, or quietly drop the second of a pair.
//! * It does not decide. A doorway measured twice on purpose — once for the
//!   frame, once for the opening — is a perfectly ordinary takeoff, and a
//!   program that silently removed one of them would be wrong in a way that
//!   is very hard to find.
//!
//! It reports. The estimator decides. That is the same rule as everywhere
//! else in here: nothing is estimated, and nothing is quietly adjusted.

use annot::Kind;

use crate::row::Row;

/// How close two markups have to be to count as the same one.
///
/// In points of paper, because that is the only unit that means the same
/// thing on an unscaled sheet as on a scaled one. The defaults are a
/// deliberate compromise: tight enough that two genuinely different members
/// on a crowded framing plan are not paired, loose enough that two markups
/// drawn by hand over the same beam on different days still are.
#[derive(Clone, Copy, Debug)]
pub struct HowClose {
    /// How far two ends may be apart and still be the same end.
    pub ends: f64,
    /// How much two areas must overlap, as a fraction of the larger.
    pub overlap: f64,
    /// How far apart two counts may be and still be the same count.
    pub counts: f64,
    /// How far two lengths may differ, as a fraction of the longer.
    pub length: f64,
}

impl Default for HowClose {
    fn default() -> HowClose {
        HowClose {
            ends: 6.0,
            overlap: 0.80,
            counts: 8.0,
            length: 0.02,
        }
    }
}

/// How sure this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Sure {
    /// Same place, same size, same subject. Almost always a mistake.
    Certain,
    /// Same place and size, but a different subject — which is often
    /// deliberate, and is the reason none of this deletes anything.
    WorthALook,
}

impl Sure {
    pub fn name(self) -> &'static str {
        match self {
            Sure::Certain => "the same",
            Sure::WorthALook => "worth a look",
        }
    }
}

/// One pair worth showing somebody.
#[derive(Clone, Debug)]
pub struct Pair {
    /// Where the two markups are in the list handed in.
    pub first: usize,
    pub second: usize,
    pub page: usize,
    pub sure: Sure,
    /// What they both measure, for the line that gets shown.
    pub subject: String,
    /// Said plainly, because a warning nobody understands gets ignored.
    pub why: String,
    /// What the totals carry twice, if this really is a double. Never
    /// subtracted from anything — only shown, so somebody can see what it
    /// would be worth to fix.
    pub carried_twice: Option<f64>,
    pub unit: String,
}

/// Everything found, and what it is worth.
#[derive(Clone, Debug, Default)]
pub struct Doubles {
    pub pairs: Vec<Pair>,
    /// Rows that could not be checked because their sheet has no scale.
    /// Reported, as everywhere else, rather than passed over in silence.
    pub unchecked: usize,
    pub unchecked_pages: Vec<usize>,
}

impl Doubles {
    pub fn certain(&self) -> usize {
        self.pairs.iter().filter(|p| p.sure == Sure::Certain).count()
    }

    pub fn found_anything(&self) -> bool {
        !self.pairs.is_empty()
    }

    /// What it is worth, said in a sentence. Never applied to a total.
    pub fn what_it_is_worth(&self, weights: crate::row::WeightColumns, rows: &[Row]) -> String {
        let pounds: f64 = self
            .pairs
            .iter()
            .filter(|p| p.sure == Sure::Certain)
            .filter_map(|p| rows.get(p.second))
            .filter_map(|row| row.pounds(weights))
            .sum();
        if pounds <= 0.0 {
            return String::new();
        }
        format!(
            "If every one of these really is a double, the totals are carrying \
             about {pounds:.0} lb twice. Nothing has been subtracted — that is \
             yours to decide, markup by markup."
        )
    }
}

/// Are these two ends the same end?
fn same_end(a: [f64; 2], b: [f64; 2], within: f64) -> bool {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt() <= within
}

/// How much two boxes share, as a fraction of the larger one.
pub fn overlap_of(a: [f64; 4], b: [f64; 4]) -> f64 {
    let left = a[0].max(b[0]);
    let bottom = a[1].max(b[1]);
    let right = a[2].min(b[2]);
    let top = a[3].min(b[3]);
    if right <= left || top <= bottom {
        return 0.0;
    }
    let shared = (right - left) * (top - bottom);
    let one = (a[2] - a[0]) * (a[3] - a[1]);
    let two = (b[2] - b[0]) * (b[3] - b[1]);
    let larger = one.max(two);
    if larger <= 0.0 {
        return 0.0;
    }
    shared / larger
}

fn middle(box_: [f64; 4]) -> [f64; 2] {
    [(box_[0] + box_[2]) / 2.0, (box_[1] + box_[3]) / 2.0]
}

/// Two numbers that are near enough the same.
fn near_enough(a: Option<f64>, b: Option<f64>, within: f64) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            let larger = a.abs().max(b.abs());
            if larger <= 0.0 {
                return true;
            }
            (a - b).abs() / larger <= within
        }
        // No measurement on either side is not a match — it is two markups
        // this cannot compare, and guessing would be the whole sin.
        _ => false,
    }
}

/// Looks through a takeoff for the same thing counted twice.
pub fn find(rows: &[Row], how: HowClose) -> Doubles {
    let mut out = Doubles::default();

    for (a, first) in rows.iter().enumerate() {
        if !first.kind.measures() {
            continue;
        }
        if !first.scaled && first.kind != Kind::Count {
            out.unchecked += 1;
            if !out.unchecked_pages.contains(&first.page) {
                out.unchecked_pages.push(first.page);
            }
            continue;
        }
        for (b, second) in rows.iter().enumerate().skip(a + 1) {
            if second.page != first.page || first.kind != second.kind {
                continue;
            }
            if !second.scaled && second.kind != Kind::Count {
                continue;
            }
            let Some(why) = looks_the_same(first, second, how) else {
                continue;
            };
            let same_subject = first.subject.trim().eq_ignore_ascii_case(second.subject.trim());
            out.pairs.push(Pair {
                first: a,
                second: b,
                page: first.page,
                sure: if same_subject { Sure::Certain } else { Sure::WorthALook },
                subject: if first.subject.trim().is_empty() {
                    "(no subject)".to_string()
                } else {
                    first.subject.clone()
                },
                why: if same_subject {
                    why
                } else {
                    format!(
                        "{why} They carry different subjects — \"{}\" and \"{}\" — so this \
                         may well be on purpose.",
                        first.subject.trim(),
                        second.subject.trim()
                    )
                },
                carried_twice: match first.kind {
                    Kind::Area | Kind::Volume => second.area,
                    Kind::Count => Some(second.count),
                    _ => second.length,
                },
                unit: second.unit.clone(),
            });
        }
    }

    // The near-certain ones first: those are what somebody wants to see.
    out.pairs.sort_by(|a, b| a.sure.cmp(&b.sure).then(a.first.cmp(&b.first)));
    out.unchecked_pages.sort_unstable();
    out
}

/// Whether two markups of the same kind sit in the same place measuring the
/// same amount, and why.
fn looks_the_same(first: &Row, second: &Row, how: HowClose) -> Option<String> {
    match first.kind {
        Kind::Count => {
            // Two count marks on top of each other.
            if !same_end(middle(first.area_box), middle(second.area_box), how.counts) {
                return None;
            }
            Some("Two count marks in the same spot.".to_string())
        }
        Kind::Area | Kind::Volume => {
            let shared = overlap_of(first.area_box, second.area_box);
            if shared < how.overlap {
                return None;
            }
            if !near_enough(first.area, second.area, how.length) {
                return None;
            }
            Some(format!(
                "Two areas lying on top of each other — {:.0}% of the same paper — \
                 measuring the same amount.",
                shared * 100.0
            ))
        }
        _ => {
            // A length: the same member if both ends land in the same place,
            // either way round, and the two measure the same.
            if !near_enough(first.length, second.length, how.length) {
                return None;
            }
            let a = ends_of(first);
            let b = ends_of(second);
            let forwards =
                same_end(a.0, b.0, how.ends) && same_end(a.1, b.1, how.ends);
            let backwards =
                same_end(a.0, b.1, how.ends) && same_end(a.1, b.0, how.ends);
            if !(forwards || backwards) {
                // Not the same ends, but possibly the same member drawn twice
                // slightly offset: fall back to how much paper they share.
                let shared = overlap_of(first.area_box, second.area_box);
                if shared < how.overlap {
                    return None;
                }
                return Some(format!(
                    "Two lengths over the same {:.0}% of the sheet, measuring the \
                     same amount.",
                    shared * 100.0
                ));
            }
            Some(if backwards {
                "Two lengths between the same two points, drawn in opposite \
                 directions."
                    .to_string()
            } else {
                "Two lengths between the same two points.".to_string()
            })
        }
    }
}

/// The two ends of a row's bounding box along its longer axis — as near as a
/// measured row gets to "where the member starts and stops" without going
/// back to the annotation's own points.
fn ends_of(row: &Row) -> ([f64; 2], [f64; 2]) {
    let [left, bottom, right, top] = row.area_box;
    if (right - left).abs() >= (top - bottom).abs() {
        ([left, bottom], [right, top])
    } else {
        ([left, top], [right, bottom])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::WeightColumns;

    fn beam(subject: &str, page: usize, at: [f64; 4], feet: f64) -> Row {
        let mut row = Row::blank();
        row.page = page;
        row.subject = subject.into();
        row.kind = Kind::Length;
        row.scaled = true;
        row.length = Some(feet);
        row.area_box = at;
        row.unit = "'".into();
        row.columns[WeightColumns::default().per_length] = "26".into();
        row
    }

    fn slab(subject: &str, at: [f64; 4], square: f64) -> Row {
        let mut row = Row::blank();
        row.subject = subject.into();
        row.kind = Kind::Area;
        row.scaled = true;
        row.area = Some(square);
        row.area_box = at;
        row
    }

    fn count(at: [f64; 4]) -> Row {
        let mut row = Row::blank();
        row.subject = "Shear Conn".into();
        row.kind = Kind::Count;
        row.count = 1.0;
        row.area_box = at;
        row
    }

    #[test]
    fn the_same_beam_measured_twice_is_found() {
        let rows = vec![
            beam("W12x26", 0, [100.0, 100.0, 400.0, 102.0], 25.0),
            beam("W12x26", 0, [101.0, 100.5, 401.0, 102.5], 25.0),
        ];
        let found = find(&rows, HowClose::default());
        assert_eq!(found.pairs.len(), 1);
        assert_eq!(found.pairs[0].sure, Sure::Certain);
        assert_eq!(found.certain(), 1);
        assert_eq!(found.pairs[0].carried_twice, Some(25.0));
    }

    #[test]
    fn two_different_beams_are_left_alone() {
        let rows = vec![
            beam("W12x26", 0, [100.0, 100.0, 400.0, 102.0], 25.0),
            beam("W12x26", 0, [100.0, 300.0, 400.0, 302.0], 25.0),
        ];
        assert!(!find(&rows, HowClose::default()).found_anything());
    }

    #[test]
    fn the_same_place_on_two_different_sheets_is_not_a_double() {
        // The same beam appears on the plan and on the section. That is how
        // drawings work, and flagging it would make this useless.
        let rows = vec![
            beam("W12x26", 0, [100.0, 100.0, 400.0, 102.0], 25.0),
            beam("W12x26", 1, [100.0, 100.0, 400.0, 102.0], 25.0),
        ];
        assert!(!find(&rows, HowClose::default()).found_anything());
    }

    #[test]
    fn a_line_drawn_backwards_over_another_is_still_a_double() {
        let mut back = beam("W12x26", 0, [100.0, 102.0, 400.0, 100.0], 25.0);
        back.area_box = [100.0, 100.0, 400.0, 102.0];
        let rows = vec![beam("W12x26", 0, [100.0, 100.0, 400.0, 102.0], 25.0), back];
        assert_eq!(find(&rows, HowClose::default()).pairs.len(), 1);
    }

    #[test]
    fn the_same_place_measuring_a_different_amount_is_not_a_double() {
        // Two lines from the same corner going different distances are two
        // different members, however close their boxes start.
        let rows = vec![
            beam("W12x26", 0, [100.0, 100.0, 400.0, 102.0], 25.0),
            beam("W12x26", 0, [100.0, 100.0, 700.0, 102.0], 50.0),
        ];
        assert!(!find(&rows, HowClose::default()).found_anything());
    }

    #[test]
    fn a_different_subject_in_the_same_place_is_only_worth_a_look() {
        // A doorway measured once for the frame and once for the opening is an
        // ordinary takeoff. Deleting one would be wrong in a way nobody finds.
        let rows = vec![
            beam("W12x26", 0, [100.0, 100.0, 400.0, 102.0], 25.0),
            beam("HANDRAIL", 0, [100.0, 100.0, 400.0, 102.0], 25.0),
        ];
        let found = find(&rows, HowClose::default());
        assert_eq!(found.pairs.len(), 1);
        assert_eq!(found.pairs[0].sure, Sure::WorthALook);
        assert_eq!(found.certain(), 0);
        assert!(found.pairs[0].why.contains("on purpose"));
    }

    #[test]
    fn a_length_and_an_area_in_the_same_place_are_not_a_pair() {
        let rows = vec![
            beam("SLAB EDGE", 0, [0.0, 0.0, 300.0, 300.0], 25.0),
            slab("SLAB", [0.0, 0.0, 300.0, 300.0], 900.0),
        ];
        assert!(!find(&rows, HowClose::default()).found_anything());
    }

    #[test]
    fn two_slabs_over_each_other_are_found() {
        let rows = vec![
            slab("SLAB", [0.0, 0.0, 300.0, 300.0], 900.0),
            slab("SLAB", [2.0, 2.0, 301.0, 301.0], 900.0),
        ];
        let found = find(&rows, HowClose::default());
        assert_eq!(found.pairs.len(), 1);
        assert!(found.pairs[0].why.contains("on top of each other"));
        assert_eq!(found.pairs[0].carried_twice, Some(900.0));
    }

    #[test]
    fn two_count_marks_in_one_spot_are_found() {
        let rows = vec![count([50.0, 50.0, 60.0, 60.0]), count([52.0, 52.0, 62.0, 62.0])];
        assert_eq!(find(&rows, HowClose::default()).pairs.len(), 1);
    }

    #[test]
    fn counts_spread_across_a_sheet_are_left_alone() {
        let rows = vec![
            count([50.0, 50.0, 60.0, 60.0]),
            count([250.0, 50.0, 260.0, 60.0]),
            count([450.0, 50.0, 460.0, 60.0]),
        ];
        assert!(!find(&rows, HowClose::default()).found_anything());
    }

    #[test]
    fn a_sheet_with_no_scale_is_reported_rather_than_checked_wrong() {
        let mut no_scale = beam("W12x26", 3, [100.0, 100.0, 400.0, 102.0], 25.0);
        no_scale.scaled = false;
        no_scale.length = None;
        let found = find(&[no_scale], HowClose::default());
        assert_eq!(found.unchecked, 1);
        assert_eq!(found.unchecked_pages, vec![3]);
    }

    #[test]
    fn nothing_is_ever_subtracted_only_reported() {
        // The whole contract of this module. It says what a double would be
        // worth and leaves the totals exactly as they were.
        let rows = vec![
            beam("W12x26", 0, [100.0, 100.0, 400.0, 102.0], 25.0),
            beam("W12x26", 0, [101.0, 100.5, 401.0, 102.5], 25.0),
        ];
        let found = find(&rows, HowClose::default());
        let said = found.what_it_is_worth(WeightColumns::default(), &rows);
        assert!(said.contains("650 lb"), "{said}");
        assert!(said.contains("Nothing has been subtracted"), "{said}");
        // And the takeoff itself is untouched.
        let summary = crate::summarise(&rows, WeightColumns::default());
        assert!((summary.pounds - 1300.0).abs() < 1e-9);
    }

    #[test]
    fn three_copies_of_one_beam_report_all_three_pairings() {
        // So somebody can see it is three, not two.
        let rows = vec![
            beam("W12x26", 0, [100.0, 100.0, 400.0, 102.0], 25.0),
            beam("W12x26", 0, [100.5, 100.0, 400.5, 102.0], 25.0),
            beam("W12x26", 0, [101.0, 100.0, 401.0, 102.0], 25.0),
        ];
        assert_eq!(find(&rows, HowClose::default()).pairs.len(), 3);
    }

    #[test]
    fn the_near_certain_ones_come_first() {
        let rows = vec![
            beam("HANDRAIL", 0, [100.0, 100.0, 400.0, 102.0], 25.0),
            beam("W12x26", 0, [100.0, 100.0, 400.0, 102.0], 25.0),
            beam("W12x26", 0, [100.5, 100.0, 400.5, 102.0], 25.0),
        ];
        let found = find(&rows, HowClose::default());
        assert_eq!(found.pairs[0].sure, Sure::Certain);
    }
}
