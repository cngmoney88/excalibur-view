//! What has to be true before anybody in the office can use this: a
//! measurement taken here goes into the drawing as a real PDF annotation,
//! survives a reopen, and reads the same number Revu would read.
//!
//! These run against a real stamped drawing set when one is present. They are
//! skipped, loudly, when it is not, rather than quietly passing on nothing.

use std::path::{Path, PathBuf};

use annot::Kind;
use hyperview::app::Tool;
use hyperview::sheet::{Doc, Draft};

/// The drawing set these tests use, if it is on this machine.
fn drawing() -> Option<PathBuf> {
    for candidate in [
        "/mnt/user-data/uploads/Fox Theater/2026-08-13 Structural Stamped.pdf",
        "tests/data/structural.pdf",
    ] {
        let path = Path::new(candidate);
        if path.exists() {
            return Some(path.to_path_buf());
        }
    }
    None
}

/// Copies the drawing somewhere writable, so a test never touches the original.
fn scratch(name: &str) -> Option<PathBuf> {
    let source = drawing()?;
    let target = std::env::temp_dir().join(format!("hyperview-{name}.pdf"));
    std::fs::copy(&source, &target).ok()?;
    // A copy inherits the original's permissions, and the drawing sets that
    // come off a share are often read-only. Hyperview refuses to save onto one,
    // which is the point of the guard, so a test working copy clears the bit.
    let mut permissions = std::fs::metadata(&target).ok()?.permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&target, permissions).ok()?;
    Some(target)
}

fn quarter_inch() -> annot::measure::Measure {
    annot::measure::imperial(48.0, "1/4\" = 1'-0\"", 16)
}

/// A W12x26 the way his tool chest carries one: the weight is in user column
/// zero, and it is his number, not one this program looked up.
fn w12x26() -> pdf::Dict {
    let mut tool = annot::Markup::new(annot::Subtype::PolyLine);
    tool.set("IT", pdf::Object::name("PolyLineDimension"));
    tool.set("MeasurementTypes", pdf::Object::Int(2));
    tool.set_subject("W12x26");
    tool.set_colour([0.0, 0.45, 0.85]);
    tool.set_width(2.0);
    tool.set(
        "BSIColumnData",
        pdf::Object::Array(vec![
            pdf::Object::text("26"),
            pdf::Object::text(""),
            pdf::Object::text(""),
            pdf::Object::text(""),
            pdf::Object::text(""),
            pdf::Object::text(""),
        ]),
    );
    tool.dict
}

#[test]
fn a_length_taken_here_is_in_the_file_when_it_is_saved() {
    let Some(path) = scratch("length") else {
        eprintln!("skipped: no drawing set on this machine");
        return;
    };
    let mut doc = Doc::open(path.clone()).expect("open");
    let before = doc.marks.len();
    doc.set_scale(0, Some(quarter_inch()));

    // Forty feet at quarter inch scale is 720 points across the sheet.
    let draft = Draft {
        tool: Tool::Length,
        points: vec![[100.0, 100.0], [820.0, 100.0]],
        strokes: Vec::new(),
        subject: "W12x26".into(),
        colour: [0, 115, 217, 255],
        template: Some(w12x26()),
        ..Draft::default()
    };
    let index = doc.place(0, draft).expect("the draft became a markup");
    assert_eq!(doc.marks[index].markup.contents(), "40'-0\"");

    let written = doc.save("Creede").expect("save");
    assert!(written >= 1, "something should have been written");

    // Reopen from disk: this is the test that matters, because it is the file
    // talking and not our own copy in memory.
    let again = Doc::open(path).expect("reopen");
    assert_eq!(
        again.marks.len(),
        before + 1,
        "the markup should be in the file"
    );
    let mine = again
        .marks
        .iter()
        .find(|m| m.subject() == "W12x26")
        .expect("our markup, by its subject");
    assert_eq!(mine.markup.contents(), "40'-0\"");
    assert_eq!(mine.kind(), Kind::Length);
    assert!(
        mine.markup.dict.get("Measure").is_some(),
        "it must carry its own /Measure so Revu reads the same number"
    );
    assert!(
        mine.markup.dict.get("AP").is_some(),
        "without an appearance stream no viewer draws it"
    );
}

