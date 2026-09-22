//! Scale check: every sheet's scale against the scales printed on it.
//!
//! A scale that is wrong is wrong for every measurement on the sheet at once,
//! and nothing about the takeoff looks off: 1/8" set on a 1/4" sheet halves
//! the steel, quietly. So the drawing is read for what it says its scale is,
//! and every sheet whose setting disagrees is named, with the fix one click
//! away.

use plugin_api::{Action, Finding, Input, Level, Markup, Output, Sheet, Table, Word};
use regex::Regex;
use std::sync::OnceLock;

use crate::geo::{self, Area};

/// What a sheet says about its scale, and where.
#[derive(Clone, Debug, PartialEq)]
pub enum Said {
    Scale { ratio: f64, area: Area },
    NotToScale { area: Area },
    AsNoted { area: Area },
}

impl Said {
    pub fn area(&self) -> Area {
        match self {
            Said::Scale { area, .. } | Said::NotToScale { area } | Said::AsNoted { area } => *area,
        }
    }
}

fn patterns() -> &'static (Regex, Regex, Regex, Regex, Regex) {
    static P: OnceLock<(Regex, Regex, Regex, Regex, Regex)> = OnceLock::new();
    P.get_or_init(|| {
        (
            // 1/4" = 1'-0", 1 1/2"=1'-0", 1" = 20'-0", 3/4"=1'
            Regex::new(r#"(\d+[- ]\d+/\d+|\d+/\d+|\d+(?:\.\d+)?)\s*"\s*=\s*(\d+(?:\.\d+)?)\s*'(?:\s*-?\s*(\d+(?:\.\d+)?)\s*")?"#).unwrap(),
            // SCALE 1:50
            Regex::new(r"SCALE\s*:?\s*1\s*:\s*(\d{1,4})\b").unwrap(),
            Regex::new(r"\bN\.?\s?T\.?\s?S\.?(?:\s|$)|NOT\s+TO\s+SCALE").unwrap(),
            Regex::new(r"SCALE\s*:?\s*AS\s+(?:NOTED|SHOWN|INDICATED)").unwrap(),
            Regex::new(r"FULL\s+SCALE|SCALE\s*:?\s*1\s*:\s*1\b").unwrap(),
        )
    })
}

fn tidy(text: &str) -> String {
    text.to_uppercase()
        .replace(['\u{2019}', '\u{2032}', '`'], "'")
        .replace(['\u{201d}', '\u{2033}', '\u{201c}'], "\"")
        .replace("''", "\"")
}

/// Every scale statement in one piece of text.
pub fn read(text: &str) -> Vec<Said> {
    let text = tidy(text);
    let (imperial, metric, nts, noted, full) = patterns();
    let area = [0.0; 4];
    let mut out = Vec::new();
    for caps in imperial.captures_iter(&text) {
        let paper = crate::callout::inches(&caps[1]);
        let feet: Option<f64> = caps[2].parse().ok();
        let inches: f64 = caps.get(3).and_then(|m| m.as_str().parse().ok()).unwrap_or(0.0);
        if let (Some(paper), Some(feet)) = (paper, feet) {
            let real = feet * 12.0 + inches;
            if paper > 0.0 && real > 0.0 {
                let ratio = real / paper;
                if (1.0..=10_000.0).contains(&ratio) {
                    out.push(Said::Scale { ratio, area });
                }
            }
        }
    }
    for caps in metric.captures_iter(&text) {
        if let Ok(n) = caps[1].parse::<f64>() {
            if n >= 1.0 {
                out.push(Said::Scale { ratio: n, area });
            }
        }
    }
    if full.is_match(&text) {
        out.push(Said::Scale { ratio: 1.0, area });
    }
    if nts.is_match(&text) {
        out.push(Said::NotToScale { area });
    }
    if noted.is_match(&text) {
        out.push(Said::AsNoted { area });
    }
    out
}

