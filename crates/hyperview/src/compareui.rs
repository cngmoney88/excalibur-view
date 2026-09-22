//! Comparing two issues of a sheet, on screen.
//!
//! The morning job: Rev 2 landed, and the question is what moved. This picks
//! the two sheets, runs the comparison, and shows the differences both as a
//! list you can click through and as marks on the drawing.
//!
//! Then the part that saves the hour: **Cloud these**, which turns the
//! differences into real revision clouds — actual PDF annotations on the newer
//! sheet, which Revu and Acrobat will show and which travel with the file when
//! it goes to a general contractor.
//!
//! What it never does is put a number on any of it. A comparison says "look
//! here"; every quantity in this program still comes from somebody measuring.

use std::path::PathBuf;

use egui::{Color32, RichText};

use crate::app::App;
use crate::compare::Kind;

/// What is being compared, and what came of it.
#[derive(Clone, Debug)]
pub struct Comparing {
    /// The older issue. Empty until chosen.
    pub older: Option<PathBuf>,
    pub older_page: u32,
    /// The newer issue is the drawing that is open, on the sheet being looked
    /// at, because that is the one somebody just received.
    pub newer_page: u32,
    /// How dark a pixel has to be to count as line-work.
    pub darkness: f32,
    pub running: bool,
    pub error: Option<String>,
    /// What was found, once it has been.
    pub found: Option<Found>,
}

#[derive(Clone, Debug)]
pub struct Found {
    pub said: String,
    pub could_not_align: bool,
    pub changes: Vec<(Kind, [f64; 4])>,
    /// Which one is being pointed at.
    pub showing: Option<usize>,
}

impl Comparing {
    pub fn new(page: u32) -> Comparing {
        Comparing {
            older: None,
            older_page: page,
            newer_page: page,
            darkness: 0.65,
            running: false,
            error: None,
            found: None,
        }
    }
}

impl App {
    pub fn begin_compare(&mut self) {
        let Some(doc) = self.doc() else {
            self.status = "Open the newer issue first, then compare it with the older one.".into();
            return;
        };
        self.comparing = Some(Comparing::new(doc.page));
    }

