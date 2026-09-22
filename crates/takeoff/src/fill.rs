//! Dynamic Fill: finding the shape of a room from the lines already on the
//! drawing, instead of tracing twenty corners by hand.
//!
//! Somebody clicks inside a bounded area. The fill spreads from that point
//! until it meets something dark — a wall, a grid line, a boundary the user
//! drew to close a doorway — and the shape it ends up with becomes an area
//! measurement.
//!
//! **The thing that makes this trustworthy is knowing when it has gone wrong.**
//! A region that is not quite closed lets the fill escape, and what comes back
//! is the area of everything it could reach. That is the classic way a takeoff
//! ends up with a number nobody checked: it looks like a measurement, it is a
//! perfectly good number, and it is the area of half the drawing. So a fill
//! that reaches the edge of where it was allowed to look, or swallows most of
//! what it was shown, comes back as [`Escaped`] and **produces no number at
//! all**. Saying "this is not closed, look here" is worth more than any area.
//!
//! Nothing here touches pdfium or the screen: it is arithmetic over a grid of
//! booleans, so it can be tested on shapes whose answers are known exactly.

/// How much of the window a fill may cover before it stops being a room.
pub const MOST: f64 = 0.95;

/// What the fill is not allowed to cross: `true` is a wall.
#[derive(Clone)]
pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub solid: Vec<bool>,
}

impl Mask {
    pub fn new(width: usize, height: usize) -> Mask {
        Mask {
            width,
            height,
            solid: vec![false; width * height],
        }
    }

    pub fn at(&self, x: usize, y: usize) -> bool {
        self.solid[y * self.width + x]
    }

    pub fn set(&mut self, x: usize, y: usize, solid: bool) {
        if x < self.width && y < self.height {
            let w = self.width;
            self.solid[y * w + x] = solid;
        }
    }

    /// Draws a line of boundary, the way a user closes off a doorway.
    pub fn draw_line(&mut self, from: (f64, f64), to: (f64, f64), thickness: f64) {
        let steps = ((to.0 - from.0).abs().max((to.1 - from.1).abs()) * 2.0).ceil() as i64 + 1;
        let radius = (thickness * 0.5).max(0.5);
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let x = from.0 + (to.0 - from.0) * t;
            let y = from.1 + (to.1 - from.1) * t;
            let r = radius.ceil() as i64;
            for dy in -r..=r {
                for dx in -r..=r {
                    if (dx * dx + dy * dy) as f64 > radius * radius + 0.25 {
                        continue;
                    }
                    let (px, py) = (x as i64 + dx, y as i64 + dy);
                    if px >= 0 && py >= 0 {
                        self.set(px as usize, py as usize, true);
                    }
                }
            }
        }
    }
}

/// What came of a fill.
#[derive(Clone, Debug, PartialEq)]
pub enum Fill {
    /// A closed region, and the shape of it.
    Found(Region),
    /// The fill escaped, or covered so much of what it was shown that it
    /// cannot be a room. **No area is reported**, on purpose.
    Escaped(Escaped),
    /// The point clicked was on a line, not inside anything.
    OnALine,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    /// The outline, in cell coordinates, closed and simplified.
    pub outline: Vec<[f64; 2]>,
    /// How many cells were filled. This is the area, exactly, and it is the
    /// number to trust: it comes from counting, not from a simplified outline.
    pub cells: usize,
    /// The outer outline's own area. That is `cells` plus `hole_cells`: what a
    /// single closed path round the outside covers, which is more than was
    /// filled whenever there is anything enclosed.
    pub outline_area: f64,
    /// Everything the fill went round rather than into — columns, shafts,
    /// stairs, anything enclosed. Their outlines, largest first.
    ///
    /// These matter for more than drawing. `outline` is a single closed path
    /// around the outside, so the shape on its own **over-measures by
    /// `hole_cells`**. Whoever turns this into a markup has to cut them out,
    /// and be told if it cannot.
    pub holes: Vec<Vec<[f64; 2]>>,
    /// Cells enclosed by the outline but not filled: what `outline` covers and
    /// `cells` does not. Exact, and counts every enclosed lump however small.
    pub hole_cells: usize,
    /// Enclosed lumps too small to be worth drawing. They are in `hole_cells`
    /// all the same — counted, just not cluttering the sheet.
    pub holes_not_drawn: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Escaped {
    /// It reached the edge of the area it was allowed to look at, which means
    /// the region is open somewhere, or larger than the window.
    OffTheEdge,
    /// It covered nearly everything it was shown. Technically closed, but not
    /// a room.
    TookEverything,
}

impl Escaped {
    /// What to tell somebody, in words they can act on.
    pub fn why(self) -> &'static str {
        match self {
            Escaped::OffTheEdge => {
                "That area is not closed — the fill ran off the edge of the sheet. Draw a \
                 boundary across the opening, or zoom out so the whole room is on screen, \
                 and try again. No area has been measured."
            }
            Escaped::TookEverything => {
                "That fill covered nearly everything on screen, so it has not been measured. \
                 It usually means a gap somewhere in the outline."
            }
        }
    }
}

