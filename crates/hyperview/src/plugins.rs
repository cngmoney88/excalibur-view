//! Plugins: an office's own tools, kept out of everybody else's copy.
//!
//! A plugin is WebAssembly, signed by the publisher, handed round by the
//! office's server the way the tool chest is. It runs here in an
//! interpreter with nothing to reach: no files, no network, no screen and
//! no drawing. It is handed the sheets and the takeoff as data and hands
//! back findings and suggested fixes, which this program carries out only
//! when somebody clicks one. See `plugin_api` for the contract.
//!
//! Three walls, each enough on its own to matter:
//!
//! - **The signature.** A plugin is loaded only when one of the keys this
//!   program was built with signed exactly those bytes. The office server
//!   checks it when an administrator adds one; every seat checks it again.
//! - **The sandbox.** A module that imports anything at all is refused —
//!   there is nothing for it to import. It gets a budget of work and of
//!   memory, and running out of either is an error, not a hang.
//! - **The fixes.** A plugin cannot change the drawing. It can only offer
//!   one of a short list of actions, carried out here, each an undo step.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hub::update::Trusted;
use plugin_api::{Answer, Input, Manifest, Output};

/// How much work one run may do. About a minute of a busy interpreter —
/// far more than reading a hundred sheets needs, and a stop for one that
/// has gone round in circles.
pub const FUEL: u64 = 60_000_000_000;

/// How much memory one run may hold.
pub const MEMORY: usize = 1536 << 20;

/// The most a plugin may hand back.
const MOST_OUTPUT: usize = 64 << 20;

pub const EXTENSION: &str = "hvplugin";

#[derive(Clone, Debug)]
pub struct Plugin {
    pub manifest: Manifest,
    pub wasm: Arc<Vec<u8>>,
    /// SHA-256 of the whole plugin file.
    pub digest: String,
    /// Which trusted key signed it.
    pub key: String,
    /// Came from the office's server, rather than added on this computer.
    pub from_office: bool,
    pub path: PathBuf,
}

/// The plugins this seat has.
#[derive(Clone, Debug, Default)]
pub struct Shelf {
    pub plugins: Vec<Plugin>,
    /// Files that were not loaded, and why.
    pub refused: Vec<(String, String)>,
}

/// Where plugins added on this computer are kept: beside the program, like
/// the tool chests.
pub fn folder() -> PathBuf {
    crate::server::profiles_folder()
        .parent()
        .map(|p| p.join("plugins"))
        .unwrap_or_else(|| PathBuf::from("plugins"))
}

/// Where the office's plugins are kept. Everything in here came from the
/// server and goes when the server stops listing it.
pub fn office_folder() -> PathBuf {
    folder().join("office")
}

impl Shelf {
    /// Everything in the two folders that passes.
    pub fn load(trusted: &Trusted) -> Shelf {
        Shelf::load_from(trusted, &folder(), &office_folder())
    }

    pub fn load_from(trusted: &Trusted, own: &Path, office: &Path) -> Shelf {
        let mut shelf = Shelf::default();
        // The office's first: when both folders hold the same plugin, the
        // one the office hands out is the one everybody is running.
        for (dir, from_office) in [(office, true), (own, false)] {
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            let mut paths: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().map(|x| x.eq_ignore_ascii_case(EXTENSION)).unwrap_or(false))
                .collect();
            paths.sort();
            for path in paths {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let loaded = std::fs::read(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| open(trusted, &bytes, &path, from_office));
                match loaded {
                    Ok(plugin) => {
                        if shelf.find(&plugin.manifest.id).is_none() {
                            shelf.plugins.push(plugin);
                        }
                    }
                    Err(why) => {
                        log::warn!("plugin {name} not loaded: {why}");
                        shelf.refused.push((name, why));
                    }
                }
            }
        }
        shelf.plugins.sort_by(|a, b| a.manifest.name.to_lowercase().cmp(&b.manifest.name.to_lowercase()));
        shelf
    }

    pub fn find(&self, id: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.manifest.id == id)
    }

    /// What the Plugins menu lists.
    pub fn menu(&self) -> Vec<ui::chrome::PluginItem> {
        let mut out = Vec::new();
        for plugin in &self.plugins {
            for command in &plugin.manifest.commands {
                out.push(ui::chrome::PluginItem {
                    fire: fire_id(&plugin.manifest.id, &command.id),
                    group: plugin.manifest.name.clone(),
                    label: command.name.clone(),
                    help: command.description.clone(),
                });
            }
        }
        out
    }

    /// Digests of the office plugins this seat already holds.
    pub fn office_digests(&self) -> HashSet<String> {
        self.plugins
            .iter()
            .filter(|p| p.from_office)
            .map(|p| p.digest.to_lowercase())
            .collect()
    }
}

