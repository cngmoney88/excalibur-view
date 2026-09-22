//! What a revision cost.
//!
//! Rev 2 was bid. Rev 3 arrives with a cloud round half of grid line 4 and a
//! note that says "beam sizes revised". The question everybody in the office
//! asks next is the same one: *what did that just do to my tonnage?*
//!
//! Today that question gets answered by taking the whole set off again and
//! eyeballing two spreadsheets side by side, which takes a day and misses
//! things. Revu will show you what changed on the **paper**; it has nothing at
//! all to say about what changed in the **quantities**. This does the second
//! one: two takeoffs in, and a line for every subject that moved.
//!
//! The rules are the same ones as everywhere else, and they matter more here
//! than anywhere, because a revision delta is the number that goes in a change
//! order:
//!
//! * **Nothing is estimated.** A subject that carries no unit weight has no
//!   weight delta — not a delta of zero.
//! * **Anything off an unscaled sheet is out of the comparison** and is said
//!   so, on both sides, because a revision that looks free because three
//!   sheets were never calibrated is the worst possible answer.
//! * **The two sides are never reconciled.** If a subject was renamed between
//!   issues this reports it as one gone and one arrived, which is the truth.
//!   Guessing that "W12x26" and "W12X26 (REV)" are the same member is exactly
//!   the kind of helpfulness that puts a wrong number in a change order.

use std::collections::BTreeMap;

use annot::Kind;

use crate::row::{Row, WeightColumns};
use crate::summary::{summarise, Summary};

/// What happened to one subject between two issues.
#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum What {
    /// It is on the new issue and was not on the old one.
    Arrived,
    /// It was on the old issue and is not on the new one.
    Gone,
    /// More of it.
    More,
    /// Less of it.
    Less,
    /// The same quantity, kept so a revision can be shown in full.
    Same,
}

impl What {
    pub fn name(self) -> &'static str {
        match self {
            What::Arrived => "new",
            What::Gone => "gone",
            What::More => "more",
            What::Less => "less",
            What::Same => "unchanged",
        }
    }

    /// Whether this is something somebody has to look at.
    pub fn moved(self) -> bool {
        self != What::Same
    }
}

/// One line of the revision.
#[derive(Clone, Debug)]
pub struct Moved {
    pub subject: String,
    pub kind: Kind,
    pub what: What,
    /// How many markups, before and after.
    pub picks: (usize, usize),
    /// The quantity in the subject's own unit, before and after.
    pub quantity: (f64, f64),
    pub unit: String,
    /// Pounds before and after. `None` on a side means that side carried no
    /// unit weight — which is not the same as weighing nothing, and is the
    /// reason `pounds_moved` can be `None`.
    pub pounds: (Option<f64>, Option<f64>),
}

impl Moved {
    /// How much the quantity moved.
    pub fn quantity_moved(&self) -> f64 {
        self.quantity.1 - self.quantity.0
    }

    /// How much the weight moved, when both sides could be weighed.
    ///
    /// `None` when either side carried no unit weight. A change order needs a
    /// number somebody can stand behind, and half of one is not that.
    pub fn pounds_moved(&self) -> Option<f64> {
        match (self.pounds.0, self.pounds.1) {
            (Some(before), Some(after)) => Some(after - before),
            // Something arriving from nothing, or going to nothing, is a real
            // delta as long as the side that exists carries a weight.
            (None, Some(after)) if self.what == What::Arrived => Some(after),
            (Some(before), None) if self.what == What::Gone => Some(-before),
            _ => None,
        }
    }
}

/// Everything two issues differ by.
#[derive(Clone, Debug, Default)]
pub struct Revision {
    pub moved: Vec<Moved>,
    /// What the whole revision is worth, from the lines that could be weighed.
    pub pounds: f64,
    /// Subjects whose weight could not be worked out on one side or the other.
    pub unweighed: Vec<String>,
    /// Measurements left out of the comparison for want of a scale.
    pub unscaled_before: usize,
    pub unscaled_after: usize,
    pub unscaled_pages: Vec<usize>,
}

