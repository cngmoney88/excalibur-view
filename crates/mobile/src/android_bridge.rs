//! The program's side of `Bridge.java`: what it asks Android to do, what
//! Android tells it, and the soft keyboard.
//!
//! Every call goes to a static method on `com.excaliburct.view.Bridge`, which
//! does the work on the activity's thread. The class is found once, through
//! the activity's own class loader: a thread the program started has only the
//! system's class loader, which has never heard of the app's classes.

use std::path::{Path, PathBuf};

use android_activity::input::{TextInputState, TextSpan};
use android_activity::AndroidApp;
use hyperview::files::Filter;
use hyperview::ocr::Word;
use jni::objects::{GlobalRef, JClass, JObject, JObjectArray, JString, JValue};
use jni::sys::jlong;
use jni::{JNIEnv, JavaVM};

const BRIDGE: &str = "com.excaliburct.view.Bridge";

pub struct Android {
    vm: JavaVM,
    class: GlobalRef,
}

impl Android {
    pub fn new(app: &AndroidApp) -> Result<Android, String> {
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }
            .map_err(|e| e.to_string())?;
        let class = {
            let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
            let found = find_bridge(&mut env, app);
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_describe();
                let _ = env.exception_clear();
            }
            found.map_err(|e| format!("{BRIDGE} could not be found: {e}"))?
        };
        Ok(Android { vm, class })
    }

    /// Calls a static method of Bridge. A Java exception is logged and
    /// cleared, never left to bring the program down.
    fn call<T>(
        &self,
        what: &str,
        call: impl FnOnce(&mut JNIEnv, &JClass) -> jni::errors::Result<T>,
    ) -> Option<T> {
        let mut env = match self.vm.attach_current_thread() {
            Ok(env) => env,
            Err(e) => {
                log::warn!("{what}: this thread could not reach Java: {e}");
                return None;
            }
        };
        let class: &JClass = self.class.as_obj().into();
        let answer = call(&mut env, class);
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
        match answer {
            Ok(value) => Some(value),
            Err(e) => {
                log::warn!("{what}: {e}");
                None
            }
        }
    }
}

fn find_bridge(env: &mut JNIEnv, app: &AndroidApp) -> jni::errors::Result<GlobalRef> {
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    let loader = env
        .call_method(&activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
        .l()?;
    let name = env.new_string(BRIDGE)?;
    let class = env
        .call_method(&loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;", &[JValue::Object(&name)])?
        .l()?;
    env.new_global_ref(class)
}

impl hyperview::platform::Bridge for Android {
    fn import(&self, asked: u64, filters: &[Filter], many: bool, into: &Path) {
        let types = hyperview::files::mime_types(filters);
        let into = into.display().to_string();
        let sent = self.call("bringing files in", |env, class| {
            let array = env.new_object_array(types.len() as i32, "java/lang/String", JObject::null())?;
            for (i, t) in types.iter().enumerate() {
                let s = env.new_string(t)?;
                env.set_object_array_element(&array, i as i32, s)?;
            }
            let into = env.new_string(&into)?;
            env.call_static_method(
                class,
                "importFiles",
                "(J[Ljava/lang/String;ZLjava/lang/String;)V",
                &[
                    JValue::Long(asked as i64),
                    JValue::Object(&array),
                    JValue::Bool(u8::from(many)),
                    JValue::Object(&into),
                ],
            )?;
            Ok(())
        });
        if sent.is_none() {
            // Nobody will answer; say so, or the list waits for ever.
            hyperview::platform::imported(asked, Vec::new());
        }
    }

    fn share(&self, path: &Path) {
        let path = path.display().to_string();
        self.call("sharing", |env, class| {
            let path = env.new_string(&path)?;
            env.call_static_method(class, "share", "(Ljava/lang/String;)V", &[JValue::Object(&path)])?;
            Ok(())
        });
    }

    fn print(&self, pdf: &Path, name: &str) {
        let pdf = pdf.display().to_string();
        self.call("printing", |env, class| {
            let pdf = env.new_string(&pdf)?;
            let name = env.new_string(name)?;
            env.call_static_method(
                class,
                "print",
                "(Ljava/lang/String;Ljava/lang/String;)V",
                &[JValue::Object(&pdf), JValue::Object(&name)],
            )?;
            Ok(())
        });
    }

    fn open_url(&self, url: &str) {
        self.call("opening a link", |env, class| {
            let url = env.new_string(url)?;
            env.call_static_method(class, "openUrl", "(Ljava/lang/String;)V", &[JValue::Object(&url)])?;
            Ok(())
        });
    }

    fn reads_words(&self) -> bool {
        true
    }

    fn read_words(&self, grey: &[u8], width: u32, height: u32) -> Option<Result<Vec<Word>, String>> {
        let answer = self.call("reading words", |env, class| {
            let bytes = env.byte_array_from_slice(grey)?;
            let read = env
                .call_static_method(
                    class,
                    "readWords",
                    "([BII)Ljava/lang/String;",
                    &[JValue::Object(&bytes), JValue::Int(width as i32), JValue::Int(height as i32)],
                )?
                .l()?;
            if read.is_null() {
                return Ok(None);
            }
            let read = JString::from(read);
            let text: String = env.get_string(&read)?.into();
            Ok(Some(text))
        });
        Some(match answer {
            Some(Some(text)) => Ok(hyperview::platform::words_from_lines(&text)),
            _ => Err("the text recogniser could not read this sheet".into()),
        })
    }
}

fn strings(env: &mut JNIEnv, array: &JObjectArray) -> Vec<PathBuf> {
    let Ok(n) = env.get_array_length(array) else {
        return Vec::new();
    };
    (0..n)
        .filter_map(|i| {
            let item = env.get_object_array_element(array, i).ok()?;
            let item = JString::from(item);
            let text: String = env.get_string(&item).ok()?.into();
            Some(PathBuf::from(text))
        })
        .collect()
}

#[no_mangle]
pub extern "system" fn Java_com_excaliburct_view_Bridge_nativeImported<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    asked: jlong,
    paths: JObjectArray<'local>,
) {
    let files = strings(&mut env, &paths);
    log::info!("{} file(s) brought in", files.len());
    hyperview::platform::imported(asked as u64, files);
}

#[no_mangle]
pub extern "system" fn Java_com_excaliburct_view_Bridge_nativeOpened<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    paths: JObjectArray<'local>,
) {
    let files = strings(&mut env, &paths);
    log::info!("{} drawing(s) opened with the app", files.len());
    hyperview::platform::opened_elsewhere(files);
}

