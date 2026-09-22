//! Revision cost, on screen.
//!
//! Open the issue that was bid and the issue that arrived, pick one on each
//! side, and this says what moved and what it is worth. It compares the
//! takeoffs, not the paper — Compare Documents already shows what changed on
//! the sheet, and that is a different question from what it did to the
//! tonnage.
//!
//! Both drawings have to be open. That is on purpose: the comparison is made
//! from live takeoffs, with their own scales and their own markups, rather
//! than from a spreadsheet somebody exported last week and may have edited.

use egui::RichText;
use takeoff::revision::{self, Revision, What};

use crate::app::App;

#[derive(Default)]
pub struct Costing {
    pub open: bool,
    /// Which open drawing was bid, by its place in the tabs.
    pub before: usize,
    /// And which one arrived.
    pub after: usize,
    /// Show the subjects that did not move.
    pub show_unchanged: bool,
}

impl App {
    /// The command behind **Document ▸ Revision Cost**.
    pub fn revision_cost(&mut self) {
        if self.docs.len() < 2 {
            self.status = "Open both issues of the drawing first — the one that was \
                           bid and the one that arrived."
                .into();
            return;
        }
        self.costing.open = true;
        // The one in front is almost always the new issue, and the one before
        // it in the tabs is almost always what it is being compared against.
        self.costing.after = self.current;
        self.costing.before = if self.current == 0 { 1 } else { 0 };
    }

    /// The revision as the two chosen drawings stand.
    pub fn revision_now(&self) -> Option<Revision> {
        let before = self.docs.get(self.costing.before)?;
        let after = self.docs.get(self.costing.after)?;
        Some(revision::compare(
            &before.rows(),
            &after.rows(),
            crate::panelbody::weights(),
        ))
    }

    pub fn revision_window(&mut self, ctx: &egui::Context) {
        if !self.costing.open {
            return;
        }
        let theme = self.chrome.theme;
        let names: Vec<String> = self
            .docs
            .iter()
            .map(|doc| {
                doc.path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        if names.len() < 2 {
            self.costing.open = false;
            return;
        }
        self.costing.before = self.costing.before.min(names.len() - 1);
        self.costing.after = self.costing.after.min(names.len() - 1);

        let mut open = true;
        let mut before = self.costing.before;
        let mut after = self.costing.after;
        let mut show_unchanged = self.costing.show_unchanged;
        let mut save_csv = false;
        let Some(revision) = self.revision_now() else {
            self.costing.open = false;
            return;
        };

        egui::Window::new("Revision Cost")
            .collapsible(false)
            .resizable(true)
            .default_width(680.0)
            .default_height(520.0)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Was bid").size(11.0));
                    egui::ComboBox::from_id_salt("revision before")
                        .selected_text(names[before].clone())
                        .width(240.0)
                        .show_ui(ui, |ui| {
                            for (at, name) in names.iter().enumerate() {
                                ui.selectable_value(&mut before, at, name);
                            }
                        });
                    ui.label(RichText::new("→").color(theme.faint).size(13.0));
                    ui.label(RichText::new("Arrived").size(11.0));
                    egui::ComboBox::from_id_salt("revision after")
                        .selected_text(names[after].clone())
                        .width(240.0)
                        .show_ui(ui, |ui| {
                            for (at, name) in names.iter().enumerate() {
                                ui.selectable_value(&mut after, at, name);
                            }
                        });
                });
                if before == after {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Those are the same drawing.")
                            .color(theme.faint)
                            .size(11.0),
                    );
                    return;
                }