/// Fills from a point.
///
/// `most` is the largest share of the whole visible area a genuine region is
/// allowed to be — [`MOST`] unless there is a reason to differ. Above it, the
/// fill is treated as an escape, because one covering the whole window is
/// telling you about a gap and not about a room.
pub fn flood(mask: &Mask, seed: (usize, usize), most: f64) -> Fill {
    if seed.0 >= mask.width || seed.1 >= mask.height {
        return Fill::OnALine;
    }
    if mask.at(seed.0, seed.1) {
        return Fill::OnALine;
    }

    let (w, h) = (mask.width, mask.height);
    let mut inside = vec![false; w * h];
    let mut stack = vec![seed];
    inside[seed.1 * w + seed.0] = true;
    let mut cells = 0usize;
    let mut touched_edge = false;

    // Scanline fill: whole runs at a time rather than a cell at a time, which
    // on a sheet-sized region is the difference between instant and a pause.
    while let Some((sx, sy)) = stack.pop() {
        let mut left = sx;
        while left > 0 && !mask.at(left - 1, sy) && !inside[sy * w + left - 1] {
            left -= 1;
            inside[sy * w + left] = true;
        }
        let mut right = sx;
        while right + 1 < w && !mask.at(right + 1, sy) && !inside[sy * w + right + 1] {
            right += 1;
            inside[sy * w + right] = true;
        }
        if left == 0 || right == w - 1 || sy == 0 || sy == h - 1 {
            touched_edge = true;
        }
        cells += right - left + 1;

        for x in left..=right {
            for ny in [sy.wrapping_sub(1), sy + 1] {
                if ny >= h {
                    continue;
                }
                if !mask.at(x, ny) && !inside[ny * w + x] {
                    inside[ny * w + x] = true;
                    stack.push((x, ny));
                }
            }
        }
    }

    if touched_edge {
        return Fill::Escaped(Escaped::OffTheEdge);
    }
    // Measured against everything on screen rather than against the open space.
    // A room in a tight view legitimately fills all the open space there is;
    // what is not legitimate is a fill that has taken the whole window, which
    // happens when a drawing's own border stops a leak before it reaches the
    // edge and the result looks closed while being the whole sheet.
    let total = (w * h) as f64;
    if total > 0.0 && cells as f64 / total > most {
        return Fill::Escaped(Escaped::TookEverything);
    }

    // Anything enclosed that the fill went round: a column, a shaft, a stair.
    // Taken out of the area, because a slab does not run through a column.
    let (holes, hole_cells, holes_not_drawn) = enclosed(mask, &inside, w, h);

    let raw = trace(&inside, w, h);
    // The outer boundary encloses what was filled **and** everything enclosed
    // within it, so that is what it is fitted against. The difference between
    // the two is `hole_cells`, and it is the caller's job to cut it out.
    let outline = simplify_to_fit(&raw, (cells + hole_cells) as f64);
    let outline_area = area_of(&outline);

    Fill::Found(Region {
        outline,
        cells,
        outline_area,
        holes,
        hole_cells,
        holes_not_drawn,
    })
}

