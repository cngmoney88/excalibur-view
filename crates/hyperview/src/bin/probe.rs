//! Bench harness: proves the tiling math and measures the real costs against a
//! production drawing set. Not shipped to users.

use hyperview::render::*;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = PathBuf::from(&args[1]);
    let lib = PathBuf::from(
        std::env::var("PDFIUM_DIR").unwrap_or_else(|_| "third_party/pdfium/linux-x64/lib".into()),
    );
    let ctx = egui::Context::default();
    let svc = Service::start(Some(lib), ctx);
    svc.send(ToWorker::Open { doc: 1, path });

    let mut pages: Vec<PageSize> = Vec::new();
    let page: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut waiting_tiles = 0usize;
    let mut tile_ms: Vec<u128> = Vec::new();
    let mut labels_done;
    let started = Instant::now();
    let mut label_started = Instant::now();

    loop {
        let msg = match svc.rx.recv_timeout(std::time::Duration::from_secs(120)) {
            Ok(m) => m,
            Err(_) => break,
        };
        match msg {
            FromWorker::LibraryFailed(why) => {
                println!("library failed:\n{why}");
                return;
            }
            FromWorker::OpenFailed { why, .. } => {
                println!("open failed: {why}");
                return;
            }
            FromWorker::Opened { pages: p, millis, .. } => {
                println!("opened {} pages in {millis} ms", p.len());
                println!("page {page} is {:?}", p[page as usize]);
                pages = p;
                svc.send(ToWorker::Preview { doc: 1, page });
                // Ask for every tile of the whole sheet at 1:1 and at 4x for a
                // window sized patch, which is what a real pan/zoom looks like.
                let s = pages[page as usize];
                let mut want = Vec::new();
                let across = (s.width / TILE as f32).ceil() as u32;
                let down = (s.height / TILE as f32).ceil() as u32;
                for ty in 0..down {
                    for tx in 0..across {
                        want.push(TileKey { doc: 1, page, bucket: 0, tx, ty });
                    }
                }
                for ty in 6..10 {
                    for tx in 6..10 {
                        want.push(TileKey { doc: 1, page, bucket: 2, tx, ty });
                    }
                }
                waiting_tiles = want.len();
                println!("requesting {waiting_tiles} tiles ({across}x{down} at 1:1 + 16 at 4x)");
                svc.send(ToWorker::Want { tiles: want });
            }
            FromWorker::Preview { page, scale, image, .. } => {
                println!(
                    "preview page {page} at {scale:.3} px/pt -> {}x{} in {} ms total",
                    image.width(),
                    image.height(),
                    started.elapsed().as_millis()
                );
                dump("preview", image);
            }
            FromWorker::Tile { key, image, millis } => {
                tile_ms.push(millis);
                if key.bucket == 2 && key.tx == 8 && key.ty == 8 {
                    dump("tile-4x", image);
                } else if key.bucket == 0 && key.tx == 2 && key.ty == 2 {
                    dump("tile-1x", image);
                }
                waiting_tiles -= 1;
                if waiting_tiles == 0 {
                    tile_ms.sort_unstable();
                    let n = tile_ms.len();
                    println!(
                        "{n} tiles in {} ms wall; per tile median {} ms, p90 {} ms, worst {} ms",
                        started.elapsed().as_millis(),
                        tile_ms[n / 2],
                        tile_ms[n * 9 / 10],
                        tile_ms[n - 1]
                    );
                    label_started = Instant::now();
                    svc.send(ToWorker::ScanLabels(1));
                }
            }
            FromWorker::Labels { first, labels, .. } => {
                for (i, l) in labels.iter().enumerate() {
                    if first + i as u32 <= 3 || (first + i as u32) % 100 == 0 {
                        println!(
                            "  sheet {:>3}: [{}] {}",
                            first as usize + i,
                            l.number,
                            l.title
                        );
                    }
                }
                labels_done = first + labels.len() as u32;
                if labels_done as usize >= pages.len() {
                    println!(
                        "labelled {labels_done} pages in {} ms ({:.1} ms/page)",
                        label_started.elapsed().as_millis(),
                        label_started.elapsed().as_millis() as f64 / labels_done as f64
                    );
                    return;
                }
            }
            _ => {}
        }
    }
}

fn dump(name: &str, image: egui::ColorImage) {
    let (w, h) = (image.width(), image.height());
    let mut bytes = Vec::with_capacity(w * h * 4);
    for px in image.as_raw() {
        bytes.push(*px);
    }
    std::fs::write(format!("/tmp/{name}.{w}x{h}.rgba"), &bytes).unwrap();
}