                ui.add_space(8.0);
                ui.label(RichText::new(revision.headline()).strong().size(13.0));
                let missing = revision.what_is_missing();
                if !missing.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(missing).color(theme.warn).size(11.0));
                }
                ui.add_space(8.0);
                ui.checkbox(&mut show_unchanged, "Also show what did not move");
                ui.separator();

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(320.0)
                    .show(ui, |ui| {
                        egui::Grid::new("revision")
                            .num_columns(6)
                            .striped(true)
                            .spacing(egui::vec2(16.0, 4.0))
                            .show(ui, |ui| {
                                for head in
                                    ["Subject", "", "Was", "Now", "Change", "Weight"]
                                {
                                    ui.label(
                                        RichText::new(head).color(theme.faint).size(10.0),
                                    );
                                }
                                ui.end_row();
                                for line in &revision.moved {
                                    if !line.what.moved() && !show_unchanged {
                                        continue;
                                    }
                                    ui.label(RichText::new(&line.subject).size(11.0));
                                    ui.label(
                                        RichText::new(line.what.name())
                                            .color(match line.what {
                                                What::Arrived | What::More => theme.warn,
                                                What::Gone | What::Less => theme.accent_text,
                                                What::Same => theme.faint,
                                            })
                                            .size(10.0),
                                    );
                                    ui.label(
                                        RichText::new(format!(
                                            "{:.2}{}",
                                            line.quantity.0,
                                            unit_of(&line.unit)
                                        ))
                                        .size(11.0),
                                    );
                                    ui.label(
                                        RichText::new(format!(
                                            "{:.2}{}",
                                            line.quantity.1,
                                            unit_of(&line.unit)
                                        ))
                                        .size(11.0),
                                    );
                                    ui.label(
                                        RichText::new(format!(
                                            "{:+.2}{}",
                                            line.quantity_moved(),
                                            unit_of(&line.unit)
                                        ))
                                        .size(11.0),
                                    );
                                    // `None` says so. It never reads as zero.
                                    ui.label(match line.pounds_moved() {
                                        Some(moved) => {
                                            RichText::new(format!("{moved:+.0} lb")).size(11.0)
                                        }
                                        None => RichText::new("no unit weight")
                                            .color(theme.warn)
                                            .size(10.0),
                                    });
                                    ui.end_row();
                                }
                            });
                    });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Save as CSV").clicked() {
                        save_csv = true;
                    }
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.label(
                                RichText::new(
                                    "Neither drawing is changed by this. It reads them \
                                     and works out the difference.",
                                )
                                .color(theme.faint)
                                .size(10.0),
                            );
                        },
                    );
                });
            });

        self.costing.before = before;
        self.costing.after = after;
        self.costing.show_unchanged = show_unchanged;
        if !open {
            self.costing.open = false;
        }
        if save_csv {
            let csv = revision::to_csv(&revision);
            self.save_beside("revision cost", "CSV", "csv", csv.into_bytes());
        }
    }
}

fn unit_of(unit: &str) -> String {
    if unit.is_empty() {
        String::new()
    } else {
        format!(" {unit}")
    }
}

#[cfg(test)]
mod tests {
    use takeoff::revision::{compare, What};
    use takeoff::{Row, WeightColumns};

    fn beam(subject: &str, feet: f64, per_foot: Option<&str>) -> Row {
        let mut row = Row::blank();
        row.subject = subject.into();
        row.kind = annot::Kind::Length;
        row.scaled = true;
        row.length = Some(feet);
        row.unit = "'".into();
        if let Some(per) = per_foot {
            row.columns[WeightColumns::default().per_length] = per.into();
        }
        row
    }

    #[test]
    fn the_window_never_writes_to_either_drawing() {
        // The contract: this reads two takeoffs and works out a difference.
        // The words are built up a piece at a time so the check does not trip
        // over its own list.
        let source = include_str!("revisionwindow.rs");
        for word in [
            format!("doc{}mut", "_"),
            format!("{}(", "save"),
            format!("dirty{} true", " ="),
        ] {
            assert!(!source.contains(&word), "a write path appeared: {word}");
        }
    }

    #[test]
    fn what_the_window_shows_is_what_the_comparison_says() {
        let before = vec![beam("W12x26", 20.0, Some("26"))];
        let after = vec![
            beam("W12x26", 20.0, Some("26")),
            beam("W12x26", 10.0, Some("26")),
        ];
        let rev = compare(&before, &after, WeightColumns::default());
        assert_eq!(rev.changes().next().unwrap().what, What::More);
        assert!((rev.pounds - 260.0).abs() < 1e-9);
        assert!(rev.headline().contains("added"));
    }
}
