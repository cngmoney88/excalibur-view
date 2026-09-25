//! The bodies of the panels, the markups grid, and the small dialogs.

use std::path::PathBuf;

use egui::{Color32, RichText};

use crate::app::{App, Tool};
use crate::render::ToWorker;
use crate::units;
use takeoff::{Column, WeightColumns};

/// Which of the six user columns carry unit weights, and what all six are
/// called: read from the tool chest in use, by name, so a company's own chest
/// weighs its own steel. Set every frame from the profile; read anywhere.
static COLUMNS: std::sync::RwLock<Vec<takeoff::user::UserColumn>> = std::sync::RwLock::new(Vec::new());

pub fn set_user_columns(profile: Option<&chest::Profile>) {
    let wanted: Vec<takeoff::user::UserColumn> = profile
        .map(|p| {
            p.custom_columns
                .iter()
                .map(|c| takeoff::user::UserColumn {
                    index: c.index,
                    name: c.name.clone(),
                    formula: c.is_formula().then(|| c.expression.clone()),
                    precision: c.precision,
                })
                .collect()
        })
        .unwrap_or_default();
    let same = COLUMNS.read().map(|now| *now == wanted).unwrap_or(false);
    if !same {
        if let Ok(mut now) = COLUMNS.write() {
            *now = wanted;
        }
    }
}

pub fn user_columns() -> Vec<takeoff::user::UserColumn> {
    COLUMNS.read().map(|c| c.clone()).unwrap_or_default()
}

pub fn weights() -> WeightColumns {
    takeoff::user::weight_columns(&user_columns())
}

/// The columns the grid shows when a profile has not said otherwise.
const DEFAULT_COLUMNS: &[Column] = &[
    Column::Page,
    Column::Subject,
    Column::Colour,
    Column::Measurement,
    Column::Quantity,
    Column::Count,
    Column::Pounds,
    Column::Tons,
    Column::Author,
    Column::Date,
];

impl App {
    /// The columns to show, taken from his profile's Markups list if it has one.
    pub fn grid_columns(&self) -> Vec<(Column, f32)> {
        let from_profile: Vec<(Column, f32)> = self
            .profile
            .as_ref()
            .map(|p| {
                p.markup_columns
                    .iter()
                    .filter(|c| c.visible)
                    .filter_map(|c| Column::from_key(&c.key).map(|k| (k, c.width.max(48.0))))
                    .collect()
            })
            .unwrap_or_default();
        if from_profile.is_empty() {
            DEFAULT_COLUMNS
                .iter()
                .map(|c| (*c, default_width(*c)))
                .collect()
        } else {
            // Quantity, pounds and tons are ours, not Revu's, so they go on
            // the end of whatever he has set up rather than replacing any of it.
            let mut columns = from_profile;
            for extra in [Column::Quantity, Column::Pounds, Column::Tons] {
                if !columns.iter().any(|(c, _)| *c == extra) {
                    columns.push((extra, default_width(extra)));
                }
            }
            columns
        }
    }

