//! Steel sizes as a drawing writes them, and what they weigh.
//!
//! A framing plan says what every member is — `W16X31`, `HSS6x6x1/4`,
//! `L4x4x1/2`, `PIPE 6 STD` — in text beside the line. Reading it back is how a
//! takeoff can be checked against the drawing it came from (the tool says
//! W16x26, the drawing says W16X31) and how a length can be sized from the
//! drawing instead of from whichever tool happened to be in hand.
//!
//! What a size weighs comes from the size itself, never from a guess:
//!
//! - **W, S, M, HP, C, MC, WT, MT, ST** name their weight: the second number in
//!   `W16X31` *is* 31 lb/ft, which is AISC's nominal weight to the digit.
//! - **Angles, HSS, pipe and plate** are worked out from their dimensions the
//!   way AISC works them out — steel at 490 lb/ft³, angles as two legs less the
//!   corner, rectangular HSS with 2t corners, pipe from the ASME wall
//!   thicknesses — which lands within about one percent of the AISC tables.
//!
//! Every weight says which of the two it is, so nobody mistakes a computed
//! weight for a tabulated one, and a weight the office's own tool chest carries
//! always wins over both.

use regex::Regex;
use std::sync::OnceLock;

/// Pounds per foot of steel one square inch in section: 490 lb/ft³ ÷ 144.
pub const LB_PER_FT_PER_SQIN: f64 = 490.0 / 144.0;
/// Pounds per square foot of steel plate one inch thick.
pub const LB_PER_SQFT_PER_IN: f64 = 490.0 / 12.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// W, S, M, HP, C, MC, WT, MT, ST: weight in the name.
    Rolled,
    Angle,
    /// Two angles back to back.
    DoubleAngle,
    TubeRect,
    TubeRound,
    Pipe,
    Plate,
}

/// Where a weight came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weighed {
    /// Written in the designation, AISC's nominal weight.
    Named,
    /// Worked out from the designation's dimensions.
    Computed,
}

impl Weighed {
    pub fn says(self) -> &'static str {
        match self {
            Weighed::Named => "AISC nominal weight, from the size",
            Weighed::Computed => "worked out from the size's dimensions",
        }
    }
}

/// A size, read.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub family: Family,
    /// The size the way AISC writes it: `W16X31`, `HSS6X6X1/4`, `L4X4X1/2`.
    pub designation: String,
    /// Pounds per foot, for anything bought by the foot.
    pub per_foot: Option<f64>,
    /// Pounds per square foot, for plate given only a thickness.
    pub per_square_foot: Option<f64>,
    pub weighed: Weighed,
}

impl Shape {
    /// Whether two sizes are the same member, however each was written.
    pub fn same_as(&self, other: &Shape) -> bool {
        self.designation == other.designation
    }
}

fn patterns() -> &'static [(Family, Regex)] {
    static PATTERNS: OnceLock<Vec<(Family, Regex)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        const N: &str = r"(\d+(?:\.\d+)?)";
        // A dimension in inches: 4, 4.5, 3/8, 1/2, 1-1/2, 1 1/2.
        // Fractions first: "1/2" read as a whole number would stop at the 1.
        const D: &str = r"(\d+[- ]\d+/\d+|\d+/\d+|\d+(?:\.\d+)?)";
        const X: &str = r"\s*[X×]\s*";
        let rx = |s: String| Regex::new(&s).expect("a valid pattern");
        vec![
            (Family::DoubleAngle, rx(format!(r"\b2L\s*{D}{X}{D}{X}{D}"))),
            (Family::Angle, rx(format!(r"\bL\s*{D}{X}{D}{X}{D}"))),
            (Family::TubeRect, rx(format!(r"\b(?:HSS|TS)\s*{D}{X}{D}{X}{D}"))),
            (Family::TubeRound, rx(format!(r"\bHSS\s*{D}{X}{D}"))),
            (Family::Pipe, rx(r#"\b(?:PIPE\s*(\d+[- ]\d+/\d+|\d+/\d+|\d+)"?\s*(STD|XXS|XS|X-STRONG|SCH\.?\s*40|SCH\.?\s*80)|(\d+[- ]\d+/\d+|\d+/\d+|\d+)"?\s*(STD|XXS|XS)\.?\s*PIPE)"#.to_string())),
            (Family::Rolled, rx(format!(r"\b(W|S|M|HP|MC|C|WT|MT|ST)\s*{N}{X}{N}"))),
            (Family::Plate, rx(format!(r"\bPL\s*{D}(?:{X}{D})?"))),
        ]
    })
}

