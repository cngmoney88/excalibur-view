//! The rest of the panels on the rail.
//!
//! Spaces, Flags, Hyperlinks, Bookmarks, Layers, Forms, Signatures and Sets.
//! Each of them is a list of something the drawing already holds, which is
//! why they are together: the work in every one is reading the file honestly
//! and saying plainly what is not there.

use crate::app::App;
use egui::RichText;

/// One named region of a drawing.
///
/// A space is how a takeoff gets broken down by room, by bay, by pour: mark
/// one out, name it, and every measurement inside it belongs to it. Kept as an
/// ordinary Polygon markup carrying `/IT /Space`, so a set marked up here
/// opens in Revu with the spaces still on it.
#[derive(Clone, Debug, PartialEq)]
pub struct Space {
    /// Which markup, by its place in the list.
    pub markup: usize,
    pub page: u32,
    pub name: String,
    pub outline: Vec<[f64; 2]>,
}

impl Space {
    /// Whether a point is inside this space.
    pub fn holds(&self, at: [f64; 2]) -> bool {
        inside(&self.outline, at)
    }
}

/// Whether a point is inside a closed loop, by the crossing rule.
pub fn inside(ring: &[[f64; 2]], at: [f64; 2]) -> bool {
    if ring.len() < 3 {
        return false;
    }
    let mut within = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[j]);
        if (a[1] > at[1]) != (b[1] > at[1]) {
            let span = b[1] - a[1];
            if span.abs() > f64::EPSILON {
                let x = a[0] + (at[1] - a[1]) / span * (b[0] - a[0]);
                if at[0] < x {
                    within = !within;
                }
            }
        }
        j = i;
    }
    within
}

/// Whether a markup is a space.
pub fn is_space(markup: &annot::Markup) -> bool {
    markup
        .dict
        .get("IT")
        .and_then(|o| o.as_name())
        .map(|n| n.as_str() == "Space")
        .unwrap_or(false)
}

/// Which space a measurement belongs to.
///
/// The smallest one containing its middle, so a bay inside a floor counts to
/// the bay rather than to both. A measurement in no space at all belongs to
/// none, and is reported that way rather than being quietly put in the first
/// one.
pub fn space_for(spaces: &[Space], page: u32, middle: [f64; 2]) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (at, space) in spaces.iter().enumerate() {
        if space.page != page || !space.holds(middle) {
            continue;
        }
        let area = takeoff::geometry::area(&space.outline).abs();
        if best.map(|(_, smallest)| area < smallest).unwrap_or(true) {
            best = Some((at, area));
        }
    }
    best.map(|(at, _)| at)
}

impl App {
    /// Every space on the drawing, in the order they were marked out.
    pub fn spaces(&self) -> Vec<Space> {
        let Some(doc) = self.doc() else {
            return Vec::new();
        };
        doc.marks
            .iter()
            .enumerate()
            .filter(|(_, m)| !m.gone && is_space(&m.markup))
            .map(|(at, m)| {
                let frame = doc.frame_of(m.page);
                Space {
                    markup: at,
                    page: m.page,
                    name: {
                        let named = m.markup.contents();
                        if named.trim().is_empty() {
                            m.markup.subject()
                        } else {
                            named
                        }
                    },
                    outline: m.on_sheet(&frame),
                }
            })
            .collect()
    }

