//! Pictures of a model from the command line, for checking the view by eye:
//!
//! `cargo run --release -p view3d --example snap -- model.ifc out-folder [width height]`
//!
//! Writes the uncut first pass and the finished model from each named view.

use std::sync::Arc;
use std::time::{Duration, Instant};

use model::shapes::{self, Group, News};
use view3d::{Background, Camera, Look, Mesh, Preset, State, View};

fn colour(group: Group) -> [u8; 4] {
    match group {
        Group::Column => [0x35, 0x61, 0x8F, 255],
        Group::Beam => [0x8D, 0xA9, 0xC4, 255],
        Group::Brace => [0x5B, 0xA0, 0x8A, 255],
        Group::Plate => [0xD2, 0x9B, 0x3F, 255],
        Group::Bolt => [0x8C, 0x6A, 0x3B, 255],
        Group::Concrete => [0xBD, 0xB8, 0xAE, 255],
        Group::Other => [0xA3, 0xAC, 0xB6, 255],
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = &args[1];
    let out = std::path::Path::new(&args[2]);
    let width: u32 = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(1600);
    let height: u32 = args.get(4).and_then(|a| a.parse().ok()).unwrap_or(1000);
    std::fs::create_dir_all(out).unwrap();
    let stem = std::path::Path::new(path).file_stem().unwrap().to_string_lossy().to_string();

    let mut view = View::start();
    loop {
        match view.state().clone() {
            State::Ready(adapter) => {
                println!("drawing on {adapter}");
                break;
            }
            State::Failed(why) => panic!("{why}"),
            State::Starting => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let started = Instant::now();
    let meshing = shapes::start(Arc::new(std::fs::read(path).unwrap()), 4);
    let mut ids: Vec<u32> = Vec::new();
    let mut colours: Vec<[u8; 4]> = Vec::new();
    let aspect = width as f32 / height as f32;
    let snap = |view: &mut View, name: &str, camera: Camera, look: Look| {
        let t = Instant::now();
        let frame = view.picture(camera, width, height, look).recv().unwrap().unwrap().unpremultiplied();
        let file = out.join(format!("{stem}-{name}.png"));
        image::save_buffer(&file, &frame.rgba, frame.width, frame.height, image::ExtendedColorType::Rgba8).unwrap();
        println!("  {} in {:?}", file.display(), t.elapsed());
    };
    for news in meshing.news.iter() {
        match news {
            News::Rough { shapes, failed, to_cut } => {
                println!("{} shapes uncut in {:?}, {} not drawn, {to_cut} to cut", shapes.len(), started.elapsed(), failed.len());
                for shape in shapes {
                    view.set_part(ids.len() as u32, Mesh { origin: shape.origin, positions: shape.positions, normals: shape.normals, indices: shape.indices });
                    ids.push(shape.id);
                    colours.push(colour(shape.group));
                }
                view.set_colours(colours.clone());
                let mut camera = Camera::default();
                camera.fit(view.bounds().unwrap(), aspect);
                snap(&mut view, "uncut", camera, Look::default());
            }
            News::Cut(shape) => {
                let part = match ids.iter().position(|&id| id == shape.id) {
                    Some(part) => part,
                    None => {
                        ids.push(shape.id);
                        colours.push(colour(shape.group));
                        view.set_colours(colours.clone());
                        ids.len() - 1
                    }
                };
                view.set_part(part as u32, Mesh { origin: shape.origin, positions: shape.positions, normals: shape.normals, indices: shape.indices });
            }
            News::Uncuttable(id, why) => println!("  #{id} stays uncut: {why}"),
            News::Finished => break,
        }
    }
    println!("cut by {:?}", started.elapsed());
    let bounds = view.bounds().unwrap();
    for preset in [Preset::Iso, Preset::Front, Preset::Top] {
        let mut camera = Camera::default();
        camera.look(preset);
        camera.fit(bounds, aspect);
        snap(&mut view, preset.name(), camera, Look::default());
    }
    // A close look at one part and what's around it: FOCUS=<its #number>.
    if let Some(focus) = std::env::var("FOCUS").ok().and_then(|f| f.trim_start_matches('#').parse::<u32>().ok()) {
        if let Some(part) = ids.iter().position(|&id| id == focus) {
            let mut camera = Camera::default();
            camera.fit(view.bounds_of([part as u32]).unwrap(), aspect);
            camera.zoom(0.5, [0.0, 0.0], aspect);
            snap(&mut view, "close", camera, Look::default());
        }
    }
    let mut camera = Camera::default();
    camera.fit(bounds, aspect);
    snap(&mut view, "dark", camera, Look { background: Background::Dark, ..Look::default() });
    camera.orthographic = true;
    camera.look(Preset::Front);
    camera.fit(bounds, aspect);
    snap(&mut view, "front-parallel", camera, Look { background: Background::White, ..Look::default() });
}
