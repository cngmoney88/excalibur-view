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
