//! Where a markup is, in the language the shop actually uses.
//!
//! Nobody on a shop floor says "at 4180, 2260 points". They say "at B-4", or
//! "the beam on line 3 between C and D". Every structural drawing carries that
//! language along its edges as grid bubbles, and every reader in the trade —
//! Revu included — throws it away the moment the sheet is on screen. A takeoff
//! comes out as a list of numbers with no idea where any of them are.
//!
//! This reads the grid off the sheet and gives every markup a location.
//!
//! It was tried against a real stamped structural set of twelve sheets while
//! it was being written, which is the only reason the margin rule below
//! exists: the first version found a grid on nine of them, and five of those
//! were schedules. `cargo run -p hyperview --example gridcheck -- <set.pdf>`
//! is that check, kept, so the next change to these rules can be tried the
//! same way rather than argued about.
//!
//! # What this is and is not allowed to do
//!
//! A grid location is a **label**, not a quantity. It changes no number, no
//! weight and no total, which is the only reason it is allowed to be worked
//! out from the sheet at all rather than clicked.
//!
//! What it must still never do is invent one. So the rule for calling
//! something a grid is deliberately hard to satisfy: three or more labels in a
//! straight line, all of one family — all letters or all numbers — and running
//! in order along that line. Scattered text on a busy sheet does not do that,
//! and a sheet with no grid produces no grid rather than a wrong one. A point
//! outside the grid is reported as outside it; it never gets the nearest
//! label as a consolation.

/// A bit of text on the sheet that might be a grid bubble.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub label: String,
    /// Middle of the text, in sheet points.
    pub at: [f64; 2],
}

/// One grid line: where it is along its own axis, and what it is called.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub label: String,
    pub at: f64,
}

/// A sheet's grid.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grid {
    /// The vertical lines — the ones labelled along the top or bottom. Sorted
    /// left to right by where they cross the sheet.
    pub vertical: Vec<Line>,
    /// The horizontal lines, labelled down one side. Sorted top to bottom.
    pub horizontal: Vec<Line>,
}

impl Grid {
    pub fn is_empty(&self) -> bool {
        self.vertical.is_empty() && self.horizontal.is_empty()
    }

    /// How it reads: "A–G / 1–8".
    pub fn extent(&self) -> String {
        let span = |lines: &[Line]| match (lines.first(), lines.last()) {
            (Some(a), Some(b)) if lines.len() > 1 => format!("{}–{}", a.label, b.label),
            (Some(a), _) => a.label.clone(),
            _ => String::new(),
        };
        match (span(&self.vertical), span(&self.horizontal)) {
            (a, b) if a.is_empty() => b,
            (a, b) if b.is_empty() => a,
            (a, b) => format!("{a} / {b}"),
        }
    }
}

/// Where something is against one axis of the grid.
#[derive(Clone, Debug, PartialEq)]
pub enum Along {
    /// On a line, within the tolerance somebody set.
    On(String),
    /// Between two of them.
    Between(String, String),
    /// Past the end of the grid on that axis.
    Beyond,
}

impl Along {
    pub fn say(&self) -> String {
        match self {
            Along::On(label) => label.clone(),
            Along::Between(a, b) => format!("{a}/{b}"),
            Along::Beyond => "—".to_string(),
        }
    }
}

/// Where something is on the sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct Where {
    pub vertical: Option<Along>,
    pub horizontal: Option<Along>,
}

impl Where {
    /// How it reads on a report: "B-4", "B/C-4", or nothing at all.
    ///
    /// Nothing at all is a real answer. A markup off the end of the grid, or
    /// on a sheet with no grid, has no grid location, and "—" is the honest
    /// way to write that.
    pub fn say(&self) -> String {
        match (&self.vertical, &self.horizontal) {
            (Some(Along::Beyond), Some(Along::Beyond)) | (None, None) => String::new(),
            (Some(v), Some(h)) => format!("{}-{}", v.say(), h.say()),
            (Some(v), None) => v.say(),
            (None, Some(h)) => h.say(),
        }
    }