    pub fn spaces_panel(&mut self, ui: &mut egui::Ui) {
        let theme = self.chrome.theme;
        let spaces = self.spaces();
        if spaces.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "No spaces marked out yet. Pick Add Space from the Tools menu and \
                     click round a room, a bay or a pour. Every measurement inside it \
                     then belongs to it, and the Markups list totals by space.",
                )
                .color(theme.faint)
                .size(11.0),
            );
            return;
        }

        // How much is in each, counted from the markups themselves.
        let counts = self.what_is_in_each_space(&spaces);
        let mut go_to: Option<usize> = None;
        let mut rename: Option<(usize, String)> = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (at, space) in spaces.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let mut name = space.name.clone();
                        if name.trim().is_empty() {
                            name = format!("Space {}", at + 1);
                        }
                        if ui.selectable_label(false, RichText::new(&name).size(12.0)).clicked() {
                            go_to = Some(space.markup);
                        }
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} markup{}",
                                        counts[at],
                                        if counts[at] == 1 { "" } else { "s" }
                                    ))
                                    .color(theme.faint)
                                    .size(10.0),
                                );
                            },
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("sheet {}", space.page + 1))
                                .color(theme.faint)
                                .size(10.0),
                        );
                        if ui.small_button("Rename").clicked() {
                            rename = Some((space.markup, space.name.clone()));
                        }
                    });
                    ui.add_space(4.0);
                }
                let loose = self.measurements_in_no_space(&spaces);
                if loose > 0 {
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "{loose} measurement{} in no space at all. They are in the \
                             totals, and in none of the spaces above.",
                            if loose == 1 { "" } else { "s" }
                        ))
                        .color(theme.warn)
                        .size(10.0),
                    );
                }
            });

        if let Some(index) = go_to {
            let page = self.doc().and_then(|d| d.marks.get(index)).map(|m| m.page);
            if let Some(page) = page {
                self.go_to(page);
            }
            if let Some(doc) = self.doc_mut() {
                doc.choose(Some(index));
            }
        }
        if let Some((index, was)) = rename {
            self.editing_text = Some(index);
            let _ = was;
        }
    }

    /// How many measurements fall inside each space.
    pub fn what_is_in_each_space(&self, spaces: &[Space]) -> Vec<usize> {
        let mut counts = vec![0usize; spaces.len()];
        let Some(doc) = self.doc() else { return counts };
        for mark in doc.marks.iter().filter(|m| !m.gone) {
            if is_space(&mark.markup) || !mark.markup.kind().measures() {
                continue;
            }
            let frame = doc.frame_of(mark.page);
            let points = mark.on_sheet(&frame);
            let Some(middle) = middle_of(&points) else { continue };
            if let Some(at) = space_for(spaces, mark.page, middle) {
                counts[at] += 1;
            }
        }
        counts
    }

    fn measurements_in_no_space(&self, spaces: &[Space]) -> usize {
        let Some(doc) = self.doc() else { return 0 };
        doc.marks
            .iter()
            .filter(|m| !m.gone && !is_space(&m.markup) && m.markup.kind().measures())
            .filter(|m| {
                let frame = doc.frame_of(m.page);
                let points = m.on_sheet(&frame);
                match middle_of(&points) {
                    Some(middle) => space_for(spaces, m.page, middle).is_none(),
                    None => true,
                }
            })
            .count()
    }
}

