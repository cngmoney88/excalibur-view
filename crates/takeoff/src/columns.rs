//! The Markups list columns.
//!
//! The names and the order are Bluebeam's, read out of the user's own profile,
//! so a column he has shown in Revu is a column that exists here under the same
//! name and in the same place.

use crate::row::{Row, WeightColumns};
use annot::Kind;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Column {
    Subject,
    Label,
    Layer,
    Space,
    PageIndex,
    Lock,
    Page,
    Status,
    Checkmark,
    Colour,
    Author,
    Date,
    CreationDate,
    X,
    Y,
    Width,
    Height,
    Comments,
    Count,
    Length,
    Area,
    Volume,
    Depth,
    WallArea,
    MeasurementWidth,
    MeasurementHeight,
    Measurement,
    Sequence,
    Capture,
    Legend,
    RiseDrop,
    Slope,
    Units,
    View3D,
    UserDefined(u8),
    /// Hyperview's own: pounds and tons, worked from the unit weights in the user
    /// columns. Revu makes you write a formula for this.
    Pounds,
    Tons,
    /// Hyperview's own: where the markup is on the sheet's own grid. Revu
    /// throws the grid away the moment the sheet is on screen.
    Grid,
    /// Hyperview's own: how many of this markup there are — one line drawn
    /// for six identical beams says ×6 here, and every total counts six.
    Quantity,
}

/// Every column, in the order Revu lists them.
pub const ALL: &[Column] = &[
    Column::Subject,
    Column::Label,
    Column::Layer,
    Column::Space,
    Column::PageIndex,
    Column::Lock,
    Column::Page,
    Column::Status,
    Column::Checkmark,
    Column::Colour,
    Column::Author,
    Column::Date,
    Column::CreationDate,
    Column::X,
    Column::Y,
    Column::Width,
    Column::Height,
    Column::Comments,
    Column::Count,
    Column::Length,
    Column::Area,
    Column::Volume,
    Column::Depth,
    Column::WallArea,
    Column::MeasurementWidth,
    Column::MeasurementHeight,
    Column::Measurement,
    Column::Sequence,
    Column::Capture,
    Column::Legend,
    Column::RiseDrop,
    Column::Slope,
    Column::Units,
    Column::View3D,
    Column::UserDefined(0),
    Column::UserDefined(1),
    Column::UserDefined(2),
    Column::UserDefined(3),
    Column::UserDefined(4),
    Column::UserDefined(5),
    Column::Pounds,
    Column::Tons,
    Column::Grid,
    Column::Quantity,
];

impl Column {
    /// The key Bluebeam uses in a profile, so a profile's column settings map
    /// straight onto these.
    pub fn key(self) -> String {
        match self {
            Column::Subject => "Subject".into(),
            Column::Label => "Label".into(),
            Column::Layer => "Layer".into(),
            Column::Space => "Space".into(),
            Column::PageIndex => "Page Index".into(),
            Column::Lock => "Lock".into(),
            Column::Page => "Page".into(),
            Column::Status => "Status".into(),
            Column::Checkmark => "Checkmark".into(),
            Column::Colour => "Color".into(),
            Column::Author => "Author".into(),
            Column::Date => "Date".into(),
            Column::CreationDate => "Creation Date".into(),
            Column::X => "X".into(),
            Column::Y => "Y".into(),
            Column::Width => "Width".into(),
            Column::Height => "Height".into(),
            Column::Comments => "Comments".into(),
            Column::Count => "Count".into(),
            Column::Length => "Length".into(),
            Column::Area => "Area".into(),
            Column::Volume => "Volume".into(),
            Column::Depth => "Depth".into(),
            Column::WallArea => "Wall Area".into(),
            Column::MeasurementWidth => "Measurement Width".into(),
            Column::MeasurementHeight => "Measurement Height".into(),
            Column::Measurement => "Measurement".into(),
            Column::Sequence => "Sequence".into(),
            Column::Capture => "Capture".into(),
            Column::Legend => "Legend".into(),
            Column::RiseDrop => "RiseDrop".into(),
            Column::Slope => "Slope".into(),
            Column::Units => "Units".into(),
            Column::View3D => "View3D".into(),
            Column::UserDefined(n) => format!("UserDefined{n}"),
            Column::Pounds => "Pounds".into(),
            Column::Tons => "Tons".into(),
            Column::Grid => "Grid".into(),
            Column::Quantity => "Qty".into(),
        }
    }

    pub fn from_key(key: &str) -> Option<Column> {
        if let Some(n) = key.strip_prefix("UserDefined") {
            return n.parse::<u8>().ok().filter(|n| *n < 6).map(Column::UserDefined);
        }
        ALL.iter().copied().find(|c| c.key() == key)
    }

    /// What goes at the top of the column on screen.
    pub fn heading(self) -> String {
        match self {
            Column::UserDefined(n) => format!("Custom {}", n + 1),
            other => other.key(),
        }
    }