    /// The strip above the navigation bar. It carries what the navigation bar
    /// does not — which drawing, how many sheets, whether the sheet has a
    /// scale, and whether the markups are in the file yet — and nothing the
    /// bar below it already shows.
    pub fn status_bar(&mut self, ctx: &egui::Context) {
        let theme = self.chrome.theme;
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                match self.doc() {
                    None => {
                        ui.label(&self.status);
                    }
                    Some(doc) => {
                        ui.label(format!(
                            "{}  ·  sheet {} of {}",
                            crate::app::name_of(&doc.path),
                            doc.page + 1,
                            doc.pages.len()
                        ));
                        ui.separator();
                        match doc.scale() {
                            Some(s) => {
                                ui.label(&s.ratio);
                            }
                            None => {
                                ui.label(
                                    RichText::new("no scale on this sheet").color(theme.warn),
                                );
                            }
                        }
                        ui.separator();
                        if doc.read_only {
                            ui.label(
                                RichText::new("read-only — Save As to keep markups")
                                    .color(theme.warn),
                            );
                        } else if doc.dirty {
                            ui.label(RichText::new("saving…").color(theme.faint));
                        } else {
                            ui.label(RichText::new("saved").color(theme.faint));
                        }
                        let rows = doc.rows();
                        let summary = takeoff::summarise(&rows, weights());
                        if summary.pounds > 0.0 {
                            ui.separator();
                            ui.label(
                                RichText::new(format!("{:.3} tons", summary.tons()))
                                    .color(theme.accent_text),
                            );
                        }
                        if summary.is_short() {
                            ui.separator();
                            ui.label(
                                RichText::new(format!(
                                    "{} left out — no scale",
                                    summary.unscaled
                                ))
                                .color(theme.warn),
                            );
                        }
                        if (doc.labelled as usize) < doc.pages.len() {
                            ui.separator();
                            ui.label(format!("reading sheet numbers… {}", doc.labelled));
                        }
                    }
                }
                // A new message shows on the right for a few seconds, over
                // the tool's hint, and then the hint comes back. Without a
                // drawing open it is already on the left.
                if self.status_seen.0 != self.status {
                    self.status_seen = (self.status.clone(), std::time::Instant::now());
                }
                let fresh = self.doc().is_some()
                    && !self.status.trim().is_empty()
                    && self.status_seen.1.elapsed() < std::time::Duration::from_secs(8);
                if fresh {
                    ctx.request_repaint_after(std::time::Duration::from_secs(1));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if fresh {
                        ui.label(RichText::new(&self.status).color(theme.accent_text));
                    } else {
                        ui.label(RichText::new(self.tool.hint()).weak());
                    }
                });
            });
        });
    }

    /// What a Dynamic Fill has found, and the two buttons that decide about it.
    ///
    /// Sits above the sheet rather than in a panel, because a shape and the
    /// decision about it belong together. A fill that escaped shows the reason
    /// and **no Apply button**: there is nothing to apply, and offering one
    /// would suggest otherwise.
    pub fn fill_bar(&mut self, ctx: &egui::Context) {
        if !self.tool.is_filling() && self.filling.outcome.is_none() {
            return;
        }
        let theme = self.chrome.theme;
        let said = self.filling.says();
        let ready = self.filling.ready().is_some();
        let mut apply = false;
        let mut clear = false;
        let mut undo_line = false;
        let mut darkness = self.filling.darkness;
        let lines = self.filling.strokes.len();

        egui::TopBottomPanel::top("fill")
            .frame(
                egui::Frame::new()
                    .fill(theme.bar)
                    .inner_margin(egui::Margin::symmetric(10, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Dynamic Fill").strong());
                    ui.separator();
                    match &said {
                        Some((words, true)) => {
                            ui.label(RichText::new(words).color(theme.warn).size(11.5));
                        }
                        Some((words, false)) => {
                            ui.label(RichText::new(words).color(theme.faint).size(11.5));
                        }
                        None => {
                            ui.label(
                                RichText::new(
                                    "Click inside an area. Draw across a doorway first if it \
                                     is open.",
                                )
                                .color(theme.faint)
                                .size(11.5),
                            );
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(ready, egui::Button::new("Apply as area"))
                            .on_hover_text("Turn this shape into a measurement. Enter.")
                            .clicked()
                        {
                            apply = true;
                        }
                        if ui.button("Clear").on_hover_text("Escape").clicked() {
                            clear = true;
                        }
                        if lines > 0 {
                            ui.separator();
                            if ui
                                .small_button(format!("Undo boundary ({lines})"))
                                .clicked()
                            {
                                undo_line = true;
                            }
                        }
                        ui.separator();
                        // Some drawings are screened back, some are heavy. One
                        // slider beats guessing why a fill leaked.
                        ui.add(
                            egui::Slider::new(&mut darkness, 0.25f32..=0.9)
                                .show_value(false)
                                .text("Line sensitivity"),
                        );
                    });
                });
            });

        if (darkness - self.filling.darkness).abs() > 0.001 {
            self.filling.darkness = darkness;
            // What counts as a line has changed, so the last answer is stale.
            self.filling.clear();
        }
        if undo_line {
            self.filling.strokes.pop();
            self.filling.clear();
        }
        if clear {
            self.filling.clear();
        }
        if apply {
            self.apply_fill();
        }
    }

    // ---- dialogs ---------------------------------------------------------

    pub fn calibrate_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.calibrating.take() else {
            return;
        };
        let mut keep = true;
        let mut apply = false;
        egui::Window::new("Set the scale from the drawing")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .max_width(560.0)
            .show(ctx, |ui| {
                ui.label("What does that dimension read?");
                ui.add_space(6.0);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.typed)
                        .hint_text("20'-6\"")
                        .desired_width(200.0),
                );
                response.request_focus();
                ui.add_space(4.0);
                ui.weak(format!(
                    "You picked {:.2} inches across the sheet.",
                    state.points / 72.0
                ));
                if let Some(error) = &state.error {
                    ui.add_space(4.0);
                    ui.colored_label(Color32::from_rgb(235, 120, 120), error);
                }
                ui.add_space(6.0);
                ui.checkbox(
                    &mut state.everywhere,
                    "Use this scale on every sheet that has none",
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let entered = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("Set the scale").clicked() || entered {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });

        if apply {
            match units::parse_length(&state.typed, self.units) {
                None => {
                    state.error = Some(
                        "Type it the way it reads on the drawing: 20'-6\", 18', 246\" or 6250mm."
                            .into(),
                    );
                }
                Some(inches) => {
                    match crate::scale::from_calibration(
                        state.points,
                        inches,
                        self.units,
                        self.denominator,
                    ) {
                        None => state.error = Some("That came out to nothing. Try again.".into()),
                        Some(measure) => {
                            let label = measure.ratio.clone();
                            if let Some(doc) = self.doc_mut() {
                                if state.everywhere {
                                    for p in 0..doc.pages.len() as u32 {
                                        if p != state.page && doc.scale_of(p).is_some() {
                                            continue;
                                        }
                                        doc.set_scale(p, Some(measure.clone()));
                                    }
                                }
                                doc.set_scale(state.page, Some(measure));
                            }
                            self.status = format!("Scale set: {label}");
                            self.tool = Tool::Length;
                            keep = false;
                        }
                    }
                }
            }
        }
        if keep {
            self.calibrating = Some(state);
        }
    }

    pub fn text_dialog(&mut self, ctx: &egui::Context) {
        let Some(index) = self.editing_text else { return };
        let Some(doc) = self.doc_mut() else {
            self.editing_text = None;
            return;
        };
        let Some(mark) = doc.marks.get(index) else {
            self.editing_text = None;
            return;
        };
        let mut text = mark.markup.contents();
        let mut done = false;
        let mut remove = false;
        egui::Window::new("Note")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .max_width(560.0)
            .show(ctx, |ui| {
                let response = ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .desired_width(320.0)
                        .desired_rows(4),
                );
                response.request_focus();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Done").clicked() {
                        done = true;
                    }
                    if ui.button("Remove").clicked() {
                        remove = true;
                    }
                });
            });
        if remove {
            doc.checkpoint();
            doc.marks[index].gone = true;
            doc.selected = None;
            self.editing_text = None;
        } else if done {
            doc.checkpoint();
            doc.marks[index].markup.set_contents(&text);
            doc.marks[index].changed = true;
            self.editing_text = None;
        } else if let Some(mark) = doc.marks.get_mut(index) {
            // Keep what has been typed so far while the window stays open.
            if mark.markup.contents() != text {
                mark.markup.set_contents(&text);
                mark.changed = true;
            }
        }
    }

    // ---- export ----------------------------------------------------------

    /// Writes the takeoff out as a report somebody can read on paper.
    pub(crate) fn export_report(&mut self) {
        let Some(doc) = self.doc() else {
            self.status = "Open a drawing first.".into();
            return;
        };
        let rows = doc.rows();
        if rows.is_empty() {
            self.status = "There is nothing on this drawing to report.".into();
            return;
        }
        // The same columns the markups list is showing, so the report and the
        // screen cannot disagree about what a takeoff says.
        let columns: Vec<Column> = self.grid_columns().into_iter().map(|(c, _)| c).collect();
        let summary = takeoff::summarise(&rows, weights());
        let drawing = doc
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let sheet = |page: usize| doc.sheet_name(page as u32);
        let report = crate::report::from_takeoff(
            &drawing,
            &self.author,
            &rows,
            &columns,
            weights(),
            &sheet,
            &summary,
        );

        let suggested = crate::docops::beside(&doc.path, "takeoff summary");
        let Some(to) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(
                suggested
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .as_ref(),
            )
            .save_file()
        else {
            return;
        };
        let to = crate::docops::free_name(&to);
        // `None` is "wherever the program found pdfium", which is the same
        // place the viewer's own worker found it.
        match crate::render::write_report(None, &report, &to) {
            Ok(done) => {
                self.status = done.said;
                self.open(to);
            }
            Err(why) => self.error = Some(why),
        }
    }

    // ---- thumbnails ------------------------------------------------------

    pub fn thumbnails(&mut self, ui: &mut egui::Ui) {
        let theme = self.chrome.theme;
        let mut go: Option<u32> = None;
        let mut action: Option<PickAction> = None;
        let Some(doc) = self.doc_mut() else {
            ui.add_space(8.0);
            ui.weak("No drawing open.");
            return;
        };
        ui.add_space(4.0);
        ui.add(
            egui::TextEdit::singleline(&mut doc.filter)
                .hint_text("Find a sheet")
                .desired_width(f32::INFINITY),
        );
        let needle = doc.filter.to_lowercase();
        let shown: Vec<u32> = (0..doc.pages.len() as u32)
            .filter(|p| {
                if needle.is_empty() {
                    return true;
                }
                let l = &doc.labels[*p as usize];
                l.number.to_lowercase().contains(&needle)
                    || l.title.to_lowercase().contains(&needle)
                    || format!("{}", p + 1) == needle
            })
            .collect();
        doc.picks.keep_within(doc.pages.len());
        let current = doc.page;
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!("{} of {} sheets", shown.len(), doc.pages.len()))
                .color(theme.faint)
                .size(11.0),
        );
        // One line that is always there, so picking the first sheet doesn't
        // push the list down under the pointer: how to pick, or what to do
        // with what's picked.
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.set_min_height(20.0);
            if doc.picks.is_empty() {
                ui.label(
                    RichText::new(format!(
                        "{}-click to pick sheets, Shift-click for a run",
                        command_key()
                    ))
                    .color(theme.faint)
                    .size(10.0),
                );
            } else {
                ui.label(
                    RichText::new(format!("{} picked", doc.picks.count()))
                        .color(theme.text)
                        .size(11.0),
                );
                if ui.small_button("Print…").clicked() {
                    action = Some(PickAction::Print);
                }
                if ui.small_button("Save as PDF…").clicked() {
                    action = Some(PickAction::Save);
                }
                if ui.small_button("Clear").clicked() {
                    action = Some(PickAction::Clear);
                }
            }
        });
        ui.add_space(4.0);

        let row = 70.0;
        let picking = !doc.picks.is_empty();
        let mut wanted: Vec<u32> = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row, shown.len(), |ui, range| {
                for index in range {
                    let page = shown[index];
                    if !doc.thumbs.contains_key(&page) && doc.asked.insert(page) {
                        wanted.push(page);
                    }
                    let width = ui.available_width();
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(width, row - 6.0), egui::Sense::click());
                    if response.clicked() {
                        let how = crate::picks::Click::from(click_modifiers(ui));
                        if doc.picks.click(page, how, current, &shown) {
                            go = Some(page);
                        }
                    }
                    if response.secondary_clicked() {
                        doc.picks.right_click(page, current);
                    }
                    response.context_menu(|ui| {
                        let n = doc.picks.sheets(current).len();
                        let these = if n == 1 {
                            "this sheet".to_string()
                        } else {
                            format!("these {n} sheets")
                        };
                        if ui.button(format!("Print {these}…")).clicked() {
                            action = Some(PickAction::Print);
                            ui.close();
                        }
                        if ui.button(format!("Save {these} as a PDF…")).clicked() {
                            action = Some(PickAction::Save);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Pick every sheet in the list").clicked() {
                            action = Some(PickAction::All);
                            ui.close();
                        }
                        if ui
                            .add_enabled(!doc.picks.is_empty(), egui::Button::new("Clear the picks"))
                            .clicked()
                        {
                            action = Some(PickAction::Clear);
                            ui.close();
                        }
                    });
                    let painter = ui.painter();
                    let here = page == current;
                    // With nothing picked, the sheet on screen is the one
                    // lit up, as it always was. Once sheets are picked they
                    // are, and the sheet on screen keeps an outline so you
                    // still know where you are.
                    let selected = if picking { doc.picks.contains(page) } else { here };
                    if selected {
                        painter.rect_filled(rect, egui::CornerRadius::same(4), theme.accent);
                    } else if response.hovered() {
                        painter.rect_filled(rect, egui::CornerRadius::same(4), theme.hover);
                    }
                    if picking && here {
                        painter.rect_stroke(
                            rect.shrink(1.0),
                            egui::CornerRadius::same(4),
                            egui::Stroke::new(1.5, if selected { Color32::WHITE } else { theme.accent }),
                            egui::StrokeKind::Inside,
                        );
                    }

                    let size = doc.pages[page as usize];
                    let box_size = egui::vec2(76.0, 56.0);
                    let fit =
                        (box_size.x / size.width.max(1.0)).min(box_size.y / size.height.max(1.0));
                    let slot = egui::Rect::from_min_size(rect.min + egui::vec2(6.0, 6.0), box_size);
                    let thumb_rect = egui::Rect::from_center_size(
                        slot.center(),
                        egui::vec2(size.width * fit, size.height * fit),
                    );
                    match doc.thumbs.get(&page) {
                        Some(texture) => {
                            painter.image(
                                texture.id(),
                                thumb_rect,
                                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        None => {
                            painter.rect_filled(thumb_rect, 2.0, theme.sunken);
                        }
                    }

                    let label = &doc.labels[page as usize];
                    let number = if label.number.is_empty() {
                        format!("Page {}", page + 1)
                    } else {
                        label.number.clone()
                    };
                    let text_left = rect.min + egui::vec2(92.0, 8.0);
                    painter.text(
                        text_left,
                        egui::Align2::LEFT_TOP,
                        number,
                        egui::FontId::proportional(14.0),
                        if selected { Color32::WHITE } else { theme.text },
                    );
                    let room = (rect.right() - text_left.x - 8.0).max(40.0);
                    let mut job = egui::text::LayoutJob::simple(
                        label.title.clone(),
                        egui::FontId::proportional(11.0),
                        if selected {
                            Color32::from_gray(225)
                        } else {
                            theme.faint
                        },
                        room,
                    );
                    job.wrap.max_rows = 2;
                    job.wrap.overflow_character = Some('\u{2026}');
                    let galley = painter.layout_job(job);
                    painter.galley(text_left + egui::vec2(0.0, 22.0), galley, theme.faint);
                }
            });
        match action {
            Some(PickAction::All) => doc.picks.all(&shown),
            Some(PickAction::Clear) => doc.picks.clear(),
            _ => {}
        }
        if !wanted.is_empty() {
            if let Some(id) = self.doc().map(|d| d.id) {
                self.svc.send(ToWorker::Thumbs { doc: id, pages: wanted });
            }
        }
        if let Some(page) = go {
            self.go_to(page);
        }
        match action {
            Some(PickAction::Print) => self.begin_print(),
            Some(PickAction::Save) => self.save_picked_sheets(),
            _ => {}
        }
    }

    /// Writes the picked sheets (or the one on screen) into a PDF of their
    /// own, markups and all. The drawing itself is left as it is.
    pub fn save_picked_sheets(&mut self) {
        if self.doc_job.as_ref().is_some_and(|job| job.running) {
            self.status = "Another document job is still running. Try again when it's done.".into();
            return;
        }
        let Some(doc) = self.doc() else { return };
        let pages = doc.picks.sheets(doc.page);
        let source = doc.path.clone();
        let numbers: Vec<String> = pages
            .iter()
            .map(|p| doc.labels.get(*p as usize).map(|l| l.number.clone()).unwrap_or_default())
            .collect();
        let stem = source
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "drawing".into());
        let mut dialog = rfd::FileDialog::new()
            .set_title("Save the sheets as a PDF")
            .set_file_name(crate::picks::file_name(&stem, &numbers))
            .add_filter("PDF", &["pdf"]);
        if let Some(folder) = source.parent() {
            dialog = dialog.set_directory(folder);
        }
        let Some(mut to) = dialog.save_file() else {
            return;
        };
        if !to
            .extension()
            .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("pdf"))
        {
            let mut name = to.as_os_str().to_os_string();
            name.push(".pdf");
            to = PathBuf::from(name);
        }
        let same = |a: &std::path::Path, b: &std::path::Path| {
            match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
                (Ok(a), Ok(b)) => a == b,
                _ => a == b,
            }
        };
        if same(&to, &source) {
            self.error = Some(
                "That's the drawing itself. Pick another name, so the sheets go into a file of their own."
                    .into(),
            );
            return;
        }
        // The copy is made from the file, so markups not written into it yet
        // go in first.
        let (dirty, read_only) = self.doc().map(|d| (d.dirty, d.read_only)).unwrap_or_default();
        if dirty && !read_only {
            self.save_now();
        }
        let left_out = self.doc().is_some_and(|d| d.dirty);
        self.doc_task += 1;
        self.quiet_task = Some(self.doc_task);
        self.quiet_left_out = left_out;
        self.svc.send(ToWorker::Document {
            job: self.doc_task,
            op: Box::new(crate::docops::Operation::Extract {
                source,
                pages: pages.clone(),
                to: to.clone(),
                and_remove: false,
            }),
        });
        let names = crate::picks::in_words(&numbers);
        self.status = if names.is_empty() {
            format!(
                "Saving {} sheet{} into {}…",
                pages.len(),
                if pages.len() == 1 { "" } else { "s" },
                to.display()
            )
        } else {
            format!("Saving {names} into {}…", to.display())
        };
    }

    // ---- tool chest ------------------------------------------------------

    pub fn tool_chest(&mut self, ui: &mut egui::Ui) {
        let theme = self.chrome.theme;
        let Some(profile) = self.profile.take() else {
            ui.add_space(8.0);
            ui.weak("No tool chest loaded.");
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Your tools, your columns and your unit weights come out of a Revu \
                     profile. Load yours and the program is set up the way you already \
                     work.",
                )
                .color(theme.faint)
                .size(11.0),
            );
            ui.add_space(8.0);
            if ui.button("Load a tool chest…").clicked() {
                self.pick_and_load_chest();
            }
            return;
        };
        ui.add_space(4.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.chest_filter)
                .hint_text("Find a tool")
                .desired_width(f32::INFINITY),
        );
        let needle = self.chest_filter.trim().to_lowercase();
        ui.add_space(4.0);
        let total: usize = profile.sets.iter().map(|s| s.tools.len()).sum();
        ui.label(
            RichText::new(format!("{} tools in {} sets", total, profile.sets.len()))
                .color(theme.faint)
                .size(11.0),
        );
        ui.add_space(4.0);

        let mut picked: Option<chest::Tool> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for set in &profile.sets {
                    let matching: Vec<&chest::Tool> = set
                        .tools
                        .iter()
                        .filter(|t| needle.is_empty() || t.subject.to_lowercase().contains(&needle))
                        .collect();
                    if matching.is_empty() {
                        continue;
                    }
                    let open = !needle.is_empty() || self.chest_open.contains(&set.title);
                    let header = ui.horizontal(|ui| {
                        // Painted rather than typed: the arrows are not in
                        // every font, and a missing glyph shows as a box.
                        let (mark, _) =
                            ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                        crate::panelbody::twist(ui.painter(), mark, open, theme.faint);
                        ui.label(RichText::new(&set.title).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{}", matching.len()))
                                    .color(theme.faint)
                                    .size(11.0),
                            );
                        });
                    });
                    if header.response.interact(egui::Sense::click()).clicked() {
                        if self.chest_open.contains(&set.title) {
                            self.chest_open.remove(&set.title);
                        } else {
                            self.chest_open.insert(set.title.clone());
                        }
                    }
                    if !open {
                        continue;
                    }
                    for tool in matching.iter().take(400) {
                        let chosen = self.subject == tool.subject;
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 20.0),
                            egui::Sense::click(),
                        );
                        let painter = ui.painter();
                        if chosen {
                            painter.rect_filled(rect, egui::CornerRadius::same(3), theme.accent);
                        } else if response.hovered() {
                            painter.rect_filled(rect, egui::CornerRadius::same(3), theme.hover);
                        }
                        let swatch = egui::Rect::from_min_size(
                            rect.min + egui::vec2(20.0, 5.0),
                            egui::vec2(10.0, 10.0),
                        );
                        painter.rect_filled(
                            swatch,
                            2.0,
                            Color32::from_rgb(
                                (tool.colour[0] * 255.0) as u8,
                                (tool.colour[1] * 255.0) as u8,
                                (tool.colour[2] * 255.0) as u8,
                            ),
                        );
                        painter.text(
                            rect.min + egui::vec2(38.0, 10.0),
                            egui::Align2::LEFT_CENTER,
                            &tool.subject,
                            egui::FontId::proportional(12.0),
                            if chosen { Color32::WHITE } else { theme.text },
                        );
                        let weight = tool
                            .pounds_per_foot()
                            .map(|w| format!("{w:.2} lb/ft"))
                            .or_else(|| {
                                tool.pounds_per_square_foot().map(|w| format!("{w:.2} psf"))
                            });
                        if let Some(weight) = weight {
                            painter.text(
                                egui::pos2(rect.right() - 8.0, rect.center().y),
                                egui::Align2::RIGHT_CENTER,
                                weight,
                                egui::FontId::proportional(10.0),
                                if chosen {
                                    Color32::from_gray(220)
                                } else {
                                    theme.faint
                                },
                            );
                        }
                        if response.clicked() {
                            picked = Some((*tool).clone());
                        }
                    }
                }
            });
        self.profile = Some(profile);

        if let Some(tool) = picked {
            self.pick_tool(&tool);
        }
    }

    /// Arms a Tool Chest tool: its subject, its colour, **and the annotation
    /// dictionary itself**, so what lands on the sheet is his tool and not an
    /// approximation of it.
    pub fn pick_tool(&mut self, tool: &chest::Tool) {
        self.subject = tool.subject.clone();
        self.color = [
            (tool.colour[0] * 255.0) as u8,
            (tool.colour[1] * 255.0) as u8,
            (tool.colour[2] * 255.0) as u8,
            255,
        ];
        self.template = Some(tool.annotation.clone());
        let (canvas, id) = Tool::for_chest(tool.kind, &tool.annotation);
        self.finish_draft();
        self.tool = canvas;
        self.chrome.tool = id.into();
    }

    // ---- measurements ----------------------------------------------------

    pub fn measurements(&mut self, ui: &mut egui::Ui) {
        let theme = self.chrome.theme;
        let Some(doc) = self.doc() else {
            ui.add_space(8.0);
            ui.weak("No drawing open.");
            return;
        };
        let rows = doc.rows();
        let summary = takeoff::summarise(&rows, weights());
        let sheet_names: Vec<String> = summary
            .unscaled_pages
            .iter()
            .map(|p| doc.sheet_name(*p as u32))
            .collect();

        ui.add_space(4.0);
        if summary.groups.is_empty() {
            ui.label(
                RichText::new("Measure something and it lands here.")
                    .color(theme.faint)
                    .size(11.0),
            );
        }
        if summary.is_short() {
            ui.colored_label(
                theme.warn,
                format!(
                    "{} measurement{} left out — no scale on {}.",
                    summary.unscaled,
                    if summary.unscaled == 1 { "" } else { "s" },
                    sheet_names.join(", ")
                ),
            );
            ui.add_space(2.0);
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for group in &summary.groups {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&group.subject).strong());
                            let sheets: Vec<String> =
                                group.pages.iter().map(|p| doc.sheet_name(*p as u32)).collect();
                            let mut note = format!(
                                "{} · {} pick{} · {}",
                                group.kind.name(),
                                group.picks,
                                if group.picks == 1 { "" } else { "s" },
                                sheets.join(", ")
                            );
                            if group.unscaled > 0 {
                                note.push_str(&format!(" · {} left out", group.unscaled));
                            }
                            ui.label(RichText::new(note).color(theme.faint).size(11.0));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if group.weighed {
                                ui.label(
                                    RichText::new(format!("{:.3} t", group.tons()))
                                        .color(theme.faint)
                                        .size(11.0),
                                );
                                ui.add_space(8.0);
                            }
                            ui.label(RichText::new(total_of(group, doc)).strong());
                        });
                    });
                    ui.separator();
                }
                if summary.pounds > 0.0 {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Total steel").strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!(
                                    "{:.3} tons  ·  {:.0} lb",
                                    summary.tons(),
                                    summary.pounds
                                ))
                                .strong()
                                .color(theme.accent_text),
                            );
                        });
                    });
                }
            });
    }

    pub fn search_panel(&mut self, ui: &mut egui::Ui) {
        let theme = self.chrome.theme;
        let Some(sheets) = self.doc().map(|d| d.pages.len() as u32) else {
            ui.add_space(8.0);
            ui.weak("No drawing open.");
            return;
        };

        ui.add_space(4.0);
        let typed = ui.add(
            egui::TextEdit::singleline(&mut self.search.needle)
                .hint_text("Find on every sheet")
                .desired_width(f32::INFINITY),
        );
        let entered = typed.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

        ui.add_space(4.0);
        let mut changed = typed.changed() || entered;
        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut self.search.whole_words, "Whole words")
                .changed()
            {
                changed = true;
            }
            if ui
                .checkbox(&mut self.search.match_case, "Match case")
                .changed()
            {
                changed = true;
            }
        });

        if changed {
            self.start_search(sheets);
        }

        ui.add_space(6.0);
        let total = self.search.total();
        if self.search.running {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new(format!(
                        "{} of {} sheets · {total} so far",
                        self.search.looked_at, self.search.sheets
                    ))
                    .color(theme.faint)
                    .size(11.0),
                );
            });
        } else if !self.search.needle.trim().is_empty() {
            let message = if total == 0 {
                format!("Nothing on any of the {sheets} sheets.")
            } else {
                format!(
                    "{total} on {} sheet{}",
                    self.search.sheets_with_something(),
                    if self.search.sheets_with_something() == 1 { "" } else { "s" }
                )
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new(message).color(theme.faint).size(11.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if total > 0 {
                        if ui.small_button("Next").clicked() {
                            let to = self.search.next();
                            self.go_to_hit(to);
                        }
                        if ui.small_button("Previous").clicked() {
                            let to = self.search.previous();
                            self.go_to_hit(to);
                        }
                    }
                });
            });
        } else {
            ui.label(
                RichText::new(
                    "Type something to find it on every sheet at once. \
                     `Whole words` keeps PL1 from answering with every PL1/2.",
                )
                .color(theme.faint)
                .size(11.0),
            );
        }
        ui.separator();

        let mut jump: Option<(u32, usize)> = None;
        let showing = self.search.showing;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (page, hits) in &self.search.hits {
                    let name = self
                        .doc()
                        .map(|d| d.sheet_name(*page))
                        .unwrap_or_else(|| format!("Sheet {}", page + 1));
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!("{name}  ·  {}", hits.len()))
                            .color(theme.faint)
                            .size(11.0),
                    );
                    for (index, hit) in hits.iter().enumerate() {
                        let chosen = showing == Some((*page, index));
                        let width = ui.available_width();
                        let galley = context_line(ui, hit, theme, chosen, width - 12.0);
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(width, galley.size().y + 8.0),
                            egui::Sense::click(),
                        );
                        let painter = ui.painter();
                        if chosen {
                            painter.rect_filled(
                                rect,
                                egui::CornerRadius::same(3),
                                theme.accent.gamma_multiply(0.35),
                            );
                        } else if response.hovered() {
                            painter.rect_filled(rect, egui::CornerRadius::same(3), theme.hover);
                        }
                        painter.galley(rect.min + egui::vec2(6.0, 4.0), galley, theme.text);
                        if response.clicked() {
                            jump = Some((*page, index));
                        }
                    }
                }
            });

        if let Some(to) = jump {
            self.go_to_hit(Some(to));
        }
    }

    pub fn start_search(&mut self, sheets: u32) {
        let Some(id) = self.doc().map(|d| d.id) else { return };
        self.search.begin(sheets);
        if self.search.running {
            self.svc.send(ToWorker::Find {
                doc: id,
                job: self.search.job,
                needle: self.search.needle.trim().to_string(),
                whole_words: self.search.whole_words,
                match_case: self.search.match_case,
            });
        } else {
            // An emptied box stops the search rather than leaving the last
            // one's answers sitting there looking current.
            self.svc.send(ToWorker::Find {
                doc: id,
                job: self.search.job,
                needle: String::new(),
                whole_words: false,
                match_case: false,
            });
        }
    }

    /// Goes to an answer and puts it in the middle of the window.
    pub fn go_to_hit(&mut self, to: Option<(u32, usize)>) {
        let Some(to) = to else { return };
        let Some(hit) = self.search.at(to) else { return };
        let area = hit.area;
        let page = to.0;
        self.search.showing = Some(to);
        self.go_to(page);
        let canvas = self.last_canvas;
        if let Some(doc) = self.doc_mut() {
            // Close enough to read what was found, without losing where it is
            // on the sheet.
            doc.view.zoom = doc.view.zoom.max(0.6);
            doc.view.centre_on(
                [(area[0] + area[2]) * 0.5, (area[1] + area[3]) * 0.5],
                canvas,
            );
        }
    }

    // ---- properties ------------------------------------------------------
    // ---- properties ------------------------------------------------------

    pub fn properties(&mut self, ui: &mut egui::Ui) {
        let theme = self.chrome.theme;
        let Some(doc) = self.doc() else {
            ui.add_space(8.0);
            ui.weak("No drawing open.");
            return;
        };
        let Some(index) = doc.selected else {
            ui.add_space(8.0);
            ui.label(
                RichText::new("Pick a markup to see what it is.")
                    .color(theme.faint)
                    .size(11.0),
            );
            return;
        };
        let Some(row) = doc.row(index) else { return };
        ui.add_space(6.0);

        let measurement = if row.caption.is_empty() {
            if row.scaled {
                "—".to_string()
            } else {
                "no scale on this sheet".to_string()
            }
        } else {
            row.caption.clone()
        };
        let mut lines: Vec<(&str, String)> = vec![
            ("Kind", row.kind.name().to_string()),
            (
                "Subject",
                if row.subject.is_empty() {
                    "(no subject)".into()
                } else {
                    row.subject.clone()
                },
            ),
            ("Sheet", doc.sheet_name(row.page as u32)),
            ("Measurement", measurement),
        ];
        // Where it is in the language the shop uses. Only when the sheet's own
        // grid says — never a nearest-guess, and no line at all when the
        // sheet has no grid on it.
        if !row.grid.is_empty() {
            lines.push(("Grid", row.grid.clone()));
        }
        if let Some(lb) = row.pounds(weights()) {
            lines.push(("Weight", format!("{lb:.0} lb  ·  {:.3} t", lb / 2000.0)));
        }
        if !row.author.is_empty() {
            lines.push(("Author", row.author.clone()));
        }
        if !row.comments.is_empty() {
            lines.push(("Comments", row.comments.clone()));
        }
        // The chest's own columns, by the names it gives them, with the
        // formula ones worked out.
        let user = user_columns();
        let named: Vec<(String, String)> = (0..6)
            .map(|n| (takeoff::user::name_of(&user, n), takeoff::user::text(&row, &user, n)))
            .filter(|(_, value)| !value.trim().is_empty())
            .collect();
        let lines: Vec<(String, String)> = lines
            .into_iter()
            .map(|(name, value)| (name.to_string(), value))
            .chain(named)
            .collect();

        for (name, value) in lines {
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).color(theme.faint).size(11.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let text = RichText::new(&value).size(12.0);
                    if value == "no scale on this sheet" {
                        ui.label(text.color(theme.warn));
                    } else {
                        ui.label(text);
                    }
                });
            });
        }

        // ---- what the estimate multiplies by ----
        // A quantity on any measurement, and a pitch on a run. Both are
        // written into the markup in keys Revu keeps, so they travel with the
        // drawing and every total, weight and export counts them.
        if !row.kind.measures() {
            return;
        }
        let many = self.doc().map(|d| d.selection().len()).unwrap_or(1);
        let pitched = matches!(row.kind, annot::Kind::Length | annot::Kind::Polylength);
        let mut quantity = row.quantity;
        let mut rise = row.slope.unwrap_or(0.0);
        let run = row.pitch_run;
        let mut new_quantity: Option<(f64, bool)> = None;
        let mut new_rise: Option<(f64, bool)> = None;
        ui.add_space(4.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Quantity").color(theme.faint).size(11.0))
                .on_hover_text(
                    "How many of this there are. Draw one line for six identical \
                     beams, set 6, and every total, weight and export counts six.",
                );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let r = ui.add(
                    egui::DragValue::new(&mut quantity)
                        .range(1.0..=9999.0)
                        .speed(0.05)
                        .prefix("×")
                        .max_decimals(3),
                );
                if r.changed() {
                    new_quantity = Some((quantity, r.drag_started() || !r.dragged()));
                }
            });
        });
        if pitched {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Slope").color(theme.faint).size(11.0))
                    .on_hover_text(
                        "The pitch, as rise over run. A rafter, stringer or brace \
                         drawn in plan is longer than it looks; give it its pitch \
                         and it is measured up the slope. Revu reads the same pitch.",
                    );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let r = ui.add(
                        egui::DragValue::new(&mut rise)
                            .range(-96.0..=96.0)
                            .speed(0.05)
                            .max_decimals(3)
                            .suffix(format!(" in {}", takeoff::row::quantity_text(run))),
                    );
                    if r.changed() {
                        new_rise = Some((rise, r.drag_started() || !r.dragged()));
                    }
                });
            });
        }
        if many > 1 {
            ui.label(
                RichText::new(format!("Changes all {many} selected."))
                    .color(theme.faint)
                    .size(10.5),
            );
        }
        if let Some((quantity, fresh)) = new_quantity {
            self.set_quantity_on_selection(quantity, fresh);
        }
        if let Some((rise, fresh)) = new_rise {
            self.set_pitch_on_selection(rise, run, fresh);
        }
    }

    /// Sets how many of each selected measurement there are. `fresh` is the
    /// first change of an edit, which is where the undo step goes — dragging
    /// the number from 1 to 12 is one step back, not eleven.
    pub fn set_quantity_on_selection(&mut self, quantity: f64, fresh: bool) {
        let Some(doc) = self.doc_mut() else { return };
        let picked = doc.selection();
        if fresh {
            doc.checkpoint_named(&if picked.len() > 1 {
                format!("Quantity on {} markups", picked.len())
            } else {
                "Quantity".to_string()
            });
        }
        for index in picked {
            let Some(mark) = doc.marks.get_mut(index) else { continue };
            if !mark.kind().measures() {
                continue;
            }
            takeoff::row::set_quantity(&mut mark.markup, quantity);
            mark.changed = true;
        }
        doc.dirty = true;
    }

    /// Puts a pitch on each selected run and re-reads what it measures, so
    /// the caption on the sheet says the length up the slope.
    pub fn set_pitch_on_selection(&mut self, rise: f64, run: f64, fresh: bool) {
        let Some(doc) = self.doc_mut() else { return };
        let picked = doc.selection();
        if fresh {
            doc.checkpoint_named("Slope");
        }
        for index in picked {
            let Some(mark) = doc.marks.get_mut(index) else { continue };
            if !matches!(mark.kind(), annot::Kind::Length | annot::Kind::Polylength) {
                continue;
            }
            takeoff::row::set_pitch(&mut mark.markup, rise, run);
            mark.changed = true;
        }
        doc.remeasure_selection();
        doc.dirty = true;
    }

    // ---- the markups grid ------------------------------------------------

    // ---- turning clicks into actions -------------------------------------

    pub fn run_commands(&mut self, ctx: &egui::Context) {
        for id in self.chrome.take_fired() {
            match id.as_str() {
                "File.Open" => self.pick_and_open(),
                "Document.Save" | "File.Save" => self.save_now(),
                "File.SaveAs" => self.save_as(),
                "Document.Print" => self.begin_print(),
                "Document.Combine" | "File.CombinePDFs" => {
                    self.begin_document_job(crate::docui::Job::Combine)
                }
                "Document.New" => self.begin_document_job(crate::docui::Job::NewBlank),
                "File.CreatePDF" | "Document.NewPackage" => {
                    self.begin_document_job(crate::docui::Job::FromPictures)
                }
                "Document.SaveAll" => self.save_all(),
                "Split.Email" => self.email_this_set(),
                "Document.RotateCounterclockwise" => {
                    self.begin_document_job(crate::docui::Job::Rotate);
                    if let Some(setting) = self.doc_job.as_mut() {
                        setting.quarter_turns = 3;
                    }
                }
                "Document.Compare" => self.begin_compare(),
                "File.Close" => self.close_tab(self.current),
                "File.OpenRecent" => self.recent_open = true,
                "File.Revert" => self.revert(),
                "Document.NumberPages" => {
                    self.begin_document_job(crate::docui::Job::Stamp);
                    // Preset to the thing somebody means by "number pages", so
                    // the common case is one more click rather than knowing
                    // that <n> is a field.
                    if let Some(setting) = self.doc_job.as_mut() {
                        for (spot, text) in setting.stamps.iter_mut() {
                            if *spot == crate::stamps::Spot::BottomRight {
                                *text = "<n> of <total>".into();
                            }
                        }
                    }
                }
                "File.CloseAll" => {
                    while !self.docs.is_empty() {
                        self.close_tab(0);
                    }
                    self.status = "All drawings closed. Everything unsaved was saved.".into();
                }
                "Edit.SelectAll" => self.select_all_on_this_sheet(),
                "Markup.FormatPainter" => self.pick_up_format(),

                // ---- lining up, spacing out and layer order ----
                "Align.Left" | "Align.Center" | "Align.Right" | "Align.Top"
                | "Align.Middle" | "Align.Bottom" => {
                    if let Some(edge) = crate::arrange::Edge::from_command(id.as_str()) {
                        self.align_selection(edge);
                    }
                }
                "Align.Width" | "Align.Height" | "Align.Size" => {
                    if let Some(same) = crate::arrange::Same::from_command(id.as_str()) {
                        self.same_size(same);
                    }
                }
                "Align.SpacingHorizontal" => self.spread_selection(true),
                "Align.SpacingVertical" => self.spread_selection(false),
                "Align.CenterDocument" => self.centre_on_sheet(),
                "Align.FlipHorizontal" => self.flip_selection(true),
                "Align.FlipVertical" => self.flip_selection(false),
                "Markup.BringToFront" => self.reorder_selection(true, true),
                "Markup.SendToBack" => self.reorder_selection(false, true),
                "Markup.BringForward" => self.reorder_selection(true, false),
                "Markup.SendBackward" => self.reorder_selection(false, false),

                // ---- the properties toolbar ----
                "DropDown.ColorLine"
                | "DropDown.ColorFill"
                | "DropDown.ColorText"
                | "DropDown.Opacity"
                | "DropDown.HatchPattern"
                | "DropDown.LineWidth"
                | "DropDown.LineStyle"
                | "DropDown.LineStart"
                | "DropDown.LineEnd"
                | "Combo.Font"
                | "Combo.FontSize" => {
                    let want = crate::pen::Choosing::from_command(id.as_str());
                    // Clicking the same one again puts it away, the way a
                    // drop-down does everywhere else.
                    self.choosing = if self.choosing == want { None } else { want };
                }
                "Button.Bold" => self.change_pen(|pen| pen.text.bold = !pen.text.bold),
                "Button.Italic" => self.change_pen(|pen| pen.text.italic = !pen.text.italic),
                "Button.Underline" => {
                    self.change_pen(|pen| pen.text.underline = !pen.text.underline)
                }
                "Button.Strikethrough" => {
                    self.change_pen(|pen| pen.text.strike = !pen.text.strike)
                }
                "Button.TextAlignLeft" => {
                    self.change_pen(|pen| pen.text.across = annot::text::Across::Left)
                }
                "Button.TextAlignCenter" => {
                    self.change_pen(|pen| pen.text.across = annot::text::Across::Middle)
                }
                "Button.TextAlignRight" => {
                    self.change_pen(|pen| pen.text.across = annot::text::Across::Right)
                }
                "Button.TextVAlignTop" => {
                    self.change_pen(|pen| pen.text.down = annot::text::Down::Top)
                }
                "Button.TextVAlignMiddle" => {
                    self.change_pen(|pen| pen.text.down = annot::text::Down::Middle)
                }
                "Button.TextVAlignBottom" => {
                    self.change_pen(|pen| pen.text.down = annot::text::Down::Bottom)
                }
                "Button.Superscript" => self.change_pen(|pen| {
                    pen.text.raised = if pen.text.raised > 0 { 0 } else { 1 }
                }),
                "Button.Subscript" => self.change_pen(|pen| {
                    pen.text.raised = if pen.text.raised < 0 { 0 } else { -1 }
                }),
                "Edit.Offset" => self.begin_repeat(false),
                "Document.Summary" => self.export_report(),
                "Document.ShopList" => self.shop_list(),
                "Document.Doubles" => self.check_doubles(),
                "Document.RevisionCost" => self.revision_cost(),
                "Edit.Multiply" => self.begin_repeat(true),
                "Help.About" => self.about = true,
                "Help.CheckForUpdates" => self.check_for_updates_now(),
                "Help.ConnectClaude" if !crate::edition::reaches_other_programs() => {
                    self.status = crate::edition::not_in_this_copy("Connecting Claude Desktop");
                }
                "Help.ConnectClaude" => self.connect_claude(),
                "Help.TestClaude" => self.test_claude(),
                "Help.Shortcuts" => self.shortcuts_open = true,
                "View.Rulers" => {
                    self.prefs.rulers = !self.prefs.rulers;
                    let _ = self.prefs.save();
                }
                "View.Crosshair" => {
                    self.prefs.crosshair = !self.prefs.crosshair;
                    let _ = self.prefs.save();
                }
                "View.MarkupText" => {
                    self.prefs.markup_text = !self.prefs.markup_text;
                    let _ = self.prefs.save();
                    self.status = if self.prefs.markup_text {
                        "Markup text is shown over the sheet. View → Markup Text on Sheet turns it off.".into()
                    } else {
                        "Markup text is hidden; a selected markup still shows its own.".into()
                    };
                }
                "View.SnapToContent" => {
                    self.prefs.snap_to_content = !self.prefs.snap_to_content;
                    let _ = self.prefs.save();
                }
                "View.SnapToMarkup" => {
                    self.prefs.snap_to_markup = !self.prefs.snap_to_markup;
                    let _ = self.prefs.save();
                }
                "Batch.Combine" => self.begin_batch(crate::batchui::Which::Combine),
                "Batch.Split" => self.begin_batch(crate::batchui::Which::Split),
                "Batch.Stamp" => self.begin_batch(crate::batchui::Which::Stamp),
                "Batch.Rotate" => self.begin_batch(crate::batchui::Which::Rotate),
                "Batch.Flatten" => self.begin_batch(crate::batchui::Which::Flatten),
                "Batch.Shrink" => self.begin_batch(crate::batchui::Which::Shrink),
                "Batch.Ocr" => self.begin_batch(crate::batchui::Which::Ocr),
                "Batch.Seal" => self.begin_batch(crate::batchui::Which::Seal),
                "Batch.SlipSheet" => self.begin_batch(crate::batchui::Which::SlipSheet),
                "Batch.Compare" => self.begin_batch(crate::batchui::Which::Compare),
                "Batch.Overlay" => self.begin_batch(crate::batchui::Which::Overlay),
                "Batch.Crop" => self.begin_batch(crate::batchui::Which::Crop),
                "Batch.ApplyStamp" => self.begin_batch(crate::batchui::Which::ApplyStamp),
                "Batch.Print" => self.begin_batch(crate::batchui::Which::Print),
                "Batch.Summary" => self.begin_batch(crate::batchui::Which::Summary),
                "Batch.Security" => self.begin_batch(crate::batchui::Which::Secure),
                "Document.Stamp" | "Document.HeadersAndFooters" => {
                    self.begin_document_job(crate::docui::Job::Stamp)
                }
                "Document.Shrink" => self.begin_document_job(crate::docui::Job::Shrink),
                "Document.Seal" | "Document.Sign" => {
                    self.begin_document_job(crate::docui::Job::Seal)
                }
                "Document.Fingerprint" => self.show_fingerprint(),
                "Document.Properties" => self.show_properties(),
                "SpellCheck" => self.begin_spell_check(),
                "Edit.SelectText" => {
                    self.tool = crate::app::Tool::Highlight;
                    self.chrome.tool = "Markup.Highlight".into();
                    self.status = "Drag over the words you want. Highlight, Underline, \
                                   Strikethrough and Squiggly all work the same way."
                        .into();
                }
                "CropImage" => self.begin_document_job(crate::docui::Job::Crop),
                "Markup.ImageFromCamera" => {
                    // Honest rather than absent: there is no camera on a shop
                    // workstation, and a button that opened nothing would be
                    // worse than one that says why.
                    self.status = "There is no camera on this machine. Take the \
                                   photograph with your phone, put it on the server, \
                                   and use Image."
                        .into();
                }
                "Document.Script" => {
                    self.status = "Excalibur View does not run scripts, on purpose: a drawing \
                                   set that can run code is a drawing set that can do \
                                   something you did not ask for. Batch does the jobs a \
                                   script would."
                        .into();
                }
                "Combo.DMS" | "DMS.Login" | "DMS.Open" | "DMS.SaveAs" | "DMS.CheckIn" => {
                    self.status = "Excalibur View has no document management system in it. \
                                   Studio is where the company's drawings \
                                   live."
                        .into();
                    self.panel = "Studio".into();
                }
                // Things a profile carries that mean nothing on their own.
                "Blank" | "Press.EscapeKey" | "Toggle.ShiftKey" | "TextBox.Page"
                | "Navigation.PageScale" | "Navigation.PageSize" | "Combo.Scale"
                | "Label.LinePreview" => {}
                "Form.Editor" => {
                    self.form_editor = !self.form_editor;
                    self.status = if self.form_editor {
                        "Form editor on: every field on the sheet is outlined and \
                         named, and clicking one opens its settings."
                            .into()
                    } else {
                        "Form editor off.".into()
                    };
                }
                "Tab.Spaces" => self.panel = "Spaces".into(),
                "Split.Dimmer" => {
                    self.dimmed = !self.dimmed;
                    self.status = if self.dimmed {
                        "The drawing is dimmed so the markups stand out.".into()
                    } else {
                        "The drawing is back to normal.".into()
                    };
                }
                "Split.Profiles" => self.panel = "Tool Chest".into(),
                "ControlPoint.AddMode" | "ControlPoint.SubtractMode"
                | "ControlPoint.ConvertMode" => {
                    self.points_mode = crate::points::Mode::from_command(id.as_str());
                    self.status = match self.points_mode {
                        Some(mode) => mode.hint().into(),
                        None => "Editing points is off.".into(),
                    };
                }
                "Document.Repair" => self.begin_document_job(crate::docui::Job::Repair),
                "Document.Unflatten" => self.begin_document_job(crate::docui::Job::Unflatten),
                "Document.PageLabels" => {
                    self.begin_document_job(crate::docui::Job::PageLabels)
                }
                "Document.ApplyRedactions" => {
                    self.begin_document_job(crate::docui::Job::Redact)
                }
                "Document.Export" => self.begin_document_job(crate::docui::Job::Export),
                "Split.Security" | "Document.Security" => {
                    self.begin_document_job(crate::docui::Job::Secure)
                }
                "Document.Ocr" | "Document.OCR" => {
                    self.begin_document_job(crate::docui::Job::Ocr)
                }
                "Document.Overlay" => self.begin_overlay(),
                "Document.RotatePages" | "Document.RotateClockwise" => {
                    self.begin_document_job(crate::docui::Job::Rotate)
                }
                "Document.InsertPages" => self.begin_document_job(crate::docui::Job::Insert),
                "Document.ExtractPages" => self.begin_document_job(crate::docui::Job::Extract),
                "Document.SplitDocument" => self.begin_document_job(crate::docui::Job::Split),
                "Document.SlipSheet" => self.begin_document_job(crate::docui::Job::SlipSheet),
                "Document.DeletePages" => self.begin_document_job(crate::docui::Job::Delete),
                "Document.CropPages" => self.begin_document_job(crate::docui::Job::Crop),
                "Document.FlattenMarkups" | "Document.Flatten" => {
                    self.begin_document_job(crate::docui::Job::Flatten)
                }
                "File.LoadChest" => self.pick_and_load_chest(),
                "File.OpenModel" => self.pick_and_open_model(),
                "File.SaveChest" => self.save_chest_as(),
                "File.Preferences" => {
                    self.editing_prefs = Some(self.prefs.clone());
                }
                "Server.Connect" => {
                    if self.standing.signed_in() {
                        self.panel = "Studio".into();
                    } else {
                        self.begin_sign_in();
                    }
                }
                "View.FitPage" | "View.FitWidth" | "View.OneFullPage" => {
                    if let Some(doc) = self.doc_mut() {
                        doc.view.fit_requested = true;
                    }
                }
                "Measure.ApplyFill" => self.apply_fill(),
                "View.Split" | "View.SplitVertical" => {
                    if let Some(doc) = self.doc_mut() {
                        let want = if doc.split == crate::sheet::Split::Vertical {
                            crate::sheet::Split::None
                        } else {
                            crate::sheet::Split::Vertical
                        };
                        doc.set_split(want);
                    }
                }
                "View.SplitHorizontal" => {
                    if let Some(doc) = self.doc_mut() {
                        let want = if doc.split == crate::sheet::Split::Horizontal {
                            crate::sheet::Split::None
                        } else {
                            crate::sheet::Split::Horizontal
                        };
                        doc.set_split(want);
                    }
                }
                "View.UnSplit" => {
                    if let Some(doc) = self.doc_mut() {
                        doc.set_split(crate::sheet::Split::None);
                    }
                }
                "View.SyncPanes" => {
                    if let Some(doc) = self.doc_mut() {
                        doc.sync_panes = !doc.sync_panes;
                        let on = doc.sync_panes;
                        self.status = if on {
                            "Both panes now move together.".into()
                        } else {
                            "The panes move separately.".into()
                        };
                    }
                }
                "View.Single"
                | "View.Continuous"
                | "View.SideBySide"
                | "View.SideBySideContinuous"
                | "View.ScrollingPages" => {
                    if let Some(want) = crate::layout::Layout::from_command(id.as_str()) {
                        if let Some(doc) = self.doc_mut() {
                            doc.layout = want;
                            doc.choose(None);
                        }
                        self.status = format!("{}.", want.name());
                    }
                }
                "View.ShowGrid" => {
                    self.prefs.grid = !self.prefs.grid;
                    let _ = self.prefs.save();
                    self.status = if self.prefs.grid {
                        "The grid is on.".into()
                    } else {
                        "The grid is off.".into()
                    };
                }
                "View.SnapToGrid" => {
                    self.prefs.snap_to_grid = !self.prefs.snap_to_grid;
                    let _ = self.prefs.save();
                    self.status = if self.prefs.snap_to_grid {
                        "Points now pull onto the grid.".into()
                    } else {
                        "Points no longer pull onto the grid.".into()
                    };
                }
                "View.PreviousView" => self.go_back_a_view(),
                "View.NextView" => self.go_forward_a_view(),
                "Toggle.FullScreen" => {
                    self.full_screen = !self.full_screen;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.full_screen));
                }
                "View.ActualSize" => {
                    if let Some(doc) = self.doc_mut() {
                        doc.view.zoom = 1.0;
                    }
                }
                "View.PageFirst" => self.go_to(0),
                "View.PageLast" => {
                    let last = self
                        .doc()
                        .map(|d| d.pages.len().saturating_sub(1) as u32);
                    if let Some(last) = last {
                        self.go_to(last);
                    }
                }
                "View.PageNext" => {
                    let page = self.doc().map(|d| d.page);
                    if let Some(page) = page {
                        self.go_to(page + 1);
                    }
                }
                "View.PagePrevious" => {
                    let page = self.doc().map(|d| d.page);
                    if let Some(page) = page.filter(|p| *p > 0) {
                        self.go_to(page - 1);
                    }
                }
                "Edit.Undo" => {
                    if let Some(doc) = self.doc_mut() {
                        if !doc.undo() {
                            self.status = "Nothing left to undo.".into();
                        }
                    }
                }
                "Edit.Redo" => {
                    if let Some(doc) = self.doc_mut() {
                        if !doc.redo() {
                            self.status = "Nothing left to put back.".into();
                        }
                    }
                }
                "Edit.UndoHistory" => self.history_open = !self.history_open,
                "Edit.Copy" => {
                    self.copy_selection();
                }
                "Edit.Cut" => self.cut_selection(),
                "Edit.Paste" => self.paste(false),
                "Edit.PasteInPlace" => self.paste(true),
                "Delete" => self.delete_selected(),
                "View.InvertColors" => {
                    let on = self.chrome.is_on("View.InvertColors");
                    self.chrome.theme = if on {
                        ui::Theme::light()
                    } else {
                        ui::Theme::dark()
                    };
                    self.chrome.theme.apply(ctx);
                }
                "Plugins.Add" => self.add_plugin(),
                "Plugins.Manage" => self.managing_plugins = true,
                "Plugins.SaveInput" => self.save_plugin_input(ctx, false),
                "Plugins.SaveInputAll" => self.save_plugin_input(ctx, true),
                other if other.starts_with("Plugin:") => self.start_plugin(other, ctx),
                other => {
                    // Tools change what the canvas does.
                    if let Some(tool) = tool_for(other) {
                        self.finish_draft();
                        // Putting Dynamic Fill down forgets the boundaries
                        // somebody struck for it. They belong to that fill,
                        // not to the drawing.
                        if self.tool.is_filling() && !tool.is_filling() {
                            self.filling.reset();
                        }
                        self.tool = tool;
                    }
                }
            }
        }
    }

    /// Turns a filled shape into an area measurement.
    ///
    /// This is where a fill stops being a proposal. Everything else about the
    /// markup is the same as one drawn by hand — the same tool from the chest,
    /// the same dictionary, the same weights — because a fabricator should not
    /// have to care which way a quantity was picked up, only that it was.
    pub fn apply_fill(&mut self) {
        let Some((outline, holes, area, enclosed)) = self.filling.ready().map(|r| {
            (
                r.outline.clone(),
                r.holes.clone(),
                r.area,
                r.enclosed,
            )
        }) else {
            self.status = "There is no filled area to apply.".into();
            return;
        };
        let page = self.filling.page;
        let subject = self.subject.clone();
        let cutout_subject = subject_for_cutout(&subject);
        let colour = self.color;
        let template = self.template.clone();

        let on_this_sheet = self.doc().map(|d| d.page) == Some(page);
        if !on_this_sheet {
            self.status = "That fill was made on another sheet.".into();
            return;
        }
        // The shape is perfectly good; an unscaled sheet just cannot say what
        // it means. It goes on as a markup with no number, and the Markups
        // list reports it as left out — which is the truth.
        let unscaled = self.doc().map(|d| d.scale_of(page).is_none()).unwrap_or(true);
        if unscaled {
            self.status = "This sheet has no scale, so the area has not been measured. \
                           Set the scale and it will read."
                .into();
        }

        let Some(doc) = self.doc_mut() else { return };
        let draft = crate::sheet::Draft {
            tool: Tool::Area,
            points: outline,
            strokes: Vec::new(),
            subject,
            colour,
            template,
            ..crate::sheet::Draft::default()
        };
        let Some(index) = doc.place(page, draft) else {
            self.error = Some("That shape could not be turned into a markup.".into());
            return;
        };
        doc.selected = Some(index);
        let reads = doc
            .marks
            .get(index)
            .map(|m| m.markup.contents())
            .unwrap_or_default();

        // Every opening the fill went round becomes its own cutout markup,
        // because a single closed path cannot have a hole in it. Without these
        // the measurement would read the column as slab — quietly, and high.
        let mut cut = 0usize;
        let mut cut_area = 0.0f64;
        for hole in &holes {
            if hole.len() < 3 {
                continue;
            }
            let this = takeoff::fill::area_of(hole);
            let cutout = crate::sheet::Draft {
                tool: Tool::AreaCutout,
                points: hole.clone(),
                strokes: Vec::new(),
                subject: cutout_subject.clone(),
                colour,
                template: None,
                ..crate::sheet::Draft::default()
            };
            if doc.place(page, cutout).is_some() {
                cut += 1;
                cut_area += this;
            }
        }

        if !unscaled {
            let mut said = if reads.is_empty() {
                format!("Area added from the fill ({area:.0} square points on the sheet).")
            } else {
                format!("Area added: {reads}.")
            };
            if cut > 0 {
                said.push_str(&format!(
                    " {cut} opening{} cut out of it.",
                    if cut == 1 { "" } else { "s" }
                ));
            }
            // Whatever the outline encloses and no cutout covers is measured
            // as though it were floor. However small it is, it is said out
            // loud: a number that is quietly high is the thing this program
            // exists not to produce.
            let over = enclosed - cut_area;
            if over > 0.5 {
                let share = over / (area + enclosed).max(1.0) * 100.0;
                said.push_str(&format!(
                    " {over:.0} square points ({share:.1}%) enclosed by the outline were not \
                     filled and are too small to cut out, so this area reads high by that much."
                ));
            }
            self.status = said;
        }
        self.filling.clear();
    }

    /// Writes the markups into the file. Says what it wrote, or why it could not.
    pub fn save_now(&mut self) {
        let author = self.author.clone();
        let Some(doc) = self.doc_mut() else { return };
        match doc.save(&author) {
            Ok(0) => self.status = "Nothing to save.".into(),
            Ok(n) => {
                self.status = format!(
                    "Saved: {n} markup{} written into the drawing.",
                    if n == 1 { "" } else { "s" }
                )
            }
            Err(e) => self.error = Some(format!("Could not save: {e}")),
        }
        // Into the file first, then up to the server. That order is the whole
        // policy: what the office sees is a copy of what is already saved here,
        // so a server that is down costs the sync and nothing else.
        self.sync_now();
    }

    /// Sends what this machine has and brings down what it has not seen.
    ///
    /// Does nothing at all when the drawing did not come from a server, or
    /// when there is no server to reach. Both are ordinary.
    pub fn sync_now(&mut self) {
        self.sync_with(false);
    }

    /// Sends and fetches, quietly: what the clock does while a shared drawing
    /// is open, so other people's markups arrive without anybody asking.
    pub fn sync_quietly(&mut self) {
        self.sync_with(true);
    }

    fn sync_with(&mut self, quiet: bool) {
        use base64::Engine;
        self.sync_asked = Some(std::time::Instant::now());
        let Some(doc) = self.doc_mut() else { return };
        let Some(attached) = doc.attached.clone() else {
            return;
        };
        let sending: Vec<(usize, String)> = crate::sync::to_send(doc.marks.iter_mut(), &attached)
            .into_iter()
            .map(|(page, markup)| {
                let mut bytes = Vec::new();
                pdf::write::write_object(&pdf::Object::Dict(markup.dict), &mut bytes);
                (
                    page as usize,
                    base64::engine::general_purpose::STANDARD.encode(bytes),
                )
            })
            .collect();
        self.ask(crate::server::Ask::Sync {
            set: attached.set,
            since: attached.revision,
            sending,
            quiet,
        });
    }
}