#[test]
fn the_weight_comes_from_his_tool_and_totals_in_tons() {
    let Some(path) = scratch("weight") else {
        eprintln!("skipped: no drawing set on this machine");
        return;
    };
    let mut doc = Doc::open(path).expect("open");
    doc.set_scale(0, Some(quarter_inch()));

    // Two forty-foot beams: eighty feet of W12x26 is 2,080 lb.
    for offset in [0.0, 50.0] {
        let draft = Draft {
            tool: Tool::Length,
            points: vec![[100.0, 100.0 + offset], [820.0, 100.0 + offset]],
            strokes: Vec::new(),
            subject: "W12x26".into(),
            colour: [0, 115, 217, 255],
            template: Some(w12x26()),
            ..Draft::default()
        };
        doc.place(0, draft).expect("placed");
    }

    let rows = doc.rows();
    let summary = takeoff::summarise(&rows, hyperview::panelbody::weights());
    assert!((summary.pounds - 2080.0).abs() < 1.0, "{}", summary.pounds);
    assert!((summary.tons() - 1.04).abs() < 0.001, "{}", summary.tons());
    assert_eq!(summary.unscaled, 0);
}

#[test]
fn a_sheet_with_no_scale_is_left_out_and_says_so() {
    let Some(path) = scratch("noscale") else {
        eprintln!("skipped: no drawing set on this machine");
        return;
    };
    let mut doc = Doc::open(path).expect("open");
    // Deliberately no scale on this sheet.
    doc.set_scale(0, None);

    let draft = Draft {
        tool: Tool::Length,
        points: vec![[100.0, 100.0], [820.0, 100.0]],
        strokes: Vec::new(),
        subject: "W12x26".into(),
        colour: [0, 115, 217, 255],
        template: Some(w12x26()),
        ..Draft::default()
    };
    let index = doc.place(0, draft).expect("placed");

    // No number at all, rather than a wrong one.
    assert_eq!(doc.marks[index].markup.contents(), "");
    let rows = doc.rows();
    let summary = takeoff::summarise(&rows, hyperview::panelbody::weights());
    assert_eq!(summary.unscaled, 1, "it must be reported, not counted");
    assert_eq!(summary.pounds, 0.0, "and never counted as zero pounds");
    assert!(summary.is_short());

    let csv = takeoff::summary::summary_csv(&summary);
    assert!(csv.contains("were left out"), "{csv}");
}

#[test]
fn setting_the_scale_afterwards_rewrites_what_the_measurements_read() {
    let Some(path) = scratch("rescale") else {
        eprintln!("skipped: no drawing set on this machine");
        return;
    };
    let mut doc = Doc::open(path).expect("open");
    doc.set_scale(0, None);
    let draft = Draft {
        tool: Tool::Length,
        points: vec![[100.0, 100.0], [820.0, 100.0]],
        strokes: Vec::new(),
        subject: "W12x26".into(),
        colour: [0, 115, 217, 255],
        template: Some(w12x26()),
        ..Draft::default()
    };
    let index = doc.place(0, draft).expect("placed");
    assert_eq!(doc.marks[index].markup.contents(), "");

    doc.set_scale(0, Some(quarter_inch()));
    assert_eq!(doc.marks[index].markup.contents(), "40'-0\"");

    // And an eighth inch scale doubles it, because the line did not move.
    doc.set_scale(0, Some(annot::measure::imperial(96.0, "1/8\" = 1'-0\"", 16)));
    assert_eq!(doc.marks[index].markup.contents(), "80'-0\"");
}

