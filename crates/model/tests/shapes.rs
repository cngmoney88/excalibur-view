//! The shapes behind the drawing, from the same hand-written model.
#![cfg(feature = "geometry")]

use model::{shapes, Model};

#[test]
fn every_part_with_a_body_gets_triangles() {
    let bytes = include_bytes!("two-bays.ifc");
    let model = Model::read(bytes).unwrap();
    let ids: Vec<u32> = model.parts.iter().map(|p| p.id).collect();
    let (made, failed) = shapes::of(bytes, &ids);
    // The plate is written with no body at all; everything else has one.
    assert_eq!(made.len(), ids.len() - 1, "failed: {failed:?}");
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].0, 400);
    for shape in &made {
        assert!(shape.indices.len() >= 36, "#{} is a solid, at least a box", shape.id);
        assert_eq!(shape.indices.len() % 3, 0);
    }
    // The column is 3,657.6 mm long; its triangles span that, in metres.
    let column = made.iter().find(|s| s.id == 100).unwrap();
    let zs: Vec<f32> = column.positions.chunks(3).map(|p| p[2]).collect();
    let span = zs.iter().cloned().fold(f32::MIN, f32::max) - zs.iter().cloned().fold(f32::MAX, f32::min);
    assert!((span - 3.6576).abs() < 0.01, "{span}");
}

/// `HV_TEST_MODELS=folder cargo test -p model --features geometry --release shapes_of_real -- --ignored --nocapture`.
#[test]
#[ignore]
fn shapes_of_real_models() {
    let folder = std::env::var("HV_TEST_MODELS").expect("HV_TEST_MODELS");
    let mut paths: Vec<_> = std::fs::read_dir(folder)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ifc")))
        .collect();
    paths.sort();
    for path in paths {
        let bytes = std::fs::read(&path).unwrap();
        let model = Model::read(&bytes).unwrap();
        let ids: Vec<u32> = model.parts.iter().map(|p| p.id).collect();
        let started = std::time::Instant::now();
        let (made, failed) = shapes::of(&bytes, &ids);
        let triangles: usize = made.iter().map(|s| s.indices.len() / 3).sum();
        println!(
            "{:<22} {:>5} of {:>5} parts drawn, {:>8} triangles, {:?}, {} failed{}",
            path.file_name().unwrap().to_string_lossy(),
            made.len(),
            ids.len(),
            triangles,
            started.elapsed(),
            failed.len(),
            failed.first().map(|(id, why)| format!(" (#{id}: {why})")).unwrap_or_default()
        );
    }
}
