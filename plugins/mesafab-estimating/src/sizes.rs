//! Sizes, typicals and pitches, read off the drawing beside each member.
//!
//! The takeoff says what a length is and how many; the drawing says it too,
//! in the text beside the line. Where they disagree, one of them is wrong,
//! and it is usually the takeoff: the W16x26 tool was in hand when the
//! W16x31 was clicked, the TYP. OF 4 was counted once, the rafter was
//! measured flat.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use plugin_api::{Action, Finding, Input, Level, Markup, Output, Table};
use regex::Regex;

use crate::callout::{self, Placed, Shape};
use crate::geo;

/// How far from a member its callout may be, in points of paper.
const REACH: f64 = 40.0;
/// How far a TYP. or a pitch note may be.
const NOTE_REACH: f64 = 60.0;

/// Which custom column holds pounds per foot, by its name.
pub fn weight_column(columns: &[String]) -> Option<usize> {
    let key = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase();
    const PER_FOOT: &[&str] = &["LBSPERFT", "LBPERFT", "LBSFT", "LBFT", "PLF", "WEIGHTPERFT", "WTPERFT", "UNITWEIGHT", "WEIGHT", "LBS"];
    PER_FOOT
        .iter()
        .find_map(|want| columns.iter().position(|c| key(c) == *want))
}

pub fn placed(words: &[plugin_api::Word]) -> Vec<Placed> {
    words.iter().map(|w| Placed { text: w.text.clone(), at: w.area }).collect()
}

fn is_member(m: &Markup) -> bool {
    matches!(m.kind.as_str(), "length" | "polylength")
}

fn subject_word(m: &Markup) -> String {
    if m.subject.trim().is_empty() {
        "no subject".into()
    } else {
        format!("'{}'", m.subject.trim())
    }
}

/// "(4) W12X26", "4 PLACES", "TYP. OF 4" — how many a note says there are.
pub fn how_many(text: &str) -> Option<u32> {
    static P: OnceLock<Vec<Regex>> = OnceLock::new();
    let patterns = P.get_or_init(|| {
        vec![
            Regex::new(r"^\s*\((\d{1,3})\)\s*[A-Z]").unwrap(),
            Regex::new(r"\b(\d{1,3})\s*(?:PLACES|PLCS\.?|PLS\.?|LOCATIONS|LOCS?\.)").unwrap(),
            Regex::new(r"\bTYP(?:ICAL|\.)?\s*(?:OF|@|AT)?\s*\(?(\d{1,3})\)?\b").unwrap(),
            Regex::new(r"\b(\d{1,3})\s*-\s*[A-Z]{1,3}\d").unwrap(),
        ]
    });
    let text = text.to_uppercase();
    patterns
        .iter()
        .find_map(|p| p.captures(&text).and_then(|c| c[1].parse::<u32>().ok()))
        .filter(|n| (2..=500).contains(n))
}

fn says_typical(text: &str) -> bool {
    let t = text.to_uppercase();
    t.contains("TYP")
}

