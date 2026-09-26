//! What a tablet's own system does for the program.
//!
//! On a desktop the program opens its own dialogs, runs the printer and hands
//! a web address to the browser itself. On an iPad or an Android tablet those
//! belong to the system, which is reached from the app around the program —
//! Swift on one, Java on the other — and not from here. That app puts a
//! [`Bridge`] in place when it starts, and the program asks through it:
//!
//! - bring files in from elsewhere on the device, copied into the app's own
//!   folder so the program can open them like any other file;
//! - send a copy of a file somewhere else (the share sheet);
//! - print a PDF with the system's printing;
//! - open a web page.
//!
//! And the app tells the program when a drawing has been opened *with*
//! Excalibur View from another app: it copies it into the app's folder and
//! hands it over the same way a second copy of the desktop program hands a
//! double-clicked drawing to the window already open.
//!
//! None of this can wait for an answer. What the system decides arrives later,
//! on whatever thread the system chose, and is picked up on the next frame.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::files::Filter;

/// The system's side, as the app around the program provides it.
pub trait Bridge: Send + Sync {
    /// Shows the system's own picker; the files chosen are to be copied into
    /// `into` and reported with [`imported`] under `asked`, which must be
    /// called even when nothing was chosen.
    fn import(&self, asked: u64, filters: &[Filter], many: bool, into: &Path);
    /// Offers a copy of the file to the rest of the device.
    fn share(&self, path: &Path);
    /// Prints a PDF, named `name` in the print queue.
    fn print(&self, pdf: &Path, name: &str);
    /// Opens a web page in the browser.
    fn open_url(&self, url: &str);
    /// Reads the words off a greyscale picture (one byte a pixel, rows from
    /// the top) with the system's own text recogniser. Called off the
    /// window's thread and allowed to take its time. `None` when this system
    /// has no recogniser to offer.
    fn read_words(&self, _grey: &[u8], _width: u32, _height: u32) -> Option<Result<Vec<crate::ocr::Word>, String>> {
        None
    }
    /// Whether [`Bridge::read_words`] will answer.
    fn reads_words(&self) -> bool {
        false
    }
}

static BRIDGE: OnceLock<Box<dyn Bridge>> = OnceLock::new();
static ASKED: AtomicU64 = AtomicU64::new(1);
static ARRIVED: Mutex<Vec<(u64, Vec<PathBuf>)>> = Mutex::new(Vec::new());
static WINDOW: OnceLock<egui::Context> = OnceLock::new();

/// Puts the system's side in place. Once, before the window opens.
pub fn install(bridge: Box<dyn Bridge>) {
    let _ = BRIDGE.set(bridge);
}

/// Lets anything that arrives wake the window to deal with it.
pub fn wake_with(ctx: &egui::Context) {
    let _ = WINDOW.set(ctx.clone());
}

fn wake() {
    if let Some(ctx) = WINDOW.get() {
        ctx.request_repaint();
    }
}

/// Whether this is the tablet program: built for one, or a desktop asked to
/// behave like one with `HYPERVIEW_TABLET=1`, which is how the tablet's list of
/// files and its larger controls are seen and tested without a tablet.
pub fn tablet() -> bool {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        return true;
    }
    static ASKED_FOR: OnceLock<bool> = OnceLock::new();
    *ASKED_FOR.get_or_init(|| {
        std::env::var("HYPERVIEW_TABLET").map(|v| !v.is_empty() && v != "0").unwrap_or(false)
    })
}

/// Whether files can be brought in from elsewhere on this device.
pub fn can_import() -> bool {
    BRIDGE.get().is_some() || (tablet() && cfg!(not(any(target_os = "android", target_os = "ios"))))
}

/// Asks for files to be brought into `into`. What arrives is collected with
/// [`take_imported`] and the number returned here.
pub fn import(filters: &[Filter], many: bool, into: &Path) -> u64 {
    let asked = ASKED.fetch_add(1, Ordering::Relaxed);
    match BRIDGE.get() {
        Some(bridge) => bridge.import(asked, filters, many, into),
        None => imported(asked, desktop_import(filters, many, into)),
    }
    asked
}

/// The system's answer to [`import`]: the files, now in the folder asked for.
pub fn imported(asked: u64, files: Vec<PathBuf>) {
    ARRIVED.lock().unwrap_or_else(|e| e.into_inner()).push((asked, files));
    wake();
}

