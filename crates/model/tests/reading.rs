//! A model written by hand, so every number in it is known: two columns
//! weighed in the standard's base quantities, two beams weighed only in
//! Tekla's own set, a brace nobody weighed, a plate in an assembly, and bolts.

use model::{Kind, Model, Source};

fn two_bays() -> Model {
    Model::read(include_bytes!("two-bays.ifc")).expect("the test model reads")
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn the_header_says_what_wrote_it() {
    let model = two_bays();
    assert_eq!(model.schema, "IFC2X3");
    assert_eq!(model.made_by, "Written by hand for the tests");
}

#[test]
fn every_part_is_read_with_its_kind() {
    let model = two_bays();
    let count = |kind| model.parts.iter().filter(|p| p.kind == kind).count();
    assert_eq!(count(Kind::Column), 2);
    assert_eq!(count(Kind::Beam), 2);
    assert_eq!(count(Kind::Brace), 1);
    assert_eq!(count(Kind::Plate), 1);
}

#[test]
fn weights_come_from_the_model_in_its_own_units() {
    let model = two_bays();
    let column = model.parts.iter().find(|p| p.id == 100).unwrap();
    // 179.4 kg is 395.5 lb; 3,657.6 mm is 12 ft.
    assert!(close(column.pounds.unwrap(), 395.51), "{:?}", column.pounds);
    assert_eq!(column.weight_from, Source::BaseQuantities);
    assert!(close(column.feet.unwrap(), 12.0));
    assert_eq!(column.material, "A992");

    let beam = model.parts.iter().find(|p| p.id == 200).unwrap();
    assert!(close(beam.pounds.unwrap(), 520.07), "{:?}", beam.pounds);
    assert_eq!(beam.weight_from, Source::Exporter);
    assert!(close(beam.feet.unwrap(), 20.0));
    assert_eq!(beam.length_from, Source::Exporter);
}

#[test]
fn a_part_with_no_weight_is_unweighed_not_weightless() {
    let model = two_bays();
    let brace = model.parts.iter().find(|p| p.kind == Kind::Brace).unwrap();
    assert_eq!(brace.pounds, None);
    assert_eq!(brace.weight_from, Source::Nowhere);
    // Its length is the extrusion it's drawn as: 4,572 mm, 15 ft.
    assert!(close(brace.feet.unwrap(), 15.0));
    assert_eq!(brace.length_from, Source::Shape);
    assert_eq!(model.unweighed(), 1);
}

#[test]
fn marks_skip_the_ids_tekla_gives_parts_with_no_mark() {
    let model = two_bays();
    let marked = model.parts.iter().find(|p| p.id == 200).unwrap();
    assert_eq!(marked.mark, "B1");
    assert_eq!(marked.assembly, "B1");
    let unmarked = model.parts.iter().find(|p| p.id == 220).unwrap();
    assert_eq!(unmarked.mark, "");
    let plate = model.parts.iter().find(|p| p.kind == Kind::Plate).unwrap();
    assert_eq!(plate.mark, "");
    assert_eq!(plate.assembly, "B1", "the plate ships with the beam");
    assert_eq!(model.assemblies(), 1);
}

#[test]
fn tonnage_by_profile_puts_the_same_section_together_whatever_its_case() {
    let model = two_bays();
    let rows = model.by_profile();
    let beams = rows.iter().find(|r| r.profile.eq_ignore_ascii_case("W12X26")).unwrap();
    assert_eq!(beams.pieces, 2, "W12X26 and W12x26 are one section");
    assert!(close(beams.feet, 40.0));
    assert!(close(beams.pounds, 1040.14));
    // Heaviest first.
    assert_eq!(rows[0].profile.to_uppercase(), "W12X26");
    let brace = rows.iter().find(|r| r.kind == Kind::Brace).unwrap();
    assert_eq!(brace.unweighed, 1);
    assert_eq!(brace.pounds, 0.0);
    let total = 2.0 * 395.51 + 2.0 * 520.07 + 9.92;
    assert!((model.pounds() - total).abs() < 0.05, "{}", model.pounds());
    assert!(model.by_profile_csv().starts_with("Kind,Profile,Pieces"));
}

#[test]
fn the_parts_list_says_where_each_number_came_from() {
    let csv = two_bays().parts_csv();
    assert!(csv.lines().count() == 7, "a header and six parts");
    assert!(csv.contains("Brace,HSS4X4X1/4,V1,,VERTICAL BRACE,,15.000,,its shape,not stated,#300"), "{csv}");
    assert_eq!(model::thousands(14484.4), "14,484");
    assert_eq!(model::thousands(999.0), "999");
}

#[test]
fn bolts_are_counted_by_the_size_they_are_ordered_in() {
    let model = two_bays();
    assert_eq!(model.bolts_by_size(), vec![("3/4\" x 2 1/2\"".to_string(), 2)]);
}

#[test]
fn something_that_isnt_an_ifc_file_is_refused_with_a_reason() {
    let e = Model::read(b"%PDF-1.7 not a model").unwrap_err();
    assert!(e.contains("isn't an IFC file"), "{e}");
}

/// The real models on a developer's machine, when there are some:
/// `HV_TEST_MODELS=folder cargo test -p model --release real -- --ignored --nocapture`.
#[test]
#[ignore]
fn real_models_read_and_total() {
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
        let started = std::time::Instant::now();
        let model = Model::read(&bytes).unwrap();
        let took = started.elapsed();
        println!(
            "{:<22} {:>6} parts {:>5} bolts {:>4} assemblies {:>9.0} lb {:>7.2} tons, {} unweighed, read in {:?} ({})",
            path.file_name().unwrap().to_string_lossy(),
            model.parts.len(),
            model.bolts.len(),
            model.assemblies(),
            model.pounds(),
            model.tons(),
            model.unweighed(),
            took,
            model.made_by.split(',').next().unwrap_or_default()
        );
        for row in model.by_profile().iter().take(6) {
            println!(
                "    {:<7} {:<18} {:>4} pcs {:>9.1} ft {:>9.0} lb{}",
                row.kind.name(),
                row.profile,
                row.pieces,
                row.feet,
                row.pounds,
                if row.unweighed > 0 { format!("  ({} unweighed)", row.unweighed) } else { String::new() }
            );
        }
    }
}
