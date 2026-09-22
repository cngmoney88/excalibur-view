//! Points, boxes and lines in sheet space.

pub type P = [f64; 2];
pub type Area = [f64; 4];

pub fn dist(a: P, b: P) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

pub fn centre(area: Area) -> P {
    [(area[0] + area[2]) * 0.5, (area[1] + area[3]) * 0.5]
}

pub fn bounds(points: &[P]) -> Option<Area> {
    let first = points.first()?;
    let mut b = [first[0], first[1], first[0], first[1]];
    for p in points {
        b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])];
    }
    Some(b)
}

pub fn union(a: Area, b: Area) -> Area {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

pub fn grow(a: Area, by: f64) -> Area {
    [a[0] - by, a[1] - by, a[2] + by, a[3] + by]
}

pub fn overlaps(a: Area, b: Area) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

/// Distance from a point to a segment.
pub fn to_segment(p: P, a: P, b: P) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length).clamp(0.0, 1.0)
    };
    dist(p, [a[0] + t * dx, a[1] + t * dy])
}

/// Distance from a point to a run of points.
pub fn to_polyline(p: P, line: &[P]) -> f64 {
    if line.len() == 1 {
        return dist(p, line[0]);
    }
    line.windows(2)
        .map(|w| to_segment(p, w[0], w[1]))
        .fold(f64::INFINITY, f64::min)
}

/// Length along a run of points.
pub fn length(points: &[P]) -> f64 {
    points.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// A straight line as direction and offset, for telling which lines lie
/// along the same line.
#[derive(Clone, Copy, Debug)]
pub struct Axis {
    /// Unit direction, pointing right (or down, when vertical).
    pub dir: P,
    /// Unit normal.
    pub normal: P,
    /// Signed distance of the line from the origin, along the normal.
    pub offset: f64,
}

impl Axis {
    pub fn of(a: P, b: P) -> Option<Axis> {
        let len = dist(a, b);
        if len < 1e-9 {
            return None;
        }
        let mut dir = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
        if dir[0] < -1e-9 || (dir[0].abs() <= 1e-9 && dir[1] < 0.0) {
            dir = [-dir[0], -dir[1]];
        }
        let normal = [-dir[1], dir[0]];
        Some(Axis {
            dir,
            normal,
            offset: normal[0] * a[0] + normal[1] * a[1],
        })
    }

    /// Where a point lies along the line.
    pub fn along(&self, p: P) -> f64 {
        self.dir[0] * p[0] + self.dir[1] * p[1]
    }

    /// How far a point is off the line.
    pub fn off(&self, p: P) -> f64 {
        (self.normal[0] * p[0] + self.normal[1] * p[1] - self.offset).abs()
    }

    /// The angle between two lines, in degrees, 0 to 90.
    pub fn angle_to(&self, other: &Axis) -> f64 {
        let cos = (self.dir[0] * other.dir[0] + self.dir[1] * other.dir[1]).abs().min(1.0);
        cos.acos().to_degrees()
    }

    pub fn at(&self, t: f64) -> P {
        let base = [self.normal[0] * self.offset, self.normal[1] * self.offset];
        [base[0] + self.dir[0] * t, base[1] + self.dir[1] * t]
    }
}

/// How much of `[a, b]` the intervals in `cover` cover, 0 to 1.
pub fn covered(a: f64, b: f64, cover: &mut Vec<(f64, f64)>) -> f64 {
    if b <= a {
        return 0.0;
    }
    cover.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
    let (mut total, mut reach) = (0.0, a);
    for &(s, e) in cover.iter() {
        let (s, e) = (s.max(reach), e.min(b));
        if e > s {
            total += e - s;
            reach = e;
        }
    }
    total / (b - a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_along_the_same_line_share_an_axis() {
        let a = Axis::of([0.0, 10.0], [100.0, 10.0]).unwrap();
        let b = Axis::of([300.0, 10.2], [150.0, 10.2]).unwrap();
        assert!(a.angle_to(&b) < 0.5);
        assert!((a.offset - b.offset).abs() < 0.5);
        assert!(a.along([50.0, 10.0]) < a.along([60.0, 10.0]));
    }

    #[test]
    fn coverage_counts_overlaps_once() {
        let mut c = vec![(0.0, 6.0), (4.0, 8.0), (20.0, 30.0)];
        assert!((covered(0.0, 10.0, &mut c) - 0.8).abs() < 1e-9);
    }
}

/// Boxes filed by where they are, so "what is near here" does not mean
/// looking at every word on the sheet for every line on it.
pub struct Grid {
    cell: f64,
    cells: std::collections::HashMap<(i32, i32), Vec<usize>>,
}

impl Grid {
    pub fn new(areas: impl Iterator<Item = Area>, cell: f64) -> Grid {
        let mut cells: std::collections::HashMap<(i32, i32), Vec<usize>> = std::collections::HashMap::new();
        for (i, a) in areas.enumerate() {
            let (x0, y0, x1, y1) = (
                (a[0] / cell).floor() as i32,
                (a[1] / cell).floor() as i32,
                (a[2] / cell).floor() as i32,
                (a[3] / cell).floor() as i32,
            );
            // A box spread over a great many cells is a title or a border
            // line of text; it is filed under its corners only.
            if (x1 - x0 + 1) * (y1 - y0 + 1) > 64 {
                for key in [(x0, y0), (x1, y1), (x0, y1), (x1, y0)] {
                    cells.entry(key).or_default().push(i);
                }
                continue;
            }
            for x in x0..=x1 {
                for y in y0..=y1 {
                    cells.entry((x, y)).or_default().push(i);
                }
            }
        }
        Grid { cell, cells }
    }

    /// Everything filed within `area`, each once.
    pub fn within(&self, area: Area) -> Vec<usize> {
        let (x0, y0, x1, y1) = (
            (area[0] / self.cell).floor() as i32,
            (area[1] / self.cell).floor() as i32,
            (area[2] / self.cell).floor() as i32,
            (area[3] / self.cell).floor() as i32,
        );
        let mut out = Vec::new();
        if (x1 - x0 + 1) as i64 * (y1 - y0 + 1) as i64 > 40_000 {
            let mut all: Vec<usize> = self.cells.values().flatten().copied().collect();
            all.sort_unstable();
            all.dedup();
            return all;
        }
        for x in x0..=x1 {
            for y in y0..=y1 {
                if let Some(list) = self.cells.get(&(x, y)) {
                    out.extend_from_slice(list);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}