/// A line of context with the matched words picked out, so somebody scanning
/// the list can see what they actually found rather than the same word twenty
/// times.
fn context_line(
    ui: &egui::Ui,
    hit: &crate::render::Hit,
    theme: ui::Theme,
    chosen: bool,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    use egui::text::{LayoutJob, TextFormat};
    let ordinary = TextFormat {
        font_id: egui::FontId::proportional(11.5),
        color: if chosen { theme.text } else { theme.faint },
        ..Default::default()
    };
    let found = TextFormat {
        font_id: egui::FontId::proportional(11.5),
        color: theme.accent_text,
        background: theme.accent.gamma_multiply(0.35),
        ..Default::default()
    };
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    job.wrap.max_rows = 2;
    job.wrap.overflow_character = Some('\u{2026}');
    // The range came from the same string, but a panel is not the place to
    // find that out the hard way if it ever stops being true.
    let at = hit.at.clone();
    if at.end <= hit.context.len() && hit.context.is_char_boundary(at.start)
        && hit.context.is_char_boundary(at.end)
    {
        job.append(&hit.context[..at.start], 0.0, ordinary.clone());
        job.append(&hit.context[at.clone()], 0.0, found);
        job.append(&hit.context[at.end..], 0.0, ordinary);
    } else {
        job.append(&hit.context, 0.0, ordinary);
    }
    ui.fonts(|f| f.layout_job(job))
}

