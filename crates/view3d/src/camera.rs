//! Where the model is looked at from, and the arithmetic that goes with it.
//!
//! Z is up, as it is in every IFC file. The camera orbits a target point:
//! yaw turns it around the vertical, pitch raises it, distance backs it off.
//! Depth is reversed (near is 1, far is 0), which keeps a 100 m building and
//! a 10 mm plate apart in the depth buffer without fiddling with planes.

pub type Vec3 = [f32; 3];

/// A column-major 4x4 matrix, the layout WGSL reads.
pub type Mat4 = [[f32; 4]; 4];

/// A box around some or all of the model, in the view's own frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl Bounds {
    pub fn of_point(p: Vec3) -> Bounds {
        Bounds { min: p, max: p }
    }

    pub fn grow(&mut self, p: Vec3) {
        for k in 0..3 {
            self.min[k] = self.min[k].min(p[k]);
            self.max[k] = self.max[k].max(p[k]);
        }
    }

    pub fn union(self, other: Bounds) -> Bounds {
        let mut out = self;
        out.grow(other.min);
        out.grow(other.max);
        out
    }

    pub fn centre(&self) -> Vec3 {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }

    /// Half the diagonal: the radius of a sphere that holds the box.
    pub fn radius(&self) -> f32 {
        let d = sub(self.max, self.min);
        (dot(d, d)).sqrt() * 0.5
    }
}

/// The views a detailer names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// From the south-east, above: the view a model is usually shown in.
    Iso,
    Top,
    Front,
    Back,
    Left,
    Right,
}

impl Preset {
    pub const ALL: [Preset; 6] = [Preset::Iso, Preset::Top, Preset::Front, Preset::Back, Preset::Left, Preset::Right];

    pub fn name(self) -> &'static str {
        match self {
            Preset::Iso => "Iso",
            Preset::Top => "Top",
            Preset::Front => "Front",
            Preset::Back => "Back",
            Preset::Left => "Left",
            Preset::Right => "Right",
        }
    }

    /// (yaw, pitch) in radians.
    fn angles(self) -> (f32, f32) {
        use std::f32::consts::FRAC_PI_2;
        match self {
            Preset::Iso => (-FRAC_PI_2 / 2.0, 35.264_f32.to_radians()),
            Preset::Top => (-FRAC_PI_2, MAX_PITCH),
            Preset::Front => (-FRAC_PI_2, 0.0),
            Preset::Back => (FRAC_PI_2, 0.0),
            Preset::Left => (std::f32::consts::PI, 0.0),
            Preset::Right => (0.0, 0.0),
        }
    }
}

const MAX_PITCH: f32 = 89.9 * std::f32::consts::PI / 180.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub target: Vec3,
    pub distance: f32,
    /// Around Z, radians. Zero looks from +X towards the target.
    pub yaw: f32,
    /// Above the horizontal, radians.
    pub pitch: f32,
    /// Vertical field of view, radians.
    pub fov_y: f32,
    /// Parallel projection: elevations and plans without perspective.
    pub orthographic: bool,
}

impl Default for Camera {
    fn default() -> Camera {
        let (yaw, pitch) = Preset::Iso.angles();
        Camera { target: [0.0; 3], distance: 10.0, yaw, pitch, fov_y: 35f32.to_radians(), orthographic: false }
    }
}

impl Camera {
    /// From the target towards the eye.
    pub fn backwards(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        [cp * cy, cp * sy, sp]
    }

    pub fn eye(&self) -> Vec3 {
        add(self.target, scale(self.backwards(), self.distance))
    }

    /// Screen right and screen up, in the model's frame.
    pub fn right_up(&self) -> (Vec3, Vec3) {
        let forward = scale(self.backwards(), -1.0);
        let right = normalize(cross(forward, [0.0, 0.0, 1.0]));
        let up = cross(right, forward);
        (right, up)
    }

    /// Half the height of the view at the target's distance.
    pub fn half_height(&self) -> f32 {
        self.distance * (self.fov_y * 0.5).tan()
    }

