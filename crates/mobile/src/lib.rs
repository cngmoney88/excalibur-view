//! Excalibur View on Android.
//!
//! Android does not start a program at `main`. It starts an activity, and the
//! activity (GameActivity, see `mobile/android`) loads this library and calls
//! `android_main` on a thread of its own. Everything after that is the same
//! program the desktop runs: `hyperview::app::App`, drawn by eframe.
//!
//! What is different here is the ground it stands on:
//!
//! - There is no terminal. Logging goes to logcat under the tag `ExcaliburView`,
//!   and so does a panic, which would otherwise vanish without a word.
//! - There is no home folder. An app owns one private folder and nothing else,
//!   so `HOME` and the XDG folders are pointed into it before anything asks.
//! - There is no program file. `current_exe()` is the system's `app_process`,
//!   so nothing that looks "beside the program" finds anything. The PDF engine
//!   is found by name among the app's own native libraries instead, and
//!   `MainActivity` sets `PDFIUM_DIR` to that folder so the tile helpers can
//!   take their own copies of it, as they do on the desktop.
//! - There is one attempt at a window. winit will not make a second event loop
//!   in the same process, so the desktop's "try the next renderer" loop cannot
//!   work here. The choice between Vulkan and OpenGL ES is made when the
//!   graphics adapter is picked instead, before anything can fail.
//!
//! On every other platform this crate is empty.

#[cfg(target_os = "android")]
mod android;