#[test]
fn deleting_a_markup_takes_it_out_of_the_file() {
    let Some(path) = scratch("delete") else {
        eprintln!("skipped: no drawing set on this machine");
        return;
    };
    let mut doc = Doc::open(path.clone()).expect("open");
    doc.set_scale(0, Some(quarter_inch()));
    let draft = Draft {
        tool: Tool::Length,
        points: vec![[100.0, 100.0], [820.0, 100.0]],
        strokes: Vec::new(),
        subject: "ToBeDeleted".into(),
        colour: [0, 115, 217, 255],
        template: Some(w12x26()),
        ..Draft::default()
    };
    doc.place(0, draft).expect("placed");
    doc.save("Creede").expect("save");

    let mut doc = Doc::open(path.clone()).expect("reopen");
    let index = doc
        .marks
        .iter()
        .position(|m| m.subject() == "ToBeDeleted")
        .expect("it is there");
    doc.checkpoint();
    doc.marks[index].gone = true;
    doc.save("Creede").expect("save again");

    let doc = Doc::open(path).expect("reopen again");
    assert!(
        !doc.marks.iter().any(|m| m.subject() == "ToBeDeleted"),
        "it should be gone"
    );
}

#[test]
fn a_run_clicked_around_corners_measures_every_leg_of_it() {
    let Some(path) = scratch("polyline") else {
        eprintln!("skipped: no drawing set on this machine");
        return;
    };
    let mut doc = Doc::open(path.clone()).expect("open");
    doc.set_scale(0, Some(quarter_inch()));

    // An L: 720 points across, then 360 down. Forty feet plus twenty is sixty.
    // A two-point line between the ends would read about 44'-9" — short by a
    // quarter, which is exactly the kind of error nobody catches until the
    // steel arrives.
    let draft = Draft {
        tool: Tool::Length,
        points: vec![[100.0, 100.0], [820.0, 100.0], [820.0, 460.0]],
        strokes: Vec::new(),
        subject: "W12x26".into(),
        colour: [0, 115, 217, 255],
        template: Some(w12x26()),
        ..Draft::default()
    };
    let index = doc.place(0, draft).expect("placed");
    assert_eq!(doc.marks[index].markup.contents(), "60'-0\"");
    assert_eq!(
        doc.marks[index].markup.subtype(),
        annot::Subtype::PolyLine,
        "three points cannot live in a two-point Line"
    );
    assert_eq!(doc.marks[index].markup.points().len(), 3);

    doc.save("Creede").expect("save");
    let again = Doc::open(path).expect("reopen");
    let mine = again
        .marks
        .iter()
        .find(|m| m.subject() == "W12x26")
        .expect("ours");
    assert_eq!(mine.markup.points().len(), 3, "all three corners survived");
    assert_eq!(mine.markup.contents(), "60'-0\"");

    // And it weighs what sixty feet of W12x26 weighs: 1,560 lb.
    let rows = again.rows();
    let summary = takeoff::summarise(&rows, hyperview::panelbody::weights());
    assert!((summary.pounds - 1560.0).abs() < 1.0, "{}", summary.pounds);
}

