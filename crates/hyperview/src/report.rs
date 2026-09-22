//! The markup summary: the thing an estimator hands over.
//!
//! A takeoff is not finished when the last markup is drawn. It is finished when
//! somebody can read what was counted, on paper, beside the drawings — every
//! markup with its subject, its sheet, what it measured and what it weighs,
//! totalled by tool. That report is what goes in a bid folder and what gets
//! argued about six months later, so two things matter more than looking nice.
//!
//! **It has to be readable without the program.** So it is written as an
//! ordinary PDF: text on pages, nothing that needs Hyperview to open it.
//!
//! **It has to carry the warnings with it.** A takeoff that left three sheets
//! out for want of a scale is a short takeoff, and the report says so at the
//! top — not in a footnote, and never by silently omitting them. A report that
//! is quietly short is worse than no report, because somebody bids off it.

use takeoff::Column;

/// One line of the report.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub cells: Vec<String>,
    /// A heading rather than a markup: a tool's name, or a total.
    pub heading: bool,
}

/// A page of it, laid out.
#[derive(Clone, Debug, Default)]
pub struct Page {
    pub lines: Vec<Line>,
}

/// The whole report, ready to write.
#[derive(Clone, Debug)]
pub struct Report {
    pub title: String,
    /// What this is a takeoff of.
    pub drawing: String,
    pub when: String,
    pub who: String,
    /// The warning, when there is one. Empty when the takeoff is whole.
    pub short_by: String,
    pub headings: Vec<String>,
    /// How wide each column is, as a fraction of the usable width.
    pub widths: Vec<f32>,
    pub pages: Vec<Page>,
}

/// A sheet of paper for the report. Letter, landscape, because a takeoff table
/// is wide and a portrait page means either four columns or unreadable type.
pub const PAGE: (f32, f32) = (792.0, 612.0);
pub const MARGIN: f32 = 40.0;
/// Where the table starts on the first page, under the heading block.
pub const FIRST_TOP: f32 = 120.0;
/// And on every page after it, under the repeated column headings.
pub const TOP: f32 = 64.0;
pub const LINE: f32 = 15.0;
pub const SIZE: f32 = 9.0;

/// How many lines fit on a page.
pub fn lines_per_page(first: bool) -> usize {
    let from_top = if first { FIRST_TOP } else { TOP };
    (((PAGE.1 - MARGIN - from_top) / LINE).floor() as usize).max(1)
}

/// Splits the lines across pages.
pub fn paginate(lines: Vec<Line>) -> Vec<Page> {
    let mut pages = Vec::new();
    let mut rest = &lines[..];
    let mut first = true;
    while !rest.is_empty() {
        let room = lines_per_page(first);
        let take = room.min(rest.len());
        pages.push(Page {
            lines: rest[..take].to_vec(),
        });
        rest = &rest[take..];
        first = false;
    }
    if pages.is_empty() {
        pages.push(Page::default());
    }
    pages
}

/// Column widths that add up to one, from what is actually in the cells.
///
/// Measured rather than guessed, because a report where the Subject column is
/// six characters wide and the Colour column is forty is a report somebody
/// re-does in a spreadsheet.
pub fn widths_for(headings: &[String], lines: &[Line]) -> Vec<f32> {
    let count = headings.len().max(1);
    let mut widest: Vec<f32> = headings
        .iter()
        .map(|h| crate::stamps::about_as_wide(h, SIZE))
        .collect();
    widest.resize(count, 0.0);
    for line in lines {
        for (at, cell) in line.cells.iter().enumerate().take(count) {
            let wide = crate::stamps::about_as_wide(cell, SIZE);
            if wide > widest[at] {
                widest[at] = wide;
            }
        }
    }
    // A column gets what it needs, within reason: nothing may take more than
    // two fifths of the page, or one long note squeezes everything else out.
    // Capping alone is not enough — with every other column narrow, the
    // capped one still ends up with nearly all of the page once the widths
    // are turned into shares. So whatever room is left over after the cap
    // goes to the columns that were not capped.
    let usable = PAGE.0 - MARGIN * 2.0;
    let most = usable * 0.40;
    let mut capped = vec![false; count];
    for (at, wide) in widest.iter_mut().enumerate() {
        if *wide > most {
            capped[at] = true;
            *wide = most;
        }
        *wide += 8.0;
    }
    let total: f32 = widest.iter().sum();
    if total <= 0.0 {
        return vec![1.0 / count as f32; count];
    }
    if total < usable {
        let slack = usable - total;
        let free: f32 = widest
            .iter()
            .zip(&capped)
            .filter(|(_, c)| !**c)
            .map(|(w, _)| *w)
            .sum();
        if free > 0.0 {
            for (wide, capped) in widest.iter_mut().zip(&capped) {
                if !*capped {
                    *wide += slack * (*wide / free);
                }
            }
        } else {
            for wide in widest.iter_mut() {
                *wide += slack / count as f32;
            }
        }
    }
    let total: f32 = widest.iter().sum();
    widest.iter().map(|w| w / total).collect()
}

