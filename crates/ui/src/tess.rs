//! Filling a shape that is not convex.
//!
//! egui fills a path by fanning triangles from its first point, which is right
//! for a convex shape and wrong for everything else. An L-shaped room, a slab
//! with a notch, a region Dynamic Fill traced round a stair — all of them come
//! out as a spray of triangles across the sheet.
//!
//! A takeoff is full of concave shapes; a rectangle is the exception. So the
//! shape is cut into triangles here first, by ear clipping, and painted as a
//! mesh. The outline it draws is the outline that was measured.

use egui::{Color32, Mesh, Pos2};

/// Cuts a simple polygon into triangles by ear clipping.
///
/// Returns indices into `points`. A polygon that cannot be cut — fewer than
/// three points, or one that crosses itself — comes back empty rather than
/// half-done, so a caller can fall back to drawing the outline alone instead of
/// painting something misleading.
pub fn triangulate(points: &[Pos2]) -> Vec<[usize; 3]> {
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }
    // A shape that crosses itself has no inside to fill, and ear clipping will
    // happily cut one up into something that covers its whole bounding box.
    // Better to refuse and let the caller draw the outline alone.
    if crosses_itself(points) {
        return Vec::new();
    }
    // Which way round it was given decides what counts as an ear.
    let clockwise = signed_area(points) < 0.0;
    let mut left: Vec<usize> = (0..n).collect();
    let mut out: Vec<[usize; 3]> = Vec::with_capacity(n.saturating_sub(2));

    // Each pass must remove at least one ear; the guard stops a shape that
    // crosses itself from spinning here for ever.
    let mut without_progress = 0usize;
    while left.len() > 3 {
        let count = left.len();
        let mut clipped = false;
        for i in 0..count {
            let (a, b, c) = (
                left[(i + count - 1) % count],
                left[i],
                left[(i + 1) % count],
            );
            if !is_ear(points, &left, a, b, c, clockwise) {
                continue;
            }
            out.push([a, b, c]);
            left.remove(i);
            clipped = true;
            break;
        }
        if clipped {
            without_progress = 0;
        } else {
            without_progress += 1;
            if without_progress > 2 {
                return Vec::new();
            }
        }
    }
    if left.len() == 3 {
        out.push([left[0], left[1], left[2]]);
    }
    out
}

/// Whether any two sides of the shape cross.
///
/// Every pair against every other: fine for the few hundred points a
/// simplified outline has, and a shape with thousands is left alone rather than
/// spending time on a check nobody is waiting for.
pub fn crosses_itself(points: &[Pos2]) -> bool {
    let n = points.len();
    if n < 4 || n > 600 {
        return false;
    }
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        for j in (i + 1)..n {
            // Sides that share a corner are allowed to touch at it.
            if j == i || (j + 1) % n == i || j == (i + 1) % n {
                continue;
            }
            let (c, d) = (points[j], points[(j + 1) % n]);
            if segments_cross(a, b, c, d) {
                return true;
            }
        }
    }
    false
}