/// Areas wholly surrounded by the filled region.
///
/// Found by filling inwards from outside the region: anything not solid, not
/// part of the region and not reachable from the outside is enclosed by it.
fn enclosed(
    // The mask itself is no longer consulted here: a lump the sea cannot reach
    // is enclosed whether it is solid line-work or empty paper, and both count
    // the same way against the area. Kept in the signature because the caller
    // reads better for it and because losing the distinction silently is
    // exactly the bug this was rewritten to fix.
    _mask: &Mask,
    inside: &[bool],
    w: usize,
    h: usize,
) -> (Vec<Vec<[f64; 2]>>, usize, usize) {
    // Everything the region touches, plus the region: the "sea" starts outside
    // all of it.
    let mut sea = vec![false; w * h];
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let push = |x: usize, y: usize, sea: &mut Vec<bool>, stack: &mut Vec<(usize, usize)>| {
        if !sea[y * w + x] && !inside[y * w + x] {
            sea[y * w + x] = true;
            stack.push((x, y));
        }
    };
    for x in 0..w {
        push(x, 0, &mut sea, &mut stack);
        push(x, h - 1, &mut sea, &mut stack);
    }
    for y in 0..h {
        push(0, y, &mut sea, &mut stack);
        push(w - 1, y, &mut sea, &mut stack);
    }
    while let Some((x, y)) = stack.pop() {
        let neighbours = [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ];
        for (nx, ny) in neighbours {
            if nx >= w || ny >= h {
                continue;
            }
            if sea[ny * w + nx] || inside[ny * w + nx] {
                continue;
            }
            sea[ny * w + nx] = true;
            stack.push((nx, ny));
        }
    }

    // What is left — not region, not sea — is enclosed.
    let mut seen = vec![false; w * h];
    let mut holes: Vec<(usize, Vec<[f64; 2]>)> = Vec::new();
    let mut total = 0usize;
    let mut skipped = 0usize;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if sea[i] || inside[i] || seen[i] {
                continue;
            }
            // One enclosed lump. Its own walls are walked through, so a shaft
            // and the wall round it are one lump rather than several, but only
            // the space inside counts as a void: the wall is not floor either.
            let mut lump = vec![false; w * h];
            let mut count = 0usize;
            let mut stack = vec![(x, y)];
            seen[i] = true;
            lump[i] = true;
            while let Some((cx, cy)) = stack.pop() {
                count += 1;
                let neighbours = [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ];
                for (nx, ny) in neighbours {
                    if nx >= w || ny >= h {
                        continue;
                    }
                    let j = ny * w + nx;
                    if sea[j] || inside[j] || seen[j] {
                        continue;
                    }
                    seen[j] = true;
                    lump[j] = true;
                    stack.push((nx, ny));
                }
            }
            // Everything enclosed counts, whether it is a solid column the
            // fill went round or a void it could not reach. Either way the
            // outer outline covers it and the fill did not, so leaving any of
            // it out of the arithmetic would let the shape measure more than
            // was actually filled — quietly, which is the worst way.
            total += count;
            // Only the ones worth looking at get an outline. A handful of
            // cells inside a wall is a smudge in the drawing, not a column,
            // and cluttering the sheet with it helps nobody. It is still
            // counted above, so nothing goes missing.
            if count < DRAW_HOLES_ABOVE {
                skipped += 1;
                continue;
            }
            let raw = trace(&lump, w, h);
            holes.push((count, simplify_to_fit(&raw, count as f64)));
        }
    }
    holes.sort_by(|a, b| b.0.cmp(&a.0));
    (holes.into_iter().map(|(_, o)| o).collect(), total, skipped)
}

/// Enclosed lumps smaller than this are counted but not drawn.
const DRAW_HOLES_ABOVE: usize = 16;