/// What has arrived for one request: nothing yet, or the one answer.
pub fn take_imported(asked: u64) -> Vec<Vec<PathBuf>> {
    let mut arrived = ARRIVED.lock().unwrap_or_else(|e| e.into_inner());
    let (mine, rest): (Vec<_>, Vec<_>) = arrived.drain(..).partition(|(n, _)| *n == asked);
    *arrived = rest;
    mine.into_iter().map(|(_, files)| files).collect()
}

/// A desktop pretending to be a tablet brings files in with its own dialog.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn desktop_import(filters: &[Filter], many: bool, into: &Path) -> Vec<PathBuf> {
    let mut dialog = rfd::FileDialog::new();
    for filter in filters {
        dialog = dialog.add_filter(&filter.name, &filter.extensions);
    }
    let picked = if many { dialog.pick_files().unwrap_or_default() } else { dialog.pick_file().into_iter().collect() };
    copy_in(&picked, into)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn desktop_import(_: &[Filter], _: bool, _: &Path) -> Vec<PathBuf> {
    Vec::new()
}

/// Copies files into a folder under names that aren't taken there, and says
/// where each one went.
pub fn copy_in(files: &[PathBuf], into: &Path) -> Vec<PathBuf> {
    let _ = std::fs::create_dir_all(into);
    files
        .iter()
        .filter_map(|from| {
            let name = from.file_name()?.to_string_lossy().to_string();
            let to = into.join(crate::files::free_name(into, &name));
            std::fs::copy(from, &to).ok()?;
            Some(to)
        })
        .collect()
}

/// Whether a file can be sent somewhere else from here.
pub fn can_share() -> bool {
    BRIDGE.get().is_some()
}

pub fn share(path: &Path) {
    if let Some(bridge) = BRIDGE.get() {
        bridge.share(path);
    }
}

/// Prints with the system's printing, when there is a system to ask. False
/// when there isn't and the program has to print for itself.
pub fn print(pdf: &Path, name: &str) -> bool {
    match BRIDGE.get() {
        Some(bridge) => {
            bridge.print(pdf, name);
            true
        }
        None => false,
    }
}

/// Opens a web page through the system. False when the program has to do it
/// itself.
pub fn open_url(url: &str) -> bool {
    match BRIDGE.get() {
        Some(bridge) => {
            bridge.open_url(url);
            true
        }
        None => false,
    }
}

/// The system's text recogniser, as an OCR engine, when it has one.
pub fn recogniser() -> Option<Box<dyn crate::ocr::Engine>> {
    BRIDGE.get().filter(|b| b.reads_words()).map(|_| Box::new(SystemReader) as Box<dyn crate::ocr::Engine>)
}

struct SystemReader;

impl crate::ocr::Engine for SystemReader {
    fn read(&self, grey: &[u8], width: u32, height: u32) -> Result<Vec<crate::ocr::Word>, String> {
        let Some(bridge) = BRIDGE.get() else {
            return Err("this device has no text recogniser".into());
        };
        read_in_tiles(grey, width, height, TILE, OVERLAP, |tile, w, h| {
            bridge.read_words(tile, w, h).unwrap_or_else(|| Err("this device has no text recogniser".into()))
        })
    }

    fn name(&self) -> String {
        if cfg!(target_os = "ios") {
            "the text recogniser built into iPadOS".into()
        } else {
            "the text recogniser built into this device".into()
        }
    }
}

/// A tablet's recogniser reads a picture a few thousand pixels across; a
/// whole sheet rendered for reading is several times that, and shrunk to fit
/// its small print would be lost. So it is read in pieces this size, each
/// overlapping the next by enough to hold a word cut in two.
const TILE: u32 = 2048;
const OVERLAP: u32 = 160;