#[test]
fn an_area_taken_here_totals_in_square_feet_and_carries_its_psf() {
    let Some(path) = scratch("area") else {
        eprintln!("skipped: no drawing set on this machine");
        return;
    };
    let mut doc = Doc::open(path).expect("open");
    doc.set_scale(0, Some(quarter_inch()));

    // A plate tool: pounds per square foot lives in user column five.
    let mut tool = annot::Markup::new(annot::Subtype::Polygon);
    tool.set("IT", pdf::Object::name("PolygonDimension"));
    tool.set("MeasurementTypes", pdf::Object::Int(1));
    tool.set_subject("PL1/2");
    tool.set_colour([0.85, 0.35, 0.1]);
    tool.set(
        "BSIColumnData",
        pdf::Object::Array(vec![
            pdf::Object::text(""),
            pdf::Object::text(""),
            pdf::Object::text(""),
            pdf::Object::text(""),
            pdf::Object::text(""),
            pdf::Object::text("20.42"),
        ]),
    );

    // 720 x 360 points is 40' x 20' — 800 square feet.
    let draft = Draft {
        tool: Tool::Area,
        points: vec![
            [100.0, 100.0],
            [820.0, 100.0],
            [820.0, 460.0],
            [100.0, 460.0],
        ],
        strokes: Vec::new(),
        subject: "PL1/2".into(),
        colour: [217, 90, 26, 255],
        template: Some(tool.dict),
        ..Draft::default()
    };
    let index = doc.place(0, draft).expect("placed");
    assert_eq!(doc.marks[index].markup.contents(), "800 sf");

    let rows = doc.rows();
    let summary = takeoff::summarise(&rows, hyperview::panelbody::weights());
    // 800 sf at 20.42 psf is 16,336 lb — 8.168 tons.
    assert!((summary.pounds - 16336.0).abs() < 1.0, "{}", summary.pounds);
    assert!((summary.tons() - 8.168).abs() < 0.001, "{}", summary.tons());
}

#[test]
fn a_read_only_drawing_says_so_rather_than_losing_the_markups() {
    let Some(source) = drawing() else {
        eprintln!("skipped: no drawing set on this machine");
        return;
    };
    let target = std::env::temp_dir().join("hyperview-readonly.pdf");
    std::fs::copy(&source, &target).expect("copy");
    let mut permissions = std::fs::metadata(&target).expect("stat").permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&target, permissions).expect("chmod");

    let mut doc = Doc::open(target.clone()).expect("open");
    assert!(doc.read_only, "it should know before anybody draws on it");
    doc.set_scale(0, Some(quarter_inch()));
    let draft = Draft {
        tool: Tool::Length,
        points: vec![[100.0, 100.0], [820.0, 100.0]],
        strokes: Vec::new(),
        subject: "W12x26".into(),
        colour: [0, 115, 217, 255],
        template: Some(w12x26()),
        ..Draft::default()
    };
    doc.place(0, draft).expect("placed");

    let error = doc.save("Creede").expect_err("it must refuse");
    assert!(error.contains("read-only"), "{error}");
    // And the work is still here, waiting for a Save As.
    assert!(doc.dirty, "nothing should have been thrown away");
    assert_eq!(doc.marks.len(), 1);
}

// ---- the shop list, end to end ---------------------------------------------

/// The grid the label scan found, put onto the Doc the way the window does.
///
/// In the running program this arrives from the render worker with the sheet
/// names. A test has no worker, so it asks for the same thing directly and
/// puts it in the same place.
fn read_the_grid(doc: &mut Doc, page: u32) -> takeoff::grid::Grid {
    let library = std::env::var("PDFIUM_DIR").ok().map(PathBuf::from);
    let grid = hyperview::render::grid_on(
        library,
        &doc.path,
        page,
        takeoff::grid::HowClose::default(),
    )
    .unwrap_or_default();
    if let Some(label) = doc.labels.get_mut(page as usize) {
        label.grid = grid.clone();
    }
    grid
}

/// Which sheet of this set carries a grid, if any does.
fn a_sheet_with_a_grid(doc: &mut Doc) -> Option<(u32, takeoff::grid::Grid)> {
    for page in 0..doc.pages.len().min(12) as u32 {
        let grid = read_the_grid(doc, page);
        if !grid.vertical.is_empty() && !grid.horizontal.is_empty() {
            return Some((page, grid));
        }
    }
    None
}

