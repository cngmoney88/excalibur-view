//! Missed members: heavy lines on a sheet that no length covers.
//!
//! On a framing plan a member is a heavy single line with its size written
//! beside it. The takeoff is lengths drawn along those lines. A heavy line
//! with no length along it is either something that isn't steel — or a
//! member that was missed, which is the most expensive kind of wrong a bid
//! can be, because nothing about the total looks off.

use plugin_api::{Action, Finding, Input, Level, Line, Output, Sheet};

use crate::callout;
use crate::geo::{self, Axis, P};

/// How far from a member its size may be written, in points of paper —
/// about three quarters of an inch.
const REACH: f64 = 60.0;

/// A straight run of line-work: the pieces a CAD program broke one line
/// into, put back together.
#[derive(Clone, Debug)]
pub struct Run {
    pub axis: Axis,
    pub from: f64,
    pub to: f64,
    pub width: f64,
}

impl Run {
    pub fn a(&self) -> P {
        self.axis.at(self.from)
    }
    pub fn b(&self) -> P {
        self.axis.at(self.to)
    }
    pub fn length(&self) -> f64 {
        self.to - self.from
    }
}

pub struct Settings {
    /// How much heavier than the sheet's usual line a member is drawn.
    pub heavier: f64,
    /// The shortest member worth reporting, in feet.
    pub shortest_feet: f64,
    pub dashed_too: bool,
    /// List heavy lines with no size beside them too.
    pub list_unsized: bool,
}

impl Settings {
    pub fn from(input: &Input) -> Settings {
        Settings {
            heavier: input.number("heavier", 1.6).max(1.0),
            shortest_feet: input.number("shortest", 3.0).max(0.0),
            dashed_too: input.toggle("dashed", false),
            list_unsized: input.toggle("unsized", false),
        }
    }
}

/// The pen width most of the sheet is drawn with.
fn usual_width(lines: &[Line]) -> f64 {
    let mut widths: Vec<f64> = lines.iter().filter(|l| l.length() > 12.0).map(|l| l.width).collect();
    if widths.is_empty() {
        return 0.0;
    }
    widths.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    widths[widths.len() / 2]
}

/// Where the drawing stops and the title block starts: a heavy line running
/// nearly the whole height near the right edge, or the whole width near the
/// bottom.
fn drawing_area(sheet: &Sheet) -> [f64; 4] {
    let (w, h) = (sheet.width, sheet.height);
    let mut right = w;
    let mut bottom = h;
    for l in &sheet.lines {
        let vertical = (l.a[0] - l.b[0]).abs() < 1.0;
        let horizontal = (l.a[1] - l.b[1]).abs() < 1.0;
        if vertical && (l.a[1] - l.b[1]).abs() > h * 0.75 && l.a[0] > w * 0.7 && l.a[0] < w * 0.97 {
            right = right.min(l.a[0]);
        }
        if horizontal && (l.a[0] - l.b[0]).abs() > w * 0.75 && l.a[1] > h * 0.75 && l.a[1] < h * 0.97 {
            bottom = bottom.min(l.a[1]);
        }
    }
    let margin = w.min(h) * 0.03;
    [margin, margin, right - 2.0, bottom - 2.0]
}

/// Pieces along the same line, end to end, made into runs.
pub fn runs(lines: &[&Line], gap: f64) -> Vec<Run> {
    let mut pieces: Vec<Run> = lines
        .iter()
        .filter_map(|l| {
            let axis = Axis::of(l.a, l.b)?;
            let (s, e) = (axis.along(l.a), axis.along(l.b));
            Some(Run { axis, from: s.min(e), to: s.max(e), width: l.width })
        })
        .collect();
    // Group by direction and offset, then join along each.
    pieces.sort_by(|a, b| {
        let ka = (a.axis.dir[1].atan2(a.axis.dir[0]).to_degrees() * 2.0).round();
        let kb = (b.axis.dir[1].atan2(b.axis.dir[0]).to_degrees() * 2.0).round();
        ka.partial_cmp(&kb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.axis.offset.partial_cmp(&b.axis.offset).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.from.partial_cmp(&b.from).unwrap_or(std::cmp::Ordering::Equal))
    });
    let mut out: Vec<Run> = Vec::new();
    for piece in pieces {
        let joined = out.iter_mut().rev().take(40).find(|r| {
            r.axis.angle_to(&piece.axis) < 0.5
                && (r.axis.offset - piece.axis.offset * dot(r.axis.normal, piece.axis.normal).signum()).abs() < 1.0
                && piece.from <= r.to + gap
                && piece.to >= r.from - gap
        });
        match joined {
            Some(r) => {
                // The same line read in the other direction measures along
                // the other way; bring it round before joining.
                let flip = dot(r.axis.dir, piece.axis.dir) < 0.0;
                let (s, e) = if flip { (-piece.to, -piece.from) } else { (piece.from, piece.to) };
                r.from = r.from.min(s);
                r.to = r.to.max(e);
                r.width = r.width.max(piece.width);
            }
            None => out.push(piece),
        }
    }
    out
}

fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// Heavy runs lying side by side a few points apart, as one: a member
/// drawn as its two flanges, a wall drawn as its two faces. Which of the two
/// it is, is what the size beside it says.
#[derive(Clone, Debug)]
pub struct Band {
    /// Down the middle.
    pub centre: Run,
    /// How far the outside lines are from the middle.
    pub half_width: f64,
    pub lines: usize,
}

pub fn bands(runs: &[Run], apart: f64) -> Vec<Band> {
    let mut used = vec![false; runs.len()];
    let mut out = Vec::new();
    // Longest first, so a band is measured along its longest line.
    let mut order: Vec<usize> = (0..runs.len()).collect();
    order.sort_by(|a, b| runs[*b].length().partial_cmp(&runs[*a].length()).unwrap_or(std::cmp::Ordering::Equal));
    for &i in &order {
        if used[i] {
            continue;
        }
        used[i] = true;
        let base = &runs[i];
        let mut members = vec![i];
        loop {
            let mut grew = false;
            for &j in &order {
                if used[j] {
                    continue;
                }
                let o = &runs[j];
                if o.axis.angle_to(&base.axis) > 1.0 {
                    continue;
                }
                let near_one = members.iter().any(|&k| {
                    let m = &runs[k];
                    let off = m.axis.off(o.a()).min(m.axis.off(o.b()));
                    off <= apart && {
                        let (p, q) = (m.axis.along(o.a()), m.axis.along(o.b()));
                        let shared = (p.max(q).min(m.to) - p.min(q).max(m.from)).max(0.0);
                        shared >= o.length().min(m.length()) * 0.5
                    }
                });
                if near_one {
                    used[j] = true;
                    members.push(j);
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        // Across the band, measured from the longest line.
        let offsets: Vec<f64> = members
            .iter()
            .map(|&k| {
                let r = &runs[k];
                let mid = geo::centre([r.a()[0], r.a()[1], r.b()[0], r.b()[1]]);
                base.axis.normal[0] * mid[0] + base.axis.normal[1] * mid[1] - base.axis.offset
            })
            .collect();
        let (lo, hi) = offsets.iter().fold((f64::MAX, f64::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
        let mut from = f64::MAX;
        let mut to = f64::MIN;
        for &k in &members {
            let r = &runs[k];
            let (p, q) = (base.axis.along(r.a()), base.axis.along(r.b()));
            from = from.min(p.min(q));
            to = to.max(p.max(q));
        }
        let mut axis = base.axis;
        axis.offset += (lo + hi) * 0.5;
        out.push(Band {
            centre: Run { axis, from, to, width: base.width },
            half_width: (hi - lo) * 0.5,
            lines: members.len(),
        });
    }
    out
}

/// The size written nearest a member, and which word it is. Text set at an
/// angle to the member — a brace's size, written along the brace — is not
/// the member's, and neither is text off past either end of it.
fn size_beside(words: &[crate::callout::Placed], grid: &geo::Grid, run: &Run) -> Option<(usize, callout::Shape)> {
    let line = [run.a(), run.b()];
    let b = geo::bounds(&line)?;
    let horizontal = run.axis.dir[0].abs() > 0.97;
    let vertical = run.axis.dir[1].abs() > 0.97;
    let mut best: Option<(usize, callout::Shape, f64)> = None;
    for i in grid.within(geo::grow(b, REACH)) {
        let w = &words[i];
        let c = geo::centre(w.at);
        let t = run.axis.along(c);
        let slack = run.length() * 0.05;
        if t < run.from - slack || t > run.to + slack {
            continue;
        }
        let d = geo::to_segment(c, line[0], line[1]);
        if d > REACH || best.as_ref().is_some_and(|(_, _, bd)| *bd <= d) {
            continue;
        }
        let (width, height) = (w.at[2] - w.at[0], w.at[3] - w.at[1]);
        let letters = w.text.chars().count().max(1) as f64;
        // Straight text is one letter tall; text at an angle is as tall as
        // it is long.
        let slanted = width.min(height) > 2.5 * width.max(height) / letters;
        let wide = width > height * 1.6;
        let along = if horizontal {
            wide && !slanted
        } else if vertical {
            !slanted
        } else {
            true
        };
        if !along {
            continue;
        }
        if let Some(shape) = callout::size_of(&w.text) {
            best = Some((i, shape, d));
        }
    }
    best.map(|(i, s, _)| (i, s))
}

/// How much of a run the lengths on the sheet cover, 0 to 1.
fn coverage(run: &Run, lengths: &[&[P]], slack: f64) -> f64 {
    let mut cover = Vec::new();
    for points in lengths {
        for w in points.windows(2) {
            let Some(axis) = Axis::of(w[0], w[1]) else { continue };
            if axis.angle_to(&run.axis) > 3.0 {
                continue;
            }
            if run.axis.off(w[0]) > slack || run.axis.off(w[1]) > slack {
                continue;
            }
            let (s, e) = (run.axis.along(w[0]), run.axis.along(w[1]));
            cover.push((s.min(e) - 4.0, s.max(e) + 4.0));
        }
    }
    geo::covered(run.from, run.to, &mut cover)
}

pub fn check(input: &Input) -> Output {
    let settings = Settings::from(input);
    let Some(sheet) = input.sheet(input.current_page).or_else(|| input.sheets.first()) else {
        return Output { title: "Missed members".into(), summary: "No sheet to read.".into(), ..Default::default() };
    };
    if sheet.lines.is_empty() {
        return Output {
            title: format!("Missed members — {}", sheet.name),
            summary: "This sheet has no line-work to read — it may be a scan. Nothing can be checked on it.".into(),
            ..Default::default()
        };
    }
    let per_point = sheet.scale.as_ref().map(|s| s.feet_per_point());
    let shortest = match per_point {
        Some(fpp) if fpp > 0.0 => settings.shortest_feet / fpp,
        _ => 36.0,
    };
    let usual = usual_width(&sheet.lines);
    let heavy = (usual * settings.heavier).max(0.5);
    let inside = drawing_area(sheet);
    let long_limit = sheet.width.max(sheet.height) * 0.85;
    let candidates: Vec<&Line> = sheet
        .lines
        .iter()
        .filter(|l| l.width >= heavy && (settings.dashed_too || !l.dashed))
        .filter(|l| {
            let b = [l.a[0].min(l.b[0]), l.a[1].min(l.b[1]), l.a[0].max(l.b[0]), l.a[1].max(l.b[1])];
            b[0] >= inside[0] && b[1] >= inside[1] && b[2] <= inside[2] && b[3] <= inside[3]
        })
        .collect();
    let all_runs: Vec<Run> = runs(&candidates, 3.0)
        .into_iter()
        .filter(|r| r.length() >= shortest && r.length() <= long_limit)
        .collect();
    let lengths: Vec<&[P]> = input
        .markups
        .iter()
        .filter(|m| m.page == sheet.page && matches!(m.kind.as_str(), "length" | "polylength"))
        .map(|m| m.points.as_slice())
        .collect();
    let words = crate::sizes::placed(&sheet.words);
    let grid = geo::Grid::new(words.iter().map(|w| w.at), 48.0);

    // Side by side within a foot and a half of building (or twenty points
    // with no scale) may be one thing drawn as two lines.
    let apart = per_point.map(|f| (1.5 / f).clamp(6.0, 40.0)).unwrap_or(20.0);
    let mut covered_count = 0usize;
    let mut hits: Vec<(usize, usize, callout::Shape)> = Vec::new();
    let mut bare: Vec<Run> = Vec::new();
    for (i, run) in all_runs.iter().enumerate() {
        if coverage(run, &lengths, 8.0) >= 0.6 {
            covered_count += 1;
            continue;
        }
        match size_beside(&words, &grid, run) {
            Some((word, shape)) => hits.push((i, word, shape)),
            None => bare.push(run.clone()),
        }
    }
    // Two lines that found the same size written beside them, lying side by
    // side, are one member drawn as two lines: taken off once, down the
    // middle. Two lines each with its own size beside it are two members.
    let mut sized: Vec<(Band, callout::Shape)> = Vec::new();
    let mut done = vec![false; hits.len()];
    for h in 0..hits.len() {
        if done[h] {
            continue;
        }
        done[h] = true;
        let (run, word, shape) = (&all_runs[hits[h].0], hits[h].1, hits[h].2.clone());
        let mut together = vec![run.clone()];
        for o in h + 1..hits.len() {
            if done[o] || hits[o].1 != word {
                continue;
            }
            let other = &all_runs[hits[o].0];
            let beside = other.axis.angle_to(&run.axis) <= 1.0
                && run.axis.off(other.a()).min(run.axis.off(other.b())) <= apart;
            if beside {
                done[o] = true;
                together.push(other.clone());
            }
        }
        for band in bands(&together, apart) {
            sized.push((band, shape.clone()));
        }
    }
    let mut no_size = bands(&bare, apart);
    // Longest first: that is where the steel is.
    let longest = |a: &Run, b: &Run| b.length().partial_cmp(&a.length()).unwrap_or(std::cmp::Ordering::Equal);
    sized.sort_by(|a, b| longest(&a.0.centre, &b.0.centre));
    no_size.sort_by(|a, b| longest(&a.centre, &b.centre));
    let feet = |run: &Run| per_point.map(|f| format!("{:.1} ft", run.length() * f)).unwrap_or_else(|| format!("{:.0} pt", run.length()));
    let area_of = |band: &Band| {
        let r = &band.centre;
        geo::grow(geo::bounds(&[r.a(), r.b()]).unwrap_or([0.0; 4]), 4.0 + band.half_width)
    };
    let mut findings: Vec<Finding> = sized
        .iter()
        .take(200)
        .map(|(band, s)| {
            Finding::new(
                Level::Check,
                format!(
                    "A heavy {} {} long with {} beside it, and no length along it.",
                    if band.lines > 1 { "member drawn as two lines," } else { "line" },
                    feet(&band.centre),
                    s.designation
                ),
            )
            .on(sheet.page)
            .at(area_of(band))
            .fix(
                format!("Take off as {}", s.designation),
                vec![Action::AddLength {
                    page: sheet.page,
                    points: vec![band.centre.a(), band.centre.b()],
                    subject: s.designation.clone(),
                }],
            )
        })
        .collect();
    if settings.list_unsized {
        findings.extend(no_size.iter().take(200).map(|band| {
            Finding::new(Level::Note, format!("A heavy line {} long with no size beside it and no length along it.", feet(&band.centre)))
                .on(sheet.page)
                .at(area_of(band))
                .fix(
                    "Take it off",
                    vec![Action::AddLength {
                        page: sheet.page,
                        points: vec![band.centre.a(), band.centre.b()],
                        subject: String::new(),
                    }],
                )
        }));
    }
    Output {
        title: format!("Missed members — {}", sheet.name),
        summary: format!(
            "{} heavy lines on this sheet. {covered_count} have a length along them. {} have a size written beside them \
             and no length — those are listed. {} more have no size beside them (walls, outlines, edges){}.{}",
            covered_count + sized.len() + no_size.len(),
            sized.len(),
            no_size.len(),
            if settings.list_unsized { " and are listed as notes" } else { "; turn on \"List lines with no size\" in Settings to see them" },
            if sheet.scale.is_none() { " The sheet has no scale, so lengths are in points." } else { "" }
        ),
        findings,
        tables: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plugin_api::{Markup, Scale, Word};

    fn line(a: P, b: P, width: f64) -> Line {
        Line { a, b, width, dashed: false }
    }

    fn a_plan(lines: Vec<Line>, words: Vec<Word>) -> Sheet {
        Sheet {
            page: 0,
            name: "S-201".into(),
            width: 2592.0,
            height: 1728.0,
            scale: Some(Scale { ratio: 48.0, text: String::new() }),
            words,
            lines,
            ..Default::default()
        }
    }

    fn hairlines() -> Vec<Line> {
        (0..40).map(|i| line([100.0, 400.0 + i as f64 * 20.0], [900.0, 400.0 + i as f64 * 20.0], 0.25)).collect()
    }

    #[test]
    fn a_heavy_line_with_a_size_and_no_length_is_found_and_can_be_taken_off() {
        let mut lines = hairlines();
        // A beam, broken in two by the CAD program, with its size above it.
        lines.push(line([1000.0, 300.0], [1200.0, 300.0], 1.4));
        lines.push(line([1200.0, 300.0], [1400.0, 300.0], 1.4));
        // Another beam, taken off.
        lines.push(line([1000.0, 600.0], [1400.0, 600.0], 1.4));
        let words = vec![Word { text: "W16X31".into(), area: [1150.0, 285.0, 1200.0, 294.0], size: 9.0 }];
        let input = Input {
            current_page: 0,
            sheets: vec![a_plan(lines, words)],
            markups: vec![Markup {
                id: "m".into(),
                page: 0,
                kind: "length".into(),
                points: vec![[1001.0, 601.0], [1399.0, 601.0]],
                quantity: 1.0,
                ..Default::default()
            }],
            ..Default::default()
        };
        let out = check(&input);
        assert_eq!(out.findings.len(), 1, "{}", out.summary);
        let f = &out.findings[0];
        assert!(f.message.contains("W16X31") && f.message.contains("22.2 ft"), "{}", f.message);
        match &f.fixes[0].actions[0] {
            Action::AddLength { subject, points, .. } => {
                assert_eq!(subject, "W16X31");
                assert!((geo::dist(points[0], points[1]) - 400.0).abs() < 0.5);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_member_drawn_as_two_lines_is_one_member_taken_off_down_its_middle() {
        let mut lines = hairlines();
        lines.push(line([1000.0, 300.0], [1600.0, 300.0], 1.4));
        lines.push(line([1000.0, 310.0], [1600.0, 310.0], 1.4));
        let words = vec![Word { text: "W8X67 (2-SPAN)(TYP)".into(), area: [1200.0, 250.0, 1320.0, 258.0], size: 8.0 }];
        let input = Input { sheets: vec![a_plan(lines, words)], ..Default::default() };
        let out = check(&input);
        assert_eq!(out.findings.len(), 1, "{:?}", out.findings);
        match &out.findings[0].fixes[0].actions[0] {
            Action::AddLength { points, subject, .. } => {
                assert_eq!(subject, "W8X67");
                assert!((points[0][1] - 305.0).abs() < 0.5, "down the middle: {points:?}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn walls_drawn_as_two_heavy_lines_are_left_alone() {
        let mut lines = hairlines();
        lines.push(line([1000.0, 300.0], [1600.0, 300.0], 1.4));
        lines.push(line([1000.0, 308.0], [1600.0, 308.0], 1.4));
        let input = Input { sheets: vec![a_plan(lines, vec![])], ..Default::default() };
        assert!(check(&input).findings.is_empty());
    }

    #[test]
    fn the_border_and_title_block_are_not_members() {
        let mut lines = hairlines();
        lines.push(line([40.0, 40.0], [2552.0, 40.0], 2.0));
        lines.push(line([2300.0, 60.0], [2300.0, 1680.0], 2.0));
        lines.push(line([2350.0, 900.0], [2550.0, 900.0], 2.0));
        let input = Input { sheets: vec![a_plan(lines, vec![])], ..Default::default() };
        assert!(check(&input).findings.is_empty(), "{:?}", check(&input).findings);
    }
}