/// Reads a large picture in overlapping pieces. A word belongs to the piece
/// its middle falls in the core of, so a word read twice in an overlap is
/// kept once, and one cut in half at a piece's edge is kept from the piece
/// that saw it whole.
pub fn read_in_tiles(
    grey: &[u8],
    width: u32,
    height: u32,
    tile: u32,
    overlap: u32,
    mut read: impl FnMut(&[u8], u32, u32) -> Result<Vec<crate::ocr::Word>, String>,
) -> Result<Vec<crate::ocr::Word>, String> {
    if width <= tile && height <= tile {
        return read(grey, width, height);
    }
    let step = tile - overlap;
    let starts = |size: u32| -> Vec<u32> {
        let mut at: Vec<u32> = (0..).map(|n| n * step).take_while(|s| *s + overlap < size.max(1)).collect();
        if at.is_empty() {
            at.push(0);
        }
        at
    };
    let mut words = Vec::new();
    for &y0 in &starts(height) {
        for &x0 in &starts(width) {
            let (w, h) = (tile.min(width - x0), tile.min(height - y0));
            let mut piece = Vec::with_capacity((w * h) as usize);
            for row in y0..y0 + h {
                let from = (row * width + x0) as usize;
                piece.extend_from_slice(&grey[from..from + w as usize]);
            }
            // The core: everything but the half of each overlap that the
            // neighbouring piece owns.
            let half = (overlap / 2) as f32;
            let left = if x0 == 0 { f32::MIN } else { x0 as f32 + half };
            let top = if y0 == 0 { f32::MIN } else { y0 as f32 + half };
            let right = if x0 + w >= width { f32::MAX } else { (x0 + w) as f32 - half };
            let bottom = if y0 + h >= height { f32::MAX } else { (y0 + h) as f32 - half };
            for mut word in read(&piece, w, h)? {
                word.area = [
                    word.area[0] + x0 as f32,
                    word.area[1] + y0 as f32,
                    word.area[2] + x0 as f32,
                    word.area[3] + y0 as f32,
                ];
                let (cx, cy) = ((word.area[0] + word.area[2]) / 2.0, (word.area[1] + word.area[3]) / 2.0);
                if cx >= left && cx < right && cy >= top && cy < bottom {
                    words.push(word);
                }
            }
        }
    }
    Ok(words)
}

/// Words as a tablet's recogniser reports them across the bridge: one a
/// line, left, top, right and bottom in pixels, a confidence from nought to
/// one, and the word, separated by tabs. Lines that don't read are dropped.
pub fn words_from_lines(text: &str) -> Vec<crate::ocr::Word> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(6, '\t');
            let mut number = || parts.next()?.trim().parse::<f32>().ok();
            let area = [number()?, number()?, number()?, number()?];
            let confidence = number()?;
            let word = parts.next()?.trim().to_string();
            (!word.is_empty()).then_some(crate::ocr::Word { text: word, area, confidence })
        })
        .collect()
}

/// What turns the text in a soft keyboard's box from `before` into `after`,
/// as the keys and text egui understands: a backspace for every character
/// taken off the end of what the two share, then what was added, with a new
/// line as Enter. A keyboard that corrects a word takes the old one back and
/// types the new one, and that comes out as exactly that. Zero-width spaces,
/// which the app keeps in the box so a backspace always has something to
/// take, never reach a drawing.
pub fn keyboard_events(before: &str, after: &str) -> Vec<egui::Event> {
    let old: Vec<char> = before.chars().collect();
    let new: Vec<char> = after.chars().collect();
    let shared = old.iter().zip(new.iter()).take_while(|(a, b)| a == b).count();
    let key = |key: egui::Key, pressed: bool| egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let mut events = Vec::new();
    for _ in shared..old.len() {
        events.push(key(egui::Key::Backspace, true));
        events.push(key(egui::Key::Backspace, false));
    }
    let added: String = new[shared..].iter().filter(|c| **c != '\u{200B}').collect();
    for piece in added.split_inclusive('\n') {
        let text = piece.trim_end_matches('\n');
        if !text.is_empty() {
            events.push(egui::Event::Text(text.to_string()));
        }
        if piece.ends_with('\n') {
            events.push(key(egui::Key::Enter, true));
            events.push(key(egui::Key::Enter, false));
        }
    }
    events
}

