//! Tool sets: the actual tools, with their annotation templates and the column
//! data that carries weights.

use annot::Kind;
pub use annot::Kind as ToolKind;
use pdf::{Dict, Name, Object, Reader};

use crate::xml::Node;

#[derive(Clone, Debug)]
pub struct Tool {
    /// What the markups list groups by, and what a fabricator would call it:
    /// `W12x26`, `PL1/2`, `Shear Conn`.
    pub subject: String,
    pub label: String,
    pub kind: Kind,
    /// The annotation dictionary, with Bluebeam's internal pointers resolved.
    pub annotation: Dict,
    /// The six user defined columns, verbatim.
    pub columns: [String; 6],
    pub colour: [f32; 3],
    /// True when the tool only carries properties to apply to the next markup
    /// drawn, rather than being stamped down as it stands.
    pub properties_only: bool,
}

impl Tool {
    /// Pounds per foot, for the linear shapes. His chest keeps it in the first
    /// user column. `None` means the tool does not carry a weight, which is not
    /// the same as weighing nothing.
    pub fn pounds_per_foot(&self) -> Option<f64> {
        match self.kind {
            Kind::Length | Kind::Polylength => number(&self.columns[0]),
            _ => None,
        }
    }

    /// Pounds per square foot, for plate, grating, floor plate and sheet. His
    /// chest keeps it in the sixth user column.
    pub fn pounds_per_square_foot(&self) -> Option<f64> {
        match self.kind {
            Kind::Area => number(&self.columns[5]),
            _ => None,
        }
    }

    /// True when this tool can put a weight on a takeoff at all.
    pub fn carries_weight(&self) -> bool {
        self.pounds_per_foot().is_some() || self.pounds_per_square_foot().is_some()
    }
}

fn number(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let value: f64 = text.parse().ok()?;
    (value > 0.0).then_some(value)
}

#[derive(Clone, Debug)]
pub struct ToolSet {
    pub title: String,
    pub tools: Vec<Tool>,
}

impl ToolSet {
    /// Reads one `<BluebeamRevuToolSet>` node.
    pub fn read(node: &Node) -> ToolSet {
        let title = crate::unpack_text(node.text_of("Title")).unwrap_or_default();
        let tools = node
            .children_named("ToolChestItem")
            .filter_map(read_tool)
            .collect();
        ToolSet { title, tools }
    }

    pub fn weighted(&self) -> usize {
        self.tools.iter().filter(|t| t.carries_weight()).count()
    }
}

fn read_tool(node: &Node) -> Option<Tool> {
    let raw = crate::unpack(node.text_of("Raw"))?;
    let object = Reader::new(&raw).object().ok()?;
    let mut annotation = object.as_dict()?.clone();

    // `/Measure /BBObjPtr_JVPEDGAIWQDIDIDL` points at a shared object kept
    // beside the tool. Splice those in so the tool stands on its own.
    let resources = collect_resources(node);
    if !resources.is_empty() {
        resolve(&mut annotation, &resources, 0);
    }

    let subject = text_value(&annotation, "Subj");
    let label = text_value(&annotation, "Label");
    let kind = Kind::read(&annotation);
    let columns = read_columns(&annotation);
    let colour = read_colour(&annotation);
    let properties_only = node.text_of("Mode").eq_ignore_ascii_case("properties");

    Some(Tool {
        subject,
        label,
        kind,
        annotation,
        columns,
        colour,
        properties_only,
    })
}

fn collect_resources(node: &Node) -> Vec<(String, Object)> {
    let mut out = Vec::new();
    for resources in node.children_named("Resources") {
        // A Resources node may hold several ID/Data pairs in order.
        let ids: Vec<&Node> = resources.children_named("ID").collect();
        let datas: Vec<&Node> = resources.children_named("Data").collect();
        for (id, data) in ids.iter().zip(datas.iter()) {
            let Some(name) = crate::unpack_text(&id.text) else {
                continue;
            };
            let Some(bytes) = crate::unpack(&data.text) else {
                continue;
            };
            if let Ok(object) = Reader::new(&bytes).object() {
                out.push((name.trim().to_string(), object));
            }
        }
    }
    out
}

const POINTER: &str = "BBObjPtr_";

fn resolve(dict: &mut Dict, resources: &[(String, Object)], depth: usize) {
    if depth > 16 {
        return;
    }
    for (_, value) in dict.0.iter_mut() {
        resolve_value(value, resources, depth);
    }
}

fn resolve_value(value: &mut Object, resources: &[(String, Object)], depth: usize) {
    if depth > 16 {
        return;
    }
    match value {
        Object::Name(name) => {
            let text = name.as_str();
            if let Some(id) = text.strip_prefix(POINTER) {
                if let Some((_, found)) = resources.iter().find(|(k, _)| k == id) {
                    *value = found.clone();
                    resolve_value(value, resources, depth + 1);
                }
            }
        }
        Object::Array(items) => {
            for item in items {
                resolve_value(item, resources, depth + 1);
            }
        }
        Object::Dict(inner) => resolve(inner, resources, depth + 1),
        Object::Stream(stream) => resolve(&mut stream.dict, resources, depth + 1),
        _ => {}
    }
}