/// The command id a menu item fires.
pub fn fire_id(plugin: &str, command: &str) -> String {
    format!("Plugin:{plugin}:{command}")
}

/// The other way round.
pub fn from_fire_id(id: &str) -> Option<(String, String)> {
    let rest = id.strip_prefix("Plugin:")?;
    let (plugin, command) = rest.split_once(':')?;
    Some((plugin.to_string(), command.to_string()))
}

/// Checks a plugin file and reads what it says about itself.
pub fn open(trusted: &Trusted, file: &[u8], path: &Path, from_office: bool) -> Result<Plugin, String> {
    let checked = hub::plugin::check(trusted, file)?;
    let manifest = manifest_of(&checked.wasm)?;
    if manifest.id != checked.header.id || manifest.version != checked.header.version {
        return Err(format!(
            "The plugin says it is {} {} but was signed as {} {}.",
            manifest.id, manifest.version, checked.header.id, checked.header.version
        ));
    }
    if manifest.abi > plugin_api::ABI {
        return Err(format!(
            "{} needs a newer Excalibur View than this one. It will load once this copy updates.",
            manifest.name
        ));
    }
    if manifest.commands.is_empty() {
        return Err(format!("{} has nothing in it to run.", manifest.name));
    }
    Ok(Plugin {
        manifest,
        wasm: Arc::new(checked.wasm),
        digest: checked.digest,
        key: checked.header.key,
        from_office,
        path: path.to_path_buf(),
    })
}

/// Checks a plugin somebody picked, and puts it in this computer's folder.
pub fn add_from_file(trusted: &Trusted, source: &Path) -> Result<(Plugin, Vec<u8>), String> {
    let bytes = std::fs::read(source).map_err(|e| format!("{}: {e}", source.display()))?;
    let checked = open(trusted, &bytes, source, false)?;
    let dir = folder();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not make the plugins folder: {e}"))?;
    let path = dir.join(format!("{}.{EXTENSION}", checked.manifest.id));
    std::fs::write(&path, &bytes).map_err(|e| format!("Could not keep the plugin: {e}"))?;
    Ok((Plugin { path, ..checked }, bytes))
}

// ---- the sandbox --------------------------------------------------------------

struct Host {
    limits: wasmi::StoreLimits,
}