/// What a cutout taken out of a filled area is called, so the Markups list
/// shows what it belongs to rather than a row named nothing.
fn subject_for_cutout(subject: &str) -> String {
    if subject.trim().is_empty() {
        "Cutout".to_string()
    } else {
        format!("{subject} cutout")
    }
}

/// One cell of the markups grid.
/// What a grouped line reads as: the thing the group actually measures.
pub(crate) fn total_of(group: &takeoff::Group, doc: &crate::sheet::Doc) -> String {
    use annot::Kind;
    let scale = group
        .pages
        .first()
        .and_then(|p| doc.scale_of(*p as u32));
    match (group.kind, scale) {
        (Kind::Count, _) => format!("{:.0}", group.count),
        (Kind::Area | Kind::Volume, Some(s)) => {
            let per = s.per_point();
            s.area(group.area / (per * per))
        }
        (_, Some(s)) => s.length(group.length / s.per_point()),
        (_, None) => "no scale".to_string(),
    }
}

fn default_width(column: Column) -> f32 {
    match column {
        Column::Subject => 190.0,
        Column::Measurement | Column::Comments | Column::Label => 140.0,
        Column::Author | Column::Date | Column::CreationDate => 110.0,
        Column::Colour => 56.0,
        Column::Quantity => 48.0,
        Column::Page => 70.0,
        _ => 80.0,
    }
}

