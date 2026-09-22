//! The measurement arithmetic.
//!
//! Every function here works on the points a person clicked, in sheet points.
//! Nothing infers, smooths or closes a shape the user left open.

/// The run of a polyline: the sum of its legs, not corner to corner.
pub fn length(points: &[[f64; 2]]) -> f64 {
    points
        .windows(2)
        .map(|p| ((p[1][0] - p[0][0]).powi(2) + (p[1][1] - p[0][1]).powi(2)).sqrt())
        .sum()
}

/// The area a closed shape encloses, by the shoelace formula. The sign is
/// dropped, so a shape picked up counter-clockwise is not negative.
pub fn area(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    if n < 3 {
        return 0.0;
    }
    let mut twice = 0.0;
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        twice += a[0] * b[1] - b[0] * a[1];
    }
    (twice * 0.5).abs()
}

/// The way round a closed shape, which is what edge form and handrail are
/// ordered by.
pub fn perimeter(points: &[[f64; 2]]) -> f64 {
    if points.len() < 3 {
        return length(points);
    }
    let mut total = length(points);
    let first = points[0];
    let last = points[points.len() - 1];
    total += ((first[0] - last[0]).powi(2) + (first[1] - last[1]).powi(2)).sqrt();
    total
}

/// The angle at the middle point of three, in degrees from 0 to 360 measured
/// the way the points were picked.
pub fn angle(points: &[[f64; 2]]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    let (a, b, c) = (points[0], points[1], points[2]);
    let first = (a[1] - b[1]).atan2(a[0] - b[0]);
    let second = (c[1] - b[1]).atan2(c[0] - b[0]);
    let mut degrees = (second - first).to_degrees();
    if degrees < 0.0 {
        degrees += 360.0;
    }
    degrees
}

/// The radius of the circle through three points. Points in a straight line
/// have no circle, and say so rather than returning something enormous.
pub fn radius_through(points: &[[f64; 2]]) -> Option<f64> {
    if points.len() < 3 {
        return None;
    }
    let (a, b, c) = (points[0], points[1], points[2]);
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    if d.abs() < 1e-9 {
        return None;
    }
    let sq = |p: [f64; 2]| p[0] * p[0] + p[1] * p[1];
    let ux = (sq(a) * (b[1] - c[1]) + sq(b) * (c[1] - a[1]) + sq(c) * (a[1] - b[1])) / d;
    let uy = (sq(a) * (c[0] - b[0]) + sq(b) * (a[0] - c[0]) + sq(c) * (b[0] - a[0])) / d;
    Some(((a[0] - ux).powi(2) + (a[1] - uy).powi(2)).sqrt())
}

/// The radius of a circle drawn as a box, taking the mean of the two axes so an
/// oval gives a sensible answer rather than one of its two radii.
pub fn radius_of_box(area: [f64; 4]) -> f64 {
    ((area[2] - area[0]).abs() + (area[3] - area[1]).abs()) / 4.0
}

/// The smallest box holding these points.
pub fn bounds(points: &[[f64; 2]]) -> Option<[f64; 4]> {
    let first = points.first()?;
    let mut b = [first[0], first[1], first[0], first[1]];
    for p in points {
        b[0] = b[0].min(p[0]);
        b[1] = b[1].min(p[1]);
        b[2] = b[2].max(p[0]);
        b[3] = b[3].max(p[1]);
    }
    Some(b)
}

/// True length up a slope, from the run on the sheet and a pitch given as rise
/// over `run`. A roof drawn at 4 in 12 is longer than the plan shows, and a
/// takeoff that ignores that is short.
pub fn along_slope(flat: f64, rise: f64, run: f64) -> f64 {
    if run.abs() < 1e-9 {
        return flat;
    }
    flat * (1.0 + (rise / run).powi(2)).sqrt()
}

/// Whether a closed shape was picked up clockwise, which is how a cutout is
/// told from the shape it is cut out of.
pub fn clockwise(points: &[[f64; 2]]) -> bool {
    let n = points.len();
    if n < 3 {
        return false;
    }
    let mut twice = 0.0;
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        twice += a[0] * b[1] - b[0] * a[1];
    }
    twice < 0.0
}

