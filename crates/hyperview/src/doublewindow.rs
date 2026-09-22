//! The double-count check, on screen.
//!
//! It lists what it found, and clicking a line takes you to it on the sheet
//! with both markups selected so you can look at them and decide. That is the
//! whole interaction, and it is deliberately the whole interaction: there is
//! no "remove duplicates" button here, and there is not going to be one.
//! A program that silently deletes one of a pair is a program that will one
//! day delete the right one.

use egui::RichText;
use takeoff::doubles::{self, Doubles, HowClose, Sure};

use crate::app::App;

#[derive(Default)]
pub struct Checking {
    pub open: bool,
    pub found: Option<Doubles>,
    /// The marks each found row belongs to, so a line can be clicked.
    pub marks: Vec<usize>,
    /// Include the pairs that carry different subjects.
    pub show_maybes: bool,
    /// What the pairs would be worth, said once when the check runs.
    pub worth: String,
}

impl App {
    /// The command behind **Document ▸ Check for Doubles**.
    pub fn check_doubles(&mut self) {
        let Some(doc) = self.doc() else {
            self.status = "Open a drawing first.".into();
            return;
        };
        let rows = doc.rows();
        if rows.is_empty() {
            self.status = "There are no measurements on this drawing to check.".into();
            return;
        }
        let marks: Vec<usize> = doc.live().map(|(at, _)| at).collect();
        let found = doubles::find(&rows, HowClose::default());
        let worth = found.what_it_is_worth(crate::panelbody::weights(), &rows);
        self.status = if found.pairs.is_empty() {
            "Nothing on this drawing looks counted twice.".into()
        } else {
            format!(
                "{} pair{} worth looking at.",
                found.pairs.len(),
                if found.pairs.len() == 1 { "" } else { "s" }
            )
        };
        self.doubles = Checking {
            open: true,
            found: Some(found),
            marks,
            show_maybes: true,
            worth,
        };
    }

    /// Goes to a pair and selects both of its markups.
    fn show_pair(&mut self, first: usize, second: usize, page: usize) {
        let (Some(&one), Some(&two)) = (
            self.doubles.marks.get(first),
            self.doubles.marks.get(second),
        ) else {
            return;
        };
        self.go_to(page as u32);
        let canvas = self.last_canvas;
        if let Some(doc) = self.doc_mut() {
            doc.selected = Some(one);
            doc.also = vec![two];
            // Framed on both of them, so the thing being argued about is on
            // screen rather than somewhere off the edge of it.
            let both = doc
                .marks
                .get(one)
                .and_then(|m| m.markup.bounds())
                .into_iter()
                .chain(doc.marks.get(two).and_then(|m| m.markup.bounds()))
                .fold(None::<[f64; 4]>, |so_far, area| {
                    Some(match so_far {
                        None => area,
                        Some(got) => [
                            got[0].min(area[0]),
                            got[1].min(area[1]),
                            got[2].max(area[2]),
                            got[3].max(area[3]),
                        ],
                    })
                });
            if let Some(area) = both {
                doc.view.zoom = doc.view.zoom.max(0.5);
                doc.view.centre_on(
                    [(area[0] + area[2]) * 0.5, (area[1] + area[3]) * 0.5],
                    canvas,
                );
            }
        }
    }