#[test]
fn a_real_framing_plan_gives_its_markups_a_grid_location() {
    let Some(path) = scratch("grid") else {
        eprintln!("skipped: no drawing set on this machine");
        return;
    };
    let mut doc = Doc::open(path).expect("open");
    let Some((page, grid)) = a_sheet_with_a_grid(&mut doc) else {
        eprintln!("skipped: no sheet in this set carries a grid this can read");
        return;
    };
    doc.set_scale(page, Some(quarter_inch()));

    // Put a beam right on an intersection of the grid that was actually found
    // on the sheet, so this is testing the real geometry and not a guess about
    // where the lines are.
    let on_a_line = [grid.vertical[1].at, grid.horizontal[1].at];
    let draft = Draft {
        tool: Tool::Length,
        points: vec![on_a_line, [on_a_line[0] + 360.0, on_a_line[1]]],
        strokes: Vec::new(),
        subject: "W12x26".into(),
        colour: [0, 115, 217, 255],
        template: Some(w12x26()),
        ..Draft::default()
    };
    let index = doc.place(page, draft).expect("the draft became a markup");

    let said = doc.grid_location(index);
    assert!(
        !said.is_empty(),
        "a markup on a grid intersection must get a location; the sheet's \
         grid is {}",
        grid.extent()
    );
    // It starts on line 1 of the vertical run, so that label has to be in it.
    assert!(
        said.contains(&grid.vertical[1].label),
        "{said:?} should mention {}",
        grid.vertical[1].label
    );
    // And the row carries it, which is what the list, the CSV and the cut
    // list all read.
    let row = doc.row(index).expect("a row");
    assert_eq!(row.grid, said);
}

#[test]
fn a_takeoff_becomes_a_cut_list_that_says_what_to_buy() {
    let Some(path) = scratch("shoplist") else {
        eprintln!("skipped: no drawing set on this machine");
        return;
    };
    let mut doc = Doc::open(path).expect("open");
    doc.set_scale(0, Some(quarter_inch()));

    // Three beams: two the same length, one longer. 720 points at quarter
    // inch scale is forty feet; 360 is twenty.
    for (n, across) in [720.0, 720.0, 360.0].iter().enumerate() {
        let y = 100.0 + n as f64 * 50.0;
        let draft = Draft {
            tool: Tool::Length,
            points: vec![[100.0, y], [100.0 + across, y]],
            strokes: Vec::new(),
            subject: "W12x26".into(),
            colour: [0, 115, 217, 255],
            template: Some(w12x26()),
            ..Draft::default()
        };
        doc.place(0, draft).expect("placed");
    }

    let rows = doc.rows();
    let list = takeoff::shoplist::build(
        &rows,
        hyperview::panelbody::weights(),
        takeoff::shoplist::INCH,
    );
    let shape = list
        .shape("W12x26")
        .expect("the shape is on the list by the name his tool gave it");
    assert_eq!(shape.pieces, 3);
    // Two at forty feet on one line, one at twenty on another. Longest first.
    assert_eq!(shape.cuts.len(), 2);
    assert!((shape.cuts[0].length - 40.0).abs() < 0.001, "{:?}", shape.cuts);
    assert_eq!(shape.cuts[0].count, 2);
    assert!((shape.cuts[1].length - 20.0).abs() < 0.001);
    assert_eq!(shape.cuts[1].count, 1);

    // A hundred feet of a twenty-six pound section, from his own tool chest.
    assert!(
        (list.pounds - 100.0 * 26.0).abs() < 0.5,
        "{} lb",
        list.pounds
    );
    assert!(list.is_whole(), "{}", list.what_is_missing());

    // And what to buy: a hundred feet in forty foot sticks, with the kerf
    // taken off every cut after the first on a stick.
    let nest = takeoff::shoplist::nest(shape, 40.0, takeoff::shoplist::INCH / 8.0);
    assert_eq!(
        nest.sticks_to_buy(),
        3,
        "two full sticks and one for the twenty: {:?}",
        nest.sticks
    );
    assert!(nest.is_whole(), "every piece came out of a stick");
    // Nothing was overfilled.
    for stick in &nest.sticks {
        let on_it: f64 = stick.pieces.iter().sum();
        assert!(on_it <= 40.0 + 1e-9, "a stick was overfilled: {on_it}");
    }
}

