//! Excalibur View's own tool chest format: `.evtools`.
//!
//! A tool chest is the office's own knowledge — what a W12x26 weighs, what a
//! moment connection is called, what colour a punch item is — and it should
//! live in a file the office owns and can read, not in another company's
//! format. So Excalibur View keeps its tools in this one: plain JSON, one file
//! per chest, with nothing in it that needs anybody else's program to open.
//!
//! A Revu profile (`.bpx`) or tool set (`.btx`) is read once, on the way in,
//! and saved as one of these; from then on it is ours. What comes across is
//! the tools, the office's column names and formulas, and the markups list's
//! columns. What does not is Revu's toolbar arrangement — Excalibur View lays
//! out its own.
//!
//! Each tool keeps the markup it stamps down as a PDF annotation dictionary
//! (`template`, base64 of the dictionary as a PDF writes it). That is the PDF
//! standard's own format, not Revu's, and it is what makes a markup drawn with
//! the tool the same markup every PDF reader shows. The fields beside it —
//! subject, label, colour, columns — are the ones people edit, and they win:
//! they are written into the template when the file is read.

use base64::Engine;
use pdf::{Name, Object, Reader};
use serde::{Deserialize, Serialize};

use crate::profile::{Column, CustomColumn, Profile};
use crate::toolset::{Tool, ToolSet};

/// The file's extension, without the dot.
pub const EXTENSION: &str = "evtools";

/// What the file says it is, so it can be told from any other JSON.
pub const FORMAT: &str = "excalibur-view-tools";

/// The version of this format. A newer file is read as far as this build
/// understands it; nothing in it is ever required that an older file lacks.
pub const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Chest {
    format: String,
    version: u32,
    name: String,
    /// Where it came from, when it was read in from something else: a line
    /// for whoever opens the file, never used for anything.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    imported_from: String,
    /// The office's own columns: "LBS Per FT", "Piece Mark", formulas.
    #[serde(default)]
    columns: Vec<OwnColumn>,
    /// Which columns the markups list shows, in order, and how wide.
    #[serde(default)]
    list: Vec<ListColumn>,
    sets: Vec<Set>,
}

#[derive(Serialize, Deserialize)]
struct OwnColumn {
    /// Which of the six a tool's `columns` it is, from nought.
    slot: usize,
    name: String,
    /// `Number`, `Text`, `Formula`, `Choice`, `Date`.
    kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    formula: String,
    #[serde(default)]
    decimals: usize,
}

#[derive(Serialize, Deserialize)]
struct ListColumn {
    key: String,
    width: f32,
    visible: bool,
}

#[derive(Serialize, Deserialize)]
struct Set {
    title: String,
    tools: Vec<OwnTool>,
}

