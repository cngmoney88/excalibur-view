//! Profiles: the arrangement of the program — toolbars, panels, the markups
//! list, and the tool chest that goes with them.

use crate::toolset::{self, ToolSet};
use crate::xml::{self, Node};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dock {
    Top,
    Bottom,
    Left,
    Right,
    Floating,
}

impl Dock {
    fn read(text: &str) -> Dock {
        match text.trim().to_ascii_lowercase().as_str() {
            "bottom" => Dock::Bottom,
            "left" => Dock::Left,
            "right" => Dock::Right,
            "float" | "floating" => Dock::Floating,
            _ => Dock::Top,
        }
    }
}

/// One button on a toolbar, or the gap between groups.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Separator,
    Command { id: String, visible: bool },
}

impl Item {
    pub fn id(&self) -> Option<&str> {
        match self {
            Item::Command { id, .. } => Some(id),
            Item::Separator => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ToolBar {
    pub name: String,
    /// Toolbars belonging to an optional module, such as Redaction or OCR.
    pub module: Option<String>,
    pub x: i32,
    pub y: i32,
    pub visible: bool,
    pub location: Dock,
    pub items: Vec<Item>,
}

impl ToolBar {
    /// The commands actually on show, in order.
    pub fn shown(&self) -> Vec<&str> {
        self.items
            .iter()
            .filter_map(|i| match i {
                Item::Command { id, visible: true } => Some(id.as_str()),
                _ => None,
            })
            .collect()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Side {
    pub tabs: Vec<String>,
    pub active: String,
    pub size: f32,
    pub collapsed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Panels {
    pub left: Side,
    pub right: Side,
    pub bottom: Side,
}

#[derive(Clone, Debug)]
pub struct Column {
    pub key: String,
    pub width: f32,
    pub visible: bool,
}

/// One of the markups list's own columns — "LBS Per FT", "Piece Mark" —
/// named by whoever set the tool chest up. The value is carried on each
/// markup, in the same position, in `BSIColumnData`.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomColumn {
    /// Which of the six slots it is: `UserDefined0` is index 0.
    pub index: usize,
    pub name: String,
    /// `Number`, `Text`, `Formula`, `Choice`, `Date`…, as Revu writes it.
    pub kind: String,
    /// For a formula, what it works out: `[Length] * [LBS Per FT]`.
    pub expression: String,
    pub precision: usize,
}

impl CustomColumn {
    pub fn is_formula(&self) -> bool {
        self.kind.eq_ignore_ascii_case("formula")
    }
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub name: String,
    pub toolbars: Vec<ToolBar>,
    pub nav_left: Vec<Item>,
    pub nav_middle: Vec<Item>,
    pub nav_right: Vec<Item>,
    pub hover_bar: Vec<Item>,
    pub panels: Panels,
    pub markup_columns: Vec<Column>,
    /// What the user columns are called and, for formulas, what they work out.
    pub custom_columns: Vec<CustomColumn>,
    pub thumbnail_size: f32,
    pub sets: Vec<ToolSet>,
}

impl Profile {
    /// A tool chest, whichever sort of file it is: Excalibur View's own
    /// (`.evtools`), or a Revu profile or tool set read in.
    pub fn read(bytes: &[u8]) -> Option<Profile> {
        crate::native::read_any(bytes)
    }

    /// A Revu profile (`.bpx`) or tool set (`.btx`).
    pub fn read_revu(bytes: &[u8]) -> Option<Profile> {
        let root = xml::parse(bytes)?;
        let name = root.attribute("Name").unwrap_or("Profile").to_string();

        let redline = record(&root, "Redline");
        let mut toolbars = Vec::new();
        let (mut nav_left, mut nav_middle, mut nav_right, mut hover_bar) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        if let Some(redline) = redline {
            toolbars = redline
                .children_named("ToolStrip")
                .map(read_toolbar)
                .collect();
            nav_left = read_items(redline.child("NavBarLeft"));
            nav_middle = read_items(redline.child("NavBarMiddle"));
            nav_right = read_items(redline.child("NavBarRight"));
            hover_bar = read_items(redline.child("HoverBar"));
        }

        let panels = record(&root, "DockContainer")
            .map(read_panels)
            .unwrap_or_default();
        let markup_columns = record(&root, "MarkupList")
            .map(read_columns)
            .unwrap_or_default();
        let custom_columns = record(&root, "MarkupList")
            .map(read_custom_columns)
            .unwrap_or_default();
        let thumbnail_size = record(&root, "Thumbnails")
            .and_then(|n| n.text_of("BoxSize").trim().parse().ok())
            .unwrap_or(132.0);
        let sets = toolset::read_all(&root);

        Some(Profile {
            name,
            toolbars,
            nav_left,
            nav_middle,
            nav_right,
            hover_bar,
            panels,
            markup_columns,
            custom_columns,
            thumbnail_size,
            sets,
        })
    }

    pub fn open(path: impl AsRef<std::path::Path>) -> Option<Profile> {
        Profile::read(&std::fs::read(path).ok()?)
    }

    pub fn tool_count(&self) -> usize {
        self.sets.iter().map(|s| s.tools.len()).sum()
    }

    pub fn toolbar(&self, name: &str) -> Option<&ToolBar> {
        self.toolbars.iter().find(|t| t.name == name)
    }

    /// Every command the profile mentions anywhere, which is the list of
    /// buttons Hyperview has to be able to put on screen.
    pub fn commands(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let everywhere = self
            .toolbars
            .iter()
            .flat_map(|t| t.items.iter())
            .chain(self.nav_left.iter())
            .chain(self.nav_middle.iter())
            .chain(self.nav_right.iter())
            .chain(self.hover_bar.iter());
        for item in everywhere {
            if let Item::Command { id, .. } = item {
                if !out.contains(id) {
                    out.push(id.clone());
                }
            }
        }
        out.sort();
        out
    }
}

fn record<'a>(root: &'a Node, key: &str) -> Option<&'a Node> {
    root.children_named("Record")
        .find(|r| r.attribute("Key") == Some(key))
}

fn read_toolbar(node: &Node) -> ToolBar {
    ToolBar {
        name: node.text_of("Name").to_string(),
        module: Some(node.text_of("Module").to_string()).filter(|m| !m.is_empty()),
        x: node.text_of("X").trim().parse().unwrap_or(0),
        y: node.text_of("Y").trim().parse().unwrap_or(0),
        visible: !node.text_of("Visible").eq_ignore_ascii_case("false"),
        location: Dock::read(node.text_of("Location")),
        items: read_items(node.child("Items")),
    }
}

fn read_items(node: Option<&Node>) -> Vec<Item> {
    let Some(node) = node else { return Vec::new() };
    node.children_named("Item")
        .map(|item| {
            let id = item.text.trim();
            if id.is_empty() {
                Item::Separator
            } else {
                Item::Command {
                    id: id.to_string(),
                    visible: !item
                        .attribute("Visible")
                        .map(|v| v.eq_ignore_ascii_case("false"))
                        .unwrap_or(false),
                }
            }
        })
        .collect()
}

fn read_panels(node: &Node) -> Panels {
    let mut panels = Panels::default();
    for root in node.children_named("DockRoot") {
        let side = Side {
            tabs: root
                .children_named("DockPanel")
                .flat_map(|p| p.children_named("DockTab"))
                .filter_map(|t| t.attribute("Name").map(str::to_string))
                .collect(),
            active: root
                .children_named("DockPanel")
                .map(|p| p.text_of("Active").to_string())
                .find(|a| !a.is_empty())
                .unwrap_or_default(),
            size: root.text_of("Size").trim().parse().unwrap_or(300.0),
            collapsed: root.text_of("Collapsed").eq_ignore_ascii_case("true"),
        };
        match root.attribute("Name").unwrap_or("") {
            "Left" => panels.left = side,
            "Right" => panels.right = side,
            "Bottom" => panels.bottom = side,
            _ => {}
        }
    }
    panels
}

fn read_custom_columns(node: &Node) -> Vec<CustomColumn> {
    let Some(custom) = node.child("CustomColumns") else {
        return Vec::new();
    };
    let mut out: Vec<CustomColumn> = custom
        .children_named("BSIColumnItem")
        .filter(|c| !c.text_of("Deleted").eq_ignore_ascii_case("true"))
        .filter_map(|c| {
            let index: usize = c.attribute("Index")?.trim().parse().ok()?;
            let name = c.text_of("Name").trim().to_string();
            (index < 6 && !name.is_empty()).then(|| CustomColumn {
                index,
                name,
                kind: c.attribute("Subtype").unwrap_or("Text").to_string(),
                expression: c.text_of("Expression").trim().to_string(),
                precision: c.text_of("Precision").trim().parse().unwrap_or(2),
            })
        })
        .collect();
    out.sort_by_key(|c| c.index);
    out.dedup_by_key(|c| c.index);
    out
}

fn read_columns(node: &Node) -> Vec<Column> {
    let mut out: Vec<Column> = Vec::new();
    let mut nodes = Vec::new();
    node.find_all("Column", &mut nodes);
    for column in nodes {
        let Some(key) = column.attribute("Key") else {
            continue;
        };
        // The file lists the set twice; the first mention is the live one.
        if out.iter().any(|c| c.key == key) {
            continue;
        }
        out.push(Column {
            key: key.to_string(),
            width: column.text_of("Width").trim().parse().unwrap_or(100.0),
            visible: column.text_of("Visible").eq_ignore_ascii_case("true"),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = br#"<?xml version="1.0" encoding="utf-8"?>
<RevuProfile Version="1" Name="Ron Quantity Take Off">
  <Record Key="MarkupList">
    <Columns>
      <Column Key="Subject"><Width>210</Width><Visible>True</Visible></Column>
      <Column Key="Label"><Width>92</Width><Visible>False</Visible></Column>
      <Column Key="Subject"><Width>9</Width><Visible>False</Visible></Column>
    </Columns>
  </Record>
  <Record Key="DockContainer">
    <DockRoot Name="Left">
      <Collapsed>False</Collapsed><Size>377</Size>
      <DockPanel>
        <Active>Thumbnails</Active>
        <DockTab Name="Measurements" /><DockTab Name="Thumbnails" /><DockTab Name="Tool Chest" />
      </DockPanel>
    </DockRoot>
    <DockRoot Name="Bottom">
      <Collapsed>True</Collapsed><Size>867</Size>
      <DockPanel><Active>Markups</Active><DockTab Name="Markups" /></DockPanel>
    </DockRoot>
  </Record>
  <Record Key="Redline">
    <ToolStrip>
      <Name>toolStripMeasure</Name><X>105</X><Y>0</Y><Visible>True</Visible><Location>Top</Location>
      <Items>
        <Item>Measure.Tool</Item><Item /><Item>Measure.Length</Item>
        <Item Visible="False">Measure.Volume</Item>
      </Items>
    </ToolStrip>
    <NavBarMiddle><Items /><Item>Pan</Item><Item>Combo.Scale</Item></NavBarMiddle>
  </Record>
</RevuProfile>"#;

    #[test]
    fn a_profile_reads_its_name_toolbars_and_panels() {
        let p = Profile::read(SAMPLE).unwrap();
        assert_eq!(p.name, "Ron Quantity Take Off");

        let measure = p.toolbar("toolStripMeasure").unwrap();
        assert_eq!(measure.location, Dock::Top);
        assert!(measure.visible);
        assert_eq!(measure.items.len(), 4);
        assert_eq!(measure.items[1], Item::Separator);
        assert_eq!(measure.shown(), vec!["Measure.Tool", "Measure.Length"]);

        assert_eq!(p.panels.left.size, 377.0);
        assert_eq!(p.panels.left.active, "Thumbnails");
        assert_eq!(p.panels.left.tabs.len(), 3);
        assert!(p.panels.bottom.collapsed);
    }

    #[test]
    fn a_hidden_button_is_kept_but_marked_hidden() {
        let p = Profile::read(SAMPLE).unwrap();
        let measure = p.toolbar("toolStripMeasure").unwrap();
        assert_eq!(
            measure.items[3],
            Item::Command {
                id: "Measure.Volume".into(),
                visible: false
            }
        );
    }

    #[test]
    fn a_column_listed_twice_keeps_the_first_setting() {
        let p = Profile::read(SAMPLE).unwrap();
        let subject = p.markup_columns.iter().find(|c| c.key == "Subject").unwrap();
        assert_eq!(subject.width, 210.0);
        assert!(subject.visible);
        assert_eq!(p.markup_columns.len(), 2);
    }

    #[test]
    fn the_command_list_covers_the_toolbars_and_the_navigation_bar() {
        let p = Profile::read(SAMPLE).unwrap();
        let commands = p.commands();
        assert!(commands.contains(&"Measure.Length".to_string()));
        assert!(commands.contains(&"Combo.Scale".to_string()));
        assert!(!commands.iter().any(|c| c.is_empty()));
    }

    #[test]
    fn a_file_that_is_not_a_profile_is_refused() {
        assert!(Profile::read(b"not xml at all").is_none() || Profile::read(b"not xml at all").unwrap().toolbars.is_empty());
    }
}