impl Revision {
    pub fn tons(&self) -> f64 {
        self.pounds / 2000.0
    }

    /// The lines somebody has to look at.
    pub fn changes(&self) -> impl Iterator<Item = &Moved> {
        self.moved.iter().filter(|m| m.what.moved())
    }

    pub fn anything_moved(&self) -> bool {
        self.moved.iter().any(|m| m.what.moved())
    }

    /// Whether the weight above is the whole story.
    pub fn is_whole(&self) -> bool {
        self.unweighed.is_empty() && self.unscaled_before == 0 && self.unscaled_after == 0
    }

    /// The headline, in a sentence that can go straight in an email.
    pub fn headline(&self) -> String {
        if !self.anything_moved() {
            return "Nothing moved. The two takeoffs carry the same quantities."
                .to_string();
        }
        let direction = if self.pounds > 0.0 { "added" } else { "removed" };
        format!(
            "This revision {direction} about {:.0} lb · {:.3} tons across {} \
             subject{}.",
            self.pounds.abs(),
            self.tons().abs(),
            self.changes().count(),
            if self.changes().count() == 1 { "" } else { "s" }
        )
    }

    /// What the headline leaves out, said plainly. Empty when it leaves out
    /// nothing.
    pub fn what_is_missing(&self) -> String {
        let mut said = Vec::new();
        let unscaled = self.unscaled_before + self.unscaled_after;
        if unscaled > 0 {
            said.push(format!(
                "{unscaled} measurement{} on sheets with no scale ({} before, {} after)",
                if unscaled == 1 { "" } else { "s" },
                self.unscaled_before,
                self.unscaled_after
            ));
        }
        if !self.unweighed.is_empty() {
            said.push(format!(
                "{} subject{} that could not be weighed on one side or the other ({})",
                self.unweighed.len(),
                if self.unweighed.len() == 1 { "" } else { "s" },
                self.unweighed.join(", ")
            ));
        }
        if said.is_empty() {
            return String::new();
        }
        format!(
            "NOT IN THAT FIGURE: {}. Nothing has been estimated to fill the gap — \
             a change order wants a number somebody can stand behind.",
            said.join("; ")
        )
    }
}

/// The quantity a kind is measured in.
fn quantity_of(group: &crate::summary::Group) -> f64 {
    match group.kind {
        Kind::Area => group.area,
        Kind::Volume => group.volume,
        Kind::Count | Kind::Angle | Kind::Markup => group.count,
        _ => group.length,
    }
}

/// Compares two takeoffs and says what moved.
///
/// `before` is the issue that was bid; `after` is the one that arrived.
pub fn compare(before: &[Row], after: &[Row], weights: WeightColumns) -> Revision {
    let one = summarise(before, weights);
    let two = summarise(after, weights);
    from_summaries(&one, &two)
}