    /// Whether this landed anywhere on the grid at all.
    pub fn is_somewhere(&self) -> bool {
        !self.say().is_empty() && self.say() != "—"
    }
}

/// Where a label sits in its own family, so an ordering can be checked.
///
/// `A` is 1 and `Z` is 26, `AA` is 27 the way a spreadsheet counts. A number
/// is itself. Anything else — a word, a mixture, an empty string — is not a
/// grid label and says so.
pub fn rank(label: &str) -> Option<(Family, i64)> {
    let label = label.trim();
    if label.is_empty() {
        return None;
    }
    // Three digits is a grid that runs to 999, which is more than any sheet
    // has. Two letters is A to ZZ, likewise. The tighter limit on letters is
    // what keeps the shorthand a drawing is covered in — TYP, EQ, UNO, SIM —
    // out of the running before the ordering check ever has to catch it.
    if label.chars().all(|c| c.is_ascii_digit()) {
        if label.len() > 3 {
            return None;
        }
        return label.parse::<i64>().ok().map(|n| (Family::Numbered, n));
    }
    if label.len() > 2 {
        return None;
    }
    if label.chars().all(|c| c.is_ascii_alphabetic()) {
        // I, O and Q are left out of grids on purpose, because they read as 1
        // and 0. Their absence is why a letter grid is not simply A..Z, and
        // why the ordering rather than the spacing is what gets checked.
        let mut n = 0i64;
        for c in label.chars() {
            n = n * 26 + (c.to_ascii_uppercase() as i64 - 'A' as i64 + 1);
        }
        return Some((Family::Lettered, n));
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Lettered,
    Numbered,
}

/// How close two bubbles have to be to count as in line, and how close a
/// markup has to be to a line to be called on it. Sheet points.
#[derive(Clone, Copy, Debug)]
pub struct HowClose {
    /// Lining the bubbles up into a row or a column.
    pub in_line: f64,
    /// Calling a markup "on" a line rather than between two.
    pub on_a_line: f64,
    /// The fewest bubbles that may be called a grid.
    pub fewest: usize,
    /// How far in from the edge of the sheet a run of bubbles may sit, as a
    /// fraction of the sheet.
    ///
    /// This is what keeps schedules out. A column of numbers down the middle
    /// of a sheet is a beam schedule, a door schedule or a bolt list; a grid
    /// is drawn round the outside of the plan where the fitter can see it.
    /// Tried against a real stamped structural set, this one rule is the
    /// difference between finding nine grids of which five are furniture, and
    /// finding the ones that are actually there.
    pub margin: f64,
    /// How far apart the first and last label may be, as a multiple of how
    /// many there are.
    ///
    /// A grid runs 1 to 6, or A to G. It does not run 6 to 456 — that is a
    /// column of part numbers that happens to be in a straight line.
    pub spread: f64,
}

impl Default for HowClose {
    fn default() -> HowClose {
        HowClose {
            in_line: 12.0,
            on_a_line: 18.0,
            fewest: 3,
            margin: 0.16,
            spread: 2.0,
        }
    }
}

/// Reads a grid off the text found on a sheet.
///
/// `sheet` is the sheet's width and height in points, which is what lets the
/// margin rule work — and the margin rule is most of what separates a grid
/// from a schedule.
///
/// Returns nothing when the sheet does not plainly have one. That is the
/// common case on a detail sheet, and no grid is the right answer for it.
pub fn read(marks: &[Mark], sheet: [f64; 2], how: HowClose) -> Grid {
    let mut grid = Grid::default();

    // Bubbles in a row across the sheet label the vertical lines; bubbles in a
    // column down the side label the horizontal ones.
    if let Some(row) = best_run(marks, sheet, how, Axis::Row) {
        grid.vertical = row;
    }
    if let Some(column) = best_run(marks, sheet, how, Axis::Column) {
        grid.horizontal = column;
    }

    // One bubble cannot be two lines. If the same label turned up on both
    // axes — a corner bubble read twice — the longer run keeps it.
    if grid.vertical.len() < grid.horizontal.len() {
        let taken: Vec<String> = grid.horizontal.iter().map(|l| l.label.clone()).collect();
        grid.vertical.retain(|l| !taken.contains(&l.label));
    } else {
        let taken: Vec<String> = grid.vertical.iter().map(|l| l.label.clone()).collect();
        grid.horizontal.retain(|l| !taken.contains(&l.label));
    }
    if grid.vertical.len() < how.fewest {
        grid.vertical.clear();
    }
    if grid.horizontal.len() < how.fewest {
        grid.horizontal.clear();
    }
    grid
}

#[derive(Clone, Copy, PartialEq)]
enum Axis {
    /// Lined up horizontally: same y, spread across x.
    Row,
    /// Lined up vertically: same x, spread down y.
    Column,
}

/// The longest run of labels that lie in a straight line, in order, all of one
/// family.
fn best_run(marks: &[Mark], sheet: [f64; 2], how: HowClose, axis: Axis) -> Option<Vec<Line>> {
    // The coordinate that must be nearly the same for them to be in line, and
    // the one they must be spread along.
    let fixed = |m: &Mark| if axis == Axis::Row { m.at[1] } else { m.at[0] };
    let along = |m: &Mark| if axis == Axis::Row { m.at[0] } else { m.at[1] };
    // How far across the sheet the shared coordinate is allowed to be: near
    // one edge or the other, never through the middle.
    let across = if axis == Axis::Row { sheet[1] } else { sheet[0] };
    let near_an_edge = |value: f64| {
        if across <= 0.0 {
            return true;
        }
        let fraction = value / across;
        fraction <= how.margin || fraction >= 1.0 - how.margin
    };

    let usable: Vec<&Mark> = marks
        .iter()
        .filter(|m| rank(&m.label).is_some())
        .filter(|m| near_an_edge(fixed(m)))
        .collect();
    let mut best: Option<Vec<Line>> = None;

    for anchor in &usable {
        let mut group: Vec<&&Mark> = usable
            .iter()
            .filter(|m| (fixed(m) - fixed(anchor)).abs() <= how.in_line)
            .collect();
        if group.len() < how.fewest {
            continue;
        }
        group.sort_by(|a, b| {
            along(a)
                .partial_cmp(&along(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for family in [Family::Lettered, Family::Numbered] {
            let of_family: Vec<&&&Mark> = group
                .iter()
                .filter(|m| rank(&m.label).map(|(f, _)| f) == Some(family))
                .collect();
            if of_family.len() < how.fewest {
                continue;
            }
            // Duplicates mean this is not a grid run — a grid does not have two
            // line Bs — so the same label twice disqualifies it rather than
            // being quietly thinned out.
            let mut labels: Vec<String> =
                of_family.iter().map(|m| m.label.to_uppercase()).collect();
            let before = labels.len();
            labels.sort();
            labels.dedup();
            if labels.len() != before {
                continue;
            }
            let ranks: Vec<i64> = of_family
                .iter()
                .filter_map(|m| rank(&m.label).map(|(_, n)| n))
                .collect();
            let rising = ranks.windows(2).all(|w| w[0] < w[1]);
            let falling = ranks.windows(2).all(|w| w[0] > w[1]);
            if !rising && !falling {
                continue;
            }
            // A grid runs 1 to 6, or A to G — consecutive, or close to it
            // where a line has been left out. A run from 6 to 456 is a column
            // of part numbers that happens to be in a straight line.
            let (low, high) = (
                ranks.iter().copied().min().unwrap_or(0),
                ranks.iter().copied().max().unwrap_or(0),
            );
            if (high - low) as f64 >= of_family.len() as f64 * how.spread {
                continue;
            }
            let mut lines: Vec<Line> = of_family
                .iter()
                .map(|m| Line {
                    label: m.label.trim().to_uppercase(),
                    at: along(m),
                })
                .collect();
            lines.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap_or(std::cmp::Ordering::Equal));
            if best.as_ref().is_none_or(|b| b.len() < lines.len()) {
                best = Some(lines);
            }
        }
    }
    best
}

impl Grid {
    /// Where a point is on this grid.
    pub fn where_is(&self, at: [f64; 2], how: HowClose) -> Where {
        Where {
            vertical: place(&self.vertical, at[0], how.on_a_line),
            horizontal: place(&self.horizontal, at[1], how.on_a_line),
        }
    }

    /// Where a whole markup is, taken from the middle of it.
    pub fn where_is_box(&self, area: [f64; 4], how: HowClose) -> Where {
        self.where_is(
            [(area[0] + area[2]) / 2.0, (area[1] + area[3]) / 2.0],
            how,
        )
    }
}

fn place(lines: &[Line], value: f64, on_a_line: f64) -> Option<Along> {
    if lines.is_empty() {
        return None;
    }
    let nearest = lines
        .iter()
        .min_by(|a, b| {
            (a.at - value)
                .abs()
                .partial_cmp(&(b.at - value).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
    if (nearest.at - value).abs() <= on_a_line {
        return Some(Along::On(nearest.label.clone()));
    }
    // Between two of them — but only if it really is between. Past the end of
    // the grid is past the end, and gets no label at all.
    for pair in lines.windows(2) {
        if value > pair[0].at && value < pair[1].at {
            return Some(Along::Between(
                pair[0].label.clone(),
                pair[1].label.clone(),
            ));
        }
    }
    Some(Along::Beyond)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An ARCH D sheet, near enough: bubbles along the top and down the left
    /// have to be near those edges to count.
    const SHEET: [f64; 2] = [1728.0, 1224.0];

    /// A sheet with grid bubbles the way a framing plan has them: letters
    /// along the top, numbers down the left.
    fn a_framing_plan() -> Vec<Mark> {
        let mut marks = Vec::new();
        for (n, label) in ["A", "B", "C", "D", "E"].iter().enumerate() {
            marks.push(Mark {
                label: label.to_string(),
                at: [200.0 + n as f64 * 300.0, 120.0],
            });
        }
        for (n, label) in ["1", "2", "3", "4"].iter().enumerate() {
            marks.push(Mark {
                label: label.to_string(),
                at: [120.0, 240.0 + n as f64 * 250.0],
            });
        }
        marks
    }

    #[test]
    fn a_framing_plans_grid_is_read_off_the_sheet() {
        let grid = read(&a_framing_plan(), SHEET, HowClose::default());
        assert_eq!(grid.vertical.len(), 5);
        assert_eq!(grid.horizontal.len(), 4);
        assert_eq!(grid.vertical[0].label, "A");
        assert_eq!(grid.vertical[4].label, "E");
        assert_eq!(grid.horizontal[0].label, "1");
        assert_eq!(grid.extent(), "A–E / 1–4");
    }

    #[test]
    fn a_beam_on_an_intersection_reads_as_that_intersection() {
        let grid = read(&a_framing_plan(), SHEET, HowClose::default());
        // Right on B and on 3.
        let at = grid.where_is([500.0, 740.0], HowClose::default());
        assert_eq!(at.say(), "B-3");
        assert!(at.is_somewhere());
    }

    #[test]
    fn a_beam_between_two_lines_says_which_two() {
        let grid = read(&a_framing_plan(), SHEET, HowClose::default());
        let at = grid.where_is([650.0, 740.0], HowClose::default());
        assert_eq!(at.say(), "B/C-3");
    }

    #[test]
    fn something_off_the_end_of_the_grid_gets_no_label_rather_than_the_nearest() {
        // The whole point. A detail drawn out in the margin is not "at E" just
        // because E is the closest thing to it.
        let grid = read(&a_framing_plan(), SHEET, HowClose::default());
        let at = grid.where_is([2400.0, 2000.0], HowClose::default());
        assert_eq!(at.vertical, Some(Along::Beyond));
        assert_eq!(at.horizontal, Some(Along::Beyond));
        assert_eq!(at.say(), "");
        assert!(!at.is_somewhere());
    }

    #[test]
    fn a_sheet_with_no_grid_produces_no_grid() {
        // A detail sheet. Text all over it, none of it a grid.
        let marks = vec![
            Mark { label: "SECTION".into(), at: [100.0, 100.0] },
            Mark { label: "A".into(), at: [300.0, 400.0] },
            Mark { label: "TYP".into(), at: [500.0, 700.0] },
            Mark { label: "3".into(), at: [900.0, 150.0] },
        ];
        let grid = read(&marks, SHEET, HowClose::default());
        assert!(grid.is_empty());
        assert_eq!(grid.where_is([400.0, 400.0], HowClose::default()).say(), "");
    }

    #[test]
    fn two_labels_are_not_enough_to_call_something_a_grid() {
        // Two numbers in a line is a dimension string, a door tag, a pair of
        // callouts — anything but a grid.
        let marks = vec![
            Mark { label: "1".into(), at: [200.0, 120.0] },
            Mark { label: "2".into(), at: [500.0, 120.0] },
        ];
        assert!(read(&marks, SHEET, HowClose::default()).is_empty());
    }

    #[test]
    fn labels_out_of_order_are_not_a_grid() {
        // A grid runs in order. Four numbers scattered along the top of a
        // sheet in no particular order are four callouts.
        let marks = vec![
            Mark { label: "3".into(), at: [200.0, 120.0] },
            Mark { label: "1".into(), at: [500.0, 120.0] },
            Mark { label: "7".into(), at: [800.0, 120.0] },
            Mark { label: "2".into(), at: [1100.0, 120.0] },
        ];
        assert!(read(&marks, SHEET, HowClose::default()).is_empty());
    }

    #[test]
    fn the_same_label_twice_in_a_line_is_not_a_grid() {
        // A grid has one line B. Two of them means this is something else.
        let marks = vec![
            Mark { label: "A".into(), at: [200.0, 120.0] },
            Mark { label: "B".into(), at: [500.0, 120.0] },
            Mark { label: "B".into(), at: [800.0, 120.0] },
            Mark { label: "C".into(), at: [1100.0, 120.0] },
        ];
        assert!(read(&marks, SHEET, HowClose::default()).vertical.is_empty());
    }

    #[test]
    fn a_grid_that_runs_right_to_left_is_still_a_grid() {
        // Plans get mirrored. The labels still have to be in order; which way
        // the order runs is not this program's business.
        let marks: Vec<Mark> = ["D", "C", "B", "A"]
            .iter()
            .enumerate()
            .map(|(n, label)| Mark {
                label: label.to_string(),
                at: [200.0 + n as f64 * 300.0, 120.0],
            })
            .collect();
        let grid = read(&marks, SHEET, HowClose::default());
        assert_eq!(grid.vertical.len(), 4);
        // Sorted by where they are on the sheet, whatever order they read in.
        assert_eq!(grid.vertical[0].label, "D");
        assert_eq!(grid.vertical[3].label, "A");
    }

    #[test]
    fn letters_and_numbers_do_not_get_mixed_into_one_run() {
        let marks = vec![
            Mark { label: "A".into(), at: [200.0, 120.0] },
            Mark { label: "1".into(), at: [500.0, 120.0] },
            Mark { label: "B".into(), at: [800.0, 120.0] },
            Mark { label: "2".into(), at: [1100.0, 120.0] },
        ];
        // Two of each is not three of either.
        assert!(read(&marks, SHEET, HowClose::default()).is_empty());
    }

    #[test]
    fn a_schedule_down_the_middle_of_a_sheet_is_not_a_grid() {
        // The one that mattered. Run against a real stamped structural set,
        // the first version of this found a "grid" on nine sheets out of
        // twelve — and five of them were beam schedules, door schedules and
        // bolt lists, because a column of numbers in a straight line looks
        // exactly like a column of grid bubbles. A grid is drawn round the
        // outside of the plan where the fitter can see it.
        let middle: Vec<Mark> = (0..8)
            .map(|n| Mark {
                label: format!("{}", n + 2),
                at: [SHEET[0] * 0.5, 300.0 + n as f64 * 40.0],
            })
            .collect();
        assert!(read(&middle, SHEET, HowClose::default()).is_empty());

        // The same eight numbers down the left edge are a grid.
        let edge: Vec<Mark> = (0..8)
            .map(|n| Mark {
                label: format!("{}", n + 2),
                at: [60.0, 300.0 + n as f64 * 40.0],
            })
            .collect();
        assert_eq!(read(&edge, SHEET, HowClose::default()).horizontal.len(), 8);
    }

    #[test]
    fn a_column_of_part_numbers_is_not_a_grid_even_at_the_edge() {
        // The other half of it: a run that climbs 6, 92, 456 is a schedule
        // column, whatever straight line it happens to sit in. A grid runs
        // 1 to 6.
        let marks: Vec<Mark> = ["6", "92", "456"]
            .iter()
            .enumerate()
            .map(|(n, label)| Mark {
                label: label.to_string(),
                at: [60.0, 300.0 + n as f64 * 40.0],
            })
            .collect();
        assert!(read(&marks, SHEET, HowClose::default()).is_empty());
    }

    #[test]
    fn a_grid_with_a_line_missing_is_still_a_grid() {
        // Lines do get left out — a plan that shows 1, 2, 3, 5, 6 because 4
        // is on another sheet is ordinary. The rule is "close to consecutive",
        // not "consecutive".
        let marks: Vec<Mark> = ["1", "2", "3", "5", "6"]
            .iter()
            .enumerate()
            .map(|(n, label)| Mark {
                label: label.to_string(),
                at: [200.0 + n as f64 * 300.0, 60.0],
            })
            .collect();
        assert_eq!(read(&marks, SHEET, HowClose::default()).vertical.len(), 5);
    }

    #[test]
    fn a_grid_lettered_past_z_keeps_counting() {
        assert_eq!(rank("A"), Some((Family::Lettered, 1)));
        assert_eq!(rank("Z"), Some((Family::Lettered, 26)));
        assert_eq!(rank("AA"), Some((Family::Lettered, 27)));
        assert!(rank("Z").unwrap().1 < rank("AA").unwrap().1);
    }

    #[test]
    fn a_word_is_not_a_grid_label() {
        // The shorthand a drawing is covered in must not be able to line up
        // into a grid in the first place.
        for shorthand in ["TYP", "EQ.", "UNO", "SIM", "OPP", "SECTION"] {
            assert_eq!(rank(shorthand), None, "{shorthand} is not a grid line");
        }
        assert_eq!(rank("EQ"), Some((Family::Lettered, 147)), "two letters still count");
        assert_eq!(rank("A1"), None, "mixed is a sheet number, not a grid line");
        assert_eq!(rank(""), None);
        assert_eq!(rank("3/4"), None);
    }

    #[test]
    fn only_one_axis_is_a_perfectly_good_answer() {
        // Plenty of sheets letter one direction and leave the other alone.
        let marks: Vec<Mark> = ["A", "B", "C"]
            .iter()
            .enumerate()
            .map(|(n, label)| Mark {
                label: label.to_string(),
                at: [200.0 + n as f64 * 300.0, 120.0],
            })
            .collect();
        let grid = read(&marks, SHEET, HowClose::default());
        assert_eq!(grid.vertical.len(), 3);
        assert!(grid.horizontal.is_empty());
        let at = grid.where_is([500.0, 900.0], HowClose::default());
        assert_eq!(at.say(), "B", "no second axis to report, so it says the one");
    }

    #[test]
    fn a_markup_is_located_by_its_middle() {
        let grid = read(&a_framing_plan(), SHEET, HowClose::default());
        // A beam spanning from B to C, centred between them.
        let at = grid.where_is_box([500.0, 730.0, 800.0, 750.0], HowClose::default());
        assert_eq!(at.say(), "B/C-3");
    }

    #[test]
    fn nothing_about_a_location_touches_a_quantity() {
        // The contract that lets a location be worked out from the sheet at
        // all rather than clicked: it is a label, and labels do not add up.
        let grid = read(&a_framing_plan(), SHEET, HowClose::default());
        let at = grid.where_is([500.0, 740.0], HowClose::default());
        let said = at.say();
        assert!(said.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '/'));
    }
}
