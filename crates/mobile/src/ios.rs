//! Excalibur View on the iPad.
//!
//! The app is a few lines of Swift (`mobile/ios/Sources`) around this
//! library. Swift's `main` calls [`excalibur_view_main`], which never returns:
//! winit hands the thread to UIKit, and everything after that is the same
//! program the desktop runs, `hyperview::app::App`, drawn with Metal.
//!
//! What only UIKit can do - the document picker, the share sheet, printing,
//! opening a link, reading words off a picture with Vision, and taking a
//! drawing opened with the app from another one - is done by the Swift side,
//! through the `exv_ios_*` functions it exports, and it reports back through
//! the two `exv_*` functions exported here.

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};

use hyperview::files::Filter;
use hyperview::ocr::Word;

extern "C" {
    fn exv_ios_import(asked: u64, types: *const c_char, many: bool, into: *const c_char);
    fn exv_ios_share(path: *const c_char);
    fn exv_ios_print(path: *const c_char, name: *const c_char);
    fn exv_ios_open_url(url: *const c_char);
    /// A word a line, as `hyperview::platform::words_from_lines` reads them,
    /// in memory the caller frees with `free`. Null when nothing could be read.
    fn exv_ios_read_words(grey: *const u8, width: i32, height: i32) -> *mut c_char;
    fn free(pointer: *mut c_void);
}

/// Where Swift's `main` hands over. Never returns.
#[no_mangle]
pub extern "C" fn excalibur_view_main() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,hyperview=info,hub=info,excalibur_view_mobile=info"),
    )
    .try_init();
    report_panics();
    log::info!("Excalibur View {} started", env!("CARGO_PKG_VERSION"));

    // The app's Documents folder is the one the Files app shows as
    // Excalibur View's, which is where drawings belong on an iPad.
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let documents = home.join("Documents");
        let _ = std::fs::create_dir_all(&documents);
        std::env::set_var("EXV_DRAWINGS", &documents);
    }
    hyperview::platform::install(Box::new(Ipad));
    let _ = hyperview::instance::start(&[], false);

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: graphics(),
        ..Default::default()
    };
    let started = eframe::run_native(
        "Excalibur View",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(Tablet { program: hyperview::app::App::new(cc, Vec::new()) }))
        }),
    );
    if let Err(e) = started {
        log::error!("the window could not start: {e}");
    }
}

fn report_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("Excalibur View {} stopped unexpectedly: {info}", env!("CARGO_PKG_VERSION"));
        default(info);
    }));
}

/// Metal, asking for what an iPad has rather than what a desktop card has.
fn graphics() -> eframe::egui_wgpu::WgpuConfiguration {
    use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew};
    let mut setup = WgpuSetupCreateNew::default();
    setup.instance_descriptor.backends = wgpu::Backends::METAL;
    setup.device_descriptor = std::sync::Arc::new(|adapter| wgpu::DeviceDescriptor {
        label: Some("Excalibur View"),
        required_features: wgpu::Features::default(),
        required_limits: wgpu::Limits {
            max_texture_dimension_2d: adapter.limits().max_texture_dimension_2d.min(16384),
            ..wgpu::Limits::downlevel_defaults()
        },
        // An iPad shares its memory with everything else on it, and the
        // system closes an app that takes too much.
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        trace: wgpu::Trace::Off,
    });
    WgpuConfiguration {
        wgpu_setup: WgpuSetup::CreateNew(setup),
        present_mode: wgpu::PresentMode::AutoVsync,
        ..Default::default()
    }
}

/// The program, kept clear of the strip along the bottom of the screen
/// where an iPad's home bar sits. egui 0.32 is not told about that strip, so
/// the app leaves it empty itself; the status bar at the top is hidden in
/// Info.plist.
struct Tablet {
    program: hyperview::app::App,
}

impl eframe::App for Tablet {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let fill = ctx.style().visuals.panel_fill;
        egui::TopBottomPanel::bottom("ipad-home-bar")
            .exact_height(14.0)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(fill))
            .show(ctx, |_| {});
        self.program.update(ctx, frame);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.program.raw_input_hook(ctx, raw);
    }

    fn on_exit(&mut self) {
        self.program.on_exit();
    }
}

// ---- asking UIKit ---------------------------------------------------------------

struct Ipad;

fn text(value: &str) -> CString {
    CString::new(value.replace('\0', "")).unwrap_or_default()
}

fn path_text(path: &Path) -> CString {
    text(&path.display().to_string())
}

impl hyperview::platform::Bridge for Ipad {
    fn import(&self, asked: u64, filters: &[Filter], many: bool, into: &Path) {
        let types = text(&hyperview::files::mime_types(filters).join(","));
        let into = path_text(into);
        unsafe { exv_ios_import(asked, types.as_ptr(), many, into.as_ptr()) };
    }

    fn share(&self, path: &Path) {
        let path = path_text(path);
        unsafe { exv_ios_share(path.as_ptr()) };
    }

    fn print(&self, pdf: &Path, name: &str) {
        let (pdf, name) = (path_text(pdf), text(name));
        unsafe { exv_ios_print(pdf.as_ptr(), name.as_ptr()) };
    }

    fn open_url(&self, url: &str) {
        let url = text(url);
        unsafe { exv_ios_open_url(url.as_ptr()) };
    }

    fn reads_words(&self) -> bool {
        true
    }

    fn read_words(&self, grey: &[u8], width: u32, height: u32) -> Option<Result<Vec<Word>, String>> {
        if grey.len() < (width as usize) * (height as usize) {
            return Some(Err("the picture is smaller than it says".into()));
        }
        let read = unsafe { exv_ios_read_words(grey.as_ptr(), width as i32, height as i32) };
        if read.is_null() {
            return Some(Err("the text recogniser could not read this sheet".into()));
        }
        let lines = unsafe { CStr::from_ptr(read) }.to_string_lossy().to_string();
        unsafe { free(read as *mut c_void) };
        Some(Ok(hyperview::platform::words_from_lines(&lines)))
    }
}

/// Reads a C array of C strings from Swift.
///
/// # Safety
/// `paths` points at `count` valid, NUL-terminated strings, or is null.
unsafe fn paths_from(paths: *const *const c_char, count: usize) -> Vec<PathBuf> {
    if paths.is_null() {
        return Vec::new();
    }
    (0..count)
        .filter_map(|i| {
            let one = *paths.add(i);
            (!one.is_null()).then(|| PathBuf::from(CStr::from_ptr(one).to_string_lossy().to_string()))
        })
        .collect()
}

/// The document picker's answer: the files, already copied into the folder
/// asked for. Called with none when the picker was put away.
///
/// # Safety
/// As [`paths_from`].
#[no_mangle]
pub unsafe extern "C" fn exv_imported(asked: u64, paths: *const *const c_char, count: usize) {
    let files = paths_from(paths, count);
    log::info!("{} file(s) brought in", files.len());
    hyperview::platform::imported(asked, files);
}

/// Drawings opened with the app from another one, already copied in.
///
/// # Safety
/// As [`paths_from`].
#[no_mangle]
pub unsafe extern "C" fn exv_opened(paths: *const *const c_char, count: usize) {
    let files = paths_from(paths, count);
    log::info!("{} drawing(s) opened with the app", files.len());
    hyperview::platform::opened_elsewhere(files);
}