/// The same, from two takeoffs that have already been totalled.
pub fn from_summaries(before: &Summary, after: &Summary) -> Revision {
    let mut out = Revision::default();
    let mut seen: BTreeMap<(String, &'static str), (Option<usize>, Option<usize>)> =
        BTreeMap::new();

    for (at, group) in before.groups.iter().enumerate() {
        seen.entry((group.subject.clone(), group.kind.name()))
            .or_default()
            .0 = Some(at);
    }
    for (at, group) in after.groups.iter().enumerate() {
        seen.entry((group.subject.clone(), group.kind.name()))
            .or_default()
            .1 = Some(at);
    }

    for ((subject, _), (one, two)) in seen {
        let old = one.and_then(|at| before.groups.get(at));
        let new = two.and_then(|at| after.groups.get(at));
        let kind = old.or(new).map(|g| g.kind).unwrap_or(Kind::Markup);
        let unit = new
            .map(|g| g.unit.clone())
            .filter(|u| !u.is_empty())
            .or_else(|| old.map(|g| g.unit.clone()))
            .unwrap_or_default();

        let quantity = (
            old.map(quantity_of).unwrap_or(0.0),
            new.map(quantity_of).unwrap_or(0.0),
        );
        // A group that was never weighed reports `None`, never zero.
        let pounds = (
            old.filter(|g| g.weighed).map(|g| g.pounds),
            new.filter(|g| g.weighed).map(|g| g.pounds),
        );
        let what = match (old, new) {
            (None, Some(_)) => What::Arrived,
            (Some(_), None) => What::Gone,
            _ if (quantity.1 - quantity.0).abs() < 1e-9 => What::Same,
            _ if quantity.1 > quantity.0 => What::More,
            _ => What::Less,
        };

        let line = Moved {
            subject,
            kind,
            what,
            picks: (
                old.map(|g| g.picks).unwrap_or(0),
                new.map(|g| g.picks).unwrap_or(0),
            ),
            quantity,
            unit,
            pounds,
        };
        if line.what.moved() {
            match line.pounds_moved() {
                Some(moved) => out.pounds += moved,
                None => out.unweighed.push(line.subject.clone()),
            }
        }
        out.moved.push(line);
    }

    // The biggest movers first, then the rest, then what did not move — the
    // order somebody reads a change order in.
    out.moved.sort_by(|a, b| {
        let weight = |m: &Moved| m.pounds_moved().map(f64::abs).unwrap_or(0.0);
        b.what
            .moved()
            .cmp(&a.what.moved())
            .then_with(|| {
                weight(b)
                    .partial_cmp(&weight(a))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.subject.cmp(&b.subject))
    });

    out.unscaled_before = before.unscaled;
    out.unscaled_after = after.unscaled;
    out.unscaled_pages = before
        .unscaled_pages
        .iter()
        .chain(&after.unscaled_pages)
        .copied()
        .collect();
    out.unscaled_pages.sort_unstable();
    out.unscaled_pages.dedup();
    out
}

/// The revision as a spreadsheet.
pub fn to_csv(revision: &Revision) -> String {
    let quote = |field: &str| {
        if field.contains(',') || field.contains('"') || field.contains('\n') {
            format!("\"{}\"", field.replace('"', "\"\""))
        } else {
            field.to_string()
        }
    };
    let mut out = String::from(
        "Subject,Type,What,Was,Now,Change,Unit,lb was,lb now,lb change\n",
    );
    for line in &revision.moved {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            quote(&line.subject),
            line.kind.name(),
            line.what.name(),
            line.quantity.0,
            line.quantity.1,
            line.quantity_moved(),
            quote(&line.unit),
            // Empty, never zero: a side with no unit weight had no weight.
            line.pounds.0.map(|p| format!("{p:.2}")).unwrap_or_default(),
            line.pounds.1.map(|p| format!("{p:.2}")).unwrap_or_default(),
            line.pounds_moved()
                .map(|p| format!("{p:.2}"))
                .unwrap_or_default(),
        ));
    }
    out.push_str(&format!(
        "{},,,,,,,,,{:.2}\n",
        quote("REVISION"),
        revision.pounds
    ));
    out.push_str(&format!("\n# {}\n", revision.headline()));
    let missing = revision.what_is_missing();
    if !missing.is_empty() {
        out.push_str(&format!("# {missing}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beam(subject: &str, page: usize, feet: f64, per_foot: Option<&str>) -> Row {
        let mut row = Row::blank();
        row.page = page;
        row.subject = subject.into();
        row.kind = Kind::Length;
        row.scaled = true;
        row.length = Some(feet);
        row.unit = "'".into();
        if let Some(per) = per_foot {
            row.columns[WeightColumns::default().per_length] = per.into();
        }
        row
    }

    fn count(subject: &str, how_many: f64) -> Row {
        let mut row = Row::blank();
        row.subject = subject.into();
        row.kind = Kind::Count;
        row.scaled = true;
        row.count = how_many;
        row
    }

    #[test]
    fn a_revision_that_changed_nothing_says_so() {
        let before = vec![beam("W12x26", 0, 20.0, Some("26"))];
        let after = vec![beam("W12x26", 0, 20.0, Some("26"))];
        let rev = compare(&before, &after, WeightColumns::default());
        assert!(!rev.anything_moved());
        assert_eq!(rev.pounds, 0.0);
        assert!(rev.headline().contains("Nothing moved"));
    }

    #[test]
    fn steel_added_by_a_revision_is_priced() {
        let before = vec![beam("W12x26", 0, 20.0, Some("26"))];
        let after = vec![
            beam("W12x26", 0, 20.0, Some("26")),
            beam("W12x26", 0, 30.0, Some("26")),
        ];
        let rev = compare(&before, &after, WeightColumns::default());
        assert!((rev.pounds - 30.0 * 26.0).abs() < 1e-9, "{}", rev.pounds);
        let line = rev.changes().next().unwrap();
        assert_eq!(line.what, What::More);
        assert_eq!(line.picks, (1, 2));
        assert!((line.quantity_moved() - 30.0).abs() < 1e-9);
        assert!(rev.headline().contains("added"));
    }

    #[test]
    fn steel_taken_out_by_a_revision_comes_off() {
        let before = vec![
            beam("W12x26", 0, 20.0, Some("26")),
            beam("W12x26", 0, 30.0, Some("26")),
        ];
        let after = vec![beam("W12x26", 0, 20.0, Some("26"))];
        let rev = compare(&before, &after, WeightColumns::default());
        assert!((rev.pounds + 30.0 * 26.0).abs() < 1e-9, "{}", rev.pounds);
        assert_eq!(rev.changes().next().unwrap().what, What::Less);
        assert!(rev.headline().contains("removed"));
    }

    #[test]
    fn a_shape_that_arrived_is_the_whole_of_its_own_weight() {
        let before = vec![beam("W12x26", 0, 20.0, Some("26"))];
        let after = vec![
            beam("W12x26", 0, 20.0, Some("26")),
            beam("HSS6x6", 0, 10.0, Some("19.02")),
        ];
        let rev = compare(&before, &after, WeightColumns::default());
        let arrived = rev
            .moved
            .iter()
            .find(|m| m.subject == "HSS6x6")
            .expect("the new shape is on the list");
        assert_eq!(arrived.what, What::Arrived);
        assert!((arrived.pounds_moved().unwrap() - 190.2).abs() < 0.001);
    }

    #[test]
    fn a_shape_that_went_away_comes_off_in_full() {
        let before = vec![
            beam("W12x26", 0, 20.0, Some("26")),
            beam("HSS6x6", 0, 10.0, Some("19.02")),
        ];
        let after = vec![beam("W12x26", 0, 20.0, Some("26"))];
        let rev = compare(&before, &after, WeightColumns::default());
        let gone = rev.moved.iter().find(|m| m.subject == "HSS6x6").unwrap();
        assert_eq!(gone.what, What::Gone);
        assert!((gone.pounds_moved().unwrap() + 190.2).abs() < 0.001);
    }

    #[test]
    fn a_subject_with_no_unit_weight_has_no_delta_rather_than_a_delta_of_zero() {
        // This is the number that goes in a change order. Half a number is
        // not one.
        let before = vec![beam("SOMETHING ODD", 0, 20.0, None)];
        let after = vec![beam("SOMETHING ODD", 0, 40.0, None)];
        let rev = compare(&before, &after, WeightColumns::default());
        let line = rev.changes().next().unwrap();
        assert_eq!(line.pounds_moved(), None);
        assert_eq!(rev.pounds, 0.0, "it is not counted as weighing nothing");
        assert_eq!(rev.unweighed, vec!["SOMETHING ODD".to_string()]);
        assert!(!rev.is_whole());
        assert!(rev.what_is_missing().contains("SOMETHING ODD"));
    }

    #[test]
    fn a_renamed_subject_is_reported_as_one_gone_and_one_arrived() {
        // Guessing that these are the same member is exactly the helpfulness
        // that puts a wrong number in a change order.
        let before = vec![beam("W12x26", 0, 20.0, Some("26"))];
        let after = vec![beam("W12X26 (REV)", 0, 20.0, Some("26"))];
        let rev = compare(&before, &after, WeightColumns::default());
        let whats: Vec<What> = rev.changes().map(|m| m.what).collect();
        assert!(whats.contains(&What::Gone));
        assert!(whats.contains(&What::Arrived));
        assert!(rev.pounds.abs() < 1e-9, "they cancel out, honestly");
    }

    #[test]
    fn an_unscaled_sheet_on_either_side_is_reported() {
        let mut before = beam("W12x26", 2, 20.0, Some("26"));
        before.scaled = false;
        before.length = None;
        let rev = compare(
            &[before],
            &[beam("W12x26", 0, 20.0, Some("26"))],
            WeightColumns::default(),
        );
        assert_eq!(rev.unscaled_before, 1);
        assert_eq!(rev.unscaled_after, 0);
        assert!(rev.unscaled_pages.contains(&2));
        assert!(!rev.is_whole());
        assert!(rev.what_is_missing().contains("no scale"));
    }

    #[test]
    fn counts_are_compared_as_counts_not_as_lengths() {
        let rev = compare(
            &[count("Shear Conn", 12.0)],
            &[count("Shear Conn", 20.0)],
            WeightColumns::default(),
        );
        let line = rev.changes().next().unwrap();
        assert_eq!(line.kind, Kind::Count);
        assert!((line.quantity_moved() - 8.0).abs() < 1e-9);
    }

    #[test]
    fn a_length_and_a_count_of_the_same_name_stay_apart() {
        let before = vec![beam("Brace", 0, 20.0, Some("6")), count("Brace", 4.0)];
        let after = vec![beam("Brace", 0, 30.0, Some("6")), count("Brace", 4.0)];
        let rev = compare(&before, &after, WeightColumns::default());
        assert_eq!(rev.changes().count(), 1, "only the length moved");
        assert_eq!(rev.changes().next().unwrap().kind, Kind::Length);
    }

    #[test]
    fn the_biggest_mover_is_at_the_top() {
        let before = vec![
            beam("W12x26", 0, 20.0, Some("26")),
            beam("L4x4", 0, 20.0, Some("6")),
        ];
        let after = vec![
            beam("W12x26", 0, 60.0, Some("26")),
            beam("L4x4", 0, 30.0, Some("6")),
        ];
        let rev = compare(&before, &after, WeightColumns::default());
        assert_eq!(rev.moved[0].subject, "W12x26");
    }

    #[test]
    fn the_spreadsheet_never_writes_a_zero_where_there_was_no_weight() {
        let rev = compare(
            &[beam("SOMETHING ODD", 0, 20.0, None)],
            &[beam("SOMETHING ODD", 0, 40.0, None)],
            WeightColumns::default(),
        );
        let csv = to_csv(&rev);
        let line = csv
            .lines()
            .find(|l| l.starts_with("SOMETHING ODD"))
            .unwrap();
        assert!(line.ends_with(",,,"), "weight cells must be empty: {line:?}");
        assert!(csv.contains("NOT IN THAT FIGURE"));
    }

    #[test]
    fn nothing_from_either_takeoff_is_changed_by_comparing_them() {
        let before = vec![beam("W12x26", 0, 20.0, Some("26"))];
        let after = vec![beam("W12x26", 0, 40.0, Some("26"))];
        let one = summarise(&before, WeightColumns::default()).pounds;
        let two = summarise(&after, WeightColumns::default()).pounds;
        let _ = compare(&before, &after, WeightColumns::default());
        assert!((summarise(&before, WeightColumns::default()).pounds - one).abs() < 1e-12);
        assert!((summarise(&after, WeightColumns::default()).pounds - two).abs() < 1e-12);
    }
}