    /// The matrix from the model's frame to clip space. `depth` is how far
    /// the model reaches from the target, for a parallel projection's planes.
    pub fn view_proj(&self, aspect: f32, depth: f32) -> Mat4 {
        let aspect = aspect.max(1e-3);
        if self.orthographic {
            // The eye backs right off, so nothing is behind it; the view is
            // as wide as a perspective one would be at the target.
            let back = self.distance + depth * 2.0 + 1.0;
            let eye = add(self.target, scale(self.backwards(), back));
            let view = look_at(eye, self.target);
            let (near, far) = (0.01, back + depth * 2.0 + 1.0);
            let hh = self.half_height().max(1e-4);
            let hw = hh * aspect;
            let proj: Mat4 = [
                [1.0 / hw, 0.0, 0.0, 0.0],
                [0.0, 1.0 / hh, 0.0, 0.0],
                [0.0, 0.0, 1.0 / (far - near), 0.0],
                [0.0, 0.0, far / (far - near), 1.0],
            ];
            mul(proj, view)
        } else {
            let view = look_at(self.eye(), self.target);
            let near = (self.distance * 0.002).max(0.001);
            let f = 1.0 / (self.fov_y * 0.5).tan();
            // Infinite far plane, reversed: depth is near / distance.
            let proj: Mat4 = [
                [f / aspect, 0.0, 0.0, 0.0],
                [0.0, f, 0.0, 0.0],
                [0.0, 0.0, 0.0, -1.0],
                [0.0, 0.0, near, 0.0],
            ];
            mul(proj, view)
        }
    }

    /// Turns the camera around the target by a drag of `dx`, `dy` pixels.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.008;
        self.pitch = (self.pitch + dy * 0.008).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Slides the target with a drag, so the model follows the pointer.
    pub fn pan(&mut self, dx: f32, dy: f32, viewport_height: f32) {
        let per_pixel = 2.0 * self.half_height() / viewport_height.max(1.0);
        let (right, up) = self.right_up();
        self.target = add(self.target, add(scale(right, -dx * per_pixel), scale(up, dy * per_pixel)));
    }

    /// Moves in by `factor` (below 1) or out (above 1), keeping the point
    /// under the pointer where it is. `at` is the pointer, -1 to 1 across the
    /// view each way, up positive.
    pub fn zoom(&mut self, factor: f32, at: [f32; 2], aspect: f32) {
        let factor = factor.clamp(0.2, 5.0);
        let (right, up) = self.right_up();
        let hh = self.half_height();
        let offset = add(scale(right, at[0] * hh * aspect), scale(up, at[1] * hh));
        self.target = add(self.target, scale(offset, 1.0 - factor));
        self.distance = (self.distance * factor).clamp(0.01, 1.0e6);
    }

    /// Frames a box so all of it shows, whichever way the camera faces, and
    /// fills the view with it: the box's corners as seen from here, not a
    /// sphere around it, which leaves a long low building a speck.
    pub fn fit(&mut self, bounds: Bounds, aspect: f32) {
        let aspect = aspect.max(1e-3);
        let (right, up) = self.right_up();
        let back = self.backwards();
        let centre = bounds.centre();
        let corners: Vec<Vec3> = (0..8)
            .map(|i| {
                let corner = [
                    if i & 1 == 0 { bounds.min[0] } else { bounds.max[0] },
                    if i & 2 == 0 { bounds.min[1] } else { bounds.max[1] },
                    if i & 4 == 0 { bounds.min[2] } else { bounds.max[2] },
                ];
                let v = sub(corner, centre);
                [dot(v, right), dot(v, up), dot(v, back)]
            })
            .collect();
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for c in &corners {
            for k in 0..2 {
                lo[k] = lo[k].min(c[k]);
                hi[k] = hi[k].max(c[k]);
            }
        }
        // Aim at the middle of the box as it appears, not the middle of it.
        let mid = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
        self.target = add(centre, add(scale(right, mid[0]), scale(up, mid[1])));
        let tan_y = (self.fov_y * 0.5).tan();
        let tan_x = tan_y * aspect;
        let margin = 1.06;
        let smallest = bounds.radius().max(0.05) * 0.1;
        self.distance = if self.orthographic {
            let half_height = corners
                .iter()
                .map(|c| ((c[1] - mid[1]).abs()).max((c[0] - mid[0]).abs() / aspect))
                .fold(0.0, f32::max);
            (half_height * margin / tan_y).max(smallest)
        } else {
            corners
                .iter()
                .map(|c| c[2] + ((c[0] - mid[0]).abs() * margin / tan_x).max((c[1] - mid[1]).abs() * margin / tan_y))
                .fold(0.0, f32::max)
                .max(smallest)
        };
    }

