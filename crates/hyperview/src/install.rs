//! Hyperview.exe putting itself on a computer, and keeping itself current.
//!
//! A company is handed one file. Whoever double-clicks it — from Downloads, a
//! shared drive, a USB stick, an email — gets the program installed for them
//! without being asked anything: it copies itself into their own app folder,
//! puts itself in the Start menu, on the desktop and in Settings → Apps, and
//! opens from there. No administrator, nothing in Program Files, no script.
//! Double-clicking an older copy later does not install it over a newer one;
//! it just opens the newer one.
//!
//! From then on it updates itself: a new version, checked against the signing
//! key compiled into this program, is put in place while the program runs and
//! takes over the next time it is opened. Nobody is restarted in the middle of
//! a takeoff.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use hub::update::{Release, Trusted};

/// The folder it installs into, under the person's own app data.
pub const FOLDER: &str = "Excalibur Hyperview";
/// What it is called once installed.
pub const PROGRAM: &str = "Hyperview.exe";

/// Keeps the version readable off the file itself.
pub static BUILD_MARK: &str = hub::build_mark!();

pub fn home() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join(FOLDER))
}

pub fn installed_program() -> Option<PathBuf> {
    home().map(|h| h.join(PROGRAM))
}

/// Whether this copy is the installed one.
pub fn is_installed_copy() -> bool {
    match (std::env::current_exe(), installed_program()) {
        (Ok(me), Some(target)) => same_file(&me, &target),
        _ => false,
    }
}

/// Whether this build installs and updates itself at all. A developer's
/// build, a non-Windows build, and a copy somebody asked to keep portable do
/// not.
fn manages_itself() -> bool {
    cfg!(windows) && !cfg!(debug_assertions) && std::env::var_os("HYPERVIEW_PORTABLE").is_none()
}

/// The first thing the program does. Returns `true` when this process should
/// simply exit, because the installed copy has been started in its place.
pub fn settle_in(args: &[OsString]) -> bool {
    std::hint::black_box(BUILD_MARK);
    if !manages_itself() {
        return false;
    }
    let (Ok(me), Some(target)) = (std::env::current_exe(), installed_program()) else {
        return false;
    };

    if same_file(&me, &target) {
        // The installed copy. Tidy up after an update, and make sure the
        // shortcuts and the Settings entry say the version that is running.
        hub::update::tidy_after_update(&target);
        if recorded_version().as_deref() != Some(env!("CARGO_PKG_VERSION")) {
            register(&target);
        }
        return false;
    }

    // A copy somewhere else. Install it, unless what is installed is the
    // same or newer — double-clicking an old download must not go backwards.
    let installed = hub::update::version_in_file(&target);
    let install = match installed.as_deref() {
        None => true,
        Some(theirs) => hub::update::newer(env!("CARGO_PKG_VERSION"), theirs),
    };
    if install {
        if let Err(e) = copy_in(&me, &target) {
            // Nowhere to install to is not a reason not to open. It runs from
            // wherever it is.
            log::warn!("could not install to {}: {e}", target.display());
            return false;
        }
        register(&target);
    }

    // Open the installed copy, with whatever drawings were double-clicked.
    match std::process::Command::new(&target).args(args).spawn() {
        Ok(_) => true,
        Err(e) => {
            log::warn!("could not start {}: {e}", target.display());
            false
        }
    }
}

fn copy_in(me: &Path, target: &Path) -> std::io::Result<()> {
    if let Some(folder) = target.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let bytes = std::fs::read(me)?;
    if target.exists() {
        hub::update::swap_in(target, &bytes)
    } else {
        std::fs::write(target, &bytes)
    }
}