/// The word to the right of `i` on the same line, if it is close enough to
/// be the rest of the same note.
fn next_on_line(words: &[Word], grid: &geo::Grid, i: usize) -> Option<usize> {
    let w = &words[i];
    let size = w.size.max(w.area[3] - w.area[1]).max(1.0);
    let mid = (w.area[1] + w.area[3]) * 0.5;
    grid.within([w.area[2] - size, w.area[1] - size, w.area[2] + size * 2.0, w.area[3] + size])
        .into_iter()
        .map(|j| (j, &words[j]))
        .filter(|(j, o)| {
            *j != i
                && o.area[0] >= w.area[2] - size * 0.2
                && o.area[0] - w.area[2] <= size * 1.8
                && ((o.area[1] + o.area[3]) * 0.5 - mid).abs() <= size * 0.5
        })
        .min_by(|a, b| a.1.area[0].partial_cmp(&b.1.area[0]).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(j, _)| j)
}

/// Every scale statement on a sheet, with where it is. A note split over
/// two or three runs of text is read whole.
pub fn notes_on(sheet: &Sheet) -> Vec<Said> {
    let words = &sheet.words;
    let grid = geo::Grid::new(words.iter().map(|w| w.area), 48.0);
    let mut out: Vec<Said> = Vec::new();
    for i in 0..words.len() {
        // Only text that could begin a scale note is worth joining up.
        if !could_start(&words[i].text) {
            continue;
        }
        let mut text = words[i].text.clone();
        let mut area = words[i].area;
        let mut at = i;
        for _ in 0..3 {
            let found = read(&text);
            if !found.is_empty() {
                for said in found {
                    let placed = match said {
                        Said::Scale { ratio, .. } => Said::Scale { ratio, area },
                        Said::NotToScale { .. } => Said::NotToScale { area },
                        Said::AsNoted { .. } => Said::AsNoted { area },
                    };
                    let already = out.iter().any(|o| {
                        std::mem::discriminant(o) == std::mem::discriminant(&placed)
                            && geo::overlaps(o.area(), placed.area())
                            && match (o, &placed) {
                                (Said::Scale { ratio: a, .. }, Said::Scale { ratio: b, .. }) => close(*a, *b),
                                _ => true,
                            }
                    });
                    if !already {
                        out.push(placed);
                    }
                }
                break;
            }
            let Some(j) = next_on_line(words, &grid, at) else { break };
            text = format!("{text} {}", words[j].text);
            area = geo::union(area, words[j].area);
            at = j;
        }
    }
    out
}

fn could_start(text: &str) -> bool {
    let t = text.to_uppercase();
    t.contains("SCALE") || t.contains('"') || t.contains('\u{201d}') || t.contains('\u{2033}')
        || t.contains("N.T") || t.contains("NTS") || t.contains("NOT TO") || t.contains("1:")
        || t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
}

pub fn close(a: f64, b: f64) -> bool {
    a > 0.0 && b > 0.0 && (a / b - 1.0).abs() < 0.015
}

/// A scale the way a drawing writes it.
pub fn named(ratio: f64) -> String {
    const NAMES: &[(&str, f64)] = &[
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
        ("1/32\" = 1'-0\"", 384.0),
        ("FULL SCALE", 1.0),
        ("6\" = 1'-0\"", 2.0),
    ];
    if let Some((name, _)) = NAMES.iter().find(|(_, r)| close(*r, ratio)) {
        return name.to_string();
    }
    if ratio >= 60.0 && (ratio / 12.0 - (ratio / 12.0).round()).abs() < 0.01 {
        return format!("1\" = {}'", (ratio / 12.0).round());
    }
    format!("1:{}", (ratio * 10.0).round() / 10.0)
}

/// What a sheet is printed on, and whether that is a half-size print.
pub fn paper(sheet: &Sheet) -> (String, bool) {
    let (w, h) = (sheet.width / 72.0, sheet.height / 72.0);
    let (long, short) = (w.max(h), w.min(h));
    let near = |a: f64, b: f64| (a - b).abs() < 0.6;
    let name = format!("{}×{}", round_half(short), round_half(long));
    let half = near(long, 17.0) && near(short, 11.0) || near(long, 18.0) && near(short, 12.0) || near(long, 21.0) && near(short, 15.0);
    (name, half)
}