fn segments_cross(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> bool {
    let d1 = cross(c, d, a);
    let d2 = cross(c, d, b);
    let d3 = cross(a, b, c);
    let d4 = cross(a, b, d);
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

fn signed_area(points: &[Pos2]) -> f32 {
    let mut twice = 0.0;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        twice += a.x * b.y - b.x * a.y;
    }
    twice * 0.5
}

fn cross(a: Pos2, b: Pos2, c: Pos2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn is_ear(points: &[Pos2], left: &[usize], a: usize, b: usize, c: usize, clockwise: bool) -> bool {
    let (pa, pb, pc) = (points[a], points[b], points[c]);
    let turn = cross(pa, pb, pc);
    // A reflex corner is not an ear, and a straight one has no triangle in it.
    if clockwise {
        if turn >= -1e-6 {
            return false;
        }
    } else if turn <= 1e-6 {
        return false;
    }
    // Nor is it an ear if any other corner falls inside the triangle.
    for &i in left {
        if i == a || i == b || i == c {
            continue;
        }
        if inside(points[i], pa, pb, pc) {
            return false;
        }
    }
    true
}

fn inside(p: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    let d1 = cross(a, b, p);
    let d2 = cross(b, c, p);
    let d3 = cross(c, a, p);
    let any_negative = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let any_positive = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(any_negative && any_positive)
}

/// A filled shape, cut into triangles so a concave one paints correctly.
///
/// Returns nothing when the shape cannot be cut, which is the caller's cue to
/// draw its outline and no fill rather than paint a lie.
pub fn filled(points: &[Pos2], colour: Color32) -> Option<Mesh> {
    let triangles = triangulate(points);
    if triangles.is_empty() {
        return None;
    }
    let mut mesh = Mesh::default();
    for p in points {
        mesh.colored_vertex(*p, colour);
    }
    for [a, b, c] in triangles {
        mesh.add_triangle(a as u32, b as u32, c as u32);
    }
    Some(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn area_of(points: &[Pos2], triangles: &[[usize; 3]]) -> f32 {
        triangles
            .iter()
            .map(|[a, b, c]| {
                cross(points[*a], points[*b], points[*c]).abs() * 0.5
            })
            .sum()
    }

    #[test]
    fn a_rectangle_becomes_two_triangles() {
        let square = [
            pos2(0.0, 0.0),
            pos2(10.0, 0.0),
            pos2(10.0, 4.0),
            pos2(0.0, 4.0),
        ];
        let triangles = triangulate(&square);
        assert_eq!(triangles.len(), 2);
        assert!((area_of(&square, &triangles) - 40.0).abs() < 0.01);
    }

    #[test]
    fn an_l_shaped_room_keeps_its_area_instead_of_fanning_out() {
        // The shape egui gets wrong. Fanned from the first point it covers the
        // whole bounding box; cut properly it covers three quarters of it.
        let ell = [
            pos2(0.0, 0.0),
            pos2(10.0, 0.0),
            pos2(10.0, 5.0),
            pos2(5.0, 5.0),
            pos2(5.0, 10.0),
            pos2(0.0, 10.0),
        ];
        let triangles = triangulate(&ell);
        assert_eq!(triangles.len(), 4, "six corners cut into four triangles");
        let covered = area_of(&ell, &triangles);
        assert!((covered - 75.0).abs() < 0.01, "an L covers 75, got {covered}");
        assert!(covered < 100.0, "and never its bounding box");
    }

    #[test]
    fn a_shape_given_the_other_way_round_still_works() {
        let backwards = [
            pos2(0.0, 10.0),
            pos2(5.0, 10.0),
            pos2(5.0, 5.0),
            pos2(10.0, 5.0),
            pos2(10.0, 0.0),
            pos2(0.0, 0.0),
        ];
        let triangles = triangulate(&backwards);
        assert_eq!(triangles.len(), 4);
        assert!((area_of(&backwards, &triangles) - 75.0).abs() < 0.01);
    }

    #[test]
    fn a_deeply_notched_shape_is_cut_without_covering_the_notch() {
        // A comb: three teeth. Fanning would fill the gaps between them.
        let comb = [
            pos2(0.0, 0.0),
            pos2(12.0, 0.0),
            pos2(12.0, 10.0),
            pos2(10.0, 10.0),
            pos2(10.0, 4.0),
            pos2(8.0, 4.0),
            pos2(8.0, 10.0),
            pos2(4.0, 10.0),
            pos2(4.0, 4.0),
            pos2(2.0, 4.0),
            pos2(2.0, 10.0),
            pos2(0.0, 10.0),
        ];
        let triangles = triangulate(&comb);
        let covered = area_of(&comb, &triangles);
        // Twelve by ten, less two notches two wide and six deep.
        assert!((covered - (120.0 - 24.0)).abs() < 0.01, "got {covered}");
    }

    #[test]
    fn too_few_points_make_no_triangles_rather_than_a_wrong_one() {
        assert!(triangulate(&[]).is_empty());
        assert!(triangulate(&[pos2(0.0, 0.0), pos2(1.0, 1.0)]).is_empty());
        assert!(filled(&[pos2(0.0, 0.0), pos2(1.0, 1.0)], Color32::RED).is_none());
    }

    #[test]
    fn a_shape_that_crosses_itself_is_refused_rather_than_painted_wrong() {
        let bowtie = [
            pos2(0.0, 0.0),
            pos2(10.0, 10.0),
            pos2(10.0, 0.0),
            pos2(0.0, 10.0),
        ];
        assert!(crosses_itself(&bowtie));
        assert!(
            triangulate(&bowtie).is_empty(),
            "a shape with no inside is refused rather than filled"
        );
        assert!(filled(&bowtie, Color32::RED).is_none());
        // And an honest shape is not mistaken for one.
        let ell = [
            pos2(0.0, 0.0),
            pos2(10.0, 0.0),
            pos2(10.0, 5.0),
            pos2(5.0, 5.0),
            pos2(5.0, 10.0),
            pos2(0.0, 10.0),
        ];
        assert!(!crosses_itself(&ell));
    }

    #[test]
    fn a_mesh_has_a_vertex_for_every_corner_and_three_indices_a_triangle() {
        let ell = [
            pos2(0.0, 0.0),
            pos2(10.0, 0.0),
            pos2(10.0, 5.0),
            pos2(5.0, 5.0),
            pos2(5.0, 10.0),
            pos2(0.0, 10.0),
        ];
        let mesh = filled(&ell, Color32::RED).expect("an L can be filled");
        assert_eq!(mesh.vertices.len(), 6);
        assert_eq!(mesh.indices.len(), 4 * 3);
    }
}