fn recorded_version() -> Option<String> {
    let path = home()?.join("installed-version.txt");
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// Start menu, desktop, and Settings → Apps.
///
/// Done through Windows' own shell and registry calls rather than PowerShell,
/// which a Remote Desktop server or a locked-down office machine will often
/// not let an ordinary person start. The desktop is asked of Windows rather
/// than assumed to be `%USERPROFILE%\Desktop`, because on a machine with
/// OneDrive backing up the desktop it is not there.
fn register(target: &Path) {
    #[cfg(windows)]
    {
        let target = target.to_path_buf();
        // Its own thread, so COM is set up exactly as this needs it and
        // nothing else in the program is affected.
        let done = std::thread::spawn(move || windows_shell::register(&target))
            .join()
            .unwrap_or(false);
        if done {
            if let Some(home) = home() {
                let _ = std::fs::write(home.join("installed-version.txt"), env!("CARGO_PKG_VERSION"));
            }
        }
    }
    #[cfg(not(windows))]
    let _ = target;
}

#[cfg(windows)]
mod windows_shell {
    use std::path::{Path, PathBuf};

    use windows::core::{Interface, HSTRING, PCWSTR};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, IPersistFile, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
    };
    use windows::Win32::UI::Shell::{
        FOLDERID_Desktop, FOLDERID_Programs, IShellLinkW, SHGetKnownFolderPath, ShellLink,
        KNOWN_FOLDER_FLAG,
    };

    const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\ExcaliburHyperview";
    /// `hyperview://open?set=…` — what FabWire's "Open in Hyperview" and a
    /// link in an email open. Per person, like everything else here.
    const PROTOCOL_KEY: &str = r"Software\Classes\hyperview";
    /// The file types this program owns, so double-clicking one out of an
    /// email opens Excalibur View and it does the right thing with it —
    /// nobody should have to find a menu to use something they were sent.
    /// (extension, class name, what Explorer calls it)
    const FILE_TYPES: &[(&str, &str, &str)] = &[
        (
            hub::license::EXTENSION,
            "ExcaliburView.License",
            "Excalibur View Office license",
        ),
        (
            chest::native::EXTENSION,
            "ExcaliburView.ToolChest",
            "Excalibur View tool chest",
        ),
    ];
    const LINK_NAME: &str = "Excalibur View.lnk";
    /// What the shortcuts were called before the rename. Taken away when the
    /// new ones are made, so nobody ends up with two icons for one program.
    const OLD_LINK_NAMES: &[&str] = &["Excalibur Hyperview.lnk"];

    fn folder(id: &windows::core::GUID) -> Option<PathBuf> {
        unsafe {
            let raw = SHGetKnownFolderPath(id, KNOWN_FOLDER_FLAG(0), None).ok()?;
            let path = raw.to_string().ok();
            CoTaskMemFree(Some(raw.0 as *const _));
            path.map(PathBuf::from)
        }
    }

    pub fn shortcut_places() -> Vec<PathBuf> {
        [folder(&FOLDERID_Programs), folder(&FOLDERID_Desktop)]
            .into_iter()
            .flatten()
            .map(|dir| dir.join(LINK_NAME))
            .collect()
    }

    fn old_shortcut_places() -> Vec<PathBuf> {
        [folder(&FOLDERID_Programs), folder(&FOLDERID_Desktop)]
            .into_iter()
            .flatten()
            .flat_map(|dir| OLD_LINK_NAMES.iter().map(move |name| dir.join(name)))
            .collect()
    }

    fn shortcut(target: &Path, at: &Path) -> windows::core::Result<()> {
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            let exe = HSTRING::from(target.as_os_str());
            link.SetPath(&exe)?;
            if let Some(dir) = target.parent() {
                link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
            }
            link.SetIconLocation(&exe, 0)?;
            link.SetDescription(&HSTRING::from("Drawing viewer, markup and takeoff"))?;
            let file: IPersistFile = link.cast()?;
            file.Save(&HSTRING::from(at.as_os_str()), true)
        }
    }

    fn set_text(key: HKEY, name: &str, value: &str) {
        let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };
        unsafe {
            let _ = RegSetValueExW(key, &HSTRING::from(name), 0, REG_SZ, Some(bytes));
        }
    }

    fn set_number(key: HKEY, name: &str, value: u32) {
        unsafe {
            let _ = RegSetValueExW(key, &HSTRING::from(name), 0, REG_DWORD, Some(&value.to_le_bytes()));
        }
    }

    fn key_text(path: &str, name: &str, value: &str) -> bool {
        let mut key = HKEY::default();
        let opened = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &HSTRING::from(path),
                0,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut key,
                None,
            )
        };
        if opened.is_err() {
            return false;
        }
        set_text(key, name, value);
        unsafe {
            let _ = RegCloseKey(key);
        }
        true
    }

    /// Makes `hyperview://` links open in this program.
    fn protocol(exe: &str) -> bool {
        key_text(PROTOCOL_KEY, "", "URL:Excalibur View")
            && key_text(PROTOCOL_KEY, "URL Protocol", "")
            && key_text(&format!(r"{PROTOCOL_KEY}\DefaultIcon"), "", &format!("\"{exe}\",0"))
            && key_text(&format!(r"{PROTOCOL_KEY}\shell\open\command"), "", &format!("\"{exe}\" \"%1\""))
    }

    /// Makes a `.evlicense` or `.evtools` open in this program.
    fn file_type(exe: &str, extension: &str, class: &str, shown_as: &str) -> bool {
        let class_key = format!(r"Software\Classes\{class}");
        key_text(&format!(r"Software\Classes\.{extension}"), "", class)
            && key_text(&class_key, "", shown_as)
            && key_text(&format!(r"{class_key}\DefaultIcon"), "", &format!("\"{exe}\",0"))
            && key_text(
                &format!(r"{class_key}\shell\open\command"),
                "",
                &format!("\"{exe}\" \"%1\""),
            )
    }

    pub fn register(target: &Path) -> bool {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        if !protocol(&target.display().to_string()) {
            log::warn!("could not register hyperview:// links");
        }
        for (extension, class, shown_as) in FILE_TYPES {
            if !file_type(&target.display().to_string(), extension, class, shown_as) {
                log::warn!("could not register .{extension} files");
            }
        }
        for old in old_shortcut_places() {
            let _ = std::fs::remove_file(old);
        }
        let mut made = 0;
        for at in shortcut_places() {
            if shortcut(target, &at).is_ok() {
                made += 1;
            }
        }
        let exe = target.display().to_string();
        let folder = target.parent().map(|p| p.display().to_string()).unwrap_or_default();
        let size_kb = std::fs::metadata(target).map(|m| (m.len() / 1024) as u32).unwrap_or(0);
        let mut key = HKEY::default();
        let opened = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &HSTRING::from(UNINSTALL_KEY),
                0,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut key,
                None,
            )
        };
        if opened.is_ok() {
            set_text(key, "DisplayName", "Excalibur View");
            set_text(key, "DisplayVersion", env!("CARGO_PKG_VERSION"));
            set_text(key, "Publisher", "Excalibur Construction Technologies");
            set_text(key, "DisplayIcon", &format!("{exe},0"));
            set_text(key, "InstallLocation", &folder);
            set_text(key, "UninstallString", &format!("\"{exe}\" --uninstall"));
            set_number(key, "NoModify", 1);
            set_number(key, "NoRepair", 1);
            set_number(key, "EstimatedSize", size_kb);
            unsafe {
                let _ = RegCloseKey(key);
            }
        }
        made > 0 && opened.is_ok()
    }

    pub fn unregister() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        for at in shortcut_places().into_iter().chain(old_shortcut_places()) {
            let _ = std::fs::remove_file(at);
        }
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(UNINSTALL_KEY));
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(PROTOCOL_KEY));
            for (extension, class, _) in FILE_TYPES {
                let _ = RegDeleteTreeW(
                    HKEY_CURRENT_USER,
                    &HSTRING::from(format!(r"Software\Classes\.{extension}")),
                );
                let _ = RegDeleteTreeW(
                    HKEY_CURRENT_USER,
                    &HSTRING::from(format!(r"Software\Classes\{class}")),
                );
            }
        }
    }
}

