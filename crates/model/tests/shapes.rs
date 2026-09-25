//! The shapes behind the drawing, from the same hand-written model.
#![cfg(feature = "geometry")]

use std::sync::Arc;
use std::time::Instant;

use model::shapes::{self, Group, News};
use model::Model;

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

#[test]
fn a_model_is_drawn_uncut_first_and_then_cut() {
    let bytes = Arc::new(include_bytes!("two-bays.ifc").to_vec());
    let meshing = shapes::start(bytes, 2);
    let mut rough = Vec::new();
    let mut cut = Vec::new();
    let mut finished = false;
    for news in meshing.news.iter() {
        match news {
            News::Rough { shapes, failed, to_cut } => {
                assert!(failed.is_empty(), "{failed:?}");
                assert_eq!(to_cut, 2, "both beams are drawn from a cut solid");
                rough = shapes.iter().map(|s| (s.id, s.group, s.exact)).collect();
            }
            News::Cut(shape) => {
                assert!(shape.exact);
                cut.push(shape.id);
            }
            News::Uncuttable(id, why) => panic!("#{id}: {why}"),
            News::Finished => {
                finished = true;
                break;
            }
        }
    }
    assert!(finished);
    // The plate and the bolts are written with no shape: not drawn, and not
    // a failure either.
    rough.sort();
    assert_eq!(
        rough,
        vec![
            (100, Group::Column, false),
            (110, Group::Column, false),
            (200, Group::Beam, false),
            (220, Group::Beam, false),
            (300, Group::Brace, false),
        ]
    );
    cut.sort();
    assert_eq!(cut, vec![200, 220]);
}

#[test]
fn what_draws_and_in_which_group() {
    assert_eq!(Group::of("IFCBEAM"), Some(Group::Beam));
    assert_eq!(Group::of("IfcColumnStandardCase"), Some(Group::Column));
    assert_eq!(Group::of("IFCMEMBER"), Some(Group::Brace));
    assert_eq!(Group::of("IFCMECHANICALFASTENER"), Some(Group::Bolt));
    assert_eq!(Group::of("IFCSLAB"), Some(Group::Concrete));
    assert_eq!(Group::of("IFCCOVERING"), Some(Group::Other));
    for not_a_thing in ["IFCOPENINGELEMENT", "IFCSPACE", "IFCSITE", "IFCELEMENTASSEMBLY", "IFCPROPERTYSET"] {
        assert_eq!(Group::of(not_a_thing), None, "{not_a_thing}");
    }
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
        let started = Instant::now();
        let meshing = shapes::start(Arc::new(bytes), 4);
        let (mut drawn, mut triangles, mut failed, mut to_cut, mut cut, mut uncut) = (0, 0, 0, 0, 0, 0);
        let mut rough_at = None;
        let mut first_failure = String::new();
        for news in meshing.news.iter() {
            match news {
                News::Rough { shapes, failed: f, to_cut: c } => {
                    rough_at = Some(started.elapsed());
                    drawn = shapes.len();
                    triangles = shapes.iter().map(|s| s.indices.len() / 3).sum::<usize>();
                    failed = f.len();
                    to_cut = c;
                    if let Some((id, why)) = f.first() {
                        first_failure = format!(" (#{id}: {why})");
                    }
                }
                News::Cut(_) => cut += 1,
                News::Uncuttable(..) => uncut += 1,
                News::Finished => break,
            }
        }
        println!(
            "{:<22} {} parts, {drawn:>5} shapes, {triangles:>8} triangles uncut, in {:?}; \
             {cut} of {to_cut} cut ({uncut} couldn't be) by {:?}; {failed} not drawn{first_failure}",
            path.file_name().unwrap().to_string_lossy(),
            model.parts.len(),
            rough_at.unwrap_or_default(),
            started.elapsed(),
        );
    }
}