/// Maps a command onto what the canvas should do with the mouse.
pub fn tool_for(id: &str) -> Option<Tool> {
    Some(match id {
        "Select" => Tool::Select,
        "Lasso" => Tool::Lasso,
        "Pan" | "Zoom" => Tool::Pan,

        // ---- measurement ----
        "Measure.Calibrate" => Tool::Calibrate,
        "Measure.Length" | "Measure.Polylength" => Tool::Length,
        "Measure.Perimeter" => Tool::Perimeter,
        "Measure.Area" => Tool::Area,
        "Measure.Volume" => Tool::Volume,
        "Measure.AreaCutout" => Tool::AreaCutout,
        "Measure.AreaEllipseCutout" => Tool::EllipseCutout,
        "Measure.Diameter" => Tool::Diameter,
        "Measure.Radius" => Tool::Radius,
        "Measure.Angle" => Tool::Angle,
        "Measure.Count" => Tool::Count,
        "Measure.DynamicFill" => Tool::Fill,
        "Measure.FillBoundary" => Tool::FillBoundary,

        // ---- shapes ----
        "Markup.Rectangle" => Tool::Rect,
        "Markup.Redaction" => Tool::Redaction,
        "Markup.Ellipse" => Tool::Ellipse,
        "Markup.Polygon" => Tool::Polygon,
        "Markup.Cloud" => Tool::Cloud,
        "Markup.Cloud9" => Tool::CloudPolygon,
        "Markup.Line" => Tool::Arrow,
        "Markup.Arrow" => Tool::Arrow,
        "Markup.Polyline" => Tool::Polyline,
        "Markup.Arc" => Tool::Arc,
        "Markup.Dimension" => Tool::Dimension,
        "Markup.Pen" => Tool::Ink,
        "Eraser" | "Edit.EraseContent" => Tool::Eraser,

        // ---- words ----
        "Markup.TextBox" => Tool::Text,
        "Markup.Typewriter" | "Edit.Text" => Tool::Typewriter,
        "Markup.Callout" => Tool::Callout,
        "Markup.Note" | "Markup.ReviewText" => Tool::Note,
        "Markup.Flag" => Tool::Flag,
        "Markup.Highlight" => Tool::Highlight,
        "Markup.Underline" => Tool::Underline,
        "Markup.Strikethrough" => Tool::Strikethrough,
        "Markup.Squiggly" => Tool::Squiggly,

        // ---- things put on the sheet ----
        "Markup.Image" => Tool::Image,
        "Button.Stamp" => Tool::Stamp,
        "Markup.Hyperlink" => Tool::Hyperlink,
        "Markup.FileAttachment" => Tool::FileAttachment,
        "Space.Add" => Tool::Space,
        "Snapshot" => Tool::Snapshot,

        // ---- form fields ----
        "Form.TextField" => Tool::FormText,
        "Form.CheckBox" => Tool::FormCheckBox,
        "Form.RadioButton" => Tool::FormRadio,
        "Form.ListBox" => Tool::FormList,
        "Form.ComboBox" => Tool::FormCombo,
        "Form.Button" => Tool::FormButton,
        "Form.DigitalSignature" | "Markup.DigitalSignature" => Tool::FormSignature,

        // ---- sketch to scale ----
        "Markup.PolygonSketchToScale" => Tool::SketchPolygon,
        "Markup.RectangleSketchToScale" => Tool::SketchRect,
        "Markup.EllipseSketchToScale" => Tool::SketchEllipse,
        "Markup.PolylineSketchToScale" => Tool::SketchPolyline,
        _ => return None,
    })
}