    pub fn look(&mut self, preset: Preset) {
        let (yaw, pitch) = preset.angles();
        self.yaw = yaw;
        self.pitch = pitch;
    }
}

/// A right-handed view matrix with Z up.
fn look_at(eye: Vec3, target: Vec3) -> Mat4 {
    let f = normalize(sub(target, eye));
    let mut s = cross(f, [0.0, 0.0, 1.0]);
    if dot(s, s) < 1e-12 {
        s = [1.0, 0.0, 0.0];
    }
    let s = normalize(s);
    let u = cross(s, f);
    [
        [s[0], u[0], -f[0], 0.0],
        [s[1], u[1], -f[1], 0.0],
        [s[2], u[2], -f[2], 0.0],
        [-dot(s, eye), -dot(u, eye), dot(f, eye), 1.0],
    ]
}

pub fn mul(a: Mat4, b: Mat4) -> Mat4 {
    let mut out = [[0.0; 4]; 4];
    for (c, column) in out.iter_mut().enumerate() {
        for (r, cell) in column.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    out
}

/// The inverse of a matrix, when it has one.
pub fn inverse(m: &Mat4) -> Option<Mat4> {
    // Row-major copy, then Gauss-Jordan with partial pivoting, in f64 so a
    // reversed-depth projection's tiny numbers come back intact.
    let mut a = [[0.0f64; 8]; 4];
    for (r, row) in a.iter_mut().enumerate() {
        for c in 0..4 {
            row[c] = m[c][r] as f64;
        }
        row[4 + r] = 1.0;
    }
    for col in 0..4 {
        let pivot = (col..4).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-30 {
            return None;
        }
        a.swap(col, pivot);
        let lead = a[col][col];
        for v in a[col].iter_mut() {
            *v /= lead;
        }
        for r in 0..4 {
            if r != col {
                let factor = a[r][col];
                if factor != 0.0 {
                    for k in 0..8 {
                        a[r][k] -= factor * a[col][k];
                    }
                }
            }
        }
    }
    let mut out = [[0.0f32; 4]; 4];
    for (c, column) in out.iter_mut().enumerate() {
        for (r, cell) in column.iter_mut().enumerate() {
            *cell = a[r][4 + c] as f32;
        }
    }
    Some(out)
}

/// A point through a matrix, divided by w.
pub fn project(m: &Mat4, p: Vec3) -> [f32; 4] {
    let mut out = [0.0; 4];
    for (r, cell) in out.iter_mut().enumerate() {
        *cell = m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r];
    }
    out
}

pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: Vec3, s: f32) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub fn normalize(a: Vec3) -> Vec3 {
    let len = dot(a, a).sqrt();
    if len > 0.0 {
        scale(a, 1.0 / len)
    } else {
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_screen(camera: &Camera, p: Vec3, aspect: f32) -> [f32; 3] {
        let c = project(&camera.view_proj(aspect, 50.0), p);
        [c[0] / c[3], c[1] / c[3], c[2] / c[3]]
    }

    #[test]
    fn the_target_is_in_the_middle_and_nearer_things_are_deeper() {
        for orthographic in [false, true] {
            let camera = Camera { target: [3.0, 4.0, 5.0], orthographic, ..Camera::default() };
            let middle = to_screen(&camera, camera.target, 1.5);
            assert!(middle[0].abs() < 1e-4 && middle[1].abs() < 1e-4, "{middle:?}");
            assert!(middle[2] > 0.0 && middle[2] < 1.0);
            let nearer = add(camera.target, scale(camera.backwards(), 1.0));
            assert!(to_screen(&camera, nearer, 1.5)[2] > middle[2], "depth is reversed");
        }
    }

    #[test]
    fn a_point_on_screen_goes_back_to_where_it_came_from() {
        for orthographic in [false, true] {
            let camera = Camera { target: [30.0, -4.0, 35.0], distance: 40.0, orthographic, ..Camera::default() };
            let m = camera.view_proj(1.5, 60.0);
            let back = inverse(&m).unwrap();
            let p = [33.5, -1.25, 37.0];
            let c = project(&m, p);
            let ndc = [c[0] / c[3], c[1] / c[3], c[2] / c[3]];
            let h = project(&back, ndc);
            let q = [h[0] / h[3], h[1] / h[3], h[2] / h[3]];
            for k in 0..3 {
                assert!((q[k] - p[k]).abs() < 2e-3, "{orthographic}: {q:?} {p:?}");
            }
        }
    }

    #[test]
    fn up_is_up_on_the_screen() {
        let camera = Camera::default();
        let above = to_screen(&camera, [0.0, 0.0, 1.0], 1.0);
        assert!(above[1] > 0.0, "{above:?}");
        let mut front = Camera::default();
        front.look(Preset::Front);
        // Looking north from the south: east is to the right.
        assert!(to_screen(&front, [1.0, 0.0, 0.0], 1.0)[0] > 0.0);
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer() {
        for orthographic in [false, true] {
            let mut camera = Camera { orthographic, ..Camera::default() };
            let aspect = 1.6;
            let at = [0.5, -0.25];
            let (right, up) = camera.right_up();
            let hh = camera.half_height();
            let point = add(camera.target, add(scale(right, at[0] * hh * aspect), scale(up, at[1] * hh)));
            camera.zoom(0.5, at, aspect);
            let seen = to_screen(&camera, point, aspect);
            assert!((seen[0] - at[0]).abs() < 1e-3 && (seen[1] - at[1]).abs() < 1e-3, "{seen:?}");
        }
    }

    #[test]
    fn a_fitted_box_is_all_on_screen() {
        let bounds = Bounds { min: [-10.0, 0.0, 30.0], max: [40.0, 12.0, 45.0] };
        for preset in Preset::ALL {
            for aspect in [0.6, 1.0, 2.2] {
                let mut camera = Camera::default();
                camera.look(preset);
                camera.fit(bounds, aspect);
                for i in 0..8 {
                    let corner = [
                        if i & 1 == 0 { bounds.min[0] } else { bounds.max[0] },
                        if i & 2 == 0 { bounds.min[1] } else { bounds.max[1] },
                        if i & 4 == 0 { bounds.min[2] } else { bounds.max[2] },
                    ];
                    let s = to_screen(&camera, corner, aspect);
                    assert!(s[0].abs() <= 1.0 && s[1].abs() <= 1.0, "{preset:?} {aspect} {s:?}");
                }
            }
        }
    }

    #[test]
    fn a_fitted_box_fills_the_view() {
        let bounds = Bounds { min: [-10.0, 0.0, 30.0], max: [40.0, 12.0, 45.0] };
        for orthographic in [false, true] {
            for preset in Preset::ALL {
                let mut camera = Camera { orthographic, ..Camera::default() };
                camera.look(preset);
                camera.fit(bounds, 1.6);
                let widest = (0..8)
                    .map(|i| {
                        let corner = [
                            if i & 1 == 0 { bounds.min[0] } else { bounds.max[0] },
                            if i & 2 == 0 { bounds.min[1] } else { bounds.max[1] },
                            if i & 4 == 0 { bounds.min[2] } else { bounds.max[2] },
                        ];
                        let s = to_screen(&camera, corner, 1.6);
                        s[0].abs().max(s[1].abs())
                    })
                    .fold(0.0, f32::max);
                assert!(widest > 0.9 && widest <= 1.0, "{orthographic} {preset:?}: {widest}");
            }
        }
    }
}
