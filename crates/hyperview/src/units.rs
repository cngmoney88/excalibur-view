//! Architectural / engineering length units.
//!
//! Everything inside the program is kept in **inches** as an `f64`. Feet-inches
//! is only ever a presentation format, which is what keeps `12'-3 1/2"` from
//! accumulating rounding error across a long chain of measurements.

use std::fmt::Write as _;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Units {
    /// 12'-3 1/2"
    FeetInches,
    /// 12.29'
    Feet,
    /// 147.5"
    Inches,
    Millimeters,
    Meters,
}

impl Units {
    pub const ALL: [Units; 5] = [
        Units::FeetInches,
        Units::Feet,
        Units::Inches,
        Units::Millimeters,
        Units::Meters,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Units::FeetInches => "feet & inches",
            Units::Feet => "decimal feet",
            Units::Inches => "inches",
            Units::Millimeters => "millimeters",
            Units::Meters => "meters",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Units::FeetInches => "ft-in",
            Units::Feet => "ft",
            Units::Inches => "in",
            Units::Millimeters => "mm",
            Units::Meters => "m",
        }
    }

    pub fn is_metric(self) -> bool {
        matches!(self, Units::Millimeters | Units::Meters)
    }

    /// An example of what somebody can type, in these units.
    pub fn hint(self) -> &'static str {
        match self {
            Units::FeetInches => "8'-6\", or 8' 6 1/2\"",
            Units::Feet => "8.5",
            Units::Inches => "102",
            Units::Millimeters => "2600",
            Units::Meters => "2.6",
        }
    }

    /// Reads what somebody typed, in inches, which is the unit every length
    /// in this program is kept in.
    pub fn parse(self, typed: &str) -> Option<f64> {
        parse_length(typed, self)
    }
}

pub const MM_PER_INCH: f64 = 25.4;

/// Rounds `value` to the nearest `1/denominator`.
fn snap(value: f64, denominator: u32) -> f64 {
    let d = denominator.max(1) as f64;
    (value * d).round() / d
}

fn gcd(a: u32, b: u32) -> u32 {
    let (mut a, mut b) = (a, b);
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a.max(1)
}

/// `147.5` inches at a denominator of 16 becomes `12'-3 1/2"`.
pub fn format_feet_inches(inches: f64, denominator: u32) -> String {
    let negative = inches < 0.0;
    let total = snap(inches.abs(), denominator);
    let feet = (total / 12.0).floor();
    let rest = total - feet * 12.0;
    // Rounding can push the remainder up to a full foot.
    let (feet, rest) = if (rest - 12.0).abs() < 1e-9 {
        (feet + 1.0, 0.0)
    } else {
        (feet, rest)
    };
    let whole = rest.floor();
    let frac = rest - whole;

    let mut out = String::new();
    if negative {
        out.push('-');
    }
    let _ = write!(out, "{}'-{}", feet as i64, whole as i64);

    let d = denominator.max(1);
    let n = (frac * d as f64).round() as u32;
    if n > 0 {
        let g = gcd(n, d);
        let _ = write!(out, " {}/{}", n / g, d / g);
    }
    out.push('"');
    out
}

/// Formats a length held in inches for display in `units`.
pub fn format(inches: f64, units: Units, denominator: u32) -> String {
    match units {
        Units::FeetInches => format_feet_inches(inches, denominator),
        Units::Feet => format!("{:.3}'", inches / 12.0),
        Units::Inches => format!("{:.3}\"", inches),
        Units::Millimeters => format!("{:.1} mm", inches * MM_PER_INCH),
        Units::Meters => format!("{:.3} m", inches * MM_PER_INCH / 1000.0),
    }
}

/// Formats an area held in square inches.
pub fn format_area(square_inches: f64, units: Units) -> String {
    match units {
        Units::FeetInches | Units::Feet | Units::Inches => {
            format!("{:.2} SF", square_inches / 144.0)
        }
        Units::Millimeters | Units::Meters => {
            let m2 = square_inches * (MM_PER_INCH / 1000.0) * (MM_PER_INCH / 1000.0);
            format!("{:.3} m²", m2)
        }
    }
}

fn parse_number(s: &str) -> Option<f64> {
    // "6", "6.5", "6 1/2", "1/2"
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut total = 0.0;
    let mut saw_any = false;
    for part in s.split_whitespace() {
        if let Some((n, d)) = part.split_once('/') {
            let n: f64 = n.trim().parse().ok()?;
            let d: f64 = d.trim().parse().ok()?;
            if d == 0.0 {
                return None;
            }
            total += n / d;
        } else {
            total += part.parse::<f64>().ok()?;
        }
        saw_any = true;
    }
    saw_any.then_some(total)
}