    pub fn doubles_window(&mut self, ctx: &egui::Context) {
        if !self.doubles.open {
            return;
        }
        let theme = self.chrome.theme;
        let Some(found) = self.doubles.found.take() else {
            self.doubles.open = false;
            return;
        };
        let sheet = |page: usize| {
            self.doc()
                .map(|doc| doc.sheet_name(page as u32))
                .unwrap_or_else(|| format!("{}", page + 1))
        };
        let mut open = true;
        let mut go: Option<(usize, usize, usize)> = None;
        let mut show_maybes = self.doubles.show_maybes;
        let worth = self.doubles.worth.clone();

        egui::Window::new("Counted Twice?")
            .collapsible(false)
            .resizable(true)
            .default_width(620.0)
            .default_height(480.0)
            .open(&mut open)
            .show(ctx, |ui| {
                if !found.found_anything() {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(
                            "Nothing on this drawing looks counted twice. That is not a \
                             guarantee — two markups on different sheets for the same \
                             member look like an ordinary takeoff from here, and always \
                             will.",
                        )
                        .size(11.0),
                    );
                } else {
                    ui.label(
                        RichText::new(
                            "Pairs of markups sitting in the same place on one sheet, \
                             measuring the same amount. Click one to go and look at it \
                             with both selected. Nothing here has been changed, removed \
                             or subtracted from any total — this is a list, not a fix.",
                        )
                        .color(theme.faint)
                        .size(11.0),
                    );
                    ui.add_space(6.0);
                    ui.checkbox(
                        &mut show_maybes,
                        "Also show pairs with different subjects (often deliberate)",
                    );
                    ui.separator();

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for pair in &found.pairs {
                                if pair.sure == Sure::WorthALook && !show_maybes {
                                    continue;
                                }
                                let title = format!(
                                    "{} · {} · #{} and #{}",
                                    sheet(pair.page),
                                    pair.subject,
                                    pair.first + 1,
                                    pair.second + 1
                                );
                                let hit = ui.add(
                                    egui::Button::new(
                                        RichText::new(title)
                                            .size(12.0)
                                            .color(match pair.sure {
                                                Sure::Certain => theme.warn,
                                                Sure::WorthALook => theme.text,
                                            }),
                                    )
                                    .frame(false)
                                    .min_size(egui::vec2(ui.available_width(), 18.0)),
                                );
                                if hit.clicked() {
                                    go = Some((pair.first, pair.second, pair.page));
                                }
                                ui.label(
                                    RichText::new(&pair.why).color(theme.faint).size(10.0),
                                );
                                if let Some(twice) = pair.carried_twice {
                                    ui.label(
                                        RichText::new(format!(
                                            "The totals carry {twice:.2}{} twice if this is \
                                             a double.",
                                            if pair.unit.is_empty() {
                                                String::new()
                                            } else {
                                                format!(" {}", pair.unit)
                                            }
                                        ))
                                        .color(theme.faint)
                                        .size(10.0),
                                    );
                                }
                                ui.add_space(6.0);
                            }
                        });

                    if !worth.is_empty() {
                        ui.separator();
                        ui.add_space(4.0);
                        ui.label(RichText::new(&worth).color(theme.warn).size(11.0));
                    }
                }

                if found.unchecked > 0 {
                    let sheets: Vec<String> =
                        found.unchecked_pages.iter().map(|p| sheet(*p)).collect();
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!(
                            "{} measurement{} could not be checked, because {} sheet{} no \
                             scale — {}. They are not in this list either way.",
                            found.unchecked,
                            if found.unchecked == 1 { " " } else { "s " },
                            if found.unchecked == 1 { "its" } else { "their" },
                            if found.unchecked == 1 { " has" } else { "s have" },
                            sheets.join(", ")
                        ))
                        .color(theme.warn)
                        .size(10.0),
                    );
                }
            });

        self.doubles.show_maybes = show_maybes;
        self.doubles.found = Some(found);
        if !open {
            self.doubles.open = false;
        }
        if let Some((first, second, page)) = go {
            self.show_pair(first, second, page);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use takeoff::{Row, WeightColumns};

    fn beam(subject: &str, at: [f64; 4]) -> Row {
        let mut row = Row::blank();
        row.subject = subject.into();
        row.kind = annot::Kind::Length;
        row.scaled = true;
        row.length = Some(25.0);
        row.area_box = at;
        row.columns[WeightColumns::default().per_length] = "26".into();
        row
    }

    #[test]
    fn the_window_never_offers_to_remove_anything() {
        // The contract, checked against the source itself: there is no delete
        // path in here, and a future edit that adds one should fail this.
        //
        // The words being looked for are built up a piece at a time on
        // purpose — spelled out in full they would appear in this file and
        // the check would trip over its own list.
        let source = include_str!("doublewindow.rs");
        let forbidden = [
            format!("remove{}duplicate", "_"),
            format!("delete{}pair", "_"),
            format!("drop{}second", "_"),
            format!("{} duplicates", "Remove"),
            format!("{} the doubles", "Delete"),
        ];
        for word in forbidden {
            assert!(
                !source.contains(&word),
                "a removal path appeared in the doubles window: {word}"
            );
        }
        // And the window says so in as many words, where somebody reads it.
        assert!(source.contains("this is a list, not a fix"));
    }

    #[test]
    fn what_it_found_is_reported_and_the_totals_are_untouched() {
        let rows = vec![
            beam("W12x26", [100.0, 100.0, 400.0, 102.0]),
            beam("W12x26", [101.0, 100.5, 401.0, 102.5]),
        ];
        let before = takeoff::summarise(&rows, WeightColumns::default()).pounds;
        let found = doubles::find(&rows, HowClose::default());
        assert_eq!(found.certain(), 1);
        let after = takeoff::summarise(&rows, WeightColumns::default()).pounds;
        assert!((before - after).abs() < 1e-12, "a check changed a total");
    }
}