// ---- the soft keyboard --------------------------------------------------------

/// A little text always in the keyboard's box, so a backspace pressed with
/// nothing typed still has something to take away, and reaches the program.
const SEED: &str = "\u{200B}";

/// The program, with what Android's keyboard types put into it.
///
/// GameActivity hands the keyboard's typing over as the whole of what is in
/// the keyboard's box rather than as keys, and winit 0.30 passes that on to
/// nobody. So the box is read every frame, and what changed since the last
/// frame goes to the program as the backspaces and text it amounts to — which
/// also covers a word the keyboard corrected, as the old word taken back and
/// the new one typed.
pub struct Tablet {
    program: hyperview::app::App,
    android: AndroidApp,
    /// What was in the keyboard's box last frame. Empty when nothing on
    /// screen was taking typing.
    boxed: String,
}

impl Tablet {
    pub fn new(program: hyperview::app::App, android: AndroidApp) -> Tablet {
        Tablet { program, android, boxed: String::new() }
    }

    fn seed(&mut self) {
        let end = SEED.len();
        self.android.set_text_input_state(TextInputState {
            text: SEED.to_string(),
            selection: TextSpan { start: end, end },
            compose_region: None,
        });
        self.boxed = SEED.to_string();
    }

    fn keyboard(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if !ctx.wants_keyboard_input() {
            if !self.boxed.is_empty() {
                self.android.set_text_input_state(TextInputState {
                    text: String::new(),
                    selection: TextSpan { start: 0, end: 0 },
                    compose_region: None,
                });
                self.boxed.clear();
            }
            return;
        }
        if self.boxed.is_empty() {
            self.seed();
            return;
        }
        let now = self.android.text_input_state().text;
        if now == self.boxed {
            return;
        }
        for event in hyperview::platform::keyboard_events(&self.boxed, &now) {
            raw.events.push(event);
        }
        if now.starts_with(SEED) {
            self.boxed = now;
        } else {
            // The seed itself was taken back: put it back for the next one.
            self.seed();
        }
    }
}

impl eframe::App for Tablet {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.program.update(ctx, frame);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.program.raw_input_hook(ctx, raw);
        self.keyboard(ctx, raw);
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.program.on_exit(gl);
    }
}
