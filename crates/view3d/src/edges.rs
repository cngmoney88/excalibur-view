//! The lines a detailer expects on a part: its corners and outline, not the
//! triangles its faces happen to be split into.
//!
//! An edge is drawn where two faces meet at more than a set angle, or where
//! a face has no neighbour (the rim of a sheet, a hole's lip on an open
//! mesh). Faces that lie flat against each other share no line, and neither
//! do the facets of a round bar, whose neighbours turn by only a few degrees.

use rustc_hash::FxHashMap;

use crate::camera::{cross, dot, normalize, sub, Vec3};

/// Faces meeting at more than this many degrees get a line between them.
pub const CREASE_DEGREES: f32 = 28.0;

/// Pairs of vertex indices, one pair a line.
pub fn feature_edges(positions: &[f32], indices: &[u32]) -> Vec<u32> {
    let limit = CREASE_DEGREES.to_radians().cos();
    let vertex = |i: u32| -> Vec3 {
        let at = i as usize * 3;
        [positions[at], positions[at + 1], positions[at + 2]]
    };
    // Vertices at the same place are one vertex here, whatever the mesh
    // says: faces are usually given their own copies so each can have its
    // own normal. A hundredth of a millimetre is the same place.
    let key = |p: Vec3| -> [i64; 3] { p.map(|c| (c as f64 * 1.0e5).round() as i64) };
    let mut same: FxHashMap<[i64; 3], u32> = FxHashMap::default();
    let count = positions.len() / 3;
    let mut canonical = Vec::with_capacity(count);
    for i in 0..count as u32 {
        canonical.push(*same.entry(key(vertex(i))).or_insert(i));
    }

    struct Seen {
        normal: Vec3,
        faces: u32,
        crease: bool,
    }
    let mut edges: FxHashMap<(u32, u32), Seen> = FxHashMap::default();
    for triangle in indices.chunks_exact(3) {
        let ids = [triangle[0], triangle[1], triangle[2]];
        if ids.iter().any(|&i| i as usize >= count) {
            continue;
        }
        let [a, b, c] = ids.map(|i| canonical[i as usize]);
        if a == b || b == c || a == c {
            continue;
        }
        let n = cross(sub(vertex(b), vertex(a)), sub(vertex(c), vertex(a)));
        if dot(n, n) < 1e-20 {
            continue;
        }
        let n = normalize(n);
        for (p, q) in [(a, b), (b, c), (c, a)] {
            let edge = (p.min(q), p.max(q));
            match edges.get_mut(&edge) {
                Some(seen) => {
                    seen.faces += 1;
                    // Measured either way round: a face wound the other way
                    // from its neighbour still lies flat against it, and a
                    // cut can leave a mesh wound both ways.
                    if dot(seen.normal, n).abs() < limit {
                        seen.crease = true;
                    }
                }
                None => {
                    edges.insert(edge, Seen { normal: n, faces: 1, crease: false });
                }
            }
        }
    }
    let mut out = Vec::new();
    for ((p, q), seen) in edges {
        if seen.faces == 1 || seen.faces > 2 || seen.crease {
            out.push(p);
            out.push(q);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit box as a mesh would give it: each face with its own four
    /// corners, split into two triangles.
    fn cube() -> (Vec<f32>, Vec<u32>) {
        let faces: [[[f32; 3]; 4]; 6] = [
            [[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]],
            [[0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]],
            [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]],
            [[0., 1., 0.], [1., 1., 0.], [1., 1., 1.], [0., 1., 1.]],
            [[0., 0., 0.], [0., 1., 0.], [0., 1., 1.], [0., 0., 1.]],
            [[1., 0., 0.], [1., 1., 0.], [1., 1., 1.], [1., 0., 1.]],
        ];
        let mut positions = Vec::new();
        let mut indices = Vec::new();
        for face in faces {
            let base = positions.len() as u32 / 3;
            for corner in face {
                positions.extend_from_slice(&corner);
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        (positions, indices)
    }

    #[test]
    fn a_box_has_twelve_edges_and_no_diagonals() {
        let (positions, indices) = cube();
        let edges = feature_edges(&positions, &indices);
        assert_eq!(edges.len(), 24, "twelve lines");
        for pair in edges.chunks(2) {
            let (a, b) = (pair[0] as usize * 3, pair[1] as usize * 3);
            let differ = (0..3).filter(|k| positions[a + k] != positions[b + k]).count();
            assert_eq!(differ, 1, "an edge of a box runs along one axis");
        }
    }

    #[test]
    fn a_flat_sheet_has_only_its_outline() {
        let positions = vec![0., 0., 0., 2., 0., 0., 2., 1., 0., 0., 1., 0., 1., 0.5, 0.];
        // Four triangles fanned from the middle.
        let indices = vec![4, 0, 1, 4, 1, 2, 4, 2, 3, 4, 3, 0];
        assert_eq!(feature_edges(&positions, &indices).len(), 8);
    }
}