#[test]
fn the_same_beam_drawn_twice_is_found_and_nothing_is_taken_off_the_total() {
    let Some(path) = scratch("doubles") else {
        eprintln!("skipped: no drawing set on this machine");
        return;
    };
    let mut doc = Doc::open(path).expect("open");
    doc.set_scale(0, Some(quarter_inch()));

    // The same beam, drawn on Tuesday and again on Thursday a hair off.
    for offset in [0.0, 1.0] {
        let draft = Draft {
            tool: Tool::Length,
            points: vec![[100.0 + offset, 200.0], [820.0 + offset, 200.5]],
            strokes: Vec::new(),
            subject: "W12x26".into(),
            colour: [0, 115, 217, 255],
            template: Some(w12x26()),
            ..Draft::default()
        };
        doc.place(0, draft).expect("placed");
    }

    let rows = doc.rows();
    let before = takeoff::summarise(&rows, hyperview::panelbody::weights()).pounds;
    let found = takeoff::doubles::find(&rows, takeoff::doubles::HowClose::default());
    assert_eq!(found.certain(), 1, "{:?}", found.pairs);

    // The whole contract: it reports, and the takeoff is exactly as it was.
    let after = takeoff::summarise(&rows, hyperview::panelbody::weights()).pounds;
    assert!((before - after).abs() < 1e-12);
    assert!(found
        .what_it_is_worth(hyperview::panelbody::weights(), &rows)
        .contains("Nothing has been subtracted"));
}

