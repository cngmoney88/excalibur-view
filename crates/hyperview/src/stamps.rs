//! Headers, footers and page numbering.
//!
//! What a shop actually does with these: a job number and an issue date on
//! every sheet before a set goes to a general contractor, and a running number
//! on a submittal so the pages can be referred to in a letter. Small jobs, done
//! to a hundred and fifty sheets at a time, which is why they are here rather
//! than done by hand.
//!
//! Everything written is **on top of the drawing, not instead of it**. A stamp
//! is added to the page's content; nothing already on the sheet is moved or
//! removed. And as everywhere else, the file that was read is not the file that
//! is written.

/// Where on the sheet a piece of text goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    TopLeft,
    TopMiddle,
    TopRight,
    BottomLeft,
    BottomMiddle,
    BottomRight,
}

impl Spot {
    pub const ALL: &'static [Spot] = &[
        Spot::TopLeft,
        Spot::TopMiddle,
        Spot::TopRight,
        Spot::BottomLeft,
        Spot::BottomMiddle,
        Spot::BottomRight,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Spot::TopLeft => "Top left",
            Spot::TopMiddle => "Top centre",
            Spot::TopRight => "Top right",
            Spot::BottomLeft => "Bottom left",
            Spot::BottomMiddle => "Bottom centre",
            Spot::BottomRight => "Bottom right",
        }
    }

    fn at_top(self) -> bool {
        matches!(self, Spot::TopLeft | Spot::TopMiddle | Spot::TopRight)
    }

    /// Where the text's left edge goes, given how wide it is.
    ///
    /// Right and centre need the text's width, which is why this takes it: a
    /// footer that is supposed to be in the bottom-right corner and is actually
    /// half off the sheet is worse than no footer.
    pub fn place(self, page: (f32, f32), text_width: f32, margin: f32) -> (f32, f32) {
        let x = match self {
            Spot::TopLeft | Spot::BottomLeft => margin,
            Spot::TopMiddle | Spot::BottomMiddle => (page.0 - text_width) * 0.5,
            Spot::TopRight | Spot::BottomRight => page.0 - text_width - margin,
        };
        let y = if self.at_top() {
            page.1 - margin
        } else {
            margin
        };
        (x.max(0.0), y)
    }
}

/// What to stamp on every sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct Stamp {
    pub spot: Spot,
    /// The text, which may contain the fields below.
    pub text: String,
    pub size: f32,
    /// Black unless somebody says otherwise.
    pub colour: [u8; 3],
    /// Distance from the edge of the sheet, in points.
    pub margin: f32,
}

impl Default for Stamp {
    fn default() -> Stamp {
        Stamp {
            spot: Spot::BottomRight,
            text: String::new(),
            size: 12.0,
            colour: [0, 0, 0],
            margin: 24.0,
        }
    }
}

/// The fields somebody can put in a header or footer.
///
/// Deliberately few and deliberately obvious. A stamping tool with a small
/// language in it becomes a thing people have to look up, and the whole point
/// is that somebody can do this without asking anybody.
pub const FIELDS: &[(&str, &str)] = &[
    ("<n>", "the sheet's number in this file: 1, 2, 3"),
    ("<total>", "how many sheets there are"),
    ("<sheet>", "the sheet number off the drawing itself: S-201"),
    ("<file>", "the file's name"),
    ("<date>", "today's date"),
];

/// Fills the fields in. Anything that is not a field is left exactly as typed,
/// including angle brackets that do not spell one of these.
pub fn fill(
    text: &str,
    page: usize,
    total: usize,
    sheet_number: &str,
    file: &str,
    date: &str,
) -> String {
    text.replace("<n>", &(page + 1).to_string())
        .replace("<total>", &total.to_string())
        .replace("<sheet>", sheet_number)
        .replace("<file>", file)
        .replace("<date>", date)
}

