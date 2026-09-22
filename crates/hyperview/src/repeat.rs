//! Offsetting a markup, and repeating one in a grid.
//!
//! Both exist because a drawing repeats itself. A bay of joists at four feet on
//! centre is one markup and twenty copies of it; a bolt pattern is one count
//! markup and a grid of them. Drawing each by hand is where an afternoon goes,
//! and — more to the point — it is where a miscount comes from, because the
//! twentieth one is drawn by somebody who has stopped looking.
//!
//! **A copy is a real markup.** Each one goes in the markups list, carries the
//! same tool, the same subject and the same unit weight, and counts in the
//! takeoff exactly as if it had been drawn. Nothing here estimates anything: a
//! person says how many and how far apart, and every one of the results is
//! something they asked for and can see, select and delete.

/// How far to move something, in the units the person typed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Offset {
    /// Across and up the sheet, in points.
    pub dx: f64,
    pub dy: f64,
}

/// A grid of copies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    /// Copies across, including the original.
    pub across: usize,
    /// Copies down, including the original.
    pub down: usize,
    /// Spacing between them, in points.
    pub spacing_x: f64,
    pub spacing_y: f64,
}

impl Default for Grid {
    fn default() -> Grid {
        Grid {
            across: 2,
            down: 1,
            spacing_x: 144.0,
            spacing_y: 144.0,
        }
    }
}

/// The most copies one command will make.
///
/// Not a technical limit — a guard against a typo. Somebody who meant 12 and
/// typed 1200 should be told, not left with a drawing that takes a minute to
/// open and a takeoff that is a hundred times too big.
pub const MOST_COPIES: usize = 400;

impl Grid {
    /// How many copies this makes, not counting the original.
    pub fn copies(&self) -> usize {
        (self.across.max(1) * self.down.max(1)).saturating_sub(1)
    }

    /// Where each copy goes, relative to the original. The original's own
    /// place is not in the list — it is already there.
    pub fn offsets(&self) -> Vec<Offset> {
        let mut out = Vec::new();
        for row in 0..self.down.max(1) {
            for column in 0..self.across.max(1) {
                if row == 0 && column == 0 {
                    continue;
                }
                out.push(Offset {
                    dx: column as f64 * self.spacing_x,
                    // Down the sheet is down on the screen, which in a PDF's own
                    // coordinates is *negative* — the one place this is easy to
                    // get backwards and produce a grid growing off the top.
                    dy: -(row as f64) * self.spacing_y,
                });
            }
        }
        out
    }

    /// Whether this is a sensible thing to do, and why not if it is not.
    pub fn sensible(&self) -> Result<(), String> {
        if self.across == 0 || self.down == 0 {
            return Err("That makes no copies.".into());
        }
        let copies = self.copies();
        if copies == 0 {
            return Err("That is one copy of itself, which is what is already there.".into());
        }
        if copies > MOST_COPIES {
            return Err(format!(
                "That is {copies} copies. This makes at most {MOST_COPIES} at a time — \
                 if that is really what you want, do it in two goes."
            ));
        }
        if self.spacing_x == 0.0 && self.spacing_y == 0.0 {
            return Err(
                "With no spacing they would all land on top of each other.".into()
            );
        }
        Ok(())
    }