/// Cuts a cell down to what fits, with a stop so it is obvious it was cut.
pub fn fit(text: &str, points: f32) -> String {
    if crate::stamps::about_as_wide(text, SIZE) <= points {
        return text.to_string();
    }
    let mut out = String::new();
    for c in text.chars() {
        let with = format!("{out}{c}…");
        if crate::stamps::about_as_wide(&with, SIZE) > points {
            break;
        }
        out.push(c);
    }
    if out.is_empty() {
        return String::new();
    }
    format!("{out}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(cells: &[&str]) -> Line {
        Line {
            cells: cells.iter().map(|c| c.to_string()).collect(),
            heading: false,
        }
    }

    #[test]
    fn a_short_report_is_one_page() {
        let pages = paginate(vec![line(&["a"]), line(&["b"])]);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].lines.len(), 2);
    }

    #[test]
    fn a_long_one_carries_on_and_loses_nothing() {
        let many: Vec<Line> = (0..200).map(|n| line(&[&n.to_string()])).collect();
        let pages = paginate(many.clone());
        assert!(pages.len() > 1);
        let back: usize = pages.iter().map(|p| p.lines.len()).sum();
        assert_eq!(back, many.len(), "no line may be dropped between pages");
        // And the order is kept.
        assert_eq!(pages[0].lines[0], many[0]);
        assert_eq!(
            pages.last().unwrap().lines.last().unwrap(),
            many.last().unwrap()
        );
    }

    #[test]
    fn the_first_page_holds_fewer_lines_because_of_the_heading_block() {
        assert!(lines_per_page(true) < lines_per_page(false));
        assert!(lines_per_page(true) > 20, "still a useful page");
    }

    #[test]
    fn nothing_to_report_is_still_a_page_rather_than_no_file() {
        // Somebody who asked for a report should get one saying there is
        // nothing, not a missing file they have to wonder about.
        let pages = paginate(Vec::new());
        assert_eq!(pages.len(), 1);
        assert!(pages[0].lines.is_empty());
    }

    #[test]
    fn columns_are_as_wide_as_what_is_in_them() {
        let headings = vec!["#".to_string(), "Subject".to_string()];
        let lines = vec![line(&["1", "W12x26 BEAM AT GRID LINE 4"])];
        let widths = widths_for(&headings, &lines);
        assert_eq!(widths.len(), 2);
        assert!(widths[1] > widths[0], "the long column is wider");
        let total: f32 = widths.iter().sum();
        assert!((total - 1.0).abs() < 0.001, "they add up to the page");
    }

    #[test]
    fn one_enormous_cell_does_not_squeeze_everything_else_out() {
        let headings = vec!["#".to_string(), "Note".to_string(), "Length".to_string()];
        let huge = "x".repeat(4000);
        let lines = vec![line(&["1", &huge, "12'-6\""])];
        let widths = widths_for(&headings, &lines);
        assert!(widths[1] < 0.75, "the note column took {}", widths[1]);
        assert!(widths[2] > 0.05, "the length column still has room");
    }

    #[test]
    fn a_cell_too_wide_for_its_column_is_cut_with_a_stop() {
        let cut = fit("W12x26 BEAM AT GRID LINE 4 SEE DETAIL 3/S-501", 60.0);
        assert!(cut.ends_with('…'));
        assert!(crate::stamps::about_as_wide(&cut, SIZE) <= 60.0 + 0.01);
        // And something that fits is left exactly alone.
        assert_eq!(fit("12'-6\"", 200.0), "12'-6\"");
    }

    #[test]
    fn a_column_with_no_room_at_all_produces_nothing_rather_than_a_lone_stop() {
        assert_eq!(fit("anything", 0.5), "");
    }

    #[test]
    fn columns_with_nothing_in_them_still_divide_the_page() {
        let widths = widths_for(&[], &[]);
        assert_eq!(widths.len(), 1);
        assert!((widths[0] - 1.0).abs() < 0.001);
    }
}

