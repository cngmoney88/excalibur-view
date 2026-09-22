//! Snapping onto a drawing's own line-work.
//!
//! A takeoff is only as good as where its points land. A length measured from
//! "about the corner of the column" to "about the face of the wall" is a
//! guess with a number on it. So as the pointer moves, the point it would put
//! down is pulled onto the drawing itself, the way Revu does it: the end of a
//! line, where two lines cross, the middle of a line, the centre of a circle,
//! or anywhere along a line — in that order of preference — with a mark on
//! screen saying which it caught.
//!
//! The line-work is read out of the PDF once per sheet, as straight segments
//! in sheet space (points, from the top left), and filed in a grid so finding
//! what is near the pointer costs the same on a sheet with fifty lines as on
//! one with half a million.

/// What a snap caught. Earlier is preferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// The end of a line, or a corner.
    Endpoint,
    /// Where two lines cross.
    Intersection,
    /// The centre of a circle.
    Centre,
    /// Halfway along a straight line.
    Midpoint,
    /// Somewhere along a line: the nearest point on it.
    OnLine,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Endpoint => "Endpoint",
            Kind::Intersection => "Intersection",
            Kind::Centre => "Centre",
            Kind::Midpoint => "Midpoint",
            Kind::OnLine => "On line",
        }
    }

    /// How much nearer something of a lower kind has to be to win, as a
    /// fraction of the reach. An end of a line a little further off beats a
    /// point on a line right under the pointer: that is what people aim for.
    fn handicap(self) -> f64 {
        match self {
            Kind::Endpoint => 0.0,
            Kind::Intersection => 0.12,
            Kind::Centre => 0.18,
            Kind::Midpoint => 0.3,
            Kind::OnLine => 0.65,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub at: [f64; 2],
    pub kind: Kind,
}

/// One straight piece of line-work, in sheet space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub a: [f32; 2],
    pub b: [f32; 2],
    /// Whether `a` and `b` are real ends — corners of the drawing — rather
    /// than points where a curve was cut into straight pieces.
    pub real_ends: (bool, bool),
    /// Straight, as drawn. A piece of a curve has no midpoint worth having.
    pub straight: bool,
}

/// A sheet's line-work, filed for finding.
#[derive(Clone, Debug, Default)]
pub struct Geometry {
    pub segments: Vec<Segment>,
    pub centres: Vec<[f32; 2]>,
    size: (f32, f32),
    cell: f32,
    across: usize,
    down: usize,
    cells: Vec<Vec<u32>>,
    centre_cells: Vec<Vec<u32>>,
}

/// More than this and a sheet is mostly hatching and text drawn as lines; the
/// first this-many are plenty to snap to.
pub const MOST: usize = 1_500_000;