/// Roughly how wide a string will be in Helvetica at a size.
///
/// An estimate, and it does not need to be more: it decides where a right-hand
/// footer starts, where being a few points out is invisible, and it keeps this
/// from needing font metrics for a job that is a stamp in a corner.
pub fn about_as_wide(text: &str, size: f32) -> f32 {
    // Helvetica's average advance is a little over half its point size; digits
    // and capitals are wider, spaces narrower.
    let units: f32 = text
        .chars()
        .map(|c| match c {
            'i' | 'j' | 'l' | 'I' | '.' | ',' | ':' | ';' | '\'' => 0.28,
            ' ' => 0.28,
            'm' | 'M' | 'W' | 'w' => 0.86,
            c if c.is_ascii_uppercase() || c.is_ascii_digit() => 0.62,
            _ => 0.52,
        })
        .sum();
    units * size
}

/// Today, written the way a drawing writes it.
pub fn today() -> String {
    // Days since the epoch, turned into a date. Not worth a date library for
    // one string in a footer, and this is the same arithmetic a date library
    // does.
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Howard Hinnant's days-to-date, which is the standard way to do this without
/// a table of month lengths and a leap-year mistake.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_footer_on_the_right_ends_at_the_margin_rather_than_off_the_sheet() {
        let page = (3024.0, 2160.0);
        let text = "MESA FAB 24-118";
        let width = about_as_wide(text, 12.0);
        let (x, y) = Spot::BottomRight.place(page, width, 24.0);
        assert!(x + width <= page.0 - 24.0 + 0.01, "x={x} width={width}");
        assert!(x > page.0 * 0.5, "it should be on the right");
        assert_eq!(y, 24.0);
    }

    #[test]
    fn a_centred_footer_is_centred() {
        let page = (1000.0, 800.0);
        let width = 200.0;
        let (x, _) = Spot::BottomMiddle.place(page, width, 24.0);
        assert_eq!(x, 400.0);
    }

    #[test]
    fn a_header_sits_at_the_top_and_a_footer_at_the_bottom() {
        let page = (1000.0, 800.0);
        assert_eq!(Spot::TopLeft.place(page, 100.0, 20.0).1, 780.0);
        assert_eq!(Spot::BottomLeft.place(page, 100.0, 20.0).1, 20.0);
    }

    #[test]
    fn text_wider_than_the_sheet_starts_at_the_edge_rather_than_off_it() {
        let page = (200.0, 200.0);
        let (x, _) = Spot::BottomRight.place(page, 500.0, 24.0);
        assert_eq!(x, 0.0);
    }

    #[test]
    fn the_fields_fill_in() {
        let filled = fill(
            "<file> · sheet <n> of <total> · <sheet>",
            2,
            12,
            "S-201",
            "Structural.pdf",
            "2026-09-18",
        );
        assert_eq!(filled, "Structural.pdf · sheet 3 of 12 · S-201");
    }

    #[test]
    fn a_sheet_number_counts_from_one_the_way_people_do() {
        assert_eq!(fill("<n>", 0, 5, "", "", ""), "1");
        assert_eq!(fill("<n>", 4, 5, "", "", ""), "5");
    }

    #[test]
    fn something_that_is_not_a_field_is_left_exactly_as_typed() {
        // Somebody writing "<SEE NOTE 3>" means that, not a field.
        let text = "REV <SEE NOTE 3> — <n>";
        assert_eq!(fill(text, 0, 1, "", "", ""), "REV <SEE NOTE 3> — 1");
    }

    #[test]
    fn a_sheet_with_no_number_on_it_leaves_a_gap_rather_than_the_word_none() {
        assert_eq!(fill("<sheet>", 0, 1, "", "", ""), "");
    }

    #[test]
    fn the_width_estimate_is_in_the_right_neighbourhood() {
        // Not exact — it does not need to be — but a twenty-character string at
        // twelve point is roughly 120 points, not 12 and not 1200.
        let width = about_as_wide("MESA FAB, INC. 24-118", 12.0);
        assert!((90.0..190.0).contains(&width), "{width}");
    }

    #[test]
    fn todays_date_is_a_date() {
        let today = today();
        assert_eq!(today.len(), 10, "{today}");
        let year: i64 = today[..4].parse().expect("a year");
        assert!((2024..2100).contains(&year), "{today}");
        assert_eq!(&today[4..5], "-");
    }

    #[test]
    fn the_calendar_arithmetic_matches_known_dates() {
        // Day nought is the epoch.
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // A leap day, which is where this sort of arithmetic goes wrong.
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(19_783), (2024, 3, 1));
        // And a century that is not a leap year.
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }
}