    /// True for columns that hold a quantity, so the grid can right align them
    /// and put a total at the foot.
    pub fn numeric(self) -> bool {
        matches!(
            self,
            Column::Count
                | Column::Length
                | Column::Area
                | Column::Volume
                | Column::Depth
                | Column::WallArea
                | Column::MeasurementWidth
                | Column::MeasurementHeight
                | Column::X
                | Column::Y
                | Column::Width
                | Column::Height
                | Column::Pounds
                | Column::Tons
                | Column::Quantity
        )
    }

    /// The raw number, for sorting and totalling. `None` for text columns and
    /// for a row whose sheet has no scale.
    pub fn number(self, row: &Row, weights: WeightColumns) -> Option<f64> {
        match self {
            Column::Count => Some(row.count),
            Column::Length => row.length,
            Column::Area => row.area,
            Column::Volume => row.volume,
            Column::Depth => row.depth,
            Column::WallArea => row.wall_area,
            Column::MeasurementWidth => row.scaled.then(|| {
                (row.area_box[2] - row.area_box[0])
                    * row.scale.as_ref().map(|s| s.per_point()).unwrap_or(0.0)
            }),
            Column::MeasurementHeight => row.scaled.then(|| {
                (row.area_box[3] - row.area_box[1])
                    * row.scale.as_ref().map(|s| s.per_point()).unwrap_or(0.0)
            }),
            Column::X => Some(row.area_box[0]),
            Column::Y => Some(row.area_box[1]),
            Column::Width => Some(row.area_box[2] - row.area_box[0]),
            Column::Height => Some(row.area_box[3] - row.area_box[1]),
            Column::Slope => row.slope,
            Column::Pounds => row.pounds(weights),
            Column::Tons => row.tons(weights),
            Column::Quantity => Some(row.quantity),
            Column::UserDefined(n) => row
                .columns
                .get(n as usize)
                .and_then(|c| c.trim().parse::<f64>().ok()),
            _ => None,
        }
    }

    /// What the cell shows. Measurements are formatted by the sheet's own
    /// scale, so a length reads `40'-0"` and not `40.00`.
    pub fn text(self, row: &Row, weights: WeightColumns) -> String {
        let scale = row.scale.as_ref();
        match self {
            Column::Subject => row.subject.clone(),
            Column::Label => row.label.clone(),
            Column::Page => format!("{}", row.page + 1),
            Column::PageIndex => format!("{}", row.page),
            Column::Author => row.author.clone(),
            Column::Comments => row.comments.clone(),
            Column::Status => row.status.clone(),
            Column::Date => tidy_date(&row.date),
            Column::CreationDate => tidy_date(&row.created),
            Column::Lock => yes_no(row.locked),
            Column::Colour => format!(
                "#{:02X}{:02X}{:02X}",
                (row.colour[0] * 255.0).round() as u8,
                (row.colour[1] * 255.0).round() as u8,
                (row.colour[2] * 255.0).round() as u8
            ),
            Column::Measurement => row.caption.clone(),
            Column::Units => row.unit.clone(),
            Column::Count => {
                if row.count == 0.0 {
                    String::new()
                } else {
                    format!("{:.0}", row.count)
                }
            }
            Column::Length | Column::Depth => match (row.scaled, self.number(row, weights), scale) {
                (true, Some(v), Some(s)) => s.length(v / s.per_point()),
                (false, _, _) => unscaled(row),
                _ => String::new(),
            },
            Column::Area | Column::WallArea => match (row.scaled, self.number(row, weights), scale) {
                (true, Some(v), Some(s)) => {
                    let per = s.per_point();
                    s.area(v / (per * per))
                }
                (false, _, _) => unscaled(row),
                _ => String::new(),
            },
            Column::Volume => match (row.scaled, row.volume, scale) {
                (true, Some(v), Some(s)) => {
                    let per = s.per_point();
                    s.volume(v / (per * per * per))
                }
                (false, _, _) => unscaled(row),
                _ => String::new(),
            },
            Column::MeasurementWidth | Column::MeasurementHeight => {
                match (self.number(row, weights), scale) {
                    (Some(v), Some(s)) => s.length(v / s.per_point()),
                    _ => unscaled(row),
                }
            }
            Column::Slope => match row.slope {
                Some(rise) => format!(
                    "{} in {}",
                    crate::row::quantity_text(rise),
                    crate::row::quantity_text(row.pitch_run)
                ),
                None => String::new(),
            },
            // How far the run climbs. The length is already measured up the
            // slope, so the climb is that length times the sine of the pitch.
            Column::RiseDrop => match (row.slope, row.length, scale) {
                (Some(rise), Some(along), Some(s)) if row.scaled => {
                    let climb = along * rise / rise.hypot(row.pitch_run);
                    s.length(climb / s.per_point())
                }
                _ => String::new(),
            },
            Column::Pounds => match row.pounds(weights) {
                Some(lb) => format!("{lb:.0}"),
                None if !row.scaled => unscaled(row),
                None => String::new(),
            },
            Column::Tons => match row.tons(weights) {
                Some(t) => format!("{t:.3}"),
                None if !row.scaled => unscaled(row),
                None => String::new(),
            },
            Column::Grid => row.grid.clone(),
            Column::Quantity => crate::row::quantity_text(row.quantity),
            Column::UserDefined(n) => row.columns.get(n as usize).cloned().unwrap_or_default(),
            Column::X | Column::Y | Column::Width | Column::Height => {
                match self.number(row, weights) {
                    Some(v) => format!("{v:.2}"),
                    None => String::new(),
                }
            }
            _ => String::new(),
        }
    }
}