/// A pitch as rise in twelve: "4:12", "4/12", "4 IN 12", "SLOPE 3/4\" PER FT".
pub fn pitch(text: &str) -> Option<f64> {
    static P: OnceLock<(Regex, Regex)> = OnceLock::new();
    let (twelve, per_foot) = P.get_or_init(|| {
        (
            Regex::new(r"\b(\d{1,2}(?:\.\d+)?|\d{1,2}[- ]\d/\d{1,2})\s*(?::|/|\bIN\b)\s*12\b").unwrap(),
            Regex::new(r#"SLOPE[^0-9]{0,6}(\d+[- ]\d+/\d+|\d+/\d+|\d+(?:\.\d+)?)\s*"?\s*(?:/|PER)\s*(?:FT|FOOT|')"#).unwrap(),
        )
    });
    let t = text.to_uppercase();
    twelve
        .captures(&t)
        .and_then(|c| callout::inches(&c[1]))
        .or_else(|| per_foot.captures(&t).and_then(|c| callout::inches(&c[1])))
        .filter(|r| *r > 0.0 && *r <= 24.0)
}

/// The words within `reach` of a line, found through the sheet's index.
pub fn near(words: &[Placed], grid: &geo::Grid, line: &[[f64; 2]], reach: f64) -> Vec<Placed> {
    let Some(b) = geo::bounds(line) else { return Vec::new() };
    grid.within(geo::grow(b, reach))
        .into_iter()
        .map(|i| &words[i])
        .filter(|w| geo::to_polyline(geo::centre(w.at), line) <= reach)
        .cloned()
        .collect()
}

fn call_it(m: &Markup, shape: &Shape, weight_col: Option<usize>) -> Vec<Action> {
    let mut actions = vec![Action::SetSubject { markup: m.id.clone(), subject: shape.designation.clone() }];
    if let (Some(col), Some(pf)) = (weight_col, shape.per_foot) {
        actions.push(Action::SetColumn { markup: m.id.clone(), column: col as u8, value: format!("{pf:.2}") });
    }
    actions
}

pub fn check(input: &Input) -> Output {
    let weight_col = weight_column(&input.columns);
    let mut findings = Vec::new();
    // By size: pieces, feet, and where its weight came from.
    let mut by_size: BTreeMap<String, (f64, f64, String, String)> = BTreeMap::new();
    let mut read = 0usize;

    for sheet in &input.sheets {
        let words = placed(&sheet.words);
        let grid = geo::Grid::new(words.iter().map(|w| w.at), 48.0);
        for m in input.markups.iter().filter(|m| m.page == sheet.page && is_member(m)) {
            read += 1;
            let area = geo::bounds(&m.points).unwrap_or([0.0; 4]);
            let nearby = near(&words, &grid, &m.points, NOTE_REACH);
            let beside = callout::beside(&nearby, &m.points, REACH).map(|(s, _)| s);
            let own = callout::size_of(&m.subject);
            let current = weight_col
                .and_then(|c| m.columns.get(c))
                .and_then(|v| v.trim().parse::<f64>().ok())
                .filter(|v| *v > 0.0);

            match (&own, &beside) {
                (None, Some(b)) => {
                    findings.push(
                        Finding::new(
                            Level::Check,
                            format!("{}: taken off as {}. The drawing says {} beside it.", sheet.name, subject_word(m), b.designation),
                        )
                        .on(sheet.page)
                        .at(geo::grow(area, 6.0))
                        .about(m.id.clone())
                        .fix(format!("Call it {}", b.designation), call_it(m, b, weight_col)),
                    );
                }
                (Some(o), Some(b)) if !o.same_as(b) => {
                    findings.push(
                        Finding::new(
                            Level::Problem,
                            format!(
                                "{}: taken off as {} but the drawing says {} beside it.",
                                sheet.name, o.designation, b.designation
                            ),
                        )
                        .on(sheet.page)
                        .at(geo::grow(area, 6.0))
                        .about(m.id.clone())
                        .fix(format!("Change to {}", b.designation), call_it(m, b, weight_col)),
                    );
                }
                _ => {}
            }

            // Its weight, against what its size says it weighs.
            if let (Some(o), Some(col)) = (&own, weight_col) {
                if let Some(pf) = o.per_foot {
                    let off = current.map(|c| (c / pf - 1.0).abs());
                    let tolerance = match o.weighed {
                        callout::Weighed::Named => 0.01,
                        callout::Weighed::Computed => 0.05,
                    };
                    let fix = vec![Action::SetColumn { markup: m.id.clone(), column: col as u8, value: format!("{pf:.2}") }];
                    match off {
                        None => findings.push(
                            Finding::new(
                                Level::Problem,
                                format!(
                                    "{}: {} has no unit weight, so it adds nothing to the tonnage. {} lb/ft ({}).",
                                    sheet.name, o.designation, pf, o.weighed.says()
                                ),
                            )
                            .on(sheet.page)
                            .at(geo::grow(area, 6.0))
                            .about(m.id.clone())
                            .fix(format!("Use {pf} lb/ft"), fix),
                        ),
                        Some(off) if off > tolerance => findings.push(
                            Finding::new(
                                Level::Check,
                                format!(
                                    "{}: {} carries {} lb/ft; the size says {} lb/ft ({}).",
                                    sheet.name,
                                    o.designation,
                                    current.unwrap_or(0.0),
                                    pf,
                                    o.weighed.says()
                                ),
                            )
                            .on(sheet.page)
                            .at(geo::grow(area, 6.0))
                            .about(m.id.clone())
                            .fix(format!("Use {pf} lb/ft"), fix),
                        ),
                        _ => {}
                    }
                }
            }

            // Typicals.
            if m.quantity <= 1.0 {
                let nearby: Vec<&Placed> = nearby.iter().collect();
                let counted = nearby.iter().find_map(|w| how_many(&w.text).map(|n| (n, w.text.clone())));
                match counted {
                    Some((n, said)) => findings.push(
                        Finding::new(
                            Level::Check,
                            format!("{}: the drawing says \"{}\" beside this {}, but it's counted once.", sheet.name, said.trim(), m.subject.trim()),
                        )
                        .on(sheet.page)
                        .at(geo::grow(area, 6.0))
                        .about(m.id.clone())
                        .fix(format!("Count it ×{n}"), vec![Action::SetQuantity { markup: m.id.clone(), quantity: n as f64 }]),
                    ),
                    None => {
                        if nearby.iter().any(|w| says_typical(&w.text)) {
                            findings.push(
                                Finding::new(
                                    Level::Note,
                                    format!(
                                        "{}: marked TYP. beside this {} and counted once. If it stands for several, set its quantity in Properties.",
                                        sheet.name,
                                        if m.subject.trim().is_empty() { "length" } else { m.subject.trim() }
                                    ),
                                )
                                .on(sheet.page)
                                .at(geo::grow(area, 6.0))
                                .about(m.id.clone()),
                            );
                        }
                    }
                }
            }

            // Pitches. A drainage slope changes nothing worth a line; a roof
            // pitch changes the length by several percent.
            if m.slope.is_none() {
                let found = nearby.iter().find_map(|w| pitch(&w.text));
                if let Some(rise) = found {
                    let grows = (1.0 + (rise / 12.0).powi(2)).sqrt() - 1.0;
                    if grows > 0.005 {
                        let flat = m.length.unwrap_or(0.0);
                        let text = if flat > 0.0 {
                            format!(" — {:.2} ft flat is {:.2} ft up the slope", flat, flat * (1.0 + grows))
                        } else {
                            String::new()
                        };
                        findings.push(
                            Finding::new(
                                Level::Check,
                                format!(
                                    "{}: the drawing shows a {}:12 pitch beside this {}, and it's measured flat{text}.",
                                    sheet.name,
                                    trim_number(rise),
                                    if m.subject.trim().is_empty() { "length" } else { m.subject.trim() }
                                ),
                            )
                            .on(sheet.page)
                            .at(geo::grow(area, 6.0))
                            .about(m.id.clone())
                            .fix("Measure up the slope", vec![Action::SetSlope { markup: m.id.clone(), rise, run: 12.0 }]),
                        );
                    }
                }
            }

            let name = own
                .as_ref()
                .map(|o| o.designation.clone())
                .unwrap_or_else(|| if m.subject.trim().is_empty() { "(no subject)".into() } else { m.subject.trim().to_string() });
            let entry = by_size.entry(name).or_insert((0.0, 0.0, String::new(), String::new()));
            entry.0 += m.quantity;
            entry.1 += m.length.unwrap_or(0.0) * m.quantity;
            if entry.2.is_empty() {
                entry.2 = current.map(|c| format!("{c:.2}")).unwrap_or_else(|| "none".into());
                entry.3 = own
                    .as_ref()
                    .and_then(|o| o.per_foot.map(|pf| format!("{pf:.2} ({})", match o.weighed {
                        callout::Weighed::Named => "AISC",
                        callout::Weighed::Computed => "computed",
                    })))
                    .unwrap_or_default();
            }
        }
    }
    findings.sort_by(|a, b| b.level.cmp(&a.level).then(a.page.cmp(&b.page)));
    let rows: Vec<Vec<String>> = by_size
        .into_iter()
        .map(|(name, (pieces, feet, carried, says))| {
            vec![name, trim_number(pieces), format!("{feet:.1}"), carried, says]
        })
        .collect();
    let problems = findings.iter().filter(|f| f.level == Level::Problem).count();
    let checks = findings.iter().filter(|f| f.level == Level::Check).count();
    Output {
        title: "Sizes, typicals and pitches".into(),
        summary: format!(
            "{read} lengths read against the text beside them. {problems} disagree with the drawing or weigh nothing; {checks} more to look at.{}",
            if weight_col.is_none() {
                " No custom column is named for pounds per foot (LBS Per FT), so weights can't be written."
            } else {
                ""
            }
        ),
        findings,
        tables: vec![Table {
            title: "By size".into(),
            columns: ["Size", "Pieces", "Feet", "lb/ft carried", "lb/ft the size says"].iter().map(|s| s.to_string()).collect(),
            rows,
            totals: Vec::new(),
        }],
    }
}

fn trim_number(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plugin_api::{Sheet, Word};

    fn word(text: &str, x: f64, y: f64) -> Word {
        Word { text: text.into(), area: [x, y, x + 6.0 * text.len() as f64, y + 8.0], size: 8.0 }
    }

    fn beam(id: &str, subject: &str, weight: &str) -> Markup {
        Markup {
            id: id.into(),
            page: 0,
            kind: "length".into(),
            subject: subject.into(),
            points: vec![[100.0, 200.0], [500.0, 200.0]],
            length: Some(20.0),
            quantity: 1.0,
            columns: vec![weight.into()],
            ..Default::default()
        }
    }

    fn input(words: Vec<Word>, markups: Vec<Markup>) -> Input {
        Input {
            sheets: vec![Sheet { page: 0, name: "S-201".into(), width: 2592.0, height: 1728.0, words, ..Default::default() }],
            markups,
            columns: vec!["LBS Per FT".into()],
            ..Default::default()
        }
    }

    #[test]
    fn a_member_taken_off_as_the_wrong_size_is_a_problem_and_the_fix_writes_the_weight_too() {
        let out = check(&input(vec![word("W16X31", 280.0, 186.0)], vec![beam("b1", "W16x26", "26")]));
        let f = out.findings.iter().find(|f| f.level == Level::Problem).unwrap();
        assert!(f.message.contains("W16X26") && f.message.contains("W16X31"), "{}", f.message);
        assert_eq!(
            f.fixes[0].actions,
            vec![
                Action::SetSubject { markup: "b1".into(), subject: "W16X31".into() },
                Action::SetColumn { markup: "b1".into(), column: 0, value: "31.00".into() },
            ]
        );
    }

    #[test]
    fn a_size_with_no_weight_adds_nothing_and_says_so() {
        let out = check(&input(vec![], vec![beam("b1", "W12X26", "")]));
        assert!(out.findings.iter().any(|f| f.level == Level::Problem && f.message.contains("no unit weight")));
    }

    #[test]
    fn typicals_and_pitches_beside_a_member_are_offered_as_fixes() {
        let out = check(&input(
            vec![word("W12X26", 280.0, 186.0), word("TYP. OF 4", 300.0, 214.0), word("4:12", 200.0, 214.0)],
            vec![beam("b1", "W12X26", "26")],
        ));
        let quantity = out.findings.iter().flat_map(|f| &f.fixes).find(|f| f.label == "Count it ×4");
        assert!(quantity.is_some(), "{:?}", out.findings);
        let slope = out.findings.iter().flat_map(|f| &f.fixes).find(|f| f.label == "Measure up the slope").unwrap();
        assert_eq!(slope.actions[0], Action::SetSlope { markup: "b1".into(), rise: 4.0, run: 12.0 });
        assert!(out.findings.iter().all(|f| f.level != Level::Problem), "size and weight agree");
    }

    #[test]
    fn counts_and_pitches_are_read_the_ways_drawings_write_them() {
        assert_eq!(how_many("(4) W12X26"), Some(4));
        assert_eq!(how_many("6 PLACES"), Some(6));
        assert_eq!(how_many("TYP. OF 3"), Some(3));
        assert_eq!(how_many("TYP"), None);
        assert_eq!(how_many("W12X26"), None);
        assert_eq!(pitch("4:12"), Some(4.0));
        assert_eq!(pitch("SLOPE 6/12"), Some(6.0));
        assert_eq!(pitch("SLOPE 1/4\" PER FT"), Some(0.25));
        assert_eq!(pitch("GRID 12"), None);
    }

    #[test]
    fn the_weight_column_is_found_by_its_name() {
        assert_eq!(weight_column(&["Qty".into(), "LBS Per FT".into()]), Some(1));
        assert_eq!(weight_column(&["PLF".into()]), Some(0));
        assert_eq!(weight_column(&["Notes".into()]), None);
    }
}