/// The little triangle that shows whether a group is open.
pub fn twist(painter: &egui::Painter, rect: egui::Rect, open: bool, colour: Color32) {
    let c = rect.center();
    let points = if open {
        vec![
            egui::pos2(c.x - 4.0, c.y - 2.0),
            egui::pos2(c.x + 4.0, c.y - 2.0),
            egui::pos2(c.x, c.y + 3.0),
        ]
    } else {
        vec![
            egui::pos2(c.x - 2.0, c.y - 4.0),
            egui::pos2(c.x + 3.0, c.y),
            egui::pos2(c.x - 2.0, c.y + 4.0),
        ]
    };
    painter.add(egui::epaint::PathShape {
        points,
        closed: true,
        fill: colour,
        stroke: egui::Stroke::NONE.into(),
    });
}

impl App {
    // ---- preferences -----------------------------------------------------

    pub fn prefs_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.editing_prefs.take() else {
            return;
        };
        let theme = self.chrome.theme;
        let mut keep = true;
        let mut apply = false;
        egui::Window::new("Preferences")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .max_width(700.0)
            .show(ctx, |ui| {
                ui.set_min_width(460.0);

                ui.label(RichText::new("Your name on a markup").strong());
                ui.add(
                    egui::TextEdit::singleline(&mut draft.author)
                        .hint_text("First and last name")
                        .desired_width(220.0),
                );
                ui.label(
                    RichText::new("Goes in the Author column, so the office can tell whose takeoff is whose.")
                        .color(theme.faint)
                        .size(11.0),
                );

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(6.0);

                ui.label(RichText::new("Measurements").strong());
                ui.horizontal(|ui| {
                    ui.label("Round to");
                    egui::ComboBox::from_id_salt("denominator")
                        .selected_text(format!("1/{}\"", draft.denominator))
                        .width(90.0)
                        .show_ui(ui, |ui| {
                            for d in [2u32, 4, 8, 16, 32, 64] {
                                if ui
                                    .selectable_label(draft.denominator == d, format!("1/{d}\""))
                                    .clicked()
                                {
                                    draft.denominator = d;
                                }
                            }
                        });
                    ui.label("Units");
                    egui::ComboBox::from_id_salt("units")
                        .selected_text(draft.units.name())
                        .width(140.0)
                        .show_ui(ui, |ui| {
                            for u in crate::units::Units::ALL {
                                if ui.selectable_label(draft.units == u, u.name()).clicked() {
                                    draft.units = u;
                                }
                            }
                        });
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(6.0);

                ui.label(RichText::new("What goes into the file").strong());
                ui.checkbox(
                    &mut draft.captions_in_file,
                    "Draw measurement numbers into the drawing",
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new(
                        "On, the number is drawn into the file, so it shows in Acrobat, in a \
                         browser and on a general contractor's screen. Bluebeam does not do \
                         this — it keeps the number and paints it in Revu only, which is why a \
                         Revu takeoff looks blank to everyone else.",
                    )
                    .color(theme.faint)
                    .size(11.0),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Turn it off if you are sending a drawing back to somebody working in \
                         Revu and you see the number twice. Either way the number itself is \
                         stored the same, so nothing is lost.",
                    )
                    .color(theme.faint)
                    .size(11.0),
                );

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(6.0);

                ui.label(RichText::new("Mouse").strong());
                ui.checkbox(&mut draft.wheel_zooms, "Scroll wheel zooms");

                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(path) = crate::prefs::Prefs::path() {
                            ui.label(
                                RichText::new(format!("Kept in {}", path.display()))
                                    .color(theme.faint)
                                    .size(10.0),
                            );
                        }
                    });
                });
            });

        if apply {
            self.units = draft.units;
            self.denominator = draft.denominator;
            self.wheel_zooms = draft.wheel_zooms;
            if !draft.author.trim().is_empty() {
                self.author = draft.author.trim().to_string();
            }
            let drawing = draft.drawing();
            if let Some(doc) = self.doc_mut() {
                doc.drawing = drawing;
            }
            match draft.save() {
                Ok(()) => self.status = "Preferences saved.".into(),
                Err(e) => self.error = Some(format!("Could not keep the settings: {e}")),
            }
            self.prefs = draft;
        } else if keep {
            self.editing_prefs = Some(draft);
        }
    }

    /// Saves the drawing and its markups under a new name, which is also the
    /// way out of a read-only original.
    pub fn save_as(&mut self) {
        let Some(doc) = self.doc() else { return };
        let suggested = doc
            .path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "drawing.pdf".into());
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(suggested)
            .add_filter("PDF", &["pdf"])
            .save_file()
        else {
            return;
        };
        let author = self.author.clone();
        let Some(doc) = self.doc_mut() else { return };
        // Point the document at the new file first, then save into it. The
        // original keeps whatever it had.
        let was = std::mem::replace(&mut doc.path, path.clone());
        if was != path {
            if let Err(e) = std::fs::copy(&was, &path) {
                doc.path = was;
                self.error = Some(format!("Could not write {}: {e}", path.display()));
                return;
            }
            // Everything in memory has to be written into the new copy.
            doc.dirty = true;
            for mark in doc.marks.iter_mut() {
                mark.changed = true;
            }
        }
        match doc.save(&author) {
            Ok(_) => self.status = format!("Saved as {}", path.display()),
            Err(e) => {
                self.error = Some(format!("Could not save: {e}"));
            }
        }
    }
}

/// What the Thumbnails panel was asked to do with the picked sheets.
#[derive(Clone, Copy, Debug, PartialEq)]
enum PickAction {
    Print,
    Save,
    Clear,
    All,
}

/// What the key held to pick several things is called on this computer.
fn command_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    }
}

/// The keys held when the mouse button came up. Read off the click itself
/// rather than the keyboard now, so a quick Ctrl-click whose Ctrl is already
/// let go by the time the frame is drawn still counts as one.
fn click_modifiers(ui: &egui::Ui) -> egui::Modifiers {
    ui.input(|i| {
        i.events
            .iter()
            .rev()
            .find_map(|event| match event {
                egui::Event::PointerButton {
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers,
                    ..
                } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or(i.modifiers)
    })
}