/// Walks the edge of a filled region and returns its outline as a closed loop
/// of cell corners.
///
/// Built from the boundary edges rather than by chasing cells: every side of
/// every filled cell whose neighbour is empty is one directed edge, wound so
/// the region is always on the same side, and the edges chain head to tail into
/// loops. Done this way the outline of a corridor one cell wide is a rectangle
/// rather than a line with no area, a shape that touches itself at a corner
/// does not send the walk off down the wrong branch, and the area of the loop
/// is the number of filled cells exactly, before any simplifying.
fn trace(inside: &[bool], w: usize, h: usize) -> Vec<[f64; 2]> {
    let filled = |x: i64, y: i64| -> bool {
        x >= 0
            && y >= 0
            && (x as usize) < w
            && (y as usize) < h
            && inside[y as usize * w + x as usize]
    };

    // Directed boundary edges, keyed by where they start. A corner can start
    // more than one where a region pinches to a point, so they are kept in a
    // list and taken one at a time.
    let mut from: std::collections::HashMap<(i64, i64), Vec<(i64, i64)>> =
        std::collections::HashMap::new();
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            if !filled(x, y) {
                continue;
            }
            let mut edge = |a: (i64, i64), b: (i64, i64)| {
                from.entry(a).or_default().push(b);
            };
            if !filled(x, y - 1) {
                edge((x, y), (x + 1, y));
            }
            if !filled(x + 1, y) {
                edge((x + 1, y), (x + 1, y + 1));
            }
            if !filled(x, y + 1) {
                edge((x + 1, y + 1), (x, y + 1));
            }
            if !filled(x - 1, y) {
                edge((x, y + 1), (x, y));
            }
        }
    }

    // Start where the answer is known: the top-left corner of the topmost,
    // leftmost filled cell is always on the outer boundary, and the edge
    // leaving it always runs right. Starting anywhere else means guessing
    // which loop you are on, and guessing wrong traces a path that crosses
    // itself — which draws as a fan of triangles and measures nonsense.
    let mut start = None;
    'find: for y in 0..h as i64 {
        for x in 0..w as i64 {
            if filled(x, y) {
                start = Some((x, y));
                break 'find;
            }
        }
    }
    let Some(start) = start else {
        return Vec::new();
    };

    let direction = |a: (i64, i64), b: (i64, i64)| -> usize {
        match (b.0 - a.0, b.1 - a.1) {
            (1, 0) => 0,
            (0, 1) => 1,
            (-1, 0) => 2,
            _ => 3,
        }
    };

    let mut out_points: Vec<[f64; 2]> = Vec::new();
    let mut at = start;
    let mut came_from = 3usize; // as if we had arrived heading up into it
    for _ in 0..(w * h * 4 + 16) {
        out_points.push([at.0 as f64, at.1 as f64]);
        let Some(out) = from.get_mut(&at) else {
            // Ran into a corner with nothing leaving it. That cannot happen on
            // a closed boundary, so whatever this is, it is not one.
            return Vec::new();
        };
        if out.is_empty() {
            return Vec::new();
        }
        // Where a region pinches to a point — two cells meeting corner to
        // corner — a corner has two edges leaving it. The sharpest turn
        // clockwise is the one that keeps the region on the same side.
        let want = [
            (came_from + 1) % 4,
            came_from,
            (came_from + 3) % 4,
            (came_from + 2) % 4,
        ];
        let mut chosen = 0usize;
        'turn: for d in want {
            for (i, dest) in out.iter().enumerate() {
                if direction(at, *dest) == d {
                    chosen = i;
                    break 'turn;
                }
            }
        }
        let next = out.remove(chosen);
        came_from = direction(at, next);
        at = next;
        if at == start {
            // Closed. Anything that did not close is discarded above rather
            // than returned as a shape, because a shape that does not close is
            // not a measurement.
            return out_points;
        }
    }
    Vec::new()
}