#[derive(Serialize, Deserialize)]
struct OwnTool {
    subject: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    label: String,
    /// Length, Area, Count… For people reading the file: the kind comes from
    /// the template, because that is what gets drawn.
    kind: String,
    /// `#rrggbb`.
    colour: String,
    /// The six column values, in the order of the chest's `columns`.
    #[serde(default)]
    columns: [String; 6],
    /// Carries properties onto the next markup drawn rather than being
    /// stamped down as it stands.
    #[serde(default, skip_serializing_if = "is_false")]
    properties_only: bool,
    /// The PDF annotation dictionary this tool stamps, base64.
    template: String,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Whether some bytes are one of these files.
pub fn is_native(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
    let text = text.trim_start_matches('\u{feff}').trim_start();
    text.starts_with('{') && text.contains(FORMAT)
}

/// Writes a chest in Excalibur View's own format.
pub fn write(profile: &Profile, imported_from: &str) -> Vec<u8> {
    let chest = Chest {
        format: FORMAT.into(),
        version: VERSION,
        name: profile.name.clone(),
        imported_from: imported_from.into(),
        columns: profile
            .custom_columns
            .iter()
            .map(|c| OwnColumn {
                slot: c.index,
                name: c.name.clone(),
                kind: c.kind.clone(),
                formula: c.expression.clone(),
                decimals: c.precision,
            })
            .collect(),
        list: profile
            .markup_columns
            .iter()
            .map(|c| ListColumn { key: c.key.clone(), width: c.width, visible: c.visible })
            .collect(),
        sets: profile
            .sets
            .iter()
            .map(|set| Set {
                title: set.title.clone(),
                tools: set.tools.iter().map(own_tool).collect(),
            })
            .collect(),
    };
    let mut out = serde_json::to_vec_pretty(&chest).unwrap_or_default();
    out.push(b'\n');
    out
}

fn own_tool(tool: &Tool) -> OwnTool {
    let mut bytes = Vec::new();
    pdf::write::write_dict(&tool.annotation, &mut bytes);
    OwnTool {
        subject: tool.subject.clone(),
        label: tool.label.clone(),
        kind: tool.kind.name().into(),
        colour: hex_colour(tool.colour),
        columns: tool.columns.clone(),
        properties_only: tool.properties_only,
        template: base64::engine::general_purpose::STANDARD.encode(bytes),
    }
}

/// Reads one of these files. `None` when it is not one, or is too broken to
/// be worth half-reading.
pub fn read(bytes: &[u8]) -> Option<Profile> {
    let text = std::str::from_utf8(bytes).ok()?.trim_start_matches('\u{feff}');
    let chest: Chest = serde_json::from_str(text).ok()?;
    if chest.format != FORMAT {
        return None;
    }
    let sets: Vec<ToolSet> = chest
        .sets
        .into_iter()
        .map(|set| ToolSet {
            title: set.title,
            tools: set.tools.into_iter().filter_map(tool_from).collect(),
        })
        .filter(|set| !set.tools.is_empty())
        .collect();
    Some(Profile {
        name: if chest.name.trim().is_empty() { "Tool chest".into() } else { chest.name },
        toolbars: Vec::new(),
        nav_left: Vec::new(),
        nav_middle: Vec::new(),
        nav_right: Vec::new(),
        hover_bar: Vec::new(),
        panels: Default::default(),
        markup_columns: chest
            .list
            .into_iter()
            .map(|c| Column { key: c.key, width: c.width, visible: c.visible })
            .collect(),
        custom_columns: chest
            .columns
            .into_iter()
            .filter(|c| c.slot < 6)
            .map(|c| CustomColumn {
                index: c.slot,
                name: c.name,
                kind: c.kind,
                expression: c.formula,
                precision: c.decimals,
            })
            .collect(),
        thumbnail_size: 132.0,
        sets,
    })
}

fn tool_from(own: OwnTool) -> Option<Tool> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(own.template.trim()).ok()?;
    let mut annotation = Reader::new(&bytes).object().ok()?.as_dict()?.clone();
    // What people edit wins over what is inside the template.
    let colour = parse_colour(&own.colour).unwrap_or([0.0, 0.0, 0.0]);
    annotation.set(Name::new("Subj"), Object::text(&own.subject));
    if own.label.is_empty() {
        annotation.remove("Label");
    } else {
        annotation.set(Name::new("Label"), Object::text(&own.label));
    }
    annotation.set(
        Name::new("C"),
        Object::Array(colour.iter().map(|c| Object::real(*c as f64)).collect()),
    );
    if own.columns.iter().any(|c| !c.is_empty()) || annotation.get("BSIColumnData").is_some() {
        annotation.set(
            Name::new("BSIColumnData"),
            Object::Array(own.columns.iter().map(|c| Object::text(c)).collect()),
        );
    }
    let kind = annot::Kind::read(&annotation);
    Some(Tool {
        subject: own.subject,
        label: own.label,
        kind,
        annotation,
        columns: own.columns,
        colour,
        properties_only: own.properties_only,
    })
}