fn text_value(dict: &Dict, key: &str) -> String {
    dict.get(key).and_then(|o| o.as_text()).unwrap_or_default()
}

fn read_columns(dict: &Dict) -> [String; 6] {
    let mut out: [String; 6] = Default::default();
    let Some(items) = dict.get("BSIColumnData").and_then(|o| o.as_array()) else {
        return out;
    };
    for (i, item) in items.iter().take(6).enumerate() {
        if let Some(text) = item.as_text() {
            out[i] = text;
        }
    }
    out
}

fn read_colour(dict: &Dict) -> [f32; 3] {
    let values = dict.get("C").map(|o| o.numbers()).unwrap_or_default();
    match values.len() {
        1 => {
            let g = values[0] as f32;
            [g, g, g]
        }
        3 => [values[0] as f32, values[1] as f32, values[2] as f32],
        // CMYK, which a few tools use.
        4 => {
            let (c, m, y, k) = (
                values[0] as f32,
                values[1] as f32,
                values[2] as f32,
                values[3] as f32,
            );
            [
                (1.0 - c) * (1.0 - k),
                (1.0 - m) * (1.0 - k),
                (1.0 - y) * (1.0 - k),
            ]
        }
        _ => [0.0, 0.0, 0.0],
    }
}

/// Reads every tool set in a document tree, wherever they sit.
pub fn read_all(root: &Node) -> Vec<ToolSet> {
    let mut nodes = Vec::new();
    root.find_all("BluebeamRevuToolSet", &mut nodes);
    nodes
        .into_iter()
        .map(ToolSet::read)
        .filter(|s| !s.tools.is_empty())
        .collect()
}

/// A tool built from nothing, for tests and for Hyperview's own defaults.
pub fn plain(subject: &str, kind: Kind) -> Tool {
    let mut annotation = Dict::new();
    annotation.set(Name::new("Subj"), Object::text(subject));
    Tool {
        subject: subject.into(),
        label: String::new(),
        kind,
        annotation,
        columns: Default::default(),
        colour: [0.0, 0.0, 0.0],
        properties_only: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict_of(text: &str) -> Dict {
        Reader::new(text.as_bytes())
            .object()
            .unwrap()
            .as_dict()
            .unwrap()
            .clone()
    }

    #[test]
    fn a_weight_of_zero_or_nothing_is_reported_as_no_weight() {
        let mut beam = plain("W12x26", Kind::Length);
        assert_eq!(beam.pounds_per_foot(), None);
        beam.columns[0] = "26.00".into();
        assert_eq!(beam.pounds_per_foot(), Some(26.0));
        beam.columns[0] = "0.00".into();
        assert_eq!(beam.pounds_per_foot(), None, "zero is not a weight");
        beam.columns[0] = "  ".into();
        assert_eq!(beam.pounds_per_foot(), None);
    }

    #[test]
    fn a_plate_reads_its_weight_from_the_area_column_not_the_linear_one() {
        let mut plate = plain("PL1/2", Kind::Area);
        plate.columns[0] = "99".into();
        plate.columns[5] = "20.42".into();
        assert_eq!(plate.pounds_per_foot(), None);
        assert_eq!(plate.pounds_per_square_foot(), Some(20.42));
    }

    #[test]
    fn a_colour_written_as_grey_or_cmyk_still_reads() {
        assert_eq!(read_colour(&dict_of("<</C[0.5]>>")), [0.5, 0.5, 0.5]);
        assert_eq!(read_colour(&dict_of("<</C[1 0 0]>>")), [1.0, 0.0, 0.0]);
        let cmyk = read_colour(&dict_of("<</C[0 1 1 0]>>"));
        assert!((cmyk[0] - 1.0).abs() < 1e-6 && cmyk[1] < 1e-6, "{cmyk:?}");
    }

    #[test]
    fn an_internal_pointer_is_spliced_in_from_the_resources() {
        let mut annotation = dict_of("<</Measure/BBObjPtr_ABC/Other 1>>");
        let resources = vec![(
            "ABC".to_string(),
            Reader::new(b"<</Type/Measure/R(0.25 in = 1 ft)>>")
                .object()
                .unwrap(),
        )];
        resolve(&mut annotation, &resources, 0);
        let measure = annotation.get("Measure").unwrap().as_dict().unwrap();
        assert_eq!(
            measure.get("R").unwrap().as_string(),
            Some(&b"0.25 in = 1 ft"[..])
        );
    }

    #[test]
    fn a_pointer_with_nothing_behind_it_is_left_alone_rather_than_dropped() {
        let mut annotation = dict_of("<</Measure/BBObjPtr_MISSING>>");
        resolve(&mut annotation, &[], 0);
        assert!(annotation.get("Measure").unwrap().is_name("BBObjPtr_MISSING"));
    }
}