/// Ramer–Douglas–Peucker, with its own stack.
///
/// Iterative rather than recursive on purpose: the outline of a curved region
/// traced pixel by pixel is thousands of points that split one at a time, and
/// the recursive form runs the thread out of stack on exactly the shapes this
/// exists to handle.
pub fn simplify(points: &[[f64; 2]], epsilon: f64) -> Vec<[f64; 2]> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;

    let mut spans = vec![(0usize, points.len() - 1)];
    while let Some((from, to)) = spans.pop() {
        if to <= from + 1 {
            continue;
        }
        let (a, b) = (points[from], points[to]);
        let mut worst = 0.0;
        let mut at = from;
        for (i, p) in points.iter().enumerate().take(to).skip(from + 1) {
            let d = distance_to_line(*p, a, b);
            if d > worst {
                worst = d;
                at = i;
            }
        }
        if worst > epsilon && at > from {
            keep[at] = true;
            spans.push((from, at));
            spans.push((at, to));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(p, _)| *p)
        .collect()
}

fn distance_to_line(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = (dx * dx + dy * dy).sqrt();
    if length < 1e-9 {
        return ((p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2)).sqrt();
    }
    ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / length
}

/// Simplifies as far as it can without the shape's area drifting from the
/// number of cells that were actually filled.
///
/// This is the join between the picture and the number. The area a measurement
/// reports comes from the outline, and the outline is a simplification of a
/// staircase of pixels. Simplify too eagerly and the shape somebody sees is no
/// longer the shape that was measured — so the tolerance is what decides how
/// far to go, not a fixed number of points.
pub fn simplify_to_fit(raw: &[[f64; 2]], exact_cells: f64) -> Vec<[f64; 2]> {
    if raw.len() < 4 {
        return raw.to_vec();
    }
    const TOLERANCE: f64 = 0.002;
    let mut best = raw.to_vec();
    for epsilon in [6.0, 4.0, 3.0, 2.0, 1.5, 1.0, 0.7, 0.5] {
        let tried = simplify(raw, epsilon);
        if tried.len() < 4 {
            continue;
        }
        let drift = (area_of(&tried) - exact_cells).abs() / exact_cells.max(1.0);
        if drift <= TOLERANCE {
            return tried;
        }
        best = tried;
    }
    // Nothing simplified cleanly enough: the staircase itself is the honest
    // answer, even though it has more points than anybody wants.
    let _ = best;
    raw.to_vec()
}