impl Geometry {
    pub fn new(size: (f32, f32), mut segments: Vec<Segment>, centres: Vec<[f32; 2]>) -> Geometry {
        segments.truncate(MOST);
        // Around 200 cells across the long side of the sheet, never smaller
        // than a couple of points.
        let cell = (size.0.max(size.1) / 200.0).max(2.0);
        let across = (size.0 / cell).ceil().max(1.0) as usize + 1;
        let down = (size.1 / cell).ceil().max(1.0) as usize + 1;
        let mut geometry = Geometry {
            segments,
            centres,
            size,
            cell,
            across,
            down,
            cells: vec![Vec::new(); across * down],
            centre_cells: vec![Vec::new(); across * down],
        };
        for (i, s) in geometry.segments.iter().enumerate() {
            let length = ((s.b[0] - s.a[0]).powi(2) + (s.b[1] - s.a[1]).powi(2)).sqrt();
            let steps = (length / (cell * 0.5)).ceil().max(1.0) as usize;
            let mut last = usize::MAX;
            for k in 0..=steps {
                let t = k as f32 / steps as f32;
                let p = [s.a[0] + (s.b[0] - s.a[0]) * t, s.a[1] + (s.b[1] - s.a[1]) * t];
                if let Some(c) = geometry.cell_of(p) {
                    if c != last {
                        geometry.cells[c].push(i as u32);
                        last = c;
                    }
                }
            }
        }
        for (i, c) in geometry.centres.iter().enumerate() {
            if let Some(at) = geometry.cell_of(*c) {
                geometry.centre_cells[at].push(i as u32);
            }
        }
        geometry
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    fn cell_of(&self, p: [f32; 2]) -> Option<usize> {
        if !(p[0].is_finite() && p[1].is_finite()) {
            return None;
        }
        let x = (p[0] / self.cell).floor();
        let y = (p[1] / self.cell).floor();
        if x < 0.0 || y < 0.0 || x as usize >= self.across || y as usize >= self.down {
            return None;
        }
        Some(y as usize * self.across + x as usize)
    }

    /// The segments filed near a point, each once.
    fn near(&self, at: [f64; 2], reach: f64) -> Vec<u32> {
        let (x0, y0) = (((at[0] - reach) / self.cell as f64).floor(), ((at[1] - reach) / self.cell as f64).floor());
        let (x1, y1) = (((at[0] + reach) / self.cell as f64).floor(), ((at[1] + reach) / self.cell as f64).floor());
        let clamp_x = |v: f64| v.clamp(0.0, (self.across - 1) as f64) as usize;
        let clamp_y = |v: f64| v.clamp(0.0, (self.down - 1) as f64) as usize;
        let mut found = Vec::new();
        // Zoomed right out, the reach covers much of the sheet; looking at
        // every cell of it would be slow and snapping at that zoom means
        // nothing anyway.
        let cells = (clamp_x(x1) - clamp_x(x0) + 1) * (clamp_y(y1) - clamp_y(y0) + 1);
        if cells > 2500 {
            return found;
        }
        for y in clamp_y(y0)..=clamp_y(y1) {
            for x in clamp_x(x0)..=clamp_x(x1) {
                found.extend_from_slice(&self.cells[y * self.across + x]);
                if found.len() > 20_000 {
                    break;
                }
            }
        }
        found.sort_unstable();
        found.dedup();
        found
    }

    /// The best thing to snap to within `reach` of `at`, if anything, from
    /// the kinds `allowed` lets through.
    pub fn snap(&self, at: [f64; 2], reach: f64, allowed: &dyn Fn(Kind) -> bool) -> Option<Hit> {
        if self.segments.is_empty() && self.centres.is_empty() {
            return None;
        }
        let mut best: Option<(Hit, f64)> = None;
        let mut consider = |p: [f64; 2], kind: Kind| {
            if !allowed(kind) {
                return;
            }
            let d = ((p[0] - at[0]).powi(2) + (p[1] - at[1]).powi(2)).sqrt();
            if d > reach {
                return;
            }
            let score = d + kind.handicap() * reach;
            if best.map_or(true, |(_, s)| score < s) {
                best = Some((Hit { at: p, kind }, score));
            }
        };

        let near = self.near(at, reach);
        // Nearest first, so the pairs checked for crossings are the ones that
        // matter.
        let mut close: Vec<(u32, f64, [f64; 2])> = Vec::with_capacity(near.len());
        for &i in &near {
            let s = &self.segments[i as usize];
            let a = [s.a[0] as f64, s.a[1] as f64];
            let b = [s.b[0] as f64, s.b[1] as f64];
            if s.real_ends.0 {
                consider(a, Kind::Endpoint);
            }
            if s.real_ends.1 {
                consider(b, Kind::Endpoint);
            }
            if s.straight {
                consider([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5], Kind::Midpoint);
            }
            let (p, d) = nearest_on(a, b, at);
            if d <= reach {
                close.push((i, d, p));
            }
        }
        close.sort_by(|x, y| x.1.total_cmp(&y.1));
        if let Some((_, _, p)) = close.first() {
            consider(*p, Kind::OnLine);
        }
        if allowed(Kind::Intersection) {
            let few = &close[..close.len().min(48)];
            for (n, (i, _, _)) in few.iter().enumerate() {
                for (j, _, _) in &few[n + 1..] {
                    let (s, t) = (&self.segments[*i as usize], &self.segments[*j as usize]);
                    if let Some(p) = crossing(s, t) {
                        consider(p, Kind::Intersection);
                    }
                }
            }
        }
        if allowed(Kind::Centre) {
            let (cx, cy) = ((at[0] / self.cell as f64).floor(), (at[1] / self.cell as f64).floor());
            let r = (reach / self.cell as f64).ceil() as i64;
            for y in (cy as i64 - r)..=(cy as i64 + r) {
                for x in (cx as i64 - r)..=(cx as i64 + r) {
                    if x < 0 || y < 0 || x as usize >= self.across || y as usize >= self.down {
                        continue;
                    }
                    for &c in &self.centre_cells[y as usize * self.across + x as usize] {
                        let p = self.centres[c as usize];
                        consider([p[0] as f64, p[1] as f64], Kind::Centre);
                    }
                }
            }
        }
        let _ = self.size;
        best.map(|(hit, _)| hit)
    }
}

/// The nearest point on a segment, and how far it is.
pub fn nearest_on(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> ([f64; 2], f64) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length2 = dx * dx + dy * dy;
    let t = if length2 < 1e-12 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length2).clamp(0.0, 1.0)
    };
    let q = [a[0] + dx * t, a[1] + dy * t];
    (q, ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt())
}

