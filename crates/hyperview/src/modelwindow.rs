//! File ▸ Open Model: a steel model's tonnage, straight from the model.
//!
//! A detailer's IFC already knows every piece and what it weighs. This reads
//! it (see the `model` crate) and shows it the way an estimator totals a job:
//! pieces, feet, pounds and tons by profile, every part with where its numbers
//! came from, and the bolts by size. It draws nothing yet; that comes next.
//!
//! A part the model gives no weight for is counted, named and left out of the
//! pounds, with a warning at the top, the same rule the takeoff follows for a
//! sheet with no scale.

use std::path::PathBuf;
use std::sync::mpsc;

use egui::{Color32, RichText};

use crate::app::App;

/// Which list the window shows.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Showing {
    #[default]
    ByProfile,
    Parts,
    Bolts,
}

/// One model on screen.
pub struct Opened {
    pub path: PathBuf,
    pub model: model::Model,
    pub open: bool,
    pub showing: Showing,
    /// Narrows the parts list: a profile, a mark, a name.
    pub filter: String,
}

/// Models being read, off the window's thread.
#[derive(Default)]
pub struct Models {
    pub opened: Vec<Opened>,
    pub reading: Vec<(PathBuf, mpsc::Receiver<Result<model::Model, String>>)>,
}

impl App {
    /// File ▸ Open Model (IFC)…
    pub fn pick_and_open_model(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Models (IFC)", &["ifc", "IFC"])
            .pick_file()
        {
            self.open_model(path);
        }
    }

    /// Reads a model on its own thread; a big one takes a few seconds and the
    /// window shouldn't stop while it does.
    pub fn open_model(&mut self, path: PathBuf) {
        if let Some(already) = self.models.opened.iter_mut().find(|m| m.path == path) {
            already.open = true;
            return;
        }
        let (tx, rx) = mpsc::channel();
        let reading = path.clone();
        std::thread::Builder::new()
            .name("reading a model".into())
            .spawn(move || {
                let read = std::fs::read(&reading)
                    .map_err(|e| format!("{}: {e}", reading.display()))
                    .and_then(|bytes| model::Model::read(&bytes));
                let _ = tx.send(read);
            })
            .ok();
        self.status = format!("Reading {}…", name_of(&path));
        self.models.reading.push((path, rx));
    }

    /// The model windows, every frame.
    pub fn model_windows(&mut self, ctx: &egui::Context) {
        let mut arrived = Vec::new();
        self.models.reading.retain(|(path, rx)| match rx.try_recv() {
            Ok(read) => {
                arrived.push((path.clone(), read));
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => false,
        });
        if !self.models.reading.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        for (path, read) in arrived {
            match read {
                Ok(model) => {
                    self.status = format!(
                        "{}: {} parts, {:.2} tons.",
                        name_of(&path),
                        model.parts.len(),
                        model.tons()
                    );
                    self.models.opened.push(Opened {
                        path,
                        model,
                        open: true,
                        showing: Showing::ByProfile,
                        filter: String::new(),
                    });
                }
                Err(why) => self.error = Some(why),
            }
        }

        let theme = self.chrome.theme;
        let mut saves: Vec<(String, String, Vec<u8>)> = Vec::new();
        for (index, opened) in self.models.opened.iter_mut().enumerate() {
            if !opened.open {
                continue;
            }
            let model = &opened.model;
            let mut open = true;
            egui::Window::new(format!("Model takeoff: {}", name_of(&opened.path)))
                .id(egui::Id::new(("model takeoff", index)))
                .collapsible(true)
                .resizable(true)
                .default_width(760.0)
                .default_height(560.0)
                .open(&mut open)
                .show(ctx, |ui| {
                    let wrote = [model.made_by.split(',').next().unwrap_or_default(), &model.schema]
                        .into_iter()
                        .filter(|s| !s.trim().is_empty())
                        .collect::<Vec<_>>()
                        .join(" · ");
                    if !wrote.is_empty() {
                        ui.label(RichText::new(wrote).color(theme.faint).size(11.0));
                    }
                    ui.label(
                        RichText::new(format!(
                            "{} parts in {} assemblies, {} bolts. {} lb, {:.2} tons.",
                            model.parts.len(),
                            model.assemblies(),
                            model.bolts.len(),
                            model::thousands(model.pounds()),
                            model.tons()
                        ))
                        .strong()
                        .size(13.0),
                    );
                    let unweighed = model.unweighed();
                    if unweighed > 0 {
                        ui.colored_label(
                            theme.warn,
                            format!(
                                "{unweighed} part{} {} no weight in the model and {} left out of the total. \
                                 They're marked in the lists below.",
                                if unweighed == 1 { "" } else { "s" },
                                if unweighed == 1 { "has" } else { "have" },
                                if unweighed == 1 { "is" } else { "are" },
                            ),
                        );
                    }
                    ui.label(
                        RichText::new(
                            "Weights are the model's own, from its base quantities or the \
                             detailing program's. Nothing here is looked up from a table.",
                        )
                        .color(theme.faint)
                        .size(10.5),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut opened.showing, Showing::ByProfile, "By profile");
                        ui.selectable_value(&mut opened.showing, Showing::Parts, "Every part");
                        ui.selectable_value(&mut opened.showing, Showing::Bolts, "Bolts");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Save the parts list…").clicked() {
                                saves.push((
                                    format!("{} parts.csv", stem_of(&opened.path)),
                                    "CSV".into(),
                                    model.parts_csv().into_bytes(),
                                ));
                            }
                            if ui.button("Save as CSV…").clicked() {
                                saves.push((
                                    format!("{} tonnage.csv", stem_of(&opened.path)),
                                    "CSV".into(),
                                    model.by_profile_csv().into_bytes(),
                                ));
                            }
                        });
                    });
                    ui.separator();
                    match opened.showing {
                        Showing::ByProfile => by_profile(ui, model, theme.warn),
                        Showing::Parts => parts(ui, model, &mut opened.filter, theme.warn),
                        Showing::Bolts => bolts(ui, model),
                    }
                });
            opened.open = open;
        }
        self.models.opened.retain(|m| m.open);
        for (suggested, filter, bytes) in saves {
            self.save_model_file(&suggested, &filter, bytes);
        }
    }

    fn save_model_file(&mut self, suggested: &str, filter: &str, bytes: Vec<u8>) {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(suggested)
            .add_filter(filter, &["csv"])
            .save_file()
        else {
            return;
        };
        match std::fs::write(&path, bytes) {
            Ok(()) => self.status = format!("Written to {}", path.display()),
            Err(e) => self.error = Some(format!("Could not write that file: {e}")),
        }
    }
}