/// Every size written in a piece of text, in the order they appear.
pub fn sizes_in(text: &str) -> Vec<Shape> {
    // Every size has a digit in it and an X, a PL or a PIPE. Most text on a
    // drawing has neither, and a plugin runs in an interpreter, where not
    // running seven patterns over a word is most of what fast means.
    if !text.bytes().any(|b| b.is_ascii_digit()) {
        return Vec::new();
    }
    let upper = text.to_uppercase().replace('\u{2033}', "\"").replace('”', "\"");
    if !(upper.contains('X') || upper.contains('×') || upper.contains("PL") || upper.contains("PIPE")) {
        return Vec::new();
    }
    let mut found: Vec<(usize, Shape)> = Vec::new();
    let mut taken: Vec<(usize, usize)> = Vec::new();
    for (family, pattern) in patterns() {
        for caps in pattern.captures_iter(&upper) {
            let whole = caps.get(0).expect("group 0");
            if taken.iter().any(|(a, b)| whole.start() < *b && *a < whole.end()) {
                continue;
            }
            let groups: Vec<&str> = (1..caps.len())
                .filter_map(|i| caps.get(i).map(|m| m.as_str()))
                .collect();
            if let Some(shape) = build(*family, &groups) {
                taken.push((whole.start(), whole.end()));
                found.push((whole.start(), shape));
            }
        }
    }
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().map(|(_, s)| s).collect()
}

/// The size a piece of text names, when it names exactly one thing first.
pub fn size_of(text: &str) -> Option<Shape> {
    sizes_in(text).into_iter().next()
}

fn build(family: Family, g: &[&str]) -> Option<Shape> {
    match family {
        Family::Rolled => {
            let (kind, depth, weight) = (g.first()?, number(g.get(1)?)?, number(g.get(2)?)?);
            // A depth and a weight that no rolled shape has is text that only
            // looks like one: a note that says "C 3 X 4 SPACES".
            let depth_ok = (2.0..=45.0).contains(&depth);
            let weight_ok = (1.0..=1000.0).contains(&weight);
            if !depth_ok || !weight_ok {
                return None;
            }
            Some(Shape {
                family,
                designation: format!("{kind}{}X{}", tidy(g[1]), tidy(g[2])),
                per_foot: Some(weight),
                per_square_foot: None,
                weighed: Weighed::Named,
            })
        }
        Family::Angle | Family::DoubleAngle => {
            let (a, b, t) = (inches(g.first()?)?, inches(g.get(1)?)?, inches(g.get(2)?)?);
            if !(1.0..=12.0).contains(&a) || !(1.0..=12.0).contains(&b) || !(0.1..=1.5).contains(&t) {
                return None;
            }
            let one = LB_PER_FT_PER_SQIN * t * (a + b - t);
            let (per, prefix) = if family == Family::DoubleAngle { (one * 2.0, "2L") } else { (one, "L") };
            Some(Shape {
                family,
                designation: format!("{prefix}{}X{}X{}", tidy(g[0]), tidy(g[1]), tidy(g[2])),
                per_foot: Some(round1(per)),
                per_square_foot: None,
                weighed: Weighed::Computed,
            })
        }
        Family::TubeRect => {
            let (h, b, t) = (inches(g.first()?)?, inches(g.get(1)?)?, inches(g.get(2)?)?);
            if !(1.0..=40.0).contains(&h) || !(1.0..=40.0).contains(&b) || !(0.1..=1.0).contains(&t) {
                return None;
            }
            // Outside corner radius 2t, inside t: the square corners less what
            // the rounding takes off.
            let area = 2.0 * t * (h + b - 2.0 * t)
                - (4.0 - std::f64::consts::PI) * ((2.0 * t).powi(2) - t * t);
            Some(Shape {
                family,
                designation: format!("HSS{}X{}X{}", tidy(g[0]), tidy(g[1]), tidy(g[2])),
                per_foot: Some(round2(LB_PER_FT_PER_SQIN * area)),
                per_square_foot: None,
                weighed: Weighed::Computed,
            })
        }
        Family::TubeRound => {
            let (d, t) = (inches(g.first()?)?, inches(g.get(1)?)?);
            if !(1.0..=30.0).contains(&d) || !(0.05..=1.0).contains(&t) || t * 2.0 >= d {
                return None;
            }
            Some(Shape {
                family,
                designation: format!("HSS{}X{}", tidy(g[0]), tidy(g[1])),
                per_foot: Some(round2(round_tube(d, t))),
                per_square_foot: None,
                weighed: Weighed::Computed,
            })
        }
        Family::Pipe => {
            let (size, grade) = match (g.first(), g.get(1)) {
                (Some(s), Some(k)) => (*s, *k),
                _ => return None,
            };
            let nps = inches(size)?;
            let grade = match grade.replace(['.', ' ', '-'], "").as_str() {
                "STD" | "SCH40" => "STD",
                "XS" | "XSTRONG" | "SCH80" => "XS",
                "XXS" => "XXS",
                _ => return None,
            };
            let (od, wall) = pipe(nps, grade)?;
            Some(Shape {
                family,
                designation: format!("PIPE{}{}", tidy(size), grade),
                per_foot: Some(round2(round_tube(od, wall))),
                per_square_foot: None,
                weighed: Weighed::Computed,
            })
        }
        Family::Plate => {
            let t = inches(g.first()?)?;
            if !(0.0625..=12.0).contains(&t) {
                return None;
            }
            let width = g.get(1).and_then(|w| inches(w));
            Some(Shape {
                family,
                designation: match width {
                    Some(_) => format!("PL{}X{}", tidy(g[0]), tidy(g[1])),
                    None => format!("PL{}", tidy(g[0])),
                },
                per_foot: width.map(|w| round2(LB_PER_FT_PER_SQIN * t * w)),
                per_square_foot: Some(round2(LB_PER_SQFT_PER_IN * t)),
                weighed: Weighed::Computed,
            })
        }
    }
}