fn round_half(v: f64) -> String {
    let r = (v * 2.0).round() / 2.0;
    if (r - r.round()).abs() < 1e-9 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

/// Whether a scale note sits in the title block — the strip along the right
/// edge or the bottom of the sheet — where it speaks for the whole sheet
/// rather than for one detail.
fn in_title_block(sheet: &Sheet, area: Area) -> bool {
    let c = geo::centre(area);
    c[0] > sheet.width * 0.82 || c[1] > sheet.height * 0.90
}

fn measures(m: &Markup) -> bool {
    !matches!(m.kind.as_str(), "markup" | "count")
}

/// The scale note a markup is drawn above: detail titles sit under the
/// detail they name, so the nearest note below a markup, not far off to one
/// side, is the one that speaks for it.
fn governing_note<'a>(sheet: &Sheet, notes: &'a [Said], m: &Markup) -> Option<&'a Said> {
    let b = geo::bounds(&m.points)?;
    let c = geo::centre(b);
    notes
        .iter()
        .filter(|n| !in_title_block(sheet, n.area()))
        .filter_map(|n| {
            let a = n.area();
            let below = a[1] - c[1];
            let side = if c[0] < a[0] {
                a[0] - c[0]
            } else if c[0] > a[2] {
                c[0] - a[2]
            } else {
                0.0
            };
            (below > 0.0 && below < sheet.height * 0.45 && side < sheet.width * 0.18).then_some((n, below + side * 2.0))
        })
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(n, _)| n)
}

fn set_to(page: u32, ratio: f64) -> Action {
    Action::SetSheetScale { page, ratio, text: named(ratio) }
}

