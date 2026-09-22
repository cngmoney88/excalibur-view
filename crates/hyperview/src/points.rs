//! Changing a markup's own points after it has been drawn.
//!
//! An area takeoff round a bay with eleven corners is not something anybody
//! wants to draw twice because the twelfth corner was missed. Revu has three
//! ways of changing one — add a point, take one away, and turn a corner into a
//! curve — and they are here for the same reason.
//!
//! Every change goes through the markup's own geometry and is followed by
//! re-reading its measurement, because a shape with a corner moved is a
//! different area and reporting the old number would be the quiet kind of
//! wrong a takeoff cannot have.

use crate::app::App;

/// What a click on a markup's outline does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Put a new corner where the outline was clicked.
    Add,
    /// Take away the corner nearest the click.
    Subtract,
    /// Round the corner nearest the click, or straighten it again.
    Convert,
}

impl Mode {
    pub fn from_command(id: &str) -> Option<Mode> {
        Some(match id {
            "ControlPoint.AddMode" => Mode::Add,
            "ControlPoint.SubtractMode" => Mode::Subtract,
            "ControlPoint.ConvertMode" => Mode::Convert,
            _ => return None,
        })
    }

    pub fn hint(self) -> &'static str {
        match self {
            Mode::Add => "Click on a markup's outline to put a corner there.",
            Mode::Subtract => "Click a corner to take it away.",
            Mode::Convert => "Click a corner to round it off, or again to straighten it.",
        }
    }
}