/// Round HSS and pipe: π t (D − t) square inches.
fn round_tube(d: f64, t: f64) -> f64 {
    LB_PER_FT_PER_SQIN * std::f64::consts::PI * t * (d - t)
}

/// ASME B36.10 outside diameters and walls, standard, extra strong and double
/// extra strong.
fn pipe(nps: f64, grade: &str) -> Option<(f64, f64)> {
    const TABLE: &[(f64, f64, f64, f64, f64)] = &[
        // nps, OD, STD, XS, XXS (0 = none)
        (0.5, 0.840, 0.109, 0.147, 0.294),
        (0.75, 1.050, 0.113, 0.154, 0.308),
        (1.0, 1.315, 0.133, 0.179, 0.358),
        (1.25, 1.660, 0.140, 0.191, 0.382),
        (1.5, 1.900, 0.145, 0.200, 0.400),
        (2.0, 2.375, 0.154, 0.218, 0.436),
        (2.5, 2.875, 0.203, 0.276, 0.552),
        (3.0, 3.500, 0.216, 0.300, 0.600),
        (3.5, 4.000, 0.226, 0.318, 0.0),
        (4.0, 4.500, 0.237, 0.337, 0.674),
        (5.0, 5.563, 0.258, 0.375, 0.750),
        (6.0, 6.625, 0.280, 0.432, 0.864),
        (8.0, 8.625, 0.322, 0.500, 0.875),
        (10.0, 10.750, 0.365, 0.500, 0.0),
        (12.0, 12.750, 0.375, 0.500, 0.0),
    ];
    let row = TABLE.iter().find(|r| (r.0 - nps).abs() < 1e-6)?;
    let wall = match grade {
        "STD" => row.2,
        "XS" => row.3,
        _ => row.4,
    };
    (wall > 0.0).then_some((row.1, wall))
}

fn number(text: &str) -> Option<f64> {
    text.trim().parse().ok()
}

/// "1-1/2", "1 1/2", "3/8", "4", "4.5" as inches.
pub fn inches(text: &str) -> Option<f64> {
    let text = text.trim();
    let (whole, fraction) = match text.split_once(['-', ' ']) {
        Some((w, f)) if f.contains('/') => (w, Some(f)),
        _ if text.contains('/') => ("0", Some(text)),
        _ => (text, None),
    };
    let mut value: f64 = whole.trim().parse().ok()?;
    if let Some(f) = fraction {
        let (n, d) = f.split_once('/')?;
        let (n, d): (f64, f64) = (n.trim().parse().ok()?, d.trim().parse().ok()?);
        if d == 0.0 {
            return None;
        }
        value += n / d;
    }
    Some(value)
}

/// How a dimension is written in a designation: "1-1/2" as AISC writes it,
/// "4.5" as it was written.
fn tidy(text: &str) -> String {
    text.trim().replace(' ', "-")
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

// ---- finding the size beside a member ---------------------------------------

/// A piece of text on a sheet, and where: `[x0, y0, x1, y1]` in sheet points.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub text: String,
    pub at: [f64; 4],
}

/// The nearest size written within `reach` points of a member drawn along
/// `line`, and how far it is. "Beside" means measured to the middle of the
/// text from the nearest point on the member.
pub fn beside(words: &[Placed], line: &[[f64; 2]], reach: f64) -> Option<(Shape, f64)> {
    if line.len() < 2 {
        return None;
    }
    let mut best: Option<(Shape, f64)> = None;
    for word in words {
        let centre = [(word.at[0] + word.at[2]) * 0.5, (word.at[1] + word.at[3]) * 0.5];
        let distance = line
            .windows(2)
            .map(|pair| to_segment(centre, pair[0], pair[1]))
            .fold(f64::INFINITY, f64::min);
        if distance > reach || best.as_ref().is_some_and(|(_, d)| *d <= distance) {
            continue;
        }
        if let Some(shape) = size_of(&word.text) {
            best = Some((shape, distance));
        }
    }
    best
}

