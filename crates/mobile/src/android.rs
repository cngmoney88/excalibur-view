use std::path::{Path, PathBuf};
use std::sync::Arc;

use android_activity::AndroidApp;
use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew};

/// What logcat files everything under: `adb logcat -s ExcaliburView`.
const TAG: &str = "ExcaliburView";

/// Called by the activity's native glue once the library is loaded. Returning
/// from here finishes the activity.
#[no_mangle]
fn android_main(app: AndroidApp) {
    start_logging();
    report_panics();
    log::info!("Excalibur View {} started", env!("CARGO_PKG_VERSION"));

    match app.internal_data_path() {
        Some(files) => give_it_a_home(&files),
        None => log::warn!("Android gave this app no data folder; settings will not be kept"),
    }

    match crate::android_bridge::Android::new(&app) {
        Ok(bridge) => hyperview::platform::install(Box::new(bridge)),
        Err(why) => log::error!("the file picker, printing and sharing are not available: {why}"),
    }
    // The window holds the inbox, so a drawing opened with the app from
    // somewhere else, before or after this, finds its way to a tab.
    let _ = hyperview::instance::start(&[], false);

    let keyboard = app.clone();
    let options = eframe::NativeOptions {
        android_app: Some(app),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: graphics(),
        ..Default::default()
    };
    let started = eframe::run_native(
        "Excalibur View",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            let program = hyperview::app::App::new(cc, Vec::new());
            Ok(Box::new(crate::android_bridge::Tablet::new(program, keyboard)))
        }),
    );
    if let Err(e) = started {
        log::error!("the window could not start: {e}");
    }

    // The process goes with the window. winit makes one event loop per
    // process and refuses a second, and Android is free to keep a process
    // alive after its activity has finished and start the activity in it
    // again. That second start would have no window and close at once.
    log::info!("Excalibur View closed");
    std::process::exit(0);
}

fn start_logging() {
    // The same levels the desktop writes to its log, so the two read alike.
    let filter = env_filter::Builder::new()
        .parse("warn,hyperview=info,hub=info,excalibur_view_mobile=info")
        .build();
    android_logger::init_once(
        android_logger::Config::default()
            .with_tag(TAG)
            .with_max_level(log::LevelFilter::Info)
            .with_filter(filter),
    );
}

/// Standard error goes nowhere on Android, which is where a panic message
/// would otherwise go. This puts it in logcat first.
fn report_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        log::error!(
            "Excalibur View {} stopped unexpectedly on thread '{}': {info}",
            env!("CARGO_PKG_VERSION"),
            thread.name().unwrap_or("unnamed"),
        );
        default(info);
    }));
}

/// Points the places a Linux program keeps things into the app's own folder.
///
/// Android sets none of `HOME`, `XDG_DATA_HOME` or `XDG_CONFIG_HOME` for an
/// app, so the `directories` crate and everything that reads `HOME` directly
/// would otherwise land on `/` or nowhere. The layout is the one a Linux home
/// has, so code that reads `$HOME/.config` and code that asks for the XDG
/// config folder still agree with each other.
///
/// `TMPDIR` and `PDFIUM_DIR` are set by `MainActivity`, which is the side that
/// knows the cache folder and the native library folder. The temporary folder
/// Rust falls back to on Android, `/data/local/tmp`, is not writable by an app.
fn give_it_a_home(files: &Path) {
    let data = files.join(".local").join("share");
    let config = files.join(".config");
    let cache = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| files.join(".cache"));
    for folder in [&data, &config, &cache] {
        if let Err(e) = std::fs::create_dir_all(folder) {
            log::warn!("could not make {}: {e}", folder.display());
        }
    }
    // Before the program starts any thread of its own: the window, the PDF
    // worker and the tile helpers all come after this.
    std::env::set_var("HOME", files);
    std::env::set_var("XDG_DATA_HOME", &data);
    std::env::set_var("XDG_CONFIG_HOME", &config);
    std::env::set_var("XDG_CACHE_HOME", &cache);
    if std::env::var_os("TMPDIR").is_none() {
        std::env::set_var("TMPDIR", &cache);
    }
    log::info!("keeping settings and data in {}", files.display());
}

/// Vulkan when the phone has a Vulkan driver that can draw into the window,
/// OpenGL ES when it does not.
///
/// The limits asked of the device are the ones every phone meets rather than
/// the desktop's, which assume a desktop graphics card: a device request that
/// asks for more than the phone has fails outright, and on Android there is no
/// second attempt at a window.
fn graphics() -> WgpuConfiguration {
    let mut setup = WgpuSetupCreateNew::default();
    setup.instance_descriptor.backends =
        wgpu::Backends::from_env().unwrap_or(wgpu::Backends::VULKAN | wgpu::Backends::GL);
    setup.native_adapter_selector = Some(Arc::new(|adapters, surface| {
        let rank = |adapter: &wgpu::Adapter| match adapter.get_info().backend {
            wgpu::Backend::Vulkan => 0,
            wgpu::Backend::Gl => 1,
            _ => 2,
        };
        let chosen = adapters
            .iter()
            .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
            .min_by_key(|a| rank(a))
            .cloned()
            .ok_or_else(|| "no graphics adapter can draw into this window".to_string())?;
        let info = chosen.get_info();
        log::info!(
            "drawing with {:?} on {} ({})",
            info.backend,
            info.name,
            info.driver_info
        );
        Ok(chosen)
    }));
    setup.device_descriptor = Arc::new(|adapter| {
        let base = if adapter.get_info().backend == wgpu::Backend::Gl {
            wgpu::Limits::downlevel_webgl2_defaults()
        } else {
            wgpu::Limits::downlevel_defaults()
        };
        wgpu::DeviceDescriptor {
            label: Some("Excalibur View"),
            required_features: wgpu::Features::default(),
            required_limits: wgpu::Limits {
                // As large as the desktop asks for when the phone has it, and
                // whatever the phone has when it does not. egui is told the
                // real figure and keeps its textures within it.
                max_texture_dimension_2d: adapter.limits().max_texture_dimension_2d.min(8192),
                ..base
            },
            // A phone shares its memory with everything else on it.
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        }
    });
    WgpuConfiguration {
        wgpu_setup: WgpuSetup::CreateNew(setup),
        present_mode: wgpu::PresentMode::AutoVsync,
        ..Default::default()
    }
}