/// What a cell says when the sheet it came from has no scale. Never a number,
/// and never blank enough to be mistaken for one.
fn unscaled(row: &Row) -> String {
    if row.kind.measures() && row.kind != Kind::Count {
        "no scale".into()
    } else {
        String::new()
    }
}

fn yes_no(value: bool) -> String {
    if value {
        "yes".into()
    } else {
        String::new()
    }
}

/// Turns `D:20260917120000Z` into `2026-09-17 12:00`.
fn tidy_date(raw: &str) -> String {
    let digits: String = raw
        .trim_start_matches("D:")
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.len() < 8 {
        return raw.to_string();
    }
    let date = format!("{}-{}-{}", &digits[0..4], &digits[4..6], &digits[6..8]);
    if digits.len() >= 12 {
        format!("{date} {}:{}", &digits[8..10], &digits[10..12])
    } else {
        date
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::measure;
    use annot::measure::imperial;
    use annot::{Markup, Subtype};
    use pdf::Ref;

    fn row(scaled: bool) -> Row {
        let mut m = Markup::new(Subtype::Line);
        m.set("IT", pdf::Object::name("LineDimension"));
        m.set_subject("W12x26");
        m.set(
            "BSIColumnData",
            pdf::Object::Array(vec![pdf::Object::text("26.00")]),
        );
        m.set_line([0.0, 0.0], [720.0, 0.0]);
        let scale = imperial(48.0, "1/4\" = 1'-0\"", 16);
        measure(0, Ref::new(1, 0), &m, scaled.then_some(&scale))
    }

    #[test]
    fn a_length_cell_reads_the_way_the_drawing_does() {
        let r = row(true);
        assert_eq!(Column::Length.text(&r, Default::default()), "40'-0\"");
        assert_eq!(Column::Subject.text(&r, Default::default()), "W12x26");
        assert_eq!(Column::Page.text(&r, Default::default()), "1");
        assert_eq!(Column::Count.text(&r, Default::default()), "1");
    }

    #[test]
    fn weight_and_tonnage_come_from_the_unit_weight_column() {
        let r = row(true);
        assert_eq!(Column::Pounds.text(&r, Default::default()), "1040");
        assert_eq!(Column::Tons.text(&r, Default::default()), "0.520");
        assert_eq!(Column::UserDefined(0).text(&r, Default::default()), "26.00");
    }

    #[test]
    fn an_unscaled_row_says_so_in_every_measured_column() {
        let r = row(false);
        assert_eq!(Column::Length.text(&r, Default::default()), "no scale");
        assert_eq!(Column::Pounds.text(&r, Default::default()), "no scale");
        assert_eq!(Column::Length.number(&r, Default::default()), None);
        // The pick itself still counts.
        assert_eq!(Column::Count.text(&r, Default::default()), "1");
    }

    #[test]
    fn every_column_maps_to_and_from_the_key_a_profile_uses() {
        for column in ALL {
            let key = column.key();
            assert_eq!(Column::from_key(&key), Some(*column), "{key}");
        }
        assert_eq!(Column::from_key("Page Index"), Some(Column::PageIndex));
        assert_eq!(Column::from_key("UserDefined3"), Some(Column::UserDefined(3)));
        assert_eq!(Column::from_key("UserDefined9"), None);
        assert_eq!(Column::from_key("Nonsense"), None);
    }

    #[test]
    fn the_column_set_covers_what_his_profile_shows() {
        // The columns he keeps visible in Revu.
        for key in [
            "Subject", "Lock", "Page", "Color", "Author", "Count", "Length", "Area",
            "UserDefined0", "UserDefined5",
        ] {
            assert!(Column::from_key(key).is_some(), "{key}");
        }
    }

    #[test]
    fn a_pdf_date_is_shown_the_way_people_write_dates() {
        assert_eq!(tidy_date("D:20260917120000Z"), "2026-09-17 12:00");
        assert_eq!(tidy_date("D:20260917"), "2026-09-17");
        assert_eq!(tidy_date(""), "");
    }
}