/// Settings → Apps → Uninstall, or `Hyperview.exe --uninstall`.
///
/// Takes the program, its shortcuts and its Settings entry away. Nothing a
/// person made is touched: drawings, markups and tool chests they saved are
/// wherever they saved them, and the company's are on the company's server.
pub fn uninstall() {
    let sure = rfd::MessageDialog::new()
        .set_title("Excalibur View")
        .set_description(
            "Remove Excalibur View from this computer?\n\n\
             Your drawings and markups are not touched.",
        )
        .set_buttons(rfd::MessageButtons::YesNo)
        .show();
    if sure != rfd::MessageDialogResult::Yes {
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED: u32 = 0x0000_0008;
        let _ = std::thread::spawn(windows_shell::unregister).join();
        // The folder goes a moment after this program has exited, because a
        // running program's own file cannot be deleted.
        if let Some(home) = home() {
            let _ = std::process::Command::new("cmd")
                .args([
                    "/c",
                    &format!(
                        "ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{}\"",
                        home.display()
                    ),
                ])
                .creation_flags(NO_WINDOW | DETACHED)
                .spawn();
        }
    }
}

// ---- updates ---------------------------------------------------------------

/// The keys this build accepts an update from.
pub fn trusted() -> Trusted {
    Trusted::pinned(crate::trust::KEYS)
}