pub fn check(input: &Input) -> Output {
    let mut findings = Vec::new();
    let mut rows = Vec::new();
    let mut good = 0usize;
    for sheet in &input.sheets {
        let notes = notes_on(sheet);
        let printed: Vec<(f64, Area)> = notes
            .iter()
            .filter_map(|n| match n {
                Said::Scale { ratio, area } => Some((*ratio, *area)),
                _ => None,
            })
            .collect();
        let mut distinct: Vec<f64> = Vec::new();
        for (r, _) in &printed {
            if !distinct.iter().any(|d| close(*d, *r)) {
                distinct.push(*r);
            }
        }
        let measured: Vec<&Markup> = input
            .markups
            .iter()
            .filter(|m| m.page == sheet.page && measures(m))
            .collect();
        let (paper_name, half) = paper(sheet);
        let set = sheet.scale.as_ref().map(|s| s.ratio);
        let set_words = sheet
            .scale
            .as_ref()
            .map(|s| if s.text.is_empty() { named(s.ratio) } else { s.text.clone() })
            .unwrap_or_else(|| "nothing".into());
        let printed_words = if distinct.is_empty() {
            if notes.iter().any(|n| matches!(n, Said::NotToScale { .. })) {
                "not to scale".to_string()
            } else {
                "—".to_string()
            }
        } else {
            distinct.iter().map(|r| named(*r)).collect::<Vec<_>>().join(", ")
        };
        // Which printed scale speaks for the sheet: the title block's when it
        // gives one, otherwise the one most of the details use.
        let governing = printed
            .iter()
            .find(|(_, a)| in_title_block(sheet, *a))
            .map(|(r, _)| *r)
            .or_else(|| {
                distinct.iter().copied().max_by_key(|d| printed.iter().filter(|(r, _)| close(*r, *d)).count())
            });
        let first_note = printed.first().map(|(_, a)| *a);
        let mut verdict = "OK".to_string();
        let n = measured.len();
        let things = if n == 1 { "measurement" } else { "measurements" };

        match (set, governing) {
            (None, None) => {
                if n > 0 {
                    verdict = "no scale".into();
                    findings.push(
                        Finding::new(
                            Level::Problem,
                            format!(
                                "{}: {n} {things} and no scale set, and no scale printed on the sheet to set it from. \
                                 They're left out of every total until it's calibrated.",
                                sheet.name
                            ),
                        )
                        .on(sheet.page),
                    );
                } else {
                    verdict = "—".into();
                }
            }
            (None, Some(g)) => {
                verdict = "not set".into();
                let level = if n > 0 { Level::Problem } else { Level::Note };
                let mut f = Finding::new(
                    level,
                    if n > 0 {
                        format!(
                            "{}: no scale set, so its {n} {things} are left out of every total. The sheet says {}.",
                            sheet.name,
                            named(g)
                        )
                    } else {
                        format!("{}: no scale set. The sheet says {}.", sheet.name, named(g))
                    },
                )
                .on(sheet.page);
                if let Some(a) = first_note {
                    f = f.at(a);
                }
                f = f.fix(format!("Set {}", named(g)), vec![set_to(sheet.page, g)]);
                if half {
                    f = f.fix(
                        format!("Set {} (half-size print)", named(g * 2.0)),
                        vec![set_to(sheet.page, g * 2.0)],
                    );
                }
                findings.push(f);
            }
            (Some(_), None) => {}
            (Some(s), Some(g)) => {
                let matches_one = distinct.iter().any(|d| close(*d, s));
                let halved = distinct.iter().copied().find(|d| close(*d * 2.0, s));
                let doubled = distinct.iter().copied().find(|d| close(*d, s * 2.0));
                if matches_one {
                    if half && close(s, g) && n > 0 {
                        verdict = "half-size?".into();
                        let mut f = Finding::new(
                            Level::Check,
                            format!(
                                "{}: an {paper_name} sheet set to {}, as printed. If this is a half-size print of a \
                                 full-size drawing, every length on it reads half what it is. Check one dimension.",
                                sheet.name,
                                named(s)
                            ),
                        )
                        .on(sheet.page);
                        if let Some(a) = first_note {
                            f = f.at(a);
                        }
                        findings.push(f.fix(
                            format!("It's half size — set {}", named(s * 2.0)),
                            vec![set_to(sheet.page, s * 2.0)],
                        ));
                    }
                } else if let (Some(p), true) = (halved, half) {
                    verdict = "half-size".into();
                    findings.push(
                        Finding::new(
                            Level::Note,
                            format!(
                                "{}: {paper_name} sheet set to {}, which is {} printed at half size. \
                                 Right if this set is half size.",
                                sheet.name,
                                named(s),
                                named(p)
                            ),
                        )
                        .on(sheet.page),
                    );
                } else {
                    verdict = "WRONG".into();
                    let factor = s / g;
                    let reads = if let Some(_) = halved {
                        "Lengths here read double what they are.".to_string()
                    } else if let Some(_) = doubled {
                        "Lengths here read half what they are.".to_string()
                    } else if factor > 1.0 {
                        format!("Lengths here read {:.2} times what they are.", factor)
                    } else {
                        format!("Lengths here read {:.0}% of what they are.", factor * 100.0)
                    };
                    let mut f = Finding::new(
                        Level::Problem,
                        format!(
                            "{}: set to {}, but the sheet says {}. {reads}",
                            sheet.name,
                            set_words,
                            printed_words
                        ),
                    )
                    .on(sheet.page);
                    if let Some(a) = first_note {
                        f = f.at(a);
                    }
                    f = f.fix(format!("Set {}", named(g)), vec![set_to(sheet.page, g)]);
                    findings.push(f);
                }

                // More than one scale on the sheet: anything measured in a
                // detail at another scale is wrong by the ratio of the two.
                if distinct.len() > 1 {
                    let mut suspects = 0;
                    for m in &measured {
                        let Some(Said::Scale { ratio, area }) = governing_note(sheet, &notes, m) else {
                            continue;
                        };
                        if close(*ratio, s) {
                            continue;
                        }
                        suspects += 1;
                        let times = s / ratio;
                        let off = if times > 1.0 {
                            format!("{:.1} times too long", times)
                        } else {
                            format!("{:.0}% of what it is", times * 100.0)
                        };
                        let what = if m.subject.is_empty() { m.kind.clone() } else { m.subject.clone() };
                        let area_m = geo::bounds(&m.points).map(|b| geo::union(b, *area)).unwrap_or(*area);
                        findings.push(
                            Finding::new(
                                Level::Check,
                                format!(
                                    "{}: this {what} sits above a detail marked {}, but the sheet is set to {}. \
                                     If it was measured in that detail, it reads {off}.",
                                    sheet.name,
                                    named(*ratio),
                                    named(s)
                                ),
                            )
                            .on(sheet.page)
                            .at(area_m)
                            .about(m.id.clone()),
                        );
                    }
                    findings.push(
                        Finding::new(
                            if suspects > 0 { Level::Check } else { Level::Note },
                            format!(
                                "{}: more than one scale on this sheet ({printed_words}). It's set to {}; \
                                 a sheet has one scale, so measure details drawn at another scale on a sheet \
                                 of their own, or calibrate each time.",
                                sheet.name,
                                named(s)
                            ),
                        )
                        .on(sheet.page),
                    );
                }
            }
        }

        // Measured in a part marked not to scale.
        for m in &measured {
            if let Some(Said::NotToScale { area }) = governing_note(sheet, &notes, m) {
                let what = if m.subject.is_empty() { m.kind.clone() } else { m.subject.clone() };
                findings.push(
                    Finding::new(
                        Level::Check,
                        format!("{}: this {what} was measured above a note that says NOT TO SCALE.", sheet.name),
                    )
                    .on(sheet.page)
                    .at(geo::bounds(&m.points).map(|b| geo::union(b, *area)).unwrap_or(*area))
                    .about(m.id.clone()),
                );
            }
        }

        if verdict == "OK" {
            good += 1;
        }
        rows.push(vec![
            sheet.name.clone(),
            paper_name,
            set_words,
            printed_words,
            n.to_string(),
            verdict,
        ]);
    }
    findings.sort_by(|a, b| b.level.cmp(&a.level).then(a.page.cmp(&b.page)));
    let problems = findings.iter().filter(|f| f.level == Level::Problem).count();
    Output {
        title: "Scale check".into(),
        summary: format!(
            "{} sheets read; {good} agree with what they print. {problems} to fix before the number goes out.",
            input.sheets.len()
        ),
        findings,
        tables: vec![Table {
            title: "Every sheet".into(),
            columns: ["Sheet", "Paper", "Set to", "Printed", "Measurements", "Verdict"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rows,
            totals: Vec::new(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plugin_api::Scale;

    fn word(text: &str, x: f64, y: f64) -> Word {
        Word { text: text.into(), area: [x, y, x + 8.0 * text.len() as f64, y + 10.0], size: 10.0 }
    }

    #[test]
    fn scales_are_read_however_they_are_written() {
        let r = |t: &str| match read(t).first() {
            Some(Said::Scale { ratio, .. }) => *ratio,
            _ => -1.0,
        };
        assert_eq!(r("SCALE: 1/4\" = 1'-0\""), 48.0);
        assert_eq!(r("1/8\"=1'-0\""), 96.0);
        assert_eq!(r("3/4\" = 1'"), 16.0);
        assert_eq!(r("1 1/2\" = 1'-0\""), 8.0);
        assert_eq!(r("1-1/2\"=1'-0\""), 8.0);
        assert_eq!(r("1\" = 20'-0\""), 240.0);
        assert_eq!(r("SCALE: 1:50"), 50.0);
        assert_eq!(r("1/4\u{201d} = 1\u{2019}-0\u{201d}"), 48.0, "curly quotes from a word processor");
        assert!(matches!(read("N.T.S.").first(), Some(Said::NotToScale { .. })));
        assert!(matches!(read("SCALE: AS NOTED").first(), Some(Said::AsNoted { .. })));
        assert!(read("SLOPE 1:12 MAX").is_empty(), "a ramp slope is not a scale");
        assert!(read("SEE 3/S-501").is_empty());
    }

    #[test]
    fn a_note_split_across_runs_is_read_whole() {
        let sheet = Sheet {
            width: 2592.0,
            height: 1728.0,
            words: vec![word("SCALE:", 100.0, 500.0), word("1/4\"", 160.0, 500.0), word("= 1'-0\"", 200.0, 500.0)],
            ..Default::default()
        };
        let notes = notes_on(&sheet);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(matches!(notes[0], Said::Scale { ratio, .. } if ratio == 48.0));
    }

    fn a_sheet(set: Option<f64>, notes: &[(&str, f64, f64)], w_in: f64, h_in: f64) -> Sheet {
        Sheet {
            page: 0,
            name: "S-201".into(),
            width: w_in * 72.0,
            height: h_in * 72.0,
            scale: set.map(|r| Scale { ratio: r, text: named(r) }),
            words: notes.iter().map(|(t, x, y)| word(t, *x, *y)).collect(),
            ..Default::default()
        }
    }

    fn a_length(page: u32, at: [f64; 2]) -> Markup {
        Markup {
            id: "m1".into(),
            page,
            kind: "length".into(),
            subject: "W12X26".into(),
            points: vec![at, [at[0] + 100.0, at[1]]],
            quantity: 1.0,
            ..Default::default()
        }
    }

    #[test]
    fn a_sheet_set_to_the_wrong_scale_is_a_problem_with_the_fix_on_it() {
        let input = Input {
            sheets: vec![a_sheet(Some(96.0), &[("SCALE: 1/4\" = 1'-0\"", 2300.0, 1650.0)], 36.0, 24.0)],
            markups: vec![a_length(0, [300.0, 300.0])],
            ..Default::default()
        };
        let out = check(&input);
        let f = &out.findings[0];
        assert_eq!(f.level, Level::Problem);
        assert!(f.message.contains("read double"), "{}", f.message);
        assert_eq!(
            f.fixes[0].actions[0],
            Action::SetSheetScale { page: 0, ratio: 48.0, text: "1/4\" = 1'-0\"".into() }
        );
    }

    #[test]
    fn a_half_size_sheet_at_half_the_printed_scale_is_right() {
        let input = Input {
            sheets: vec![a_sheet(Some(96.0), &[("SCALE: 1/4\" = 1'-0\"", 1100.0, 760.0)], 17.0, 11.0)],
            markups: vec![a_length(0, [300.0, 300.0])],
            ..Default::default()
        };
        let out = check(&input);
        assert!(out.findings.iter().all(|f| f.level != Level::Problem), "{:?}", out.findings);
    }

    #[test]
    fn an_unset_sheet_with_measurements_is_a_problem_and_a_half_size_one_gets_both_choices() {
        let input = Input {
            sheets: vec![a_sheet(None, &[("SCALE: 1/4\" = 1'-0\"", 1100.0, 760.0)], 17.0, 11.0)],
            markups: vec![a_length(0, [300.0, 300.0])],
            ..Default::default()
        };
        let out = check(&input);
        assert_eq!(out.findings[0].level, Level::Problem);
        assert_eq!(out.findings[0].fixes.len(), 2);
    }

    #[test]
    fn a_length_measured_in_a_detail_at_another_scale_is_caught() {
        // A 1/4" plan with a 3/4" detail in its lower left; the sheet is set
        // to 1/4" and a beam was measured inside the detail.
        let input = Input {
            sheets: vec![a_sheet(
                Some(48.0),
                &[
                    ("SCALE: 1/4\" = 1'-0\"", 2300.0, 1650.0),
                    ("SCALE: 3/4\" = 1'-0\"", 200.0, 1400.0),
                ],
                36.0,
                24.0,
            )],
            markups: vec![a_length(0, [180.0, 1250.0]), Markup { id: "m2".into(), ..a_length(0, [1500.0, 300.0]) }],
            ..Default::default()
        };
        let out = check(&input);
        let flagged: Vec<&Finding> = out.findings.iter().filter(|f| !f.markups.is_empty()).collect();
        assert_eq!(flagged.len(), 1, "{:?}", out.findings);
        assert_eq!(flagged[0].markups[0], "m1");
        assert!(flagged[0].message.contains("3.0 times too long"), "{}", flagged[0].message);
    }
}