    pub fn compare_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut comparing) = self.comparing.take() else {
            return;
        };
        let theme = self.chrome.theme;
        let Some(doc) = self.doc() else { return };
        let newer = doc.path.clone();
        let sheets = doc.pages.len();

        let mut keep = true;
        let mut go = false;
        let mut pick_older = false;
        let mut cloud = false;
        let mut show: Option<usize> = None;

        egui::Window::new("Compare Documents")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .max_width(740.0)
            .show(ctx, |ui| {
                ui.set_min_width(500.0);

                ui.label(RichText::new("The newer issue").color(theme.faint).size(11.0));
                ui.label(
                    RichText::new(format!(
                        "{} — sheet {} of {sheets}",
                        newer.file_name().unwrap_or_default().to_string_lossy(),
                        comparing.newer_page + 1
                    ))
                    .size(11.0),
                );
                ui.add_space(10.0);

                ui.label(RichText::new("The older issue").color(theme.faint).size(11.0));
                ui.horizontal(|ui| {
                    match &comparing.older {
                        Some(path) => {
                            ui.label(
                                RichText::new(
                                    path.file_name().unwrap_or_default().to_string_lossy(),
                                )
                                .size(11.0),
                            );
                        }
                        None => {
                            ui.label(
                                RichText::new("Nothing chosen yet.")
                                    .color(theme.faint)
                                    .size(11.0),
                            );
                        }
                    }
                    if ui.small_button("Choose…").clicked() {
                        pick_older = true;
                    }
                });
                if comparing.older.is_some() {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Sheet").color(theme.faint).size(11.0));
                        let mut n = comparing.older_page + 1;
                        if ui
                            .add(egui::DragValue::new(&mut n).range(1..=9999))
                            .changed()
                        {
                            comparing.older_page = n.saturating_sub(1);
                        }
                        ui.label(
                            RichText::new("in the older file")
                                .color(theme.faint)
                                .size(10.0),
                        );
                    });
                }

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Line darkness").color(theme.faint).size(11.0));
                    ui.add(egui::Slider::new(&mut comparing.darkness, 0.3..=0.9).show_value(false));
                    ui.label(
                        RichText::new("Raise it for a faint scan.")
                            .color(theme.faint)
                            .size(10.0),
                    );
                });

                // ---- what was found ----------------------------------------
                if let Some(found) = &comparing.found {
                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(&found.said)
                            .color(if found.could_not_align {
                                theme.warn
                            } else {
                                theme.text
                            })
                            .size(11.0),
                    );

                    if !found.changes.is_empty() {
                        ui.add_space(8.0);
                        egui::ScrollArea::vertical()
                            .max_height(180.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                for (n, (kind, area)) in found.changes.iter().enumerate() {
                                    let picked = found.showing == Some(n);
                                    let label = format!(
                                        "{}  {}  ·  {:.0} × {:.0}",
                                        n + 1,
                                        kind.word(),
                                        area[2] - area[0],
                                        area[3] - area[1]
                                    );
                                    if ui
                                        .selectable_label(picked, RichText::new(label).size(11.0))
                                        .clicked()
                                    {
                                        show = Some(n);
                                    }
                                }
                            });
                        ui.add_space(8.0);
                        if ui
                            .button("Cloud these on the drawing")
                            .on_hover_text(
                                "Draws a revision cloud round each difference, as real \
                                 markups. They go in the markups list, they save into the \
                                 file, and Revu will show them.",
                            )
                            .clicked()
                        {
                            cloud = true;
                        }
                    }
                }

                if let Some(error) = &comparing.error {
                    ui.add_space(8.0);
                    ui.colored_label(Color32::from_rgb(235, 120, 120), error);
                }

                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    let ready = comparing.older.is_some() && !comparing.running;
                    if ui.add_enabled(ready, egui::Button::new("Compare")).clicked() {
                        go = true;
                    }
                    if ui.button("Close").clicked() {
                        keep = false;
                    }
                    if comparing.running {
                        ui.add_space(8.0);
                        ui.spinner();
                        ui.label(
                            RichText::new("Looking at both sheets…")
                                .color(theme.faint)
                                .size(11.0),
                        );
                    }
                });
            });

        if pick_older {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Drawing sets (PDF)", &["pdf"])
                .set_title("The older issue")
                .pick_file()
            {
                comparing.older = Some(path);
                comparing.found = None;
            }
        }
        if let Some(n) = show {
            if let Some(found) = comparing.found.as_mut() {
                found.showing = Some(n);
                let area = found.changes[n].1;
                self.centre_on_area(area);
            }
        }
        if go {
            if let Some(older) = comparing.older.clone() {
                comparing.running = true;
                comparing.error = None;
                comparing.found = None;
                self.compare_task += 1;
                let job = self.compare_task;
                self.svc.send(crate::render::ToWorker::Compare {
                    job,
                    older,
                    older_page: comparing.older_page,
                    newer: newer.clone(),
                    newer_page: comparing.newer_page,
                    darkness: comparing.darkness,
                });
            }
        }
        if cloud {
            if let Some(found) = comparing.found.clone() {
                let made = self.cloud_the_changes(&found.changes);
                self.status = made;
            }
        }
        if keep {
            self.comparing = Some(comparing);
        }
    }

    pub fn comparison_done(&mut self, job: u64, found: crate::render::Compared) {
        if job != self.compare_task {
            return;
        }
        if let Some(comparing) = self.comparing.as_mut() {
            comparing.running = false;
            comparing.found = Some(Found {
                said: found.said.clone(),
                could_not_align: found.could_not_align,
                changes: found.changes.clone(),
                showing: None,
            });
        }
        self.status = found.said;
    }

    pub fn comparison_failed(&mut self, job: u64, why: String) {
        if job != self.compare_task {
            return;
        }
        match self.comparing.as_mut() {
            Some(comparing) => {
                comparing.running = false;
                comparing.error = Some(why);
            }
            None => self.error = Some(why),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comparison_starts_on_the_sheet_being_looked_at() {
        // Because the sheet somebody is on is the one they just opened the
        // revision to look at.
        let comparing = Comparing::new(7);
        assert_eq!(comparing.newer_page, 7);
        assert_eq!(comparing.older_page, 7);
        assert!(comparing.older.is_none());
        assert!(comparing.found.is_none());
    }

    #[test]
    fn the_darkness_starts_where_a_plotted_drawing_wants_it() {
        let comparing = Comparing::new(0);
        assert!(comparing.darkness > 0.5 && comparing.darkness < 0.8);
    }
}

// ---- turning differences into markups --------------------------------------

impl App {
    /// Draws a revision cloud round each difference, as real annotations.
    ///
    /// A cloud is not a decoration this program invents: it is a Square
    /// annotation with a cloudy border effect, which is exactly what Revu
    /// writes and what every other reader already knows how to draw. So these
    /// go in the markups list, save into the file, and appear for whoever the
    /// set is sent to.
    ///
    /// They carry no measurement. A cloud says "this changed", and if somebody
    /// needs to know how much steel that is, they measure it with a tool — the
    /// same rule as everywhere else in this program.
    pub fn cloud_the_changes(&mut self, changes: &[(Kind, [f64; 4])]) -> String {
        if changes.is_empty() {
            return "There was nothing to cloud.".into();
        }
        let author = self.author.clone();
        let Some(doc) = self.doc_mut() else {
            return "No drawing is open.".into();
        };
        let page = doc.page;
        let frame = doc.frame_of(page);

        let mut made = 0usize;
        doc.checkpoint();
        for (kind, area) in changes {
            // A little room around it, so the cloud sits outside what changed
            // rather than through it.
            let margin = 6.0;
            let corners = [
                [area[0] - margin, area[1] - margin],
                [area[2] + margin, area[3] + margin],
            ];
            let colour = match kind {
                Kind::Added => [0x2f, 0x9e, 0x44, 0xff],
                Kind::Removed => [0xd0, 0x45, 0x3c, 0xff],
                Kind::Changed => [0xe0, 0x8b, 0x1a, 0xff],
            };
            let draft = crate::sheet::Draft {
                tool: crate::app::Tool::Rect,
                points: corners.to_vec(),
                strokes: Vec::new(),
                subject: format!("Revision — {}", kind.word()),
                colour,
                template: None,
                ..crate::sheet::Draft::default()
            };
            let Some(mut markup) = draft.into_markup(&frame, None) else {
                continue;
            };
            // The border effect that makes it a cloud rather than a box. `/I`
            // is how deep the scallops are; 2 is what Revu uses by default and
            // what people recognise.
            let mut effect = pdf::Dict::new();
            effect.set("S", pdf::Object::name("C"));
            effect.set("I", pdf::Object::Real(2.0));
            markup.dict.set("BE", pdf::Object::Dict(effect));
            markup.dict.set("T", pdf::Object::text(&author));
            doc.marks.push(crate::sheet::Mark::new(page, markup));
            made += 1;
        }
        doc.dirty = true;

        format!(
            "{made} revision cloud{} drawn on this sheet. They are markups like any \
             other — edit them, delete them, or save and send the set on. Nothing has \
             been measured.",
            if made == 1 { "" } else { "s" }
        )
    }

    /// Puts a difference in the middle of the window.
    pub fn centre_on_area(&mut self, area: [f64; 4]) {
        let middle = [(area[0] + area[2]) * 0.5, (area[1] + area[3]) * 0.5];
        let canvas = self.last_canvas;
        if let Some(doc) = self.doc_mut() {
            doc.view.centre_on(middle, canvas);
        }
    }
}

// ---- two issues, one on top of the other -----------------------------------

/// The overlay window's state.
#[derive(Clone)]
pub struct Overlaying {
    pub older: Option<PathBuf>,
    pub older_page: u32,
    pub newer_page: u32,
    pub darkness: f32,
    /// Correct for a sheet that was replotted a few thousandths over. On by
    /// default, because without it a re-plot shows as two drawings side by side
    /// in different colours and tells nobody anything.
    pub align_them: bool,
    pub running: bool,
    pub error: Option<String>,
    pub said: Option<String>,
    pub picture: Option<egui::TextureHandle>,
}

impl Overlaying {
    pub fn new(page: u32) -> Overlaying {
        Overlaying {
            older: None,
            older_page: page,
            newer_page: page,
            darkness: 0.65,
            align_them: true,
            running: false,
            error: None,
            said: None,
            picture: None,
        }
    }
}

impl App {
    pub fn begin_overlay(&mut self) {
        let Some(doc) = self.doc() else {
            self.status = "Open one of the two sheets first.".into();
            return;
        };
        self.overlaying = Some(Overlaying::new(doc.page));
    }

    pub fn overlay_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut overlaying) = self.overlaying.take() else {
            return;
        };
        let theme = self.chrome.theme;
        let Some(doc) = self.doc() else { return };
        let newer = doc.path.clone();

        let mut keep = true;
        let mut go = false;
        let mut pick_older = false;

        egui::Window::new("Overlay Pages")
            .collapsible(false)
            .resizable(true)
            .default_width(760.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Both issues printed over each other: red is the older, blue is \
                         the newer, dark grey is both.",
                    )
                    .color(theme.faint)
                    .size(11.0),
                );
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("The other issue").color(theme.faint).size(11.0));
                    match &overlaying.older {
                        Some(path) => {
                            ui.label(
                                RichText::new(
                                    path.file_name().unwrap_or_default().to_string_lossy(),
                                )
                                .size(11.0),
                            );
                        }
                        None => {
                            ui.label(RichText::new("none yet").color(theme.faint).size(11.0));
                        }
                    }
                    if ui.small_button("Choose…").clicked() {
                        pick_older = true;
                    }
                    ui.add_space(10.0);
                    ui.label(RichText::new("Sheet").color(theme.faint).size(11.0));
                    let mut n = overlaying.older_page + 1;
                    if ui
                        .add(egui::DragValue::new(&mut n).range(1..=9999))
                        .changed()
                    {
                        overlaying.older_page = n.saturating_sub(1);
                    }
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut overlaying.align_them, "Line them up");
                    ui.label(
                        RichText::new("Corrects a sheet that was replotted slightly over.")
                            .color(theme.faint)
                            .size(10.0),
                    );
                    ui.add_space(12.0);
                    ui.label(RichText::new("Darkness").color(theme.faint).size(11.0));
                    ui.add(
                        egui::Slider::new(&mut overlaying.darkness, 0.3..=0.9).show_value(false),
                    );
                });

                if let Some(said) = &overlaying.said {
                    ui.add_space(8.0);
                    ui.label(RichText::new(said).color(theme.faint).size(11.0));
                }
                if let Some(error) = &overlaying.error {
                    ui.add_space(8.0);
                    ui.colored_label(Color32::from_rgb(235, 120, 120), error);
                }

                if let Some(picture) = &overlaying.picture {
                    ui.add_space(8.0);
                    let width = ui.available_width().max(200.0);
                    let size = picture.size_vec2();
                    let scale = width / size.x;
                    ui.add(
                        egui::Image::new(picture)
                            .fit_to_exact_size(egui::vec2(width, size.y * scale)),
                    );
                }

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let ready = overlaying.older.is_some() && !overlaying.running;
                    if ui.add_enabled(ready, egui::Button::new("Overlay")).clicked() {
                        go = true;
                    }
                    if ui.button("Close").clicked() {
                        keep = false;
                    }
                    if overlaying.running {
                        ui.add_space(8.0);
                        ui.spinner();
                    }
                });
            });

        if pick_older {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Drawing sets (PDF)", &["pdf"])
                .set_title("The other issue")
                .pick_file()
            {
                overlaying.older = Some(path);
                overlaying.picture = None;
            }
        }
        if go {
            if let Some(older) = overlaying.older.clone() {
                overlaying.running = true;
                overlaying.error = None;
                self.compare_task += 1;
                let job = self.compare_task;
                self.svc.send(crate::render::ToWorker::Overlay {
                    job,
                    older,
                    older_page: overlaying.older_page,
                    newer,
                    newer_page: overlaying.newer_page,
                    darkness: overlaying.darkness,
                    align_them: overlaying.align_them,
                });
            }
        }
        if keep {
            self.overlaying = Some(overlaying);
        }
    }

    pub fn overlay_done(&mut self, job: u64, image: egui::ColorImage, said: String, ctx: &egui::Context) {
        if job != self.compare_task {
            return;
        }
        let picture = ctx.load_texture("hyperview-overlay", image, egui::TextureOptions::LINEAR);
        if let Some(overlaying) = self.overlaying.as_mut() {
            overlaying.running = false;
            overlaying.picture = Some(picture);
            overlaying.said = Some(said.clone());
        }
        self.status = said;
    }
}