/// The area of a closed polygon, by the shoelace formula.
pub fn area_of(points: &[[f64; 2]]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut twice = 0.0;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        twice += a[0] * b[1] - b[0] * a[1];
    }
    (twice * 0.5).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A room: walls round the outside of a `w` by `h` grid.
    fn a_room(w: usize, h: usize) -> Mask {
        let mut mask = Mask::new(w, h);
        for x in 0..w {
            mask.set(x, 0, true);
            mask.set(x, h - 1, true);
        }
        for y in 0..h {
            mask.set(0, y, true);
            mask.set(w - 1, y, true);
        }
        mask
    }

    #[test]
    fn a_closed_room_measures_the_area_inside_its_walls() {
        // Walls one cell thick round a 40 x 30 grid leaves 38 x 28 inside.
        let mask = a_room(40, 30);
        let Fill::Found(region) = flood(&mask, (20, 15), MOST) else {
            panic!("a closed room should fill");
        };
        assert_eq!(region.cells, 38 * 28);
        assert_eq!(region.hole_cells, 0, "nothing enclosed in an empty room");
        // With nothing enclosed, the shape drawn is the number measured.
        let drift = (region.outline_area - region.cells as f64).abs() / region.cells as f64;
        assert!(drift < 0.01, "outline {} vs cells {}", region.outline_area, region.cells);
    }

    #[test]
    fn a_room_with_a_doorway_left_open_measures_nothing_at_all() {
        let mut mask = a_room(40, 30);
        // Knock a doorway in the wall.
        for y in 12..18 {
            mask.set(0, y, false);
        }
        match flood(&mask, (20, 15), MOST) {
            Fill::Escaped(why) => {
                assert_eq!(why, Escaped::OffTheEdge);
                // The message has to tell somebody what to do about it.
                assert!(why.why().contains("not closed"), "{}", why.why());
                assert!(why.why().contains("No area has been measured"));
            }
            other => panic!("an open room must not produce a number: {other:?}"),
        }
    }

    #[test]
    fn closing_the_doorway_by_hand_makes_it_measurable_again() {
        let mut mask = a_room(40, 30);
        for y in 12..18 {
            mask.set(0, y, false);
        }
        assert!(matches!(flood(&mask, (20, 15), MOST), Fill::Escaped(_)));

        // This is what a user drawing a line across the opening does.
        mask.draw_line((0.0, 11.0), (0.0, 18.0), 2.0);
        let Fill::Found(region) = flood(&mask, (20, 15), MOST) else {
            panic!("closing the opening should make it fill");
        };
        assert!(region.cells > 900, "{}", region.cells);
    }

    #[test]
    fn clicking_on_a_wall_measures_nothing() {
        let mask = a_room(40, 30);
        assert_eq!(flood(&mask, (0, 0), MOST), Fill::OnALine);
        assert_eq!(flood(&mask, (20, 0), MOST), Fill::OnALine);
    }

    #[test]
    fn a_column_inside_a_slab_is_taken_out_of_the_area() {
        let mut mask = a_room(60, 40);
        // A column, eight by eight, in the middle.
        for y in 16..24 {
            for x in 26..34 {
                mask.set(x, y, true);
            }
        }
        let Fill::Found(region) = flood(&mask, (5, 5), MOST) else {
            panic!("the slab should fill");
        };
        // Inside the walls is 58 x 38; the column takes 64 of it.
        assert_eq!(region.cells, 58 * 38 - 64);
        // The column is enclosed by the fill, so a single closed path round
        // the outside covers it. Saying so is what lets it be cut out rather
        // than quietly measured as slab.
        assert_eq!(region.hole_cells, 64);
        assert_eq!(region.holes.len(), 1);
        let covered = region.cells + region.hole_cells;
        let drift = (region.outline_area - covered as f64).abs() / covered as f64;
        assert!(drift < 0.01, "the outline covers the column too: {drift}");
    }

    #[test]
    fn a_shaft_the_fill_goes_round_is_reported_as_a_hole() {
        let mut mask = a_room(60, 40);
        // A shaft with its own walls: the fill goes round it, and the space
        // inside it is not part of the slab.
        for x in 24..36 {
            mask.set(x, 14, true);
            mask.set(x, 26, true);
        }
        for y in 14..27 {
            mask.set(24, y, true);
            mask.set(35, y, true);
        }
        let Fill::Found(region) = flood(&mask, (5, 5), MOST) else {
            panic!("the slab should fill");
        };
        assert_eq!(region.holes.len(), 1, "one shaft");
        // The shaft and the walls round it: 12 by 13 enclosed altogether.
        assert_eq!(region.hole_cells, 12 * 13);
        assert!(!region.holes[0].is_empty());
    }

    #[test]
    fn a_fill_that_swallows_the_whole_window_is_refused() {
        // No walls at all, but bounded by the edge — which is caught first.
        let mask = Mask::new(40, 30);
        assert!(matches!(
            flood(&mask, (20, 15), MOST),
            Fill::Escaped(Escaped::OffTheEdge)
        ));
    }

    #[test]
    fn a_region_covering_nearly_everything_inside_the_walls_is_refused() {
        let mask = a_room(40, 30);
        // Allowing only half, a fill of the whole inside is too much to be a
        // room, and is refused rather than measured.
        match flood(&mask, (20, 15), 0.5) {
            Fill::Escaped(why) => assert_eq!(why, Escaped::TookEverything),
            other => panic!("it should have been refused: {other:?}"),
        }
    }

    #[test]
    fn an_l_shaped_room_keeps_its_corner() {
        let mut mask = Mask::new(40, 40);
        for x in 0..40 {
            mask.set(x, 0, true);
            mask.set(x, 39, true);
        }
        for y in 0..40 {
            mask.set(0, y, true);
            mask.set(39, y, true);
        }
        // Block out the top right quarter, making an L.
        for y in 1..20 {
            for x in 20..39 {
                mask.set(x, y, true);
            }
        }
        let Fill::Found(region) = flood(&mask, (5, 5), MOST) else {
            panic!("an L should fill");
        };
        // 38 x 38 less the 19 x 19 that was blocked out.
        assert_eq!(region.cells, 38 * 38 - 19 * 19);
        let drift = (region.outline_area - region.cells as f64).abs() / region.cells as f64;
        assert!(drift < 0.01, "an L's outline must match its area: {drift}");
        // Six corners, give or take the odd extra point.
        assert!(
            region.outline.len() >= 6 && region.outline.len() <= 12,
            "an L should simplify to about six corners, got {}",
            region.outline.len()
        );
    }

    #[test]
    fn a_corridor_one_cell_wide_still_has_an_area() {
        let mut mask = Mask::new(30, 9);
        for x in 0..30 {
            mask.set(x, 0, true);
            mask.set(x, 2, true);
            mask.set(x, 8, true);
        }
        for y in 0..9 {
            mask.set(0, y, true);
            mask.set(29, y, true);
        }
        let Fill::Found(region) = flood(&mask, (15, 1), MOST) else {
            panic!("a corridor should fill");
        };
        assert_eq!(region.cells, 28, "one cell tall, twenty-eight long");
        assert!(
            region.outline_area > 20.0,
            "a thin corridor must not come back with no area: {}",
            region.outline_area
        );
    }

    #[test]
    fn simplifying_never_moves_the_area_more_than_a_whisker() {
        // A staircase of pixels round a circle: the worst case for tracing.
        let mut mask = Mask::new(80, 80);
        for y in 0..80 {
            for x in 0..80 {
                let d = ((x as f64 - 40.0).powi(2) + (y as f64 - 40.0).powi(2)).sqrt();
                if d > 30.0 {
                    mask.set(x, y, true);
                }
            }
        }
        let Fill::Found(region) = flood(&mask, (40, 40), MOST) else {
            panic!("a circle should fill");
        };
        let drift = (region.outline_area - region.cells as f64).abs() / region.cells as f64;
        assert!(drift < 0.01, "a circle drifted by {drift}");
    }

    #[test]
    fn a_boundary_that_pinches_to_a_point_does_not_send_the_trace_astray() {
        // Two blocks touching corner to corner inside a room. The outline of
        // the space around them passes through that shared corner twice, and
        // taking the wrong edge there produces a loop that crosses itself —
        // which looks like a fan of triangles on screen and measures nonsense.
        let mut mask = a_room(30, 30);
        for y in 12..15 {
            for x in 12..15 {
                mask.set(x, y, true);
            }
        }
        for y in 15..18 {
            for x in 15..18 {
                mask.set(x, y, true);
            }
        }
        let Fill::Found(region) = flood(&mask, (3, 3), MOST) else {
            panic!("the room should fill");
        };
        assert_eq!(region.cells, 28 * 28 - 18, "two three-by-three blocks taken out");
        assert_eq!(region.hole_cells, 18, "and both are counted as enclosed");
        assert_eq!(region.holes_not_drawn, 2, "though neither is big enough to draw");
        let covered = region.cells + region.hole_cells;
        let drift = (region.outline_area - covered as f64).abs() / covered as f64;
        assert!(
            drift < 0.01,
            "a pinched outline must close cleanly: drift {drift}, outline {} vs {covered}",
            region.outline_area
        );
    }

    #[test]
    fn a_leak_through_a_single_cell_gap_is_caught() {
        // Two rooms side by side with one cell of wall missing between them
        // and the outer wall broken as well: the fill gets out, and out is out.
        let mut mask = a_room(60, 30);
        for y in 1..29 {
            mask.set(30, y, true);
        }
        mask.set(30, 15, false);
        mask.set(59, 15, false);
        assert!(
            matches!(flood(&mask, (10, 10), MOST), Fill::Escaped(Escaped::OffTheEdge)),
            "a one-cell gap is still a gap"
        );
    }

    #[test]
    fn the_shoelace_agrees_with_a_rectangle_anybody_can_check() {
        let square = [[0.0, 0.0], [10.0, 0.0], [10.0, 4.0], [0.0, 4.0]];
        assert!((area_of(&square) - 40.0).abs() < 1e-9);
    }
}