/// The middle of a run of points.
pub fn middle_of(points: &[[f64; 2]]) -> Option<[f64; 2]> {
    if points.is_empty() {
        return None;
    }
    let n = points.len() as f64;
    Some([
        points.iter().map(|p| p[0]).sum::<f64>() / n,
        points.iter().map(|p| p[1]).sum::<f64>() / n,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_space(name: &str, page: u32, outline: Vec<[f64; 2]>) -> Space {
        Space {
            markup: 0,
            page,
            name: name.into(),
            outline,
        }
    }

    fn box_(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
        vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    #[test]
    fn a_measurement_inside_a_space_belongs_to_it() {
        let spaces = vec![a_space("Bay 1", 0, box_(0.0, 0.0, 100.0, 100.0))];
        assert_eq!(space_for(&spaces, 0, [50.0, 50.0]), Some(0));
        assert_eq!(space_for(&spaces, 0, [500.0, 50.0]), None);
    }

    #[test]
    fn a_space_on_another_sheet_does_not_claim_it() {
        let spaces = vec![a_space("Bay 1", 3, box_(0.0, 0.0, 100.0, 100.0))];
        assert_eq!(space_for(&spaces, 0, [50.0, 50.0]), None);
        assert_eq!(space_for(&spaces, 3, [50.0, 50.0]), Some(0));
    }

    #[test]
    fn a_bay_inside_a_floor_counts_to_the_bay() {
        // Otherwise every quantity is counted twice the moment somebody marks
        // out both, which is exactly what people do.
        let spaces = vec![
            a_space("Floor", 0, box_(0.0, 0.0, 1000.0, 1000.0)),
            a_space("Bay 1", 0, box_(0.0, 0.0, 100.0, 100.0)),
        ];
        assert_eq!(space_for(&spaces, 0, [50.0, 50.0]), Some(1));
        assert_eq!(space_for(&spaces, 0, [500.0, 500.0]), Some(0));
    }

    #[test]
    fn a_measurement_in_no_space_belongs_to_none_rather_than_to_the_first() {
        let spaces = vec![a_space("Bay 1", 0, box_(0.0, 0.0, 100.0, 100.0))];
        assert_eq!(space_for(&spaces, 0, [900.0, 900.0]), None);
    }

    #[test]
    fn an_l_shaped_space_does_not_claim_the_notch() {
        let l = vec![
            [0.0, 0.0],
            [100.0, 0.0],
            [100.0, 40.0],
            [40.0, 40.0],
            [40.0, 100.0],
            [0.0, 100.0],
        ];
        let spaces = vec![a_space("L", 0, l)];
        assert_eq!(space_for(&spaces, 0, [20.0, 20.0]), Some(0));
        assert_eq!(space_for(&spaces, 0, [80.0, 80.0]), None, "the notch is outside");
    }

    #[test]
    fn a_space_is_told_apart_from_an_ordinary_shape() {
        let mut space = annot::Markup::new(annot::Subtype::Polygon);
        space.set("IT", pdf::Object::name("Space"));
        assert!(is_space(&space));
        assert!(!is_space(&annot::Markup::new(annot::Subtype::Polygon)));
    }
}

// ---- layers ----------------------------------------------------------------

/// One optional content group: a layer of the drawing that can be turned off.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub reference: pdf::Ref,
    pub name: String,
    pub on: bool,
}

/// Every layer a drawing declares, and whether it is on.
///
/// A drawing set out of Revit or AutoCAD often has them: the grid, the
/// dimensions, the notes, the background architectural. Being able to turn the
/// architectural background off is the difference between reading a congested
/// sheet and not.
pub fn layers_of(file: &pdf::Document) -> Vec<Layer> {
    let catalog = file.catalog();
    let Some(properties) = file
        .follow(catalog.get("OCProperties").unwrap_or(&pdf::Object::Null))
        .as_dict()
        .cloned()
    else {
        return Vec::new();
    };
    let all = file.follow(properties.get("OCGs").unwrap_or(&pdf::Object::Null));
    let Some(all) = all.as_array() else {
        return Vec::new();
    };

    // The default view says which are off. Anything not named there is on.
    let default = file.follow(properties.get("D").unwrap_or(&pdf::Object::Null));
    let off: std::collections::HashSet<pdf::Ref> = default
        .as_dict()
        .map(|d| file.follow(d.get("OFF").unwrap_or(&pdf::Object::Null)))
        .and_then(|o| o.as_array().map(|a| a.iter().filter_map(|o| o.as_ref()).collect()))
        .unwrap_or_default();

    all.iter()
        .filter_map(|item| {
            let reference = item.as_ref()?;
            let group = file.get(reference);
            let name = group
                .as_dict()
                .and_then(|d| d.get("Name"))
                .and_then(|o| o.as_text())
                .unwrap_or_else(|| format!("Layer {}", reference.number));
            Some(Layer {
                reference,
                name,
                on: !off.contains(&reference),
            })
        })
        .collect()
}

/// Writes which layers are on into the drawing's own default view.
///
/// This is a real change to the file rather than a setting in this program:
/// a sheet sent on with the architectural background turned off arrives that
/// way for whoever opens it, which is usually the point.
pub fn write_layers(file: &pdf::Document, update: &mut pdf::write::Update, layers: &[Layer]) {
    let Some(catalog_ref) = file.xref.trailer.get("Root").and_then(|o| o.as_ref()) else {
        return;
    };
    let catalog = file.catalog();
    let Some(properties_ref) = catalog.get("OCProperties").and_then(|o| o.as_ref()) else {
        // Inline rather than its own object: rewrite the catalog.
        let mut catalog = catalog.clone();
        let Some(mut properties) = file
            .follow(catalog.get("OCProperties").unwrap_or(&pdf::Object::Null))
            .as_dict()
            .cloned()
        else {
            return;
        };
        set_default_view(file, &mut properties, layers);
        catalog.set("OCProperties", pdf::Object::Dict(properties));
        update.replace(catalog_ref, pdf::Object::Dict(catalog));
        return;
    };
    let object = file.get(properties_ref);
    let Some(mut properties) = object.as_dict().cloned() else {
        return;
    };
    set_default_view(file, &mut properties, layers);
    update.replace(properties_ref, pdf::Object::Dict(properties));
}

fn set_default_view(file: &pdf::Document, properties: &mut pdf::Dict, layers: &[Layer]) {
    let mut default = file
        .follow(properties.get("D").unwrap_or(&pdf::Object::Null))
        .as_dict()
        .cloned()
        .unwrap_or_default();
    let off: Vec<pdf::Object> = layers
        .iter()
        .filter(|l| !l.on)
        .map(|l| pdf::Object::Ref(l.reference))
        .collect();
    let on: Vec<pdf::Object> = layers
        .iter()
        .filter(|l| l.on)
        .map(|l| pdf::Object::Ref(l.reference))
        .collect();
    default.set("OFF", pdf::Object::Array(off));
    default.set("ON", pdf::Object::Array(on));
    // Base state on, with the off list naming the exceptions, which is what
    // every writer produces and what every reader expects.
    default.set("BaseState", pdf::Object::name("ON"));
    properties.set("D", pdf::Object::Dict(default));
}

// ---- bookmarks -------------------------------------------------------------

/// One entry in a drawing's own table of contents.
#[derive(Clone, Debug, PartialEq)]
pub struct Bookmark {
    pub title: String,
    /// Which sheet it goes to, when it goes to one in this file.
    pub page: Option<u32>,
    /// How far in it is nested.
    pub depth: usize,
}

/// A drawing's bookmarks, flattened with their depth kept.
pub fn bookmarks_of(file: &pdf::Document) -> Vec<Bookmark> {
    let catalog = file.catalog();
    let Some(outlines) = file
        .follow(catalog.get("Outlines").unwrap_or(&pdf::Object::Null))
        .as_dict()
        .cloned()
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let pages = file.pages();
    let mut at = outlines.get("First").and_then(|o| o.as_ref());
    let mut seen = 0usize;
    walk_bookmarks(file, &pages, &mut at.take(), 0, &mut out, &mut seen);
    out
}

fn walk_bookmarks(
    file: &pdf::Document,
    pages: &[pdf::Ref],
    start: &mut Option<pdf::Ref>,
    depth: usize,
    out: &mut Vec<Bookmark>,
    seen: &mut usize,
) {
    let mut at = *start;
    // A file can be built with a loop in its outline, and a reader that
    // followed it would never finish.
    while let Some(reference) = at {
        *seen += 1;
        if *seen > 5000 || depth > 12 {
            return;
        }
        let object = file.get(reference);
        let Some(dict) = object.as_dict() else { return };
        let title = dict
            .get("Title")
            .and_then(|o| o.as_text())
            .unwrap_or_default();
        out.push(Bookmark {
            title,
            page: page_of_destination(file, pages, dict),
            depth,
        });
        if let Some(first) = dict.get("First").and_then(|o| o.as_ref()) {
            let mut child = Some(first);
            walk_bookmarks(file, pages, &mut child, depth + 1, out, seen);
        }
        at = dict.get("Next").and_then(|o| o.as_ref());
    }
}

/// Which sheet a bookmark goes to.
fn page_of_destination(
    file: &pdf::Document,
    pages: &[pdf::Ref],
    dict: &pdf::Dict,
) -> Option<u32> {
    // Either a destination outright, or an action carrying one.
    let destination = match dict.get("Dest") {
        Some(dest) => file.follow(dest),
        None => {
            let action = file.follow(dict.get("A")?);
            let action = action.as_dict()?;
            file.follow(action.get("D")?)
        }
    };
    let array = destination.as_array()?;
    let target = array.first()?.as_ref()?;
    pages.iter().position(|p| *p == target).map(|at| at as u32)
}

// ---- the panels themselves -------------------------------------------------

impl App {
    pub fn layers_panel(&mut self, ui: &mut egui::Ui) {
        let theme = self.chrome.theme;
        let layers = self.doc().map(|d| layers_of(&d.file)).unwrap_or_default();
        if layers.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "This drawing has no layers in it. A set out of Revit or AutoCAD \
                     usually does; one that has been through a plotter usually does not.",
                )
                .color(theme.faint)
                .size(11.0),
            );
            return;
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Turning one off changes the drawing's own default, so a sheet sent on \
                 with the architectural background off arrives that way. It is saved \
                 with everything else.",
            )
            .color(theme.faint)
            .size(10.0),
        );
        ui.add_space(8.0);

        let mut changed: Option<Vec<Layer>> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut now = layers.clone();
                for layer in now.iter_mut() {
                    if ui.checkbox(&mut layer.on, &layer.name).changed() {
                        changed = Some(Vec::new());
                    }
                }
                if changed.is_some() {
                    changed = Some(now);
                }
            });

        if let Some(now) = changed {
            self.set_layers(&now);
        }
    }

    /// Writes which layers are on, and re-reads the drawing so the change
    /// shows.
    fn set_layers(&mut self, layers: &[Layer]) {
        let Some(doc) = self.doc() else { return };
        if doc.read_only {
            self.error = Some(
                "This drawing cannot be written to, so its layers cannot be changed. \
                 Save a copy somewhere you can write and change that."
                    .into(),
            );
            return;
        }
        let path = doc.path.clone();
        let mut update = pdf::write::Update::new(&doc.file);
        write_layers(&doc.file, &mut update, layers);
        let bytes = update.apply(&doc.file);
        if let Err(why) = std::fs::write(&path, bytes) {
            self.error = Some(format!("could not save the layer change: {why}"));
            return;
        }
        let off = layers.iter().filter(|l| !l.on).count();
        self.status = if off == 0 {
            "Every layer is on.".into()
        } else {
            format!(
                "{off} layer{} turned off, and saved into the drawing.",
                if off == 1 { "" } else { "s" }
            )
        };
        self.reopen(&path);
    }

    pub fn bookmarks_panel(&mut self, ui: &mut egui::Ui) {
        let theme = self.chrome.theme;
        let marks = self.doc().map(|d| bookmarks_of(&d.file)).unwrap_or_default();
        let sheets = self.doc().map(|d| d.pages.len()).unwrap_or(0);
        if marks.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "This drawing has no bookmarks. A set combined out of separate \
                     files usually does; a plotted one usually does not.",
                )
                .color(theme.faint)
                .size(11.0),
            );
            ui.add_space(8.0);
            if ui
                .button("Make one per sheet, from the sheet numbers")
                .on_hover_text(
                    "Uses the numbers Excalibur View read off the drawings, so the list \
                     reads S-201 rather than 7.",
                )
                .clicked()
            {
                self.bookmark_every_sheet();
            }
            return;
        }
        let mut go_to: Option<u32> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for mark in &marks {
                    ui.horizontal(|ui| {
                        ui.add_space(mark.depth as f32 * 12.0);
                        let label = if mark.title.trim().is_empty() {
                            "(no title)".to_string()
                        } else {
                            mark.title.clone()
                        };
                        if ui.selectable_label(false, RichText::new(label).size(12.0)).clicked() {
                            go_to = mark.page;
                        }
                        if let Some(page) = mark.page {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!("{}", page + 1))
                                            .color(theme.faint)
                                            .size(10.0),
                                    );
                                },
                            );
                        }
                    });
                }
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("{} of {sheets} sheets", marks.len()))
                        .color(theme.faint)
                        .size(10.0),
                );
            });
        if let Some(page) = go_to {
            self.go_to(page);
        }
    }

    /// Writes one bookmark per sheet, named the way the sheet names itself.
    fn bookmark_every_sheet(&mut self) {
        let Some(doc) = self.doc() else { return };
        if doc.read_only {
            self.error = Some("This drawing cannot be written to.".into());
            return;
        }
        let path = doc.path.clone();
        let titles: Vec<String> = (0..doc.pages.len())
            .map(|page| {
                let label = doc.labels.get(page);
                match label {
                    Some(label) if !label.number.trim().is_empty() => {
                        if label.title.trim().is_empty() {
                            label.number.clone()
                        } else {
                            format!("{} — {}", label.number, label.title)
                        }
                    }
                    _ => format!("Sheet {}", page + 1),
                }
            })
            .collect();

        let file = &doc.file;
        let pages = file.pages();
        let mut update = pdf::write::Update::new(file);
        // Reserved first so each entry can point at the next and the one
        // before it, which is how a PDF's outline is strung together.
        let refs: Vec<pdf::Ref> = titles
            .iter()
            .map(|_| update.add(pdf::Object::Null))
            .collect();
        let outlines_ref = update.add(pdf::Object::Null);
        for (at, title) in titles.iter().enumerate() {
            let Some(page_ref) = pages.get(at).copied() else { continue };
            let mut entry = pdf::Dict::new();
            entry.set("Title", pdf::Object::text(title));
            entry.set("Parent", pdf::Object::Ref(outlines_ref));
            if at > 0 {
                entry.set("Prev", pdf::Object::Ref(refs[at - 1]));
            }
            if at + 1 < refs.len() {
                entry.set("Next", pdf::Object::Ref(refs[at + 1]));
            }
            entry.set(
                "Dest",
                pdf::Object::Array(vec![
                    pdf::Object::Ref(page_ref),
                    pdf::Object::name("Fit"),
                ]),
            );
            update.replace(refs[at], pdf::Object::Dict(entry));
        }
        let mut outlines = pdf::Dict::new();
        outlines.set("Type", pdf::Object::name("Outlines"));
        if let (Some(first), Some(last)) = (refs.first(), refs.last()) {
            outlines.set("First", pdf::Object::Ref(*first));
            outlines.set("Last", pdf::Object::Ref(*last));
        }
        outlines.set("Count", pdf::Object::Int(refs.len() as i64));
        update.replace(outlines_ref, pdf::Object::Dict(outlines));

        if let Some(catalog_ref) = file.xref.trailer.get("Root").and_then(|o| o.as_ref()) {
            let mut catalog = file.catalog();
            catalog.set("Outlines", pdf::Object::Ref(outlines_ref));
            catalog.set("PageMode", pdf::Object::name("UseOutlines"));
            update.replace(catalog_ref, pdf::Object::Dict(catalog));
        }
        let bytes = update.apply(file);
        if let Err(why) = std::fs::write(&path, bytes) {
            self.error = Some(format!("could not save the bookmarks: {why}"));
            return;
        }
        self.status = format!(
            "{} bookmarks written, one per sheet, named the way the sheets name \
             themselves.",
            titles.len()
        );
        self.reopen(&path);
    }

    /// Everything on the drawing of one kind, as a list to click through.
    fn markup_list(
        &mut self,
        ui: &mut egui::Ui,
        empty: &str,
        keep: impl Fn(&annot::Markup) -> bool,
        describe: impl Fn(&annot::Markup) -> String,
    ) {
        let theme = self.chrome.theme;
        let found: Vec<(usize, u32, String)> = match self.doc() {
            Some(doc) => doc
                .marks
                .iter()
                .enumerate()
                .filter(|(_, m)| !m.gone && keep(&m.markup))
                .map(|(at, m)| (at, m.page, describe(&m.markup)))
                .collect(),
            None => Vec::new(),
        };
        if found.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new(empty).color(theme.faint).size(11.0));
            return;
        }
        let mut go_to: Option<usize> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (at, page, what) in &found {
                    ui.horizontal(|ui| {
                        if ui.selectable_label(false, RichText::new(what).size(12.0)).clicked() {
                            go_to = Some(*at);
                        }
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    RichText::new(format!("{}", page + 1))
                                        .color(theme.faint)
                                        .size(10.0),
                                );
                            },
                        );
                    });
                }
            });
        if let Some(index) = go_to {
            let page = self.doc().and_then(|d| d.marks.get(index)).map(|m| m.page);
            if let Some(page) = page {
                self.go_to(page);
            }
            if let Some(doc) = self.doc_mut() {
                doc.choose(Some(index));
            }
        }
    }

    pub fn flags_panel(&mut self, ui: &mut egui::Ui) {
        self.markup_list(
            ui,
            "Nothing flagged. The Flag tool marks something to come back to, and \
             everything flagged shows up here.",
            |m| {
                m.subtype() == annot::Subtype::Text
                    && m.dict
                        .get("Name")
                        .and_then(|o| o.as_name())
                        .map(|n| n.as_str() == "Paragraph")
                        .unwrap_or(false)
            },
            |m| {
                let said = m.contents();
                if said.trim().is_empty() {
                    "Flag".to_string()
                } else {
                    said
                }
            },
        );
    }

    pub fn hyperlinks_panel(&mut self, ui: &mut egui::Ui) {
        self.markup_list(
            ui,
            "No links on this drawing. The Hyperlink tool makes an area of a sheet \
             open something when it is clicked.",
            |m| m.subtype() == annot::Subtype::Link,
            |m| {
                let address = m
                    .dict
                    .get("A")
                    .and_then(|o| o.as_dict())
                    .and_then(|d| d.get("URI"))
                    .and_then(|o| o.as_text())
                    .unwrap_or_else(|| m.contents());
                if address.trim().is_empty() {
                    "A link that goes nowhere".to_string()
                } else {
                    address
                }
            },
        );
    }

    pub fn forms_panel(&mut self, ui: &mut egui::Ui) {
        let theme = self.chrome.theme;
        ui.add_space(4.0);
        let mut editor = self.form_editor;
        if ui
            .checkbox(&mut editor, "Show every field on the sheet")
            .changed()
        {
            self.form_editor = editor;
        }
        ui.add_space(6.0);
        self.markup_list(
            ui,
            "No form fields on this drawing. The Form tools put boxes on a sheet that \
             somebody can fill in — a transmittal, an RFI, an inspection sheet.",
            crate::forms::is_field,
            |m| {
                let name = m
                    .dict
                    .get("T")
                    .and_then(|o| o.as_text())
                    .unwrap_or_default();
                let kind = crate::forms::field_of(m).map(|f| f.name()).unwrap_or("Field");
                format!("{name} · {kind}")
            },
        );
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Fields are saved into the drawing itself, so whoever it is sent to \
                 can fill it in with Acrobat, Revu or a browser.",
            )
            .color(theme.faint)
            .size(10.0),
        );
    }

    pub fn signatures_panel(&mut self, ui: &mut egui::Ui) {
        let theme = self.chrome.theme;
        self.markup_list(
            ui,
            "No places to sign on this drawing. The Signature tool puts one on.",
            |m| {
                crate::forms::field_of(m) == Some(crate::forms::Field::Signature)
            },
            |m| {
                let name = m
                    .dict
                    .get("T")
                    .and_then(|o| o.as_text())
                    .unwrap_or_default();
                let signed = m.dict.has("V");
                format!(
                    "{name} · {}",
                    if signed { "signed" } else { "not signed" }
                )
            },
        );
        ui.add_space(8.0);
        ui.label(
            RichText::new(crate::seal::NO_DIGITAL_SIGNATURE)
                .color(theme.warn)
                .size(10.0),
        );
    }
}
