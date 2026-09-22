//! One Hyperview window per person, with every drawing in it as a tab.
//!
//! Hyperview is the default PDF program on a machine, so every drawing
//! double-clicked in Explorer or opened from an email starts it again. Left to
//! itself that is a second window, then a third, each hiding the last — which
//! looks exactly like the first drawing having been closed. Revu does not do
//! that and neither does this: the second start hands its drawings to the
//! window already open, which opens them as tabs and comes to the front, and
//! the second start goes away.
//!
//! The hand-over is a lock file and an inbox folder in the person's own app
//! data. Per person, not per machine, so two people on the same Remote Desktop
//! server never find their drawings opening in each other's windows. A copy
//! that crashed leaves nothing behind: the operating system lets go of its
//! lock with it.
//!
//! `--new-window` opens a separate window anyway, for somebody who wants one
//! on each screen.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};

/// The lock, held for as long as this window is open.
static HELD: OnceLock<File> = OnceLock::new();

/// What starting up came to.
#[derive(Debug, PartialEq)]
pub enum Start {
    /// This is the window. Open the drawings here.
    First,
    /// Another window is open and has been handed the drawings. Exit.
    HandedOver,
}

pub(crate) fn folder() -> Option<PathBuf> {
    if let Some(home) = crate::install::home() {
        return Some(home);
    }
    directories::ProjectDirs::from("com", "Excalibur", "Hyperview").map(|d| d.data_local_dir().to_path_buf())
}

fn inbox(root: &Path) -> PathBuf {
    root.join("inbox")
}

/// Becomes the window, or hands `files` to the one already open.
///
/// Anything unexpected — no app data folder, a disk that will not take a lock
/// — means this simply opens as its own window. Two windows is a nuisance;
/// no window is a program that does not work.
pub fn start(files: &[PathBuf]) -> Start {
    match folder() {
        Some(root) => start_in(&root, files),
        None => Start::First,
    }
}

pub fn start_in(root: &Path, files: &[PathBuf]) -> Start {
    if std::fs::create_dir_all(inbox(root)).is_err() {
        return Start::First;
    }
    let Ok(lock) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("window.lock"))
    else {
        return Start::First;
    };
    match lock.try_lock() {
        Ok(()) => {
            let _ = HELD.set(lock);
            Start::First
        }
        Err(std::fs::TryLockError::WouldBlock) => {
            if hand_over(root, files).is_ok() {
                allow_the_other_window_to_the_front();
                Start::HandedOver
            } else {
                Start::First
            }
        }
        Err(_) => Start::First,
    }
}

/// Becomes the window once the one open now has closed — what a copy started
/// by "Restart now" does, since the window that started it is still closing.
/// Gives up after `patience` and behaves like any other start.
pub fn start_when_free(files: &[PathBuf], patience: Duration) -> Start {
    let Some(root) = folder() else {
        return Start::First;
    };
    let until = std::time::Instant::now() + patience;
    let _ = std::fs::create_dir_all(inbox(&root));
    while std::time::Instant::now() < until {
        let Ok(lock) = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(root.join("window.lock"))
        else {
            break;
        };
        match lock.try_lock() {
            Ok(()) => {
                let _ = HELD.set(lock);
                return Start::First;
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => break,
        }
    }
    start_in(&root, files)
}

/// Leaves the drawings in the inbox for the window that is open. An empty
/// request just brings it to the front.
fn hand_over(root: &Path, files: &[PathBuf]) -> std::io::Result<()> {
    let here = std::env::current_dir().unwrap_or_default();
    let lines: Vec<String> = files
        .iter()
        // A hyperview:// link is not a file and must not be made into a path
        // under wherever this was started from.
        .map(|f| {
            let link = f.to_str().is_some_and(crate::deskui::is_link);
            if f.is_absolute() || link { f.clone() } else { here.join(f) }
        })
        .map(|f| f.display().to_string())
        .collect();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = format!("{stamp}-{}", std::process::id());
    let part = inbox(root).join(format!("{name}.part"));
    std::fs::write(&part, lines.join("\n"))?;
    // Renamed into place whole, so the window never reads half a list.
    std::fs::rename(&part, inbox(root).join(format!("{name}.open")))
}

/// Windows only lets the program somebody just clicked put a window in front
/// of them. This start was the one they clicked; it passes that on.
fn allow_the_other_window_to_the_front() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY};
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
}

/// Whether this process is the one window the others hand drawings to.
pub fn is_the_window() -> bool {
    HELD.get().is_some()
}

/// What the open window hears: drawings to open, possibly none (come to the
/// front). Only the window holding the lock listens; a `--new-window` copy
/// does not, or the two would race for the same drawings.
pub fn listen(ctx: egui::Context) -> Receiver<Vec<PathBuf>> {
    let (tx, rx) = crossbeam_channel::unbounded();
    if !is_the_window() {
        return rx;
    }
    if let Some(root) = folder() {
        let _ = std::thread::Builder::new()
            .name("hyperview-inbox".into())
            .spawn(move || watch(&root, &tx, &ctx));
    }
    rx
}

fn watch(root: &Path, tx: &Sender<Vec<PathBuf>>, ctx: &egui::Context) {
    loop {
        for request in take(root) {
            if tx.send(request).is_err() {
                return;
            }
            ctx.request_repaint();
        }
        // Four times a second is instant to a person and nothing to a disk.
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Everything waiting in the inbox, oldest first, taken out of it.
pub fn take(root: &Path) -> Vec<Vec<PathBuf>> {
    let Ok(entries) = std::fs::read_dir(inbox(root)) else {
        return Vec::new();
    };
    let mut waiting: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "open"))
        .collect();
    waiting.sort();
    waiting
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let _ = std::fs::remove_file(&path);
            Some(
                text.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(PathBuf::from)
                    .collect(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let n: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        let dir = std::env::temp_dir().join(format!("hv-instance-{n}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_second_start_hands_its_drawings_to_the_window_already_open() {
        let root = scratch();
        // The window that is open holds the lock (here, this test does).
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(root.join("window.lock"))
            .unwrap();
        std::fs::create_dir_all(inbox(&root)).unwrap();
        lock.try_lock().unwrap();

        let started = start_in(
            &root,
            &[PathBuf::from("/jobs/Fox Theater/S-101.pdf"), PathBuf::from("/jobs/Quote.pdf")],
        );
        assert_eq!(started, Start::HandedOver);
        let heard = take(&root);
        assert_eq!(
            heard,
            vec![vec![
                PathBuf::from("/jobs/Fox Theater/S-101.pdf"),
                PathBuf::from("/jobs/Quote.pdf")
            ]]
        );
        // Taken once, not twice.
        assert!(take(&root).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn with_nothing_open_it_is_the_window() {
        let root = scratch();
        // Taking the lock inside start_in would hold it for the rest of the
        // test process, so this checks the pieces rather than the static.
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(root.join("window.lock"))
            .unwrap();
        assert!(lock.try_lock().is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_start_with_no_drawings_just_brings_the_window_forward() {
        let root = scratch();
        std::fs::create_dir_all(inbox(&root)).unwrap();
        hand_over(&root, &[]).unwrap();
        assert_eq!(take(&root), vec![Vec::<PathBuf>::new()]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