// ---- turning a takeoff into a report ---------------------------------------

/// Builds the report from what the markups list is showing.
///
/// From the same rows and columns the list shows, on purpose: a report that
/// disagrees with the screen is a report somebody stops trusting, and the way
/// that happens is two pieces of code working out the same numbers separately.
pub fn from_takeoff(
    drawing: &str,
    who: &str,
    rows: &[takeoff::Row],
    columns: &[Column],
    weights: takeoff::WeightColumns,
    sheet: &dyn Fn(usize) -> String,
    summary: &takeoff::summary::Summary,
) -> Report {
    let headings: Vec<String> = std::iter::once("#".to_string())
        .chain(columns.iter().map(|c| c.heading()))
        .collect();

    let mut lines: Vec<Line> = Vec::new();
    for (n, row) in rows.iter().enumerate() {
        let mut cells = vec![(n + 1).to_string()];
        for column in columns {
            // The words, not the raw numbers: this is read by a person, and
            // "12'-6\"" is what the drawing says. The CSV export is the one
            // that hands a spreadsheet the number.
            cells.push(if *column == Column::Page {
                sheet(row.page)
            } else {
                column.text(row, weights)
            });
        }
        lines.push(Line {
            cells,
            heading: false,
        });
    }

    // The totals, as their own block at the end, marked as headings so they are
    // drawn in a way somebody's eye stops at.
    if !summary.groups.is_empty() {
        lines.push(Line {
            cells: vec![String::new()],
            heading: false,
        });
        lines.push(Line {
            cells: vec!["TOTALS".to_string()],
            heading: true,
        });
        for group in &summary.groups {
            lines.push(Line {
                cells: vec![
                    String::new(),
                    group.subject.clone(),
                    format!(
                        "{} markup{}",
                        group.picks,
                        if group.picks == 1 { "" } else { "s" }
                    ),
                    if group.pounds > 0.0 {
                        format!("{:.0} lb", group.pounds)
                    } else {
                        String::new()
                    },
                ],
                heading: true,
            });
        }
        if summary.pounds > 0.0 {
            lines.push(Line {
                cells: vec![
                    String::new(),
                    "ALL".to_string(),
                    format!("{} markups", summary.picks),
                    format!("{:.0} lb · {:.3} tons", summary.pounds, summary.tons()),
                ],
                heading: true,
            });
        }
    }

    // The warning that makes the difference between a report and a wrong bid.
    let short_by = if summary.unscaled > 0 {
        let sheets: Vec<String> = summary
            .unscaled_pages
            .iter()
            .map(|page| sheet(*page))
            .collect();
        format!(
            "SHORT: {} measurement{} not in these totals, because {} sheet{} no scale{}. \
             Set a scale on those sheets and take this report again.",
            summary.unscaled,
            if summary.unscaled == 1 { " is" } else { "s are" },
            if summary.unscaled == 1 { "its" } else { "their" },
            if summary.unscaled == 1 { " has" } else { "s have" },
            if sheets.is_empty() {
                String::new()
            } else {
                format!(" — {}", sheets.join(", "))
            }
        )
    } else {
        String::new()
    };

    let widths = widths_for(&headings, &lines);
    Report {
        title: "Takeoff summary".into(),
        drawing: drawing.to_string(),
        when: crate::stamps::today(),
        who: who.to_string(),
        short_by,
        headings,
        widths,
        pages: paginate(lines),
    }
}