#[test]
fn a_quantity_and_a_slope_are_in_the_file_and_in_every_total() {
    let Some(path) = scratch("quantity") else {
        eprintln!("skipped: no drawing set on this machine");
        return;
    };
    let mut doc = Doc::open(path.clone()).expect("open");
    doc.set_scale(0, Some(quarter_inch()));
    // Twelve feet of run on the sheet: 216 points at quarter inch.
    let draft = Draft {
        tool: Tool::Length,
        points: vec![[100.0, 100.0], [316.0, 100.0]],
        subject: "Rafter".into(),
        ..Draft::default()
    };
    let index = doc.place(0, draft).expect("placed");
    takeoff::row::set_quantity(&mut doc.marks[index].markup, 6.0);
    takeoff::row::set_pitch(&mut doc.marks[index].markup, 4.0, 12.0);
    doc.marks[index].changed = true;
    doc.choose(Some(index));
    doc.remeasure_selection();
    // Up a 4 in 12 pitch, twelve feet of run is 12'-7 13/16".
    assert!(doc.marks[index].markup.contents().starts_with("12'-7"), "{}", doc.marks[index].markup.contents());
    doc.save("Creede").expect("save");

    let again = Doc::open(path.clone()).expect("reopen");
    let rafter = again.live().find(|(_, m)| m.subject() == "Rafter").map(|(i, _)| i).expect("in the file");
    let row = again.row(rafter).unwrap();
    assert_eq!(row.quantity, 6.0);
    assert_eq!(row.slope, Some(4.0));
    let summary = takeoff::summarise(&again.rows(), Default::default());
    let group = summary.groups.iter().find(|g| g.subject == "Rafter").unwrap();
    assert!((group.length - 6.0 * 12.649_110_640_673_518).abs() < 0.01, "{}", group.length);
    // To look at it in the program: HV_KEEP_DRAWING=somewhere.pdf.
    if let Ok(keep) = std::env::var("HV_KEEP_DRAWING") {
        let _ = std::fs::copy(&path, keep);
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_text_box_is_picked_up_by_its_middle_and_moves_where_it_is_dragged() {
    let Some(path) = scratch("move") else {
        eprintln!("skipped: no drawing set on this machine");
        return;
    };
    let mut doc = Doc::open(path.clone()).expect("open");
    let draft = Draft {
        tool: Tool::Rect,
        points: vec![[200.0, 200.0], [300.0, 240.0]],
        ..Draft::default()
    };
    let index = doc.place(0, draft).expect("placed");
    let frame = doc.frame();
    let reach = 144.0;
    // The middle of the box, nowhere near a corner or an edge.
    assert_eq!(hyperview::view::hit_at(&doc, &frame, 0, [250.0, 220.0], reach), Some(index));
    assert_eq!(hyperview::view::hit_at(&doc, &frame, 0, [600.0, 600.0], reach), None);
    // Dragged 50 points right and 30 down on the sheet.
    let (a, b) = (frame.to_pdf([250.0, 220.0]), frame.to_pdf([300.0, 250.0]));
    doc.marks[index].markup.move_by(b[0] - a[0], b[1] - a[1]);
    let moved = doc.marks[index].on_sheet(&frame);
    let left = moved.iter().map(|p| p[0]).fold(f64::MAX, f64::min);
    let top = moved.iter().map(|p| p[1]).fold(f64::MAX, f64::min);
    assert!((left - 250.0).abs() < 0.01 && (top - 230.0).abs() < 0.01, "{moved:?}");
    let _ = std::fs::remove_file(path);
}

/// A real drawing, read the way a plugin is handed it, and a real plugin run
/// over it: `HV_TEST_PLUGIN=path/to.wasm HV_TEST_DRAWING=path/to.pdf cargo
/// test -p hyperview --test end_to_end a_real_sheet -- --ignored --nocapture`.
#[test]
#[ignore]
fn a_real_sheet_read_for_a_plugin_and_run_through_one() {
    let wasm = std::fs::read(std::env::var("HV_TEST_PLUGIN").expect("HV_TEST_PLUGIN")).unwrap();
    let drawing = std::path::PathBuf::from(std::env::var("HV_TEST_DRAWING").expect("HV_TEST_DRAWING"));
    let pages = hyperview::render::pages_in(std::env::var("PDFIUM_DIR").ok().map(PathBuf::from), &drawing).unwrap();
    let manifest = hyperview::plugins::manifest_of(&wasm).unwrap();
    let mut sheets = Vec::new();
    for page in 0..pages as u32 {
        let started = std::time::Instant::now();
        let reading = hyperview::render::read_for_plugin(std::env::var("PDFIUM_DIR").ok().map(PathBuf::from), &drawing, page, true, true).unwrap();
        println!(
            "sheet {page}: {} words, {} lines{} in {:?}",
            reading.words.len(),
            reading.lines.len(),
            if reading.cut_short { " (cut short)" } else { "" },
            started.elapsed()
        );
        let scale = std::env::var("HV_TEST_SCALE").ok().and_then(|r| r.parse::<f64>().ok());
        sheets.push(plugin_api::Sheet {
            page,
            name: format!("Sheet {}", page + 1),
            width: reading.size.0,
            height: reading.size.1,
            scale: scale.map(|ratio| plugin_api::Scale { ratio, text: String::new() }),
            words: reading.words,
            lines: reading.lines,
            lines_cut_short: reading.cut_short,
        });
    }
    for command in &manifest.commands {
        let input = plugin_api::Input {
            command: command.id.clone(),
            current_page: std::env::var("HV_TEST_PAGE").ok().and_then(|p| p.parse().ok()).unwrap_or(0),
            sheets: sheets.clone(),
            columns: vec!["LBS Per FT".into()],
            ..Default::default()
        };
        let started = std::time::Instant::now();
        let output = hyperview::plugins::run(&wasm, &input).unwrap();
        println!("\n== {} in {:?}: {}", command.name, started.elapsed(), output.summary);
        if let Ok(dir) = std::env::var("HV_TEST_DUMP") {
            let _ = std::fs::write(
                std::path::Path::new(&dir).join(format!("{}.json", command.id)),
                serde_json::to_vec_pretty(&output.findings).unwrap(),
            );
        }
        for f in output.findings.iter().take(40) {
            println!("  [{:?}] {} {:?}", f.level, f.message, f.fixes.iter().map(|x| &x.label).collect::<Vec<_>>());
        }
        for t in &output.tables {
            for row in t.rows.iter().take(8) {
                println!("  | {}", row.join(" | "));
            }
        }
    }
}