/// Whether this copy puts updates in place itself: the installed copy of a
/// release build that trusts a signing key.
pub fn updates_itself() -> bool {
    manages_itself() && is_installed_copy() && !trusted().is_empty()
}

/// Whether `release` is already in place, waiting for the next start.
pub fn already_waiting(release: &Release) -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|me| hub::update::version_in_file(&me))
        .map(|v| v == release.version)
        .unwrap_or(false)
}

/// The version in place where this program runs from, when it is newer than
/// the one running: put there by an update, or by somebody double-clicking a
/// newer download while this window was open. It takes over at the next start.
pub fn waiting_version() -> Option<String> {
    let program = installed_program()
        .filter(|p| p.exists())
        .or_else(|| std::env::current_exe().ok())?;
    hub::update::version_in_file(&program)
        .filter(|v| hub::update::newer(v, env!("CARGO_PKG_VERSION")))
}

/// What a copy started by "Restart now" is told, so it waits for this window
/// to close instead of handing its drawings to it.
pub const RESTARTED: &str = "--restarted";

/// Starts the version now in place, with `open` — the drawings this window
/// has open — and returns once it is started. The caller closes this window.
pub fn restart(open: &[PathBuf]) -> Result<(), String> {
    let program = installed_program()
        .filter(|p| p.exists())
        .or_else(|| std::env::current_exe().ok())
        .ok_or_else(|| "where Excalibur View is installed could not be found".to_string())?;
    std::process::Command::new(&program)
        .arg(RESTARTED)
        .args(open)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{}: {e}", program.display()))
}

/// Checks a downloaded update against the compiled-in keys and, only if it
/// passes, puts it where this program is so the next start runs it.
pub fn put_in_place(release: &Release, bytes: &[u8]) -> Result<(), String> {
    if release.platform != hub::feed::APP_PLATFORM {
        return Err(format!("that update is for {}, not this program", release.platform));
    }
    trusted().check(release, bytes).map_err(|refusal| refusal.to_string())?;
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    hub::update::swap_in(&me, bytes)
        .map_err(|e| format!("version {} could not be put in place: {e}", release.version))
}

// ---- the PDF engine --------------------------------------------------------

/// The PDF engine, carried inside the program and written out beside it the
/// first time it is needed. Named for its size, so a program carrying a
/// different engine writes a different file rather than using a stale one.
#[cfg(embedded_pdfium)]
static PDFIUM: &[u8] = include_bytes!(env!("HYPERVIEW_PDFIUM"));

pub fn pdfium_library() -> Option<PathBuf> {
    #[cfg(embedded_pdfium)]
    {
        let folder = std::env::current_exe()
            .ok()
            .and_then(|me| me.parent().map(PathBuf::from))
            .filter(|dir| writable(dir))
            .or_else(home)?;
        // The extension the platform's loader asks for. Size in the name, so
        // a program carrying a different engine writes a different file
        // rather than using a stale one.
        let extension = if cfg!(target_os = "windows") {
            "dll"
        } else if cfg!(target_os = "macos") {
            "dylib"
        } else {
            "so"
        };
        let path = folder.join(format!("pdfium-{}.{extension}", PDFIUM.len()));
        let whole = std::fs::metadata(&path)
            .map(|m| m.len() == PDFIUM.len() as u64)
            .unwrap_or(false);
        if !whole {
            let _ = std::fs::create_dir_all(&folder);
            let part = path.with_extension("part");
            std::fs::write(&part, PDFIUM).ok()?;
            std::fs::rename(&part, &path).ok()?;
        }
        Some(path)
    }
    #[cfg(not(embedded_pdfium))]
    {
        None
    }
}

#[cfg(embedded_pdfium)]
fn writable(dir: &Path) -> bool {
    let probe = dir.join(".hyperview-write-test");
    let ok = std::fs::write(&probe, b"x").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}
