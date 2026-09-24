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
//!
//! `--combine` is Explorer's "Combine in Excalibur View". Explorer starts the
//! program once for every PDF selected, all at the same moment, so each start
//! hands its one file over marked for combining, the window gathers them for
//! a moment, and they arrive in the Combine window together rather than as a
//! pile of tabs.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};

/// The lock, held for as long as this window is open.
static HELD: OnceLock<File> = OnceLock::new();

/// The first line of a hand-over that is for combining, not opening.
const COMBINE: &str = "--combine";

/// What a start handed to the window that is open.
#[derive(Debug, Default, PartialEq)]
pub struct Handed {
    /// Drawings, or `hyperview://` links.
    pub files: Vec<PathBuf>,
    /// Put these in one PDF rather than opening them.
    pub combine: bool,
}

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
pub fn start(files: &[PathBuf], combine: bool) -> Start {
    match folder() {
        Some(root) => start_in(&root, files, combine),
        None => Start::First,
    }
}

pub fn start_in(root: &Path, files: &[PathBuf], combine: bool) -> Start {
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
            if hand_over(root, files, combine).is_ok() {
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
    start_in(&root, files, false)
}

/// Leaves files for this window to combine, the same way every other start
/// Explorer made at the same moment leaves its own. `false` when there is no
/// inbox to leave them in, and they should simply be opened.
pub fn combine_here(files: &[PathBuf]) -> bool {
    is_the_window() && folder().is_some_and(|root| hand_over(&root, files, true).is_ok())
}

/// Leaves the drawings in the inbox for the window that is open. An empty
/// request just brings it to the front.
fn hand_over(root: &Path, files: &[PathBuf], combine: bool) -> std::io::Result<()> {
    let here = std::env::current_dir().unwrap_or_default();
    let mut lines: Vec<String> = combine.then(|| COMBINE.to_string()).into_iter().collect();
    lines.extend(files
        .iter()
        // A hyperview:// link is not a file and must not be made into a path
        // under wherever this was started from.
        .map(|f| {
            let link = f.to_str().is_some_and(crate::deskui::is_link);
            if f.is_absolute() || link { f.clone() } else { here.join(f) }
        })
        .map(|f| f.display().to_string()));
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
pub fn listen(ctx: egui::Context) -> Receiver<Handed> {
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

fn watch(root: &Path, tx: &Sender<Handed>, ctx: &egui::Context) {
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
pub fn take(root: &Path) -> Vec<Handed> {
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
            let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty()).peekable();
            let combine = lines.next_if_eq(&COMBINE).is_some();
            Some(Handed {
                files: lines.map(PathBuf::from).collect(),
                combine,
            })
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

    /// A path that is absolute on the machine this test is running on.
    ///
    /// `/jobs/S-101.pdf` looks absolute and on Windows is not: a path with no
    /// drive letter is relative to the current drive, `is_absolute` says so,
    /// and `hand_over` correctly joins it onto the working directory -- which
    /// is the right behaviour and made the test wrong. Second time today a
    /// test has claimed a bug that only existed in the test, so it is written
    /// down here rather than fixed quietly.
    fn absolute(rest: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!("C:\\jobs\\{rest}"))
        } else {
            PathBuf::from(format!("/jobs/{rest}"))
        }
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

        let sheet = absolute("Fox Theater/S-101.pdf");
        let quote = absolute("Quote.pdf");
        let started = start_in(&root, &[sheet.clone(), quote.clone()], false);
        assert_eq!(started, Start::HandedOver);
        let heard = take(&root);
        assert_eq!(heard, vec![Handed { files: vec![sheet, quote], combine: false }]);
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
        hand_over(&root, &[], false).unwrap();
        assert_eq!(take(&root), vec![Handed::default()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn files_explorer_hands_over_for_combining_arrive_marked_for_combining() {
        let root = scratch();
        std::fs::create_dir_all(inbox(&root)).unwrap();
        let first = absolute("S-101.pdf");
        let second = absolute("S-102.pdf");
        hand_over(&root, &[first.clone()], true).unwrap();
        hand_over(&root, &[second.clone()], true).unwrap();
        hand_over(&root, &[absolute("Quote.pdf")], false).unwrap();
        let heard = take(&root);
        assert_eq!(heard.len(), 3);
        assert_eq!(heard[0], Handed { files: vec![first], combine: true });
        assert_eq!(heard[1], Handed { files: vec![second], combine: true });
        assert!(!heard[2].combine, "an ordinary open is not swept up with them");
        let _ = std::fs::remove_dir_all(&root);
    }
}