fn instantiate(wasm: &[u8]) -> Result<(wasmi::Store<Host>, wasmi::Instance), String> {
    let mut config = wasmi::Config::default();
    config.consume_fuel(true);
    let engine = wasmi::Engine::new(&config);
    let module = wasmi::Module::new(&engine, wasm)
        .map_err(|e| format!("The plugin could not be read as WebAssembly: {e}"))?;
    // There is nothing to import. A plugin that asks for something wants to
    // reach outside the sandbox, and does not run.
    let wants: Vec<String> = module
        .imports()
        .map(|i| format!("{}.{}", i.module(), i.name()))
        .collect();
    if !wants.is_empty() {
        return Err(format!(
            "The plugin asks for things Excalibur View does not give plugins: {}.",
            wants.join(", ")
        ));
    }
    let limits = wasmi::StoreLimitsBuilder::new()
        .memory_size(MEMORY)
        .instances(1)
        .memories(1)
        .tables(4)
        .build();
    let mut store = wasmi::Store::new(&engine, Host { limits });
    store.limiter(|host| &mut host.limits);
    store
        .set_fuel(FUEL)
        .map_err(|e| format!("The plugin's budget could not be set: {e}"))?;
    let linker = wasmi::Linker::<Host>::new(&engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .and_then(|pre| pre.start(&mut store))
        .map_err(|e| explain(&e))?;
    Ok((store, instance))
}

fn explain(e: &wasmi::Error) -> String {
    let text = e.to_string();
    if text.contains("fuel") {
        "The plugin took too long and was stopped.".into()
    } else if text.contains("memory") && (text.contains("limit") || text.contains("grow")) {
        "The plugin wanted more memory than it is allowed and was stopped.".into()
    } else {
        format!("The plugin stopped with an error: {text}")
    }
}

/// Reads an answer out of the plugin's memory: address high, length low.
fn take(store: &wasmi::Store<Host>, memory: &wasmi::Memory, packed: i64) -> Result<Vec<u8>, String> {
    let at = ((packed as u64) >> 32) as usize;
    let len = (packed as u64 & 0xffff_ffff) as usize;
    if len > MOST_OUTPUT {
        return Err("The plugin answered with more than Excalibur View will read.".into());
    }
    let data = memory.data(store);
    data.get(at..at + len)
        .map(|b| b.to_vec())
        .ok_or_else(|| "The plugin answered from outside its own memory.".into())
}

/// What a plugin says about itself.
pub fn manifest_of(wasm: &[u8]) -> Result<Manifest, String> {
    let (mut store, instance) = instantiate(wasm)?;
    let memory = instance
        .get_memory(&store, "memory")
        .ok_or("The plugin has no memory to talk through.")?;
    let manifest = instance
        .get_typed_func::<(), i64>(&store, "hv_manifest")
        .map_err(|_| "The plugin does not say what it is (no hv_manifest).".to_string())?;
    let packed = manifest.call(&mut store, ()).map_err(|e| explain(&e))?;
    let bytes = take(&store, &memory, packed)?;
    serde_json::from_slice(&bytes).map_err(|e| format!("The plugin's description could not be read: {e}"))
}

/// Runs one command. Blocking — call it off the window's thread.
pub fn run(wasm: &[u8], input: &Input) -> Result<Output, String> {
    let (mut store, instance) = instantiate(wasm)?;
    let memory = instance
        .get_memory(&store, "memory")
        .ok_or("The plugin has no memory to talk through.")?;
    let alloc = instance
        .get_typed_func::<i32, i32>(&store, "hv_alloc")
        .map_err(|_| "The plugin cannot be handed anything (no hv_alloc).".to_string())?;
    let go = instance
        .get_typed_func::<(i32, i32), i64>(&store, "hv_run")
        .map_err(|_| "The plugin cannot be run (no hv_run).".to_string())?;
    let bytes = plugin_api::pack(input);
    let len = i32::try_from(bytes.len()).map_err(|_| "There is too much to hand the plugin.".to_string())?;
    let at = alloc.call(&mut store, len).map_err(|e| explain(&e))?;
    memory
        .write(&mut store, at as u32 as usize, &bytes)
        .map_err(|_| "The plugin gave no room for what it was handed.".to_string())?;
    let packed = go.call(&mut store, (at, len)).map_err(|e| explain(&e))?;
    let answer = take(&store, &memory, packed)?;
    match serde_json::from_slice::<Answer>(&answer) {
        Ok(Answer::Done(output)) => Ok(output),
        Ok(Answer::Failed(why)) => Err(why),
        Err(e) => Err(format!("The plugin's answer could not be read: {e}")),
    }
}

// ---- settings ------------------------------------------------------------------

/// What each plugin's settings were last set to on this computer.
pub type Settings = BTreeMap<String, BTreeMap<String, serde_json::Value>>;

fn settings_file() -> PathBuf {
    folder().join("settings.json")
}

pub fn load_settings() -> Settings {
    std::fs::read(settings_file())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &Settings) {
    let _ = std::fs::create_dir_all(folder());
    if let Ok(bytes) = serde_json::to_vec_pretty(settings) {
        let _ = std::fs::write(settings_file(), bytes);
    }
}

/// A plugin's settings with its defaults filled in.
pub fn settings_for(manifest: &Manifest, saved: &Settings) -> BTreeMap<String, serde_json::Value> {
    let mut out = BTreeMap::new();
    let mine = saved.get(&manifest.id);
    for setting in &manifest.settings {
        let value = mine
            .and_then(|m| m.get(&setting.id))
            .cloned()
            .unwrap_or_else(|| setting.default_value());
        out.insert(setting.id.clone(), value);
    }
    out
}

// ---- turning the takeoff into what a plugin reads ------------------------------

/// Inches in one of a scale's units.
fn inches_in(unit: &str) -> f64 {
    match unit.trim().trim_end_matches('.').to_lowercase().as_str() {
        "'" | "ft" | "feet" | "foot" => 12.0,
        "\"" | "in" | "inch" | "inches" => 1.0,
        "yd" | "yard" | "yards" => 36.0,
        "mm" => 1.0 / 25.4,
        "cm" => 1.0 / 2.54,
        "m" => 1.0 / 0.0254,
        _ => 12.0,
    }
}

/// A sheet's scale as a plugin reads it.
pub fn scale_of(measure: &annot::measure::Measure) -> plugin_api::Scale {
    let unit = measure.x.first().map(|f| f.unit.clone()).unwrap_or_default();
    plugin_api::Scale {
        ratio: measure.per_point() * inches_in(&unit) * 72.0,
        text: measure.ratio.clone(),
    }
}

/// Feet in one of a scale's units.
pub fn feet_per_unit(measure: &annot::measure::Measure) -> f64 {
    let unit = measure.x.first().map(|f| f.unit.clone()).unwrap_or_default();
    inches_in(&unit) / 12.0
}

/// How a markup is named to a plugin: its own name when it has one, its
/// place in the list when it does not.
pub fn markup_id(index: usize, markup: &annot::Markup) -> String {
    let name = markup.name();
    if name.is_empty() {
        format!("#{index}")
    } else {
        name
    }
}

pub fn kind_word(kind: annot::Kind) -> &'static str {
    use annot::Kind;
    match kind {
        Kind::Length => "length",
        Kind::Polylength => "polylength",
        Kind::Area => "area",
        Kind::Volume => "volume",
        Kind::Count => "count",
        Kind::Diameter => "diameter",
        Kind::Radius => "radius",
        Kind::Angle => "angle",
        _ => "markup",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plugin written by hand in WebAssembly's text form: it says what it
    /// is, and answers every run with the same thing.
    fn tiny(run_body: &str) -> Vec<u8> {
        let manifest = r#"{"id":"tiny","name":"Tiny","version":"1.0.0","commands":[{"id":"hello","name":"Hello"}]}"#;
        let answer = r#"{"done":{"title":"Hello","findings":[{"level":"check","message":"Look here","page":0,"area":[1,2,3,4]}]}}"#;
        let escape = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let text = format!(
            r#"(module
                (memory (export "memory") 1)
                (global $next (mut i32) (i32.const 8192))
                (data (i32.const 0) "{m}")
                (data (i32.const 2048) "{a}")
                (func (export "hv_alloc") (param i32) (result i32) (local i32)
                    global.get $next
                    local.set 1
                    global.get $next
                    local.get 0
                    i32.add
                    global.set $next
                    local.get 1)
                (func (export "hv_manifest") (result i64)
                    i64.const {ml})
                (func (export "hv_run") (param i32 i32) (result i64)
                    {run_body}
                    i64.const {packed}))"#,
            m = escape(manifest),
            a = escape(answer),
            ml = manifest.len(),
            packed = (2048i64 << 32) | answer.len() as i64,
        );
        wat::parse_str(text).unwrap()
    }

    #[test]
    fn a_plugin_says_what_it_is_and_answers() {
        let wasm = tiny("");
        let manifest = manifest_of(&wasm).unwrap();
        assert_eq!(manifest.id, "tiny");
        assert_eq!(manifest.commands[0].name, "Hello");
        let input = Input {
            command: "hello".into(),
            ..Input::default()
        };
        let output = run(&wasm, &input).unwrap();
        assert_eq!(output.title, "Hello");
        assert_eq!(output.findings[0].area, Some([1.0, 2.0, 3.0, 4.0]));
    }

    #[test]
    fn a_plugin_that_never_finishes_is_stopped() {
        let wasm = tiny("(loop $forever (br $forever))");
        let started = std::time::Instant::now();
        // Smaller budget than the real one, so the test is quick; the same
        // wall stops both.
        let (mut store, instance) = instantiate(&wasm).unwrap();
        store.set_fuel(50_000_000).unwrap();
        let go = instance.get_typed_func::<(i32, i32), i64>(&store, "hv_run").unwrap();
        let e = go.call(&mut store, (0, 0)).unwrap_err();
        assert!(explain(&e).contains("too long"), "{e}");
        assert!(started.elapsed().as_secs() < 30);
    }

    #[test]
    fn a_plugin_that_grabs_memory_is_stopped() {
        // Twenty-six thousand pages of 64 KiB is well over the allowance.
        let wasm = tiny("(drop (memory.grow (i32.const 26000))) (if (i32.lt_s (memory.size) (i32.const 2)) (then unreachable))");
        let e = run(&wasm, &Input::default()).unwrap_err();
        assert!(e.contains("stopped"), "{e}");
    }

    #[test]
    fn a_plugin_that_reaches_outside_is_not_run() {
        let wasm = wat::parse_str(
            r#"(module (import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32)))
                       (memory (export "memory") 1))"#,
        )
        .unwrap();
        let e = manifest_of(&wasm).unwrap_err();
        assert!(e.contains("does not give plugins"), "{e}");
        assert!(e.contains("fd_write"));
    }

    #[test]
    fn a_plugin_that_answers_from_nowhere_is_caught() {
        let wasm = wat::parse_str(
            r#"(module (memory (export "memory") 1)
                 (func (export "hv_manifest") (result i64) i64.const 0x7fff000000000010))"#,
        )
        .unwrap();
        assert!(manifest_of(&wasm).unwrap_err().contains("outside"));
    }

    #[test]
    fn a_signed_plugin_on_the_shelf_is_loaded_and_a_forged_one_is_listed_as_refused() {
        use ed25519_dalek::{Signer, SigningKey};
        use sha2::{Digest, Sha256};
        let dir = std::env::temp_dir().join(format!("hv-plugins-{}", std::process::id()));
        let (own, office) = (dir.join("own"), dir.join("office"));
        std::fs::create_dir_all(&own).unwrap();
        std::fs::create_dir_all(&office).unwrap();
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let trusted = Trusted {
            keys: vec![("test".into(), signing.verifying_key().to_bytes())],
        };
        let seal = |wasm: &[u8], key: &SigningKey| {
            let sha = hub::update::hex(&Sha256::digest(wasm));
            let signature = key.sign(&plugin_api::signing_payload("tiny", "1.0.0", &sha));
            plugin_api::seal(
                &plugin_api::Header {
                    id: "tiny".into(),
                    version: "1.0.0".into(),
                    name: "Tiny".into(),
                    sha256: sha,
                    bytes: wasm.len() as u64,
                    key: "test".into(),
                    signature: hub::update::hex(&signature.to_bytes()),
                },
                wasm,
            )
        };
        let wasm = tiny("");
        std::fs::write(office.join("tiny.hvplugin"), seal(&wasm, &signing)).unwrap();
        std::fs::write(own.join("forged.hvplugin"), seal(&wasm, &SigningKey::from_bytes(&[8u8; 32]))).unwrap();
        let shelf = Shelf::load_from(&trusted, &own, &office);
        assert_eq!(shelf.plugins.len(), 1);
        assert!(shelf.plugins[0].from_office);
        assert_eq!(shelf.refused.len(), 1);
        assert_eq!(shelf.refused[0].0, "forged.hvplugin");
        let menu = shelf.menu();
        assert_eq!(menu[0].fire, "Plugin:tiny:hello");
        assert_eq!(from_fire_id(&menu[0].fire), Some(("tiny".into(), "hello".into())));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A real plugin through the sandbox: `HV_TEST_PLUGIN=path/to.wasm
    /// cargo test -p hyperview --lib real_plugin -- --ignored`.
    #[test]
    #[ignore]
    fn a_real_plugin_runs_in_the_sandbox() {
        let path = std::env::var("HV_TEST_PLUGIN").expect("HV_TEST_PLUGIN");
        let wasm = std::fs::read(path).unwrap();
        let started = std::time::Instant::now();
        let manifest = manifest_of(&wasm).unwrap();
        println!("{} {} — {} commands, read in {:?}", manifest.name, manifest.version, manifest.commands.len(), started.elapsed());
        for command in &manifest.commands {
            let mut input = Input {
                command: command.id.clone(),
                sheets: vec![plugin_api::Sheet {
                    page: 0,
                    name: "S-201".into(),
                    width: 2592.0,
                    height: 1728.0,
                    scale: Some(plugin_api::Scale { ratio: 96.0, text: "1/8\" = 1'-0\"".into() }),
                    words: (0..3000)
                        .map(|i| plugin_api::Word {
                            text: if i % 7 == 0 { "W12X26".into() } else if i == 5 { "SCALE: 1/4\" = 1'-0\"".into() } else { format!("NOTE {i}") },
                            area: [(i % 60) as f64 * 40.0, (i / 60) as f64 * 30.0, (i % 60) as f64 * 40.0 + 30.0, (i / 60) as f64 * 30.0 + 8.0],
                            size: 8.0,
                        })
                        .collect(),
                    lines: (0..40_000)
                        .map(|i| plugin_api::Line {
                            a: [(i % 200) as f64 * 12.0, (i / 200) as f64 * 8.0],
                            b: [(i % 200) as f64 * 12.0 + 10.0, (i / 200) as f64 * 8.0],
                            width: if i % 50 == 0 { 1.4 } else { 0.25 },
                            dashed: false,
                        })
                        .collect(),
                    ..Default::default()
                }],
                markups: (0..500)
                    .map(|i| plugin_api::Markup {
                        id: format!("#{i}"),
                        kind: "length".into(),
                        subject: "W12X26".into(),
                        points: vec![[i as f64 * 4.0, 100.0], [i as f64 * 4.0 + 50.0, 100.0]],
                        length: Some(10.0),
                        quantity: 1.0,
                        columns: vec!["26".into()],
                        pounds: Some(260.0),
                        ..Default::default()
                    })
                    .collect(),
                columns: vec!["LBS Per FT".into()],
                ..Input::default()
            };
            input.settings = settings_for(&manifest, &Settings::new());
            let started = std::time::Instant::now();
            let output = run(&wasm, &input).unwrap();
            println!("{:12} {:>5} findings, {:>2} tables in {:?} — {}", command.id, output.findings.len(), output.tables.len(), started.elapsed(), output.summary);
        }
    }

    #[test]
    fn quarter_inch_scale_reads_as_forty_eight() {
        let measure = annot::measure::imperial(48.0, "1/4\" = 1'-0\"", 16);
        let scale = scale_of(&measure);
        assert!((scale.ratio - 48.0).abs() < 1e-9, "{}", scale.ratio);
        assert!((feet_per_unit(&measure) - 1.0).abs() < 1e-12);
    }
}