/// Distance from a point to a segment.
pub fn to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length).clamp(0.0, 1.0)
    };
    let (x, y) = (a[0] + t * dx, a[1] + t * dy);
    ((p[0] - x).powi(2) + (p[1] - y).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weight(text: &str) -> f64 {
        size_of(text).and_then(|s| s.per_foot).unwrap_or(-1.0)
    }

    #[test]
    fn a_rolled_shape_weighs_what_its_name_says() {
        let s = size_of("W16X31").unwrap();
        assert_eq!(s.designation, "W16X31");
        assert_eq!(s.per_foot, Some(31.0));
        assert_eq!(s.weighed, Weighed::Named);
        assert_eq!(size_of("w 12 x 26 (typ)").unwrap().designation, "W12X26");
        assert_eq!(size_of("C10X15.3").unwrap().per_foot, Some(15.3));
        assert_eq!(size_of("MC12X10.6").unwrap().designation, "MC12X10.6");
        assert_eq!(size_of("WT6X13").unwrap().per_foot, Some(13.0));
        assert_eq!(size_of("HP14X73").unwrap().per_foot, Some(73.0));
    }

    #[test]
    fn angles_and_tubes_land_within_a_percent_of_the_aisc_tables() {
        // AISC v16: L4x4x1/2 12.8, L3x3x1/4 4.9, HSS6x6x1/4 19.02,
        // HSS8x4x3/8 27.48, Pipe 6 Std 18.97, HSS6.625x0.280 19.02 (nominal t).
        let close = |got: f64, table: f64| (got - table).abs() / table < 0.015;
        assert!(close(weight("L4X4X1/2"), 12.8), "{}", weight("L4X4X1/2"));
        assert!(close(weight("L3x3x1/4"), 4.9), "{}", weight("L3x3x1/4"));
        assert!(close(weight("HSS6X6X1/4"), 19.02), "{}", weight("HSS6X6X1/4"));
        assert!(close(weight("HSS8x4x3/8"), 27.48), "{}", weight("HSS8x4x3/8"));
        assert!(close(weight("PIPE 6 STD"), 18.97), "{}", weight("PIPE 6 STD"));
        assert!(close(weight("6\" STD PIPE"), 18.97), "{}", weight("6\" STD PIPE"));
        assert!(close(weight("HSS6.625X0.280"), 19.02), "{}", weight("HSS6.625X0.280"));
        assert!(close(weight("2L4X4X1/2"), 25.6), "{}", weight("2L4X4X1/2"));
        assert_eq!(size_of("TS6X6X1/4").unwrap().designation, "HSS6X6X1/4");
    }

    #[test]
    fn plate_by_the_foot_and_by_the_square_foot() {
        let bar = size_of("PL1/2X6").unwrap();
        assert_eq!(bar.per_foot, Some(10.21));
        let sheet = size_of("PL 3/4").unwrap();
        assert_eq!(sheet.per_foot, None);
        assert_eq!(sheet.per_square_foot, Some(30.63));
        assert_eq!(size_of("PL1-1/2X12").unwrap().designation, "PL1-1/2X12");
    }

    #[test]
    fn text_that_only_looks_like_a_size_is_not_one() {
        assert!(size_of("SEE 3/S-501").is_none());
        assert!(size_of("W 900 X 12").is_none(), "no rolled shape is 900 deep");
        assert!(size_of("L4X4X6").is_none(), "no angle is six inches thick");
    }

    #[test]
    fn several_sizes_in_one_note_come_out_in_order() {
        let all = sizes_in("W12X26 W/ L4X4X3/8 CLIPS EA END");
        let names: Vec<String> = all.iter().map(|s| s.designation.clone()).collect();
        assert_eq!(names, vec!["W12X26", "L4X4X3/8"]);
    }

    #[test]
    fn the_size_beside_a_beam_is_the_nearest_one_within_reach() {
        let words = vec![
            Placed { text: "W16X31".into(), at: [100.0, 88.0, 160.0, 96.0] },
            Placed { text: "W24X55".into(), at: [100.0, 300.0, 160.0, 308.0] },
            Placed { text: "GRID B".into(), at: [100.0, 101.0, 140.0, 108.0] },
        ];
        let beam = [[50.0, 100.0], [400.0, 100.0]];
        let (shape, distance) = beside(&words, &beam, 30.0).unwrap();
        assert_eq!(shape.designation, "W16X31");
        assert!(distance < 10.0);
        assert!(beside(&words, &[[50.0, 500.0], [400.0, 500.0]], 30.0).is_none());
    }
}