/// Where on a run of points a click falls: which segment, and how far along.
pub fn nearest_edge(points: &[[f64; 2]], at: [f64; 2], closed: bool) -> Option<(usize, [f64; 2], f64)> {
    if points.len() < 2 {
        return None;
    }
    let count = if closed { points.len() } else { points.len() - 1 };
    let mut best: Option<(usize, [f64; 2], f64)> = None;
    for i in 0..count {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = dx * dx + dy * dy;
        let t = if len2 <= f64::EPSILON {
            0.0
        } else {
            (((at[0] - a[0]) * dx + (at[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
        };
        let on = [a[0] + dx * t, a[1] + dy * t];
        let away = ((on[0] - at[0]).powi(2) + (on[1] - at[1]).powi(2)).sqrt();
        if best.as_ref().map(|(_, _, b)| away < *b).unwrap_or(true) {
            best = Some((i, on, away));
        }
    }
    best
}

/// Which corner a click is nearest, and how far off it is.
pub fn nearest_corner(points: &[[f64; 2]], at: [f64; 2]) -> Option<(usize, f64)> {
    let mut best: Option<(usize, f64)> = None;
    for (i, p) in points.iter().enumerate() {
        let away = ((p[0] - at[0]).powi(2) + (p[1] - at[1]).powi(2)).sqrt();
        if best.as_ref().map(|(_, b)| away < *b).unwrap_or(true) {
            best = Some((i, away));
        }
    }
    best
}

/// Rounds one corner of a run of points into a short curve.
///
/// A fillet rather than a true curve, because a PDF annotation carries a run
/// of points and nothing else: what goes in the file is the curve, sampled.
/// The radius is a share of the shorter of the two sides, so a corner between
/// two short sides does not round into the middle of the shape.
pub fn round_corner(points: &[[f64; 2]], at: usize, closed: bool) -> Vec<[f64; 2]> {
    let n = points.len();
    if n < 3 || at >= n {
        return points.to_vec();
    }
    if !closed && (at == 0 || at == n - 1) {
        // The two ends of an open run are not corners.
        return points.to_vec();
    }
    let before = points[(at + n - 1) % n];
    let here = points[at];
    let after = points[(at + 1) % n];

    let reach = |a: [f64; 2], b: [f64; 2]| ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    let in_leg = reach(before, here);
    let out_leg = reach(here, after);
    if in_leg < 1e-6 || out_leg < 1e-6 {
        return points.to_vec();
    }
    let radius = (in_leg.min(out_leg) * 0.35).max(0.5);

    let towards = |from: [f64; 2], to: [f64; 2], by: f64| {
        let run = reach(from, to).max(1e-9);
        [
            from[0] + (to[0] - from[0]) / run * by,
            from[1] + (to[1] - from[1]) / run * by,
        ]
    };
    let start = towards(here, before, radius);
    let end = towards(here, after, radius);

    const STEPS: usize = 8;
    let mut curve = Vec::with_capacity(STEPS + 1);
    for step in 0..=STEPS {
        let t = step as f64 / STEPS as f64;
        let u = 1.0 - t;
        // A quadratic through the corner: the corner itself is the control
        // point, which is what a fillet is.
        curve.push([
            u * u * start[0] + 2.0 * u * t * here[0] + t * t * end[0],
            u * u * start[1] + 2.0 * u * t * here[1] + t * t * end[1],
        ]);
    }

    let mut out = Vec::with_capacity(n + STEPS);
    out.extend_from_slice(&points[..at]);
    out.extend_from_slice(&curve);
    out.extend_from_slice(&points[at + 1..]);
    out
}

impl App {
    /// Acts on a click when a point mode is in hand.
    ///
    /// Comes back true when it did something, so the canvas knows not to treat
    /// the click as an ordinary selection.
    pub fn edit_points_at(&mut self, at: [f64; 2]) -> bool {
        let Some(mode) = self.points_mode else {
            return false;
        };
        let (index, sheet_points, closed, reach) = {
            let Some(doc) = self.doc() else { return false };
            let Some(index) = doc.selected else {
                self.status = "Select the markup whose points you want to change.".into();
                return false;
            };
            let Some(mark) = doc.marks.get(index) else { return false };
            let frame = doc.frame();
            let closed = matches!(
                mark.markup.subtype(),
                annot::Subtype::Polygon | annot::Subtype::Square | annot::Subtype::Circle
            );
            (
                index,
                mark.on_sheet(&frame),
                closed,
                14.0 / doc.view.zoom as f64,
            )
        };
        if sheet_points.len() < 2 {
            self.status = "That markup has no corners to change.".into();
            return false;
        }

        let changed = match mode {
            Mode::Add => {
                let Some((segment, on, away)) = nearest_edge(&sheet_points, at, closed) else {
                    return false;
                };
                if away > reach {
                    self.status = "Click on the markup's own outline.".into();
                    return false;
                }
                let mut points = sheet_points.clone();
                points.insert(segment + 1, on);
                Some(points)
            }
            Mode::Subtract => {
                let Some((corner, away)) = nearest_corner(&sheet_points, at) else {
                    return false;
                };
                if away > reach {
                    self.status = "Click the corner you want to take away.".into();
                    return false;
                }
                let least = if closed { 3 } else { 2 };
                if sheet_points.len() <= least {
                    self.status = format!(
                        "A {} needs at least {least} corners, so this one cannot lose \
                         another.",
                        if closed { "shape" } else { "line" }
                    );
                    return false;
                }
                let mut points = sheet_points.clone();
                points.remove(corner);
                Some(points)
            }
            Mode::Convert => {
                let Some((corner, away)) = nearest_corner(&sheet_points, at) else {
                    return false;
                };
                if away > reach {
                    self.status = "Click the corner you want to round off.".into();
                    return false;
                }
                Some(round_corner(&sheet_points, corner, closed))
            }
        };

        let Some(points) = changed else { return false };
        let Some(doc) = self.doc_mut() else { return false };
        doc.checkpoint_named(match mode {
            Mode::Add => "Add a corner",
            Mode::Subtract => "Take a corner away",
            Mode::Convert => "Round a corner",
        });
        let frame = doc.frame();
        let in_pdf = frame.points_to_pdf(&points);
        if let Some(mark) = doc.marks.get_mut(index) {
            // A shape with only its rectangle cannot hold corners, so changing
            // one turns it into the shape it now is.
            if matches!(
                mark.markup.subtype(),
                annot::Subtype::Square | annot::Subtype::Circle
            ) {
                mark.markup
                    .set("Subtype", pdf::Object::name(annot::Subtype::Polygon.as_str()));
            }
            mark.markup.set_vertices(&in_pdf);
            mark.markup.dict.remove("AP");
            mark.changed = true;
        }
        doc.remeasure_selection();
        doc.dirty = true;
        self.status = format!(
            "{} corners now. Anything it measures reads what it measures now.",
            points.len()
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_square() -> Vec<[f64; 2]> {
        vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]]
    }

    #[test]
    fn a_click_on_an_edge_finds_that_edge_and_the_spot_on_it() {
        let (segment, on, away) = nearest_edge(&a_square(), [50.0, 2.0], true).expect("an edge");
        assert_eq!(segment, 0);
        assert!((on[0] - 50.0).abs() < 1e-9 && on[1].abs() < 1e-9);
        assert!((away - 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_closed_shape_has_an_edge_back_to_where_it_started() {
        // Without it, clicking the left-hand wall of a room finds the top.
        let (segment, _, away) = nearest_edge(&a_square(), [2.0, 50.0], true).expect("an edge");
        assert_eq!(segment, 3);
        assert!(away < 3.0);
    }

    #[test]
    fn an_open_run_has_no_edge_back_to_the_start() {
        let run = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0]];
        let (segment, _, _) = nearest_edge(&run, [2.0, 50.0], false).expect("an edge");
        // Nearest is the first segment, not an imaginary closing one.
        assert!(segment < 2, "{segment}");
    }

    #[test]
    fn a_corner_rounds_into_a_curve_that_starts_and_ends_on_the_sides() {
        let rounded = round_corner(&a_square(), 1, true);
        assert!(rounded.len() > a_square().len());
        // The corner itself is gone, and nothing has left the shape.
        assert!(!rounded.iter().any(|p| (p[0] - 100.0).abs() < 1e-9 && p[1].abs() < 1e-9));
        for p in &rounded {
            assert!(p[0] >= -0.001 && p[0] <= 100.001, "{p:?}");
            assert!(p[1] >= -0.001 && p[1] <= 100.001, "{p:?}");
        }
    }

    #[test]
    fn the_ends_of_an_open_run_are_not_corners_to_round() {
        let run = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0]];
        assert_eq!(round_corner(&run, 0, false), run);
        assert_eq!(round_corner(&run, 2, false), run);
        assert!(round_corner(&run, 1, false).len() > 3);
    }

    #[test]
    fn a_corner_between_two_short_sides_rounds_gently() {
        // A radius taken from the longer side would round through the middle
        // of the shape and come out inside out.
        let tight = vec![[0.0, 0.0], [4.0, 0.0], [4.0, 200.0], [0.0, 200.0]];
        let rounded = round_corner(&tight, 1, true);
        for p in &rounded {
            assert!(p[0] >= -0.001 && p[0] <= 4.001, "{p:?}");
        }
    }
}