fn hex_colour(c: [f32; 3]) -> String {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(c[0]), byte(c[1]), byte(c[2]))
}

fn parse_colour(text: &str) -> Option<[f32; 3]> {
    let hex = text.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let part = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok().map(|v| v as f32 / 255.0);
    Some([part(0)?, part(2)?, part(4)?])
}

/// Whatever sort of chest `bytes` is, as a chest.
pub fn read_any(bytes: &[u8]) -> Option<Profile> {
    if is_native(bytes) {
        read(bytes)
    } else {
        Profile::read_revu(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use annot::Kind;

    fn a_chest() -> Profile {
        let mut beam = crate::toolset::plain("W12x26", Kind::Length);
        beam.annotation = Reader::new(
            b"<< /Type /Annot /Subtype /PolyLine /Subj (W12x26) /IT /PolyLineDimension \
               /C [1 0 0] /BSIColumnData [(26) () () () () ()] /Measure << /Type /Measure >> >>",
        )
        .object()
        .unwrap()
        .as_dict()
        .unwrap()
        .clone();
        beam.kind = Kind::read(&beam.annotation);
        beam.columns[0] = "26".into();
        beam.colour = [1.0, 0.0, 0.0];
        Profile {
            name: "Steel Takeoff".into(),
            toolbars: Vec::new(),
            nav_left: Vec::new(),
            nav_middle: Vec::new(),
            nav_right: Vec::new(),
            hover_bar: Vec::new(),
            panels: Default::default(),
            markup_columns: vec![Column { key: "Subject".into(), width: 210.0, visible: true }],
            custom_columns: vec![CustomColumn {
                index: 0,
                name: "LBS Per FT".into(),
                kind: "Number".into(),
                expression: String::new(),
                precision: 2,
            }],
            thumbnail_size: 132.0,
            sets: vec![ToolSet { title: "Beams".into(), tools: vec![beam] }],
        }
    }

    #[test]
    fn a_chest_comes_back_as_it_went_out() {
        let chest = a_chest();
        let bytes = write(&chest, "Revu profile: Steel Takeoff.bpx");
        assert!(is_native(&bytes));
        let back = read(&bytes).expect("our own file reads");
        assert_eq!(back.name, "Steel Takeoff");
        assert_eq!(back.custom_columns, chest.custom_columns);
        let tool = &back.sets[0].tools[0];
        assert_eq!(tool.subject, "W12x26");
        assert_eq!(tool.kind, chest.sets[0].tools[0].kind);
        assert_eq!(tool.pounds_per_foot(), Some(26.0), "the weight comes across");
        assert!(tool.annotation.get("Measure").is_some(), "and so does the rest of the template");
        assert!(back.toolbars.is_empty(), "the toolbars are Excalibur View's own");
    }

    #[test]
    fn what_people_edit_in_the_file_wins() {
        let text = String::from_utf8(write(&a_chest(), "")).unwrap();
        let edited = text
            .replace("\"subject\": \"W12x26\"", "\"subject\": \"W12x30\"")
            .replace("\"26\"", "\"30\"")
            .replace("#ff0000", "#0000ff");
        let tool = read(edited.as_bytes()).unwrap().sets.remove(0).tools.remove(0);
        assert_eq!(tool.subject, "W12x30");
        assert_eq!(tool.pounds_per_foot(), Some(30.0));
        assert_eq!(tool.colour, [0.0, 0.0, 1.0]);
        assert_eq!(tool.annotation.get("Subj").and_then(|o| o.as_text()).as_deref(), Some("W12x30"));
    }

    #[test]
    fn other_json_is_not_a_tool_chest() {
        assert!(!is_native(b"{\"hello\": 1}"));
        assert!(read(b"{\"format\": \"something-else\", \"version\": 1, \"name\": \"x\", \"sets\": []}").is_none());
        assert!(read(b"not json").is_none());
    }
}