/// Where two segments cross, when they cross somewhere along both. Lines that
/// merely meet end to end are left to the endpoint snap.
fn crossing(s: &Segment, t: &Segment) -> Option<[f64; 2]> {
    let (p, r) = ([s.a[0] as f64, s.a[1] as f64], [(s.b[0] - s.a[0]) as f64, (s.b[1] - s.a[1]) as f64]);
    let (q, u) = ([t.a[0] as f64, t.a[1] as f64], [(t.b[0] - t.a[0]) as f64, (t.b[1] - t.a[1]) as f64]);
    let cross = r[0] * u[1] - r[1] * u[0];
    if cross.abs() < 1e-9 {
        return None;
    }
    let w = [q[0] - p[0], q[1] - p[1]];
    let a = (w[0] * u[1] - w[1] * u[0]) / cross;
    let b = (w[0] * r[1] - w[1] * r[0]) / cross;
    const EDGE: f64 = 1e-6;
    if a <= EDGE || a >= 1.0 - EDGE || b <= EDGE || b >= 1.0 - EDGE {
        return None;
    }
    Some([p[0] + r[0] * a, p[1] + r[1] * a])
}

/// Builds segments from path pieces as a PDF gives them. Coordinates are
/// already in sheet space.
#[derive(Default)]
pub struct Tracer {
    pub segments: Vec<Segment>,
    pub centres: Vec<[f32; 2]>,
    start: Option<[f32; 2]>,
    at: Option<[f32; 2]>,
    /// The on-curve points of the subpath so far, and whether it has been
    /// nothing but curves — which is how a circle comes out of a CAD program.
    curve_points: Vec<[f32; 2]>,
    only_curves: bool,
}

impl Tracer {
    pub fn move_to(&mut self, p: [f32; 2]) {
        self.finish_subpath(false);
        self.start = Some(p);
        self.at = Some(p);
        self.curve_points = vec![p];
        self.only_curves = true;
    }

    pub fn line_to(&mut self, p: [f32; 2]) {
        if let Some(from) = self.at {
            self.push(from, p, (true, true), true);
        }
        self.at = Some(p);
        self.only_curves = false;
    }

