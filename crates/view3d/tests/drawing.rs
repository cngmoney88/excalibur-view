//! Drawing the hand-written model off the screen, and clicking on it.
//!
//! These need a graphics adapter. A machine with none (a build server with
//! no GPU and no software renderer) skips them and says so, rather than
//! failing: the view itself reports the same thing to the person using it.

use std::sync::Arc;
use std::time::{Duration, Instant};

use model::shapes::{self, News};
use view3d::{Background, Camera, Look, Mesh, State, View};

fn ready() -> Option<View> {
    let mut view = View::start();
    let until = Instant::now() + Duration::from_secs(30);
    while Instant::now() < until {
        match view.state().clone() {
            State::Ready(_) => return Some(view),
            State::Failed(why) => {
                eprintln!("skipped: {why}");
                return None;
            }
            State::Starting => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    panic!("the view never started");
}

fn two_bays(view: &mut View) -> Vec<u32> {
    let bytes = Arc::new(include_bytes!("../../model/tests/two-bays.ifc").to_vec());
    let meshing = shapes::start(bytes, 1);
    let mut ids = Vec::new();
    for news in meshing.news.iter() {
        match news {
            News::Rough { shapes, .. } => {
                for shape in shapes {
                    view.set_part(ids.len() as u32, mesh(&shape));
                    ids.push(shape.id);
                }
            }
            News::Cut(shape) => {
                let part = ids.iter().position(|&id| id == shape.id).unwrap();
                view.set_part(part as u32, mesh(&shape));
            }
            News::Uncuttable(..) => {}
            News::Finished => break,
        }
    }
    view.set_colours(vec![[90, 130, 180, 255]; ids.len()]);
    ids
}

fn mesh(shape: &shapes::Shape) -> Mesh {
    Mesh {
        origin: shape.origin,
        positions: shape.positions.clone(),
        normals: shape.normals.clone(),
        indices: shape.indices.clone(),
    }
}

fn wait_for_frame(view: &mut View, serial: u64) -> view3d::Frame {
    let until = Instant::now() + Duration::from_secs(30);
    while Instant::now() < until {
        if let Some(frame) = view.frame() {
            if frame.serial == serial {
                return frame;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("no picture came back");
}

fn fitted(view: &View, aspect: f32) -> Camera {
    let mut camera = Camera::default();
    camera.fit(view.bounds().expect("the model has shapes"), aspect);
    camera
}

#[test]
fn the_model_is_drawn_and_the_part_in_the_middle_can_be_clicked() {
    let Some(mut view) = ready() else { return };
    let ids = two_bays(&mut view);
    assert_eq!(ids.len(), 5);
    let (w, h) = (320, 200);
    let camera = fitted(&view, w as f32 / h as f32);
    let serial = view.draw(camera, w, h, Look::default());
    let frame = wait_for_frame(&mut view, serial);
    assert_eq!((frame.width, frame.height), (w, h));
    assert_eq!(frame.rgba.len(), (w * h * 4) as usize);

    // Some of the picture is steel: blue more than red, unlike the grey
    // backdrop.
    let steel = frame.rgba.chunks_exact(4).filter(|p| p[2] as i32 - p[0] as i32 > 25).count();
    assert!(steel > 500, "only {steel} pixels of steel");
    // Every pixel is opaque against the studio backdrop.
    assert!(frame.rgba.chunks_exact(4).all(|p| p[3] == 255));

    // Somewhere on the model a click finds a part, and a corner of the
    // view finds none.
    let mut found = None;
    'search: for y in (0..h).step_by(8) {
        for x in (0..w).step_by(8) {
            let p = ((y * w + x) * 4) as usize;
            if frame.rgba[p + 2] as i32 - frame.rgba[p] as i32 > 25 {
                found = view.pick(camera, w, h, Look::default(), x, y).recv().unwrap();
                break 'search;
            }
        }
    }
    let part = found.expect("a click on steel finds a part");
    assert!((part as usize) < ids.len());
    assert_eq!(view.pick(camera, w, h, Look::default(), 0, 0).recv().unwrap(), None);
}

#[test]
fn a_hidden_part_is_neither_drawn_nor_clicked() {
    let Some(mut view) = ready() else { return };
    let ids = two_bays(&mut view);
    view.set_colours(vec![[90, 130, 180, 0]; ids.len()]);
    let (w, h) = (160, 100);
    let camera = fitted(&view, 1.6);
    let serial = view.draw(camera, w, h, Look::default());
    let frame = wait_for_frame(&mut view, serial);
    assert!(frame.rgba.chunks_exact(4).all(|p| (p[2] as i32 - p[0] as i32) < 25), "nothing blue");
    for (x, y) in [(80, 50), (60, 40), (100, 60)] {
        assert_eq!(view.pick(camera, w, h, Look::default(), x, y).recv().unwrap(), None);
    }
}

#[test]
fn a_picture_with_no_background_is_transparent_around_the_model() {
    let Some(mut view) = ready() else { return };
    two_bays(&mut view);
    let look = Look { background: Background::Clear, ..Look::default() };
    let camera = fitted(&view, 1.0);
    let frame = view.picture(camera, 256, 256, look).recv().unwrap().unwrap().unpremultiplied();
    assert_eq!(&frame.rgba[..4], &[0, 0, 0, 0], "the corner is empty");
    assert!(frame.rgba.chunks_exact(4).any(|p| p[3] == 255), "the model is solid");
}

#[test]
fn cutting_away_the_top_leaves_less_drawn() {
    let Some(mut view) = ready() else { return };
    two_bays(&mut view);
    let camera = fitted(&view, 1.0);
    let bounds = view.bounds().unwrap();
    let steel = |view: &mut View, look: Look| {
        let serial = view.draw(camera, 200, 200, look);
        let frame = wait_for_frame(view, serial);
        frame.rgba.chunks_exact(4).filter(|p| p[2] as i32 - p[0] as i32 > 25).count()
    };
    let whole = steel(&mut view, Look::default());
    let middle = (bounds.min[2] + bounds.max[2]) * 0.5;
    let cut = steel(&mut view, Look { cut_above: Some(middle), ..Look::default() });
    assert!(cut < whole, "{cut} of {whole}");
    assert!(cut > 0);
}

#[test]
fn a_probe_near_a_corner_catches_the_corner() {
    let Some(mut view) = ready() else { return };
    let ids = two_bays(&mut view);
    let (w, h) = (400u32, 300u32);
    let aspect = w as f32 / h as f32;
    let column = ids.iter().position(|&id| id == 100).unwrap() as u32;
    let bounds = view.bounds_of([column]).unwrap();
    let mut camera = Camera::default();
    camera.fit(view.bounds().unwrap(), aspect);
    // The top of the column, on screen, nudged a few pixels off it.
    let top = [bounds.max[0], bounds.min[1], bounds.max[2]];
    let m = view.view_proj(camera, aspect);
    let c = view3d::camera::project(&m, top);
    let (sx, sy) = ((c[0] / c[3] * 0.5 + 0.5) * w as f32, (0.5 - c[1] / c[3] * 0.5) * h as f32);
    let mut caught = None;
    for (dx, dy) in [(-3.0, 3.0), (-3.0, 4.0), (-4.0, 5.0), (-2.0, 6.0)] {
        let (x, y) = ((sx + dx) as u32, (sy + dy) as u32);
        if let Some(probe) = view.probe(camera, w, h, Look::default(), x, y).recv().unwrap() {
            caught = Some(probe);
            break;
        }
    }
    let probe = caught.expect("the pointer is on the column");
    assert_eq!(probe.snapped, view3d::Snap::Corner, "{probe:?}");
    let whole = view.bounds().unwrap();
    for k in 0..3 {
        assert!(probe.point[k] >= whole.min[k] - 1e-3 && probe.point[k] <= whole.max[k] + 1e-3, "{probe:?}");
    }
    assert_eq!(view.probe(camera, w, h, Look::default(), 1, 1).recv().unwrap(), None);
}