/// Parses what a person would actually type into a dimension box.
///
/// Accepts `20'`, `20'-6"`, `20' 6 1/2"`, `20-6`, `6 1/2"`, `246`, `1200mm`,
/// `1.2 m`. A bare number is read as `default`.
pub fn parse_length(input: &str, default: Units) -> Option<f64> {
    let raw = input.trim().to_ascii_lowercase().replace(',', "");
    if raw.is_empty() {
        return None;
    }
    // Normalize the typographic quotes people paste out of Word and Bluebeam.
    let raw = raw
        .replace('\u{2019}', "'")
        .replace('\u{2032}', "'")
        .replace('\u{201d}', "\"")
        .replace('\u{2033}', "\"");

    if let Some(rest) = raw.strip_suffix("mm") {
        return parse_number(rest).map(|v| v / MM_PER_INCH);
    }
    if let Some(rest) = raw.strip_suffix("cm") {
        return parse_number(rest).map(|v| v * 10.0 / MM_PER_INCH);
    }
    if let Some(rest) = raw.strip_suffix('m') {
        return parse_number(rest).map(|v| v * 1000.0 / MM_PER_INCH);
    }
    if let Some(rest) = raw.strip_suffix("ft") {
        return parse_number(rest).map(|v| v * 12.0);
    }
    if let Some(rest) = raw.strip_suffix("in") {
        return parse_number(rest);
    }

    if let Some(tick) = raw.find('\'') {
        let feet = parse_number(&raw[..tick])?;
        let mut rest = raw[tick + 1..].trim();
        rest = rest.trim_start_matches('-').trim();
        let rest = rest.trim_end_matches('"').trim();
        let inches = if rest.is_empty() { 0.0 } else { parse_number(rest)? };
        return Some(feet * 12.0 + inches);
    }

    if let Some(rest) = raw.strip_suffix('"') {
        return parse_number(rest);
    }

    // "20-6" means twenty feet six inches to anybody holding a tape measure,
    // but only when both halves are whole numbers and the second is < 12.
    if let Some((a, b)) = raw.split_once('-') {
        if let (Some(f), Some(i)) = (parse_number(a), parse_number(b)) {
            if i >= 0.0 && i < 12.0 && a.trim().parse::<i64>().is_ok() {
                return Some(f * 12.0 + i);
            }
        }
    }

    let n = parse_number(&raw)?;
    Some(match default {
        Units::FeetInches | Units::Inches => n,
        Units::Feet => n * 12.0,
        Units::Millimeters => n / MM_PER_INCH,
        Units::Meters => n * 1000.0 / MM_PER_INCH,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tape_measure_reading_comes_back_in_inches() {
        assert_eq!(parse_length("20'", Units::FeetInches), Some(240.0));
        assert_eq!(parse_length("20'-6\"", Units::FeetInches), Some(246.0));
        assert_eq!(parse_length("20' 6 1/2\"", Units::FeetInches), Some(246.5));
        assert_eq!(parse_length("20-6", Units::FeetInches), Some(246.0));
        assert_eq!(parse_length("6 1/2\"", Units::FeetInches), Some(6.5));
        assert_eq!(parse_length("246", Units::Inches), Some(246.0));
        assert_eq!(parse_length("1200mm", Units::FeetInches), Some(1200.0 / 25.4));
    }

    #[test]
    fn feet_and_inches_read_back_the_way_they_were_typed() {
        assert_eq!(format_feet_inches(246.5, 16), "20'-6 1/2\"");
        assert_eq!(format_feet_inches(240.0, 16), "20'-0\"");
        assert_eq!(format_feet_inches(0.0, 16), "0'-0\"");
        assert_eq!(format_feet_inches(11.96875, 16), "1'-0\"");
        assert_eq!(format_feet_inches(147.5, 16), "12'-3 1/2\"");
        assert_eq!(format_feet_inches(-18.25, 16), "-1'-6 1/4\"");
    }

    #[test]
    fn eighths_are_not_reported_as_sixteenths() {
        assert_eq!(format_feet_inches(12.25, 16), "1'-0 1/4\"");
        assert_eq!(format_feet_inches(12.0625, 16), "1'-0 1/16\"");
    }

    #[test]
    fn nonsense_is_rejected_rather_than_guessed_at() {
        assert_eq!(parse_length("", Units::FeetInches), None);
        assert_eq!(parse_length("about twenty", Units::FeetInches), None);
        assert_eq!(parse_length("6/0", Units::FeetInches), None);
    }
}