/// A drawing opened *with* Excalibur View from another app, already copied
/// into the app's folder. It goes to the window the way a double-clicked
/// drawing does on a desktop.
pub fn opened_elsewhere(files: Vec<PathBuf>) {
    crate::instance::hand_to_window(&files);
    wake();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_arrives_for_one_request_is_kept_apart_from_what_arrives_for_another() {
        imported(900_001, vec![PathBuf::from("a.pdf")]);
        imported(900_002, vec![PathBuf::from("b.pdf"), PathBuf::from("c.pdf")]);
        assert!(take_imported(900_003).is_empty());
        assert_eq!(take_imported(900_002), vec![vec![PathBuf::from("b.pdf"), PathBuf::from("c.pdf")]]);
        assert_eq!(take_imported(900_001), vec![vec![PathBuf::from("a.pdf")]]);
        assert!(take_imported(900_001).is_empty());
    }

    #[test]
    fn a_recognisers_words_come_across_with_their_boxes_and_bad_lines_are_dropped() {
        let words = words_from_lines("10\t20\t110\t40\t0.9\tW12x26\nnonsense\n5\t5\t9\t9\t0.5\t\n1\t2\t3\t4\t0.8\tTYP\tmore\n");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "W12x26");
        assert_eq!(words[0].area, [10.0, 20.0, 110.0, 40.0]);
        assert!((words[0].confidence - 0.9).abs() < 1e-6);
        assert_eq!(words[1].text, "TYP\tmore");
    }

    fn keys(events: &[egui::Event]) -> String {
        events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Text(t) => Some(t.clone()),
                egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => Some("<".into()),
                egui::Event::Key { key: egui::Key::Enter, pressed: true, .. } => Some("⏎".into()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn what_a_soft_keyboard_types_reaches_the_program_as_keys() {
        let seed = "\u{200B}";
        assert_eq!(keys(&keyboard_events(seed, &format!("{seed}B"))), "B");
        assert_eq!(keys(&keyboard_events(&format!("{seed}Bea"), &format!("{seed}Beam "))), "m ");
        // A correction: "teh" becomes "the".
        assert_eq!(keys(&keyboard_events(&format!("{seed}teh"), &format!("{seed}the"))), "<<he");
        // Backspace past everything typed takes the seed, and still counts.
        assert_eq!(keys(&keyboard_events(seed, "")), "<");
        assert_eq!(keys(&keyboard_events(seed, &format!("{seed}ok\nnext"))), "ok⏎next");
    }

    #[test]
    fn a_big_picture_is_read_in_pieces_and_every_word_is_kept_once_where_it_really_is() {
        // A 500 × 300 picture with a "word" every 40 pixels, read 128 at a
        // time with 32 of overlap. The reader finds a word wherever a whole
        // 20 × 10 box of it is in its piece.
        let (width, height) = (500u32, 300u32);
        let truth: Vec<[f32; 4]> = (0..12)
            .flat_map(|i| (0..7).map(move |j| [5.0 + i as f32 * 40.0, 5.0 + j as f32 * 40.0]))
            .map(|[x, y]| [x, y, x + 20.0, y + 10.0])
            .collect();
        let grey = vec![255u8; (width * height) as usize];
        let mut pieces = 0;
        let mut origin = (0u32, 0u32);
        let found = read_in_tiles(&grey, width, height, 128, 32, |_, w, h| {
            pieces += 1;
            // Work out where this piece is from how many came before it.
            let per_row = (0..).map(|n| n * 96).take_while(|s: &u32| s + 32 < width).count() as u32;
            let n = pieces - 1;
            origin = ((n % per_row) * 96, (n / per_row) * 96);
            Ok(truth
                .iter()
                .filter(|b| {
                    b[0] >= origin.0 as f32
                        && b[1] >= origin.1 as f32
                        && b[2] <= (origin.0 + w) as f32
                        && b[3] <= (origin.1 + h) as f32
                })
                .map(|b| crate::ocr::Word {
                    text: format!("{},{}", b[0], b[1]),
                    area: [b[0] - origin.0 as f32, b[1] - origin.1 as f32, b[2] - origin.0 as f32, b[3] - origin.1 as f32],
                    confidence: 1.0,
                })
                .collect())
        })
        .unwrap();
        assert!(pieces > 4);
        let mut areas: Vec<[f32; 4]> = found.iter().map(|w| w.area).collect();
        areas.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut expected = truth.clone();
        expected.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(areas, expected, "every word once, in the whole picture's pixels");
    }

    #[test]
    fn a_small_picture_is_read_whole() {
        let mut calls = 0;
        read_in_tiles(&[0u8; 100], 10, 10, 2048, 160, |_, w, h| {
            calls += 1;
            assert_eq!((w, h), (10, 10));
            Ok(Vec::new())
        })
        .unwrap();
        assert_eq!(calls, 1);
    }

    #[test]
    fn files_brought_in_never_overwrite_what_is_already_there() {
        let base = std::env::temp_dir().join(format!("exv-platform-{}", std::process::id()));
        let (from, into) = (base.join("from"), base.join("into"));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&from).unwrap();
        std::fs::create_dir_all(&into).unwrap();
        std::fs::write(from.join("S-101.pdf"), b"new").unwrap();
        std::fs::write(into.join("S-101.pdf"), b"old").unwrap();
        let landed = copy_in(&[from.join("S-101.pdf")], &into);
        assert_eq!(landed, vec![into.join("S-101 2.pdf")]);
        assert_eq!(std::fs::read(into.join("S-101.pdf")).unwrap(), b"old");
        assert_eq!(std::fs::read(into.join("S-101 2.pdf")).unwrap(), b"new");
    }
}