fn by_profile(ui: &mut egui::Ui, model: &model::Model, warn: Color32) {
    let rows = model.by_profile();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("model by profile").striped(true).num_columns(6).show(ui, |ui| {
            for heading in ["Kind", "Profile", "Pieces", "Feet", "Pounds", "Tons"] {
                ui.label(RichText::new(heading).strong().size(11.0));
            }
            ui.end_row();
            for row in &rows {
                ui.label(row.kind.name());
                ui.label(&row.profile);
                ui.label(row.pieces.to_string());
                ui.label(if row.unmeasured == row.pieces { "—".into() } else { format!("{:.1}", row.feet) });
                if row.unweighed > 0 {
                    ui.colored_label(
                        warn,
                        format!("{} ({} unweighed)", model::thousands(row.pounds), row.unweighed),
                    );
                } else {
                    ui.label(model::thousands(row.pounds));
                }
                ui.label(format!("{:.3}", row.pounds / 2000.0));
                ui.end_row();
            }
            ui.label(RichText::new("Total").strong());
            ui.label("");
            ui.label(RichText::new(model.parts.len().to_string()).strong());
            ui.label("");
            ui.label(RichText::new(model::thousands(model.pounds())).strong());
            ui.label(RichText::new(format!("{:.3}", model.tons())).strong());
            ui.end_row();
        });
    });
}

fn parts(ui: &mut egui::Ui, model: &model::Model, filter: &mut String, warn: Color32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Show only").size(11.0));
        ui.add(egui::TextEdit::singleline(filter).hint_text("a profile, mark or name").desired_width(220.0));
    });
    let wanted = filter.trim().to_lowercase();
    let shown: Vec<&model::Part> = model
        .parts
        .iter()
        .filter(|p| {
            wanted.is_empty()
                || [&p.profile, &p.mark, &p.assembly, &p.name, &p.material]
                    .iter()
                    .any(|s| s.to_lowercase().contains(&wanted))
        })
        .collect();
    ui.label(RichText::new(format!("{} of {} parts", shown.len(), model.parts.len())).size(10.5));
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("model parts").striped(true).num_columns(8).show(ui, |ui| {
            for heading in ["Kind", "Profile", "Mark", "Assembly", "Material", "Feet", "Pounds", "Weight from"] {
                ui.label(RichText::new(heading).strong().size(11.0));
            }
            ui.end_row();
            for p in shown {
                ui.label(p.kind.name());
                ui.label(&p.profile);
                ui.label(&p.mark);
                ui.label(&p.assembly);
                ui.label(&p.material);
                ui.label(p.feet.map(|f| format!("{f:.2}")).unwrap_or_else(|| "—".into()));
                match p.pounds {
                    Some(lb) => ui.label(format!("{lb:.1}")),
                    None => ui.colored_label(warn, "none"),
                };
                ui.label(RichText::new(p.weight_from.name()).size(10.5));
                ui.end_row();
            }
        });
    });
}

fn bolts(ui: &mut egui::Ui, model: &model::Model) {
    let rows = model.bolts_by_size();
    if rows.is_empty() {
        ui.label("The model has no bolts in it.");
        return;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("model bolts").striped(true).num_columns(2).show(ui, |ui| {
            ui.label(RichText::new("Size").strong().size(11.0));
            ui.label(RichText::new("Count").strong().size(11.0));
            ui.end_row();
            for (size, count) in rows {
                ui.label(size);
                ui.label(count.to_string());
                ui.end_row();
            }
        });
    });
}

fn name_of(path: &std::path::Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().to_string()
}

fn stem_of(path: &std::path::Path) -> String {
    path.file_stem().unwrap_or_default().to_string_lossy().to_string()
}