    pub fn curve_to(&mut self, c1: [f32; 2], c2: [f32; 2], p: [f32; 2]) {
        let Some(from) = self.at else {
            self.at = Some(p);
            return;
        };
        // Four straight pieces follow a drawing curve closely enough to snap
        // along; only the curve's own ends are corners.
        const PIECES: usize = 4;
        let mut last = from;
        for k in 1..=PIECES {
            let t = k as f32 / PIECES as f32;
            let q = bezier(from, c1, c2, p, t);
            self.push(last, q, (k == 1, k == PIECES), false);
            last = q;
        }
        self.at = Some(p);
        self.curve_points.push(p);
    }

    pub fn close(&mut self) {
        if let (Some(from), Some(start)) = (self.at, self.start) {
            if (from[0] - start[0]).abs() > 1e-3 || (from[1] - start[1]).abs() > 1e-3 {
                self.push(from, start, (true, true), true);
                self.only_curves = false;
            }
        }
        self.finish_subpath(true);
        self.at = self.start;
    }

    pub fn finish(mut self) -> (Vec<Segment>, Vec<[f32; 2]>) {
        self.finish_subpath(false);
        (self.segments, self.centres)
    }

    fn push(&mut self, a: [f32; 2], b: [f32; 2], real_ends: (bool, bool), straight: bool) {
        if self.segments.len() >= MOST {
            return;
        }
        if (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4 {
            return;
        }
        self.segments.push(Segment { a, b, real_ends, straight });
    }

    /// A closed run of curves whose points sit at one distance from their
    /// middle is a circle, and its middle is worth snapping to.
    fn finish_subpath(&mut self, closed: bool) {
        let points = std::mem::take(&mut self.curve_points);
        let circle = self.only_curves && (closed || points.first() == points.last()) && points.len() >= 4;
        self.only_curves = false;
        if !circle {
            return;
        }
        let ring: Vec<[f32; 2]> = if points.first() == points.last() {
            points[..points.len() - 1].to_vec()
        } else {
            points
        };
        if ring.len() < 3 {
            return;
        }
        let n = ring.len() as f32;
        let cx = ring.iter().map(|p| p[0]).sum::<f32>() / n;
        let cy = ring.iter().map(|p| p[1]).sum::<f32>() / n;
        let radii: Vec<f32> = ring.iter().map(|p| ((p[0] - cx).powi(2) + (p[1] - cy).powi(2)).sqrt()).collect();
        let mean = radii.iter().sum::<f32>() / n;
        if mean > 0.2 && radii.iter().all(|r| (r - mean).abs() <= mean * 0.02) {
            self.centres.push([cx, cy]);
        }
    }
}

fn bezier(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    [
        a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
        a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(a: [f32; 2], b: [f32; 2]) -> Segment {
        Segment { a, b, real_ends: (true, true), straight: true }
    }

    fn all(_: Kind) -> bool {
        true
    }

    /// A column grid: two walls meeting at a corner, and a beam line crossing
    /// the middle of one of them.
    fn plan() -> Geometry {
        Geometry::new(
            (2592.0, 1728.0),
            vec![
                line([100.0, 100.0], [500.0, 100.0]),
                line([500.0, 100.0], [500.0, 400.0]),
                line([300.0, 50.0], [300.0, 300.0]),
            ],
            Vec::new(),
        )
    }

    #[test]
    fn the_corner_wins_over_the_line_under_the_pointer() {
        let hit = plan().snap([496.0, 104.0], 9.0, &all).unwrap();
        assert_eq!(hit.kind, Kind::Endpoint);
        assert_eq!(hit.at, [500.0, 100.0]);
    }

    #[test]
    fn where_two_lines_cross_is_caught() {
        let hit = plan().snap([303.0, 103.0], 9.0, &all).unwrap();
        assert_eq!(hit.kind, Kind::Intersection);
        assert!((hit.at[0] - 300.0).abs() < 1e-6 && (hit.at[1] - 100.0).abs() < 1e-6);
    }

    #[test]
    fn halfway_along_a_wall_is_its_midpoint() {
        let hit = plan().snap([502.0, 251.0], 9.0, &all).unwrap();
        assert_eq!(hit.kind, Kind::Midpoint);
        assert_eq!(hit.at, [500.0, 250.0]);
    }

    #[test]
    fn anywhere_else_along_a_line_lands_on_the_line() {
        let hit = plan().snap([420.0, 105.0], 9.0, &all).unwrap();
        assert_eq!(hit.kind, Kind::OnLine);
        assert!((hit.at[0] - 420.0).abs() < 1e-6 && (hit.at[1] - 100.0).abs() < 1e-6);
    }

    #[test]
    fn nothing_within_reach_is_nothing() {
        assert!(plan().snap([900.0, 900.0], 9.0, &all).is_none());
    }

    #[test]
    fn a_kind_turned_off_is_not_offered() {
        let only_ends = |k: Kind| k == Kind::Endpoint;
        assert!(plan().snap([420.0, 105.0], 9.0, &only_ends).is_none());
    }

    #[test]
    fn a_circle_drawn_as_four_curves_has_a_centre_to_snap_to() {
        // How every CAD program writes a circle: four quarter curves.
        let (cx, cy, r) = (1000.0f32, 800.0f32, 40.0f32);
        let k = 0.552_284_8 * r;
        let mut t = Tracer::default();
        t.move_to([cx + r, cy]);
        t.curve_to([cx + r, cy + k], [cx + k, cy + r], [cx, cy + r]);
        t.curve_to([cx - k, cy + r], [cx - r, cy + k], [cx - r, cy]);
        t.curve_to([cx - r, cy - k], [cx - k, cy - r], [cx, cy - r]);
        t.curve_to([cx + k, cy - r], [cx + r, cy - k], [cx + r, cy]);
        t.close();
        let (segments, centres) = t.finish();
        assert_eq!(centres.len(), 1);
        assert!((centres[0][0] - cx).abs() < 0.01 && (centres[0][1] - cy).abs() < 0.01);
        let g = Geometry::new((2592.0, 1728.0), segments, centres);
        let hit = g.snap([1003.0, 797.0], 9.0, &all).unwrap();
        assert_eq!(hit.kind, Kind::Centre);
        // And the circle itself can be snapped along, but a cut between two
        // curve pieces is not a corner.
        let near = [(cx + r * 0.7072 + 2.0) as f64, (cy + r * 0.7072 + 2.0) as f64];
        let on = g.snap(near, 9.0, &all).unwrap();
        assert_eq!(on.kind, Kind::OnLine);
    }

    #[test]
    fn a_closed_rectangle_has_four_corners_and_four_sides() {
        let mut t = Tracer::default();
        t.move_to([10.0, 10.0]);
        t.line_to([110.0, 10.0]);
        t.line_to([110.0, 60.0]);
        t.line_to([10.0, 60.0]);
        t.close();
        let (segments, centres) = t.finish();
        assert_eq!(segments.len(), 4);
        assert!(centres.is_empty());
    }

    #[test]
    fn a_sheet_with_a_great_many_lines_still_answers_quickly() {
        // Two hundred thousand short lines, the density of a hatched section.
        let mut lines = Vec::new();
        for i in 0..200_000u32 {
            let x = (i % 1000) as f32 * 2.5;
            let y = (i / 1000) as f32 * 8.0;
            lines.push(line([x, y], [x + 2.0, y + 3.0]));
        }
        let g = Geometry::new((2592.0, 1728.0), lines, Vec::new());
        let started = std::time::Instant::now();
        for k in 0..200 {
            let _ = g.snap([100.0 + k as f64 * 3.0, 400.0], 9.0, &all);
        }
        // Two hundred looks well inside a frame each, even in a debug build.
        assert!(started.elapsed().as_millis() < 2000, "{:?}", started.elapsed());
    }
}
