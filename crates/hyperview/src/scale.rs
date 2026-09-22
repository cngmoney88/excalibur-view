//! Drawing scale: the bridge between points on the sheet and feet in the field.
//!
//! Nothing here guesses. A scale is either one the user picked from the list or
//! one they calibrated against a dimension they read off the drawing, and what
//! gets written into the file is a real PDF `/Measure` dictionary on the page's
//! viewport — the same place Revu keeps it, so Revu reads it back.

use annot::measure::{imperial, metric, Measure};

use crate::units::Units;

/// The scales that appear on real construction documents, in the order a
/// drafter would look for them. The number is real inches of building per inch
/// of paper: 48 for quarter inch scale.
pub const IMPERIAL: &[(&str, f64)] = &[
    ("3\" = 1'-0\"", 4.0),
    ("1 1/2\" = 1'-0\"", 8.0),
    ("1\" = 1'-0\"", 12.0),
    ("3/4\" = 1'-0\"", 16.0),
    ("1/2\" = 1'-0\"", 24.0),
    ("3/8\" = 1'-0\"", 32.0),
    ("1/4\" = 1'-0\"", 48.0),
    ("3/16\" = 1'-0\"", 64.0),
    ("1/8\" = 1'-0\"", 96.0),
    ("3/32\" = 1'-0\"", 128.0),
    ("1/16\" = 1'-0\"", 192.0),
    ("1\" = 10'", 120.0),
    ("1\" = 20'", 240.0),
    ("1\" = 30'", 360.0),
    ("1\" = 40'", 480.0),
    ("1\" = 50'", 600.0),
    ("1\" = 60'", 720.0),
    ("1\" = 100'", 1200.0),
];

/// Metric scales, as their ratio: 1:50 is fifty.
pub const METRIC: &[(&str, f64)] = &[
    ("1:10", 10.0),
    ("1:20", 20.0),
    ("1:25", 25.0),
    ("1:50", 50.0),
    ("1:75", 75.0),
    ("1:100", 100.0),
    ("1:200", 200.0),
    ("1:500", 500.0),
];

pub fn presets(units: Units) -> &'static [(&'static str, f64)] {
    if units.is_metric() {
        METRIC
    } else {
        IMPERIAL
    }
}

/// A named scale from the list.
pub fn named(label: &str, ratio: f64, units: Units, denominator: u32) -> Measure {
    if units.is_metric() {
        metric(ratio, label)
    } else {
        imperial(ratio, label, denominator)
    }
}

/// Built from two clicks a known distance apart. `real_inches` is what the
/// dimension on the drawing reads; `points` is how far apart the two clicks
/// were on the sheet.
///
/// The label says outright that it was calibrated, and how far off a named
/// scale it landed, because a takeoff that came from a rubber ruler should say
/// so on its face.
pub fn from_calibration(
    points: f64,
    real_inches: f64,
    units: Units,
    denominator: u32,
) -> Option<Measure> {
    if points <= 1e-6 || real_inches <= 0.0 {
        return None;
    }
    // Real inches of building per inch of paper.
    let ratio = real_inches / points * 72.0;
    let label = match nearest_named(ratio) {
        Some((name, named_ratio)) => {
            let drift = (ratio / named_ratio - 1.0).abs();
            if drift < 0.01 {
                format!("calibrated, {name}")
            } else {
                format!("calibrated ({:.1}% off {name})", drift * 100.0)
            }
        }
        None => "calibrated".to_string(),
    };
    Some(named(&label, ratio, units, denominator))
}

/// Finds the named scale a calibration landed closest to, so the program can
/// tell the user "that came out to 1/4 inch" instead of a bare decimal.
pub fn nearest_named(ratio: f64) -> Option<(&'static str, f64)> {
    IMPERIAL
        .iter()
        .chain(METRIC.iter())
        .map(|(name, r)| (*name, *r))
        .min_by(|a, b| {
            let da = (ratio / a.1).ln().abs();
            let db = (ratio / b.1).ln().abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .filter(|(_, r)| (ratio / r).ln().abs() < 0.08)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quarter_inch_scale_turns_five_inches_of_paper_into_twenty_feet() {
        let m = named("1/4\" = 1'-0\"", 48.0, Units::FeetInches, 16);
        assert_eq!(m.length(360.0), "20'-0\"");
    }

    #[test]
    fn calibrating_on_a_known_dimension_names_the_scale_it_matches() {
        // 20'-0" measured across 5 inches of paper is 1/4" = 1'-0".
        let m = from_calibration(360.0, 240.0, Units::FeetInches, 16).unwrap();
        assert_eq!(m.ratio, "calibrated, 1/4\" = 1'-0\"");
        assert_eq!(m.length(360.0), "20'-0\"");
    }

    #[test]
    fn a_calibration_a_little_off_a_named_scale_says_how_far_off_it_is() {
        // 5 inches of paper reading 19'-2" is about 4% off quarter inch scale.
        let m = from_calibration(360.0, 230.0, Units::FeetInches, 16).unwrap();
        assert!(m.ratio.contains("off 1/4"), "{}", m.ratio);
    }

    #[test]
    fn a_calibration_near_no_named_scale_claims_nothing() {
        let m = from_calibration(360.0, 191.0, Units::FeetInches, 16).unwrap();
        assert_eq!(m.ratio, "calibrated");
    }

    #[test]
    fn a_calibration_of_zero_length_is_refused() {
        assert!(from_calibration(0.0, 240.0, Units::FeetInches, 16).is_none());
        assert!(from_calibration(360.0, 0.0, Units::FeetInches, 16).is_none());
    }

    #[test]
    fn a_metric_scale_reads_in_millimetres() {
        let m = named("1:50", 50.0, Units::Millimeters, 16);
        // One inch of paper at 1:50 is 50 inches of building: 1270 mm.
        assert_eq!(m.length(72.0), "1270 mm");
    }
}