/// Whether a point falls inside a closed shape, by ray casting. Used to decide
/// which shape a cutout belongs to.
pub fn contains(points: &[[f64; 2]], at: [f64; 2]) -> bool {
    let n = points.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (points[i], points[j]);
        if (a[1] > at[1]) != (b[1] > at[1]) {
            let x = (b[0] - a[0]) * (at[1] - a[1]) / (b[1] - a[1]) + a[0];
            if at[0] < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE: [[f64; 2]; 4] = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];

    #[test]
    fn a_run_adds_its_legs_rather_than_measuring_corner_to_corner() {
        let dogleg = [[0.0, 0.0], [300.0, 0.0], [300.0, 400.0]];
        assert_eq!(length(&dogleg), 700.0);
        assert_eq!(length(&[[0.0, 0.0]]), 0.0);
        assert_eq!(length(&[]), 0.0);
    }

    #[test]
    fn an_area_is_positive_whichever_way_round_it_was_picked() {
        assert_eq!(area(&SQUARE), 10_000.0);
        let backwards: Vec<[f64; 2]> = SQUARE.iter().rev().copied().collect();
        assert_eq!(area(&backwards), 10_000.0);
    }

    #[test]
    fn an_l_shaped_room_is_not_measured_as_its_bounding_box() {
        let ell = [
            [0.0, 0.0],
            [100.0, 0.0],
            [100.0, 50.0],
            [50.0, 50.0],
            [50.0, 100.0],
            [0.0, 100.0],
        ];
        assert_eq!(area(&ell), 7_500.0);
        assert_eq!(perimeter(&ell), 400.0);
    }

    #[test]
    fn a_shape_with_too_few_points_has_no_area() {
        assert_eq!(area(&[[0.0, 0.0], [10.0, 0.0]]), 0.0);
    }

    #[test]
    fn a_perimeter_closes_the_shape_but_a_length_does_not() {
        assert_eq!(perimeter(&SQUARE), 400.0);
        assert_eq!(length(&SQUARE), 300.0);
    }

    #[test]
    fn a_right_angle_reads_ninety_degrees() {
        let corner = [[100.0, 0.0], [0.0, 0.0], [0.0, 100.0]];
        assert!((angle(&corner) - 90.0).abs() < 1e-9);
        let straight = [[100.0, 0.0], [0.0, 0.0], [-100.0, 0.0]];
        assert!((angle(&straight) - 180.0).abs() < 1e-9);
    }

    #[test]
    fn three_points_on_a_circle_give_back_its_radius() {
        let on_a_circle = [[50.0, 0.0], [0.0, 50.0], [-50.0, 0.0]];
        let r = radius_through(&on_a_circle).unwrap();
        assert!((r - 50.0).abs() < 1e-9, "{r}");
    }

    #[test]
    fn three_points_in_a_line_have_no_radius_rather_than_a_huge_one() {
        assert!(radius_through(&[[0.0, 0.0], [10.0, 0.0], [20.0, 0.0]]).is_none());
    }

    #[test]
    fn a_four_in_twelve_roof_is_longer_than_the_plan_shows() {
        // 12 feet of run at 4:12 is 12.649 feet along the slope.
        let sloped = along_slope(12.0, 4.0, 12.0);
        assert!((sloped - 12.649_110_640_673_518).abs() < 1e-9, "{sloped}");
        assert_eq!(along_slope(12.0, 0.0, 12.0), 12.0, "flat is flat");
    }

    #[test]
    fn a_cutout_picked_up_the_other_way_round_is_recognised() {
        assert!(!clockwise(&SQUARE));
        let backwards: Vec<[f64; 2]> = SQUARE.iter().rev().copied().collect();
        assert!(clockwise(&backwards));
    }

    #[test]
    fn a_point_inside_a_shape_is_told_from_one_outside() {
        assert!(contains(&SQUARE, [50.0, 50.0]));
        assert!(!contains(&SQUARE, [150.0, 50.0]));
        assert!(!contains(&SQUARE, [-1.0, 50.0]));
    }

    #[test]
    fn the_bounds_of_nothing_are_nothing() {
        assert_eq!(bounds(&[]), None);
        assert_eq!(bounds(&SQUARE), Some([0.0, 0.0, 100.0, 100.0]));
    }
}