    pub fn describe(&self) -> String {
        let copies = self.copies();
        format!(
            "{copies} {} — {} across by {} down.",
            if copies == 1 { "copy" } else { "copies" },
            self.across,
            self.down
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_of_four_makes_three_copies() {
        let grid = Grid {
            across: 4,
            down: 1,
            spacing_x: 48.0,
            spacing_y: 0.0,
        };
        assert_eq!(grid.copies(), 3);
        let offsets = grid.offsets();
        assert_eq!(offsets.len(), 3);
        assert_eq!(offsets[0], Offset { dx: 48.0, dy: 0.0 });
        assert_eq!(offsets[2], Offset { dx: 144.0, dy: 0.0 });
    }

    #[test]
    fn down_the_sheet_is_down_the_sheet() {
        // The one that is easy to get backwards: a PDF counts up from the
        // bottom, so a grid going down the drawing has negative offsets.
        let grid = Grid {
            across: 1,
            down: 3,
            spacing_x: 0.0,
            spacing_y: 100.0,
        };
        let offsets = grid.offsets();
        assert_eq!(offsets[0], Offset { dx: 0.0, dy: -100.0 });
        assert_eq!(offsets[1], Offset { dx: 0.0, dy: -200.0 });
    }

    #[test]
    fn a_grid_fills_row_by_row_and_leaves_out_the_original() {
        let grid = Grid {
            across: 3,
            down: 2,
            spacing_x: 10.0,
            spacing_y: 20.0,
        };
        let offsets = grid.offsets();
        assert_eq!(offsets.len(), 5, "six places, one of them already taken");
        assert!(!offsets.contains(&Offset { dx: 0.0, dy: 0.0 }));
        assert!(offsets.contains(&Offset { dx: 20.0, dy: -20.0 }));
    }

    #[test]
    fn a_typo_that_would_make_a_thousand_copies_is_refused_with_the_number() {
        let grid = Grid {
            across: 1200,
            down: 1,
            spacing_x: 10.0,
            spacing_y: 0.0,
        };
        let why = grid.sensible().expect_err("that is a typo");
        assert!(why.contains("1199"), "{why}");
        assert!(why.contains("two goes"), "{why}");
    }

    #[test]
    fn asking_for_one_copy_of_itself_says_so_rather_than_doing_nothing() {
        let grid = Grid {
            across: 1,
            down: 1,
            spacing_x: 10.0,
            spacing_y: 10.0,
        };
        assert!(grid.sensible().is_err());
        assert!(grid.offsets().is_empty());
    }

    #[test]
    fn copies_with_no_spacing_are_refused_rather_than_stacked() {
        let grid = Grid {
            across: 4,
            down: 1,
            spacing_x: 0.0,
            spacing_y: 0.0,
        };
        let why = grid.sensible().expect_err("they would stack");
        assert!(why.contains("on top of each other"));
    }

    #[test]
    fn nothing_at_all_is_refused_rather_than_dividing_by_it() {
        assert!(Grid { across: 0, down: 5, spacing_x: 1.0, spacing_y: 1.0 }
            .sensible()
            .is_err());
        assert!(Grid { across: 5, down: 0, spacing_x: 1.0, spacing_y: 1.0 }
            .sensible()
            .is_err());
    }

    #[test]
    fn the_description_counts_copies_not_places() {
        let grid = Grid {
            across: 4,
            down: 1,
            spacing_x: 48.0,
            spacing_y: 0.0,
        };
        assert!(grid.describe().starts_with("3 copies"), "{}", grid.describe());
    }
}

// ---- the windows -----------------------------------------------------------

use egui::{Color32, RichText};

use crate::app::App;

/// What is being set up, for either job.
#[derive(Clone, Debug)]
pub struct Repeating {
    pub multiply: bool,
    /// Typed in whatever units the person works in.
    pub across_text: String,
    pub down_text: String,
    pub spacing_x_text: String,
    pub spacing_y_text: String,
    pub error: Option<String>,
}

impl Repeating {
    pub fn new(multiply: bool) -> Repeating {
        Repeating {
            multiply,
            across_text: if multiply { "2".into() } else { String::new() },
            down_text: if multiply { "1".into() } else { String::new() },
            spacing_x_text: String::new(),
            spacing_y_text: String::new(),
            error: None,
        }
    }
}

impl App {
    pub fn begin_repeat(&mut self, multiply: bool) {
        let picked = self
            .doc()
            .map(|d| d.selected.is_some())
            .unwrap_or(false)
            || self.doc().map(|d| !d.also.is_empty()).unwrap_or(false);
        if !picked {
            self.status = if multiply {
                "Select a markup first, then repeat it.".into()
            } else {
                "Select a markup first, then offset it.".into()
            };
            return;
        }
        self.repeating = Some(Repeating::new(multiply));
    }

    pub fn repeat_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut repeating) = self.repeating.take() else {
            return;
        };
        let theme = self.chrome.theme;
        let units = self.prefs.units;
        let scale = self.doc().and_then(|d| d.scale_of(d.page));
        let mut keep = true;
        let mut go = false;

        let title = if repeating.multiply { "Multiply" } else { "Offset" };
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .max_width(620.0)
            .show(ctx, |ui| {
                ui.set_min_width(380.0);
                ui.label(
                    RichText::new(if repeating.multiply {
                        "Repeat what is selected in a grid. Every copy is a real markup \
                         and counts in the takeoff."
                    } else {
                        "Move what is selected by a set distance."
                    })
                    .color(theme.faint)
                    .size(11.0),
                );
                ui.add_space(12.0);

                if repeating.multiply {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Copies across").color(theme.faint).size(11.0));
                        ui.add(
                            egui::TextEdit::singleline(&mut repeating.across_text)
                                .desired_width(60.0),
                        );
                        ui.add_space(10.0);
                        ui.label(RichText::new("down").color(theme.faint).size(11.0));
                        ui.add(
                            egui::TextEdit::singleline(&mut repeating.down_text)
                                .desired_width(60.0),
                        );
                    });
                    ui.add_space(8.0);
                }

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if repeating.multiply { "Spacing across" } else { "Across" })
                            .color(theme.faint)
                            .size(11.0),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut repeating.spacing_x_text)
                            .hint_text(hint(scale.is_some(), units))
                            .desired_width(120.0),
                    );
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if repeating.multiply { "Spacing down" } else { "Down" })
                            .color(theme.faint)
                            .size(11.0),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut repeating.spacing_y_text)
                            .hint_text(hint(scale.is_some(), units))
                            .desired_width(120.0),
                    );
                });

                ui.add_space(6.0);
                ui.label(
                    RichText::new(if scale.is_some() {
                        "In the sheet's own units, at the scale it is set to."
                    } else {
                        "This sheet has no scale, so distances are in points — 72 to \
                         the inch on paper. Set a scale to type feet."
                    })
                    .color(theme.faint)
                    .size(10.0),
                );

                if let Some(grid) = self.grid_from(&repeating) {
                    ui.add_space(8.0);
                    match grid.sensible() {
                        Ok(()) => {
                            ui.label(
                                RichText::new(grid.describe())
                                    .color(theme.accent_text)
                                    .size(11.0),
                            );
                        }
                        Err(why) => {
                            ui.label(RichText::new(why).color(theme.warn).size(11.0));
                        }
                    }
                }

                if let Some(error) = &repeating.error {
                    ui.add_space(8.0);
                    ui.colored_label(Color32::from_rgb(235, 120, 120), error);
                }

                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    let ready = match self.grid_from(&repeating) {
                        Some(grid) if repeating.multiply => grid.sensible().is_ok(),
                        Some(_) => true,
                        None => false,
                    };
                    if ui.add_enabled(ready, egui::Button::new(title)).clicked() {
                        go = true;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });

        if go {
            if let Some(grid) = self.grid_from(&repeating) {
                let said = if repeating.multiply {
                    self.multiply_selection(grid)
                } else {
                    self.offset_selection(Offset {
                        dx: grid.spacing_x,
                        dy: grid.spacing_y,
                    })
                };
                self.status = said;
                keep = false;
            }
        }
        if keep {
            self.repeating = Some(repeating);
        }
    }

    /// Reads the boxes into a grid, in points.
    fn grid_from(&self, repeating: &Repeating) -> Option<Grid> {
        let scale = self.doc().and_then(|d| d.scale_of(d.page));
        let read = |text: &str| -> Option<f64> {
            let text = text.trim();
            if text.is_empty() {
                return Some(0.0);
            }
            // Through the same reader the scale box uses, so "4'" means four
            // feet here exactly as it does everywhere else on a sheet.
            crate::units::parse_length(text, self.prefs.units).map(|inches| match &scale {
                // `per_point` is how many of the sheet's units one point is
                // worth, so its reciprocal turns a distance back into points.
                Some(measure) => {
                    let per_point = measure.per_point();
                    if per_point.abs() < f64::EPSILON {
                        inches * 72.0
                    } else {
                        inches / per_point
                    }
                }
                None => inches * 72.0,
            })
        };
        let spacing_x = read(&repeating.spacing_x_text)?;
        let spacing_y = read(&repeating.spacing_y_text)?;
        if !repeating.multiply {
            return Some(Grid {
                across: 1,
                down: 1,
                spacing_x,
                // Typed as "down the sheet", which is negative in a PDF.
                spacing_y: -spacing_y,
            });
        }
        Some(Grid {
            across: repeating.across_text.trim().parse::<usize>().ok()?,
            down: repeating.down_text.trim().parse::<usize>().ok()?,
            spacing_x,
            spacing_y,
        })
    }

    /// Which markups are selected, as indices.
    fn picked(&self) -> Vec<usize> {
        self.doc().map(|d| d.selection()).unwrap_or_default()
    }

    pub fn offset_selection(&mut self, by: Offset) -> String {
        let picked = self.picked();
        if picked.is_empty() {
            return "Nothing is selected.".into();
        }
        let Some(doc) = self.doc_mut() else {
            return "No drawing is open.".into();
        };
        doc.checkpoint();
        let mut moved = 0usize;
        for at in &picked {
            if let Some(mark) = doc.marks.get_mut(*at) {
                mark.markup.move_by(by.dx, by.dy);
                mark.changed = true;
                moved += 1;
            }
        }
        doc.dirty = true;
        format!(
            "{moved} markup{} moved. Undo puts {} back.",
            if moved == 1 { "" } else { "s" },
            if moved == 1 { "it" } else { "them" }
        )
    }

    pub fn multiply_selection(&mut self, grid: Grid) -> String {
        if let Err(why) = grid.sensible() {
            return why;
        }
        let picked = self.picked();
        if picked.is_empty() {
            return "Nothing is selected.".into();
        }
        let offsets = grid.offsets();
        let Some(doc) = self.doc_mut() else {
            return "No drawing is open.".into();
        };
        let page = doc.page;
        doc.checkpoint();

        let mut made = 0usize;
        for at in &picked {
            let Some(original) = doc.marks.get(*at) else {
                continue;
            };
            let markup = original.markup.clone();
            for offset in &offsets {
                let mut copy = markup.clone();
                copy.move_by(offset.dx, offset.dy);
                // A fresh name, because two markups sharing one is how six
                // seats end up disagreeing about which is which.
                copy.dict.set(
                    pdf::Name::new("NM"),
                    pdf::Object::text(&annot::name::fresh()),
                );
                doc.marks.push(crate::sheet::Mark::new(page, copy));
                made += 1;
            }
        }
        doc.dirty = true;
        format!(
            "{made} cop{} made. Each one is a markup like any other and counts in the \
             takeoff.",
            if made == 1 { "y" } else { "ies" }
        )
    }
}

fn hint(scaled: bool, units: crate::units::Units) -> String {
    if !scaled {
        return "72 = one inch".into();
    }
    match units {
        crate::units::Units::FeetInches => "4' 0\"".into(),
        _ => "4".into(),
    }
}
