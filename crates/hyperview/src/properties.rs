//! What the file says about itself, and changing it.
//!
//! Revu's Document Properties. Worth having for one reason above the others:
//! a drawing set that arrives with somebody else's job number in its Title is
//! a set that will be filed under somebody else's job, and nobody ever looks
//! at this window until that has already happened.

use crate::app::App;

/// What a file says about itself.
#[derive(Clone, Default, PartialEq)]
pub struct About {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
    /// What made the file. Not editable: it is a record of what happened, and
    /// a program that rewrites it is telling a lie about where a set came from.
    pub creator: String,
    pub producer: String,
    pub created: String,
    pub changed: String,
}

/// Everything shown in the window, including the parts nobody can change.
#[derive(Clone)]
pub struct Properties {
    pub about: About,
    /// What it was when the window opened, so Save can tell whether anything
    /// was actually changed.
    pub was: About,
    pub path: std::path::PathBuf,
    pub sheets: usize,
    pub bytes: u64,
    pub version: String,
    pub encrypted: bool,
    /// Which lock, when it has one and it is open.
    pub lock: Option<pdf::opening::Lock>,
    /// Every sheet size in the set, largest first, with how many there are.
    pub sizes: Vec<(String, usize)>,
    pub markups: usize,
    pub error: Option<String>,
}

/// Reads a date the way a PDF writes one: `D:20260918143000-06'00'`.
pub fn readable_date(raw: &str) -> String {
    let digits: String = raw
        .trim_start_matches("D:")
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.len() < 8 {
        return raw.to_string();
    }
    let month = match &digits[4..6] {
        "01" => "January",
        "02" => "February",
        "03" => "March",
        "04" => "April",
        "05" => "May",
        "06" => "June",
        "07" => "July",
        "08" => "August",
        "09" => "September",
        "10" => "October",
        "11" => "November",
        "12" => "December",
        _ => return raw.to_string(),
    };
    let day = digits[6..8].trim_start_matches('0');
    let year = &digits[0..4];
    if digits.len() >= 12 {
        format!("{day} {month} {year}, {}:{}", &digits[8..10], &digits[10..12])
    } else {
        format!("{day} {month} {year}")
    }
}

/// A sheet size named the way a drawing office names it.
pub fn paper_named(width: f64, height: f64) -> String {
    let (across, down) = if width >= height {
        (width, height)
    } else {
        (height, width)
    };
    let inches = |points: f64| points / 72.0;
    let (w, h) = (inches(across), inches(down));
    let close = |a: f64, b: f64| (a - b).abs() < 0.6;
    let name = [
        ("ANSI A", 11.0, 8.5),
        ("ANSI B", 17.0, 11.0),
        ("ANSI C", 22.0, 17.0),
        ("ANSI D", 34.0, 22.0),
        ("ANSI E", 44.0, 34.0),
        ("ARCH A", 12.0, 9.0),
        ("ARCH B", 18.0, 12.0),
        ("ARCH C", 24.0, 18.0),
        ("ARCH D", 36.0, 24.0),
        ("ARCH E", 48.0, 36.0),
        ("ARCH E1", 42.0, 30.0),
        ("A4", 11.69, 8.27),
        ("A3", 16.54, 11.69),
        ("A2", 23.39, 16.54),
        ("A1", 33.11, 23.39),
        ("A0", 46.81, 33.11),
    ]
    .iter()
    .find(|(_, a, b)| close(w, *a) && close(h, *b))
    .map(|(name, _, _)| *name);

    match name {
        Some(name) => format!("{name} — {:.0} × {:.0} in", w, h),
        None => format!("{:.1} × {:.1} in", w, h),
    }
}

impl App {
    pub fn show_properties(&mut self) {
        let Some(doc) = self.doc() else {
            self.status = "Open a drawing first.".into();
            return;
        };
        let file = &doc.file;
        let info = file
            .xref
            .trailer
            .get("Info")
            .map(|o| file.follow(o))
            .and_then(|o| o.as_dict().cloned())
            .unwrap_or_default();
        let text = |key: &str| {
            info.get(key)
                .map(|o| file.follow(o))
                .and_then(|o| o.as_text())
                .unwrap_or_default()
        };
        let about = About {
            title: text("Title"),
            author: text("Author"),
            subject: text("Subject"),
            keywords: text("Keywords"),
            creator: text("Creator"),
            producer: text("Producer"),
            created: readable_date(&text("CreationDate")),
            changed: readable_date(&text("ModDate")),
        };

        // Sheet sizes, most common first: a set of forty D-size sheets with one
        // letter page in it is worth seeing at a glance, because that one page
        // is usually a transmittal somebody meant to take out.
        let mut counted: std::collections::BTreeMap<String, usize> = Default::default();
        for size in &doc.pages {
            *counted
                .entry(paper_named(size.width as f64, size.height as f64))
                .or_default() += 1;
        }
        let mut sizes: Vec<(String, usize)> = counted.into_iter().collect();
        sizes.sort_by(|a, b| b.1.cmp(&a.1));

        self.properties = Some(Properties {
            was: about.clone(),
            about,
            path: doc.path.clone(),
            sheets: doc.pages.len(),
            bytes: std::fs::metadata(&doc.path).map(|m| m.len()).unwrap_or(0),
            version: file.version(),
            encrypted: file.encrypted,
            lock: doc.locked,
            sizes,
            markups: doc.live().count(),
            error: None,
        });
    }

    pub fn properties_window(&mut self, ctx: &egui::Context) {
        let Some(mut showing) = self.properties.take() else {
            return;
        };
        let theme = self.chrome.theme;
        let mut keep = true;
        let mut save = false;

        egui::Window::new("Document Properties")
            .open(&mut keep)
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        showing
                            .path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default(),
                    )
                    .strong(),
                );
                ui.label(
                    egui::RichText::new(showing.path.display().to_string())
                        .color(theme.faint)
                        .size(10.0),
                );
                ui.add_space(12.0);

                // ---- what can be changed -------------------------------
                ui.label(egui::RichText::new("What the file says it is").strong());
                ui.add_space(4.0);
                egui::Grid::new("properties-editable")
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        for (label, value) in [
                            ("Title", &mut showing.about.title),
                            ("Author", &mut showing.about.author),
                            ("Subject", &mut showing.about.subject),
                            ("Keywords", &mut showing.about.keywords),
                        ] {
                            ui.label(egui::RichText::new(label).color(theme.faint).size(11.0));
                            ui.add(
                                egui::TextEdit::singleline(value)
                                    .desired_width(340.0),
                            );
                            ui.end_row();
                        }
                    });
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "These travel with the set. A drawing whose Title still says \
                         somebody else's job number gets filed under somebody else's job.",
                    )
                    .color(theme.faint)
                    .size(10.0),
                );

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(8.0);

                // ---- what cannot ---------------------------------------
                ui.label(egui::RichText::new("What it is").strong());
                ui.add_space(4.0);
                let sizes = if showing.sizes.is_empty() {
                    "—".to_string()
                } else {
                    showing
                        .sizes
                        .iter()
                        .map(|(name, count)| {
                            if *count == 1 {
                                name.clone()
                            } else {
                                format!("{name} ({count})")
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                egui::Grid::new("properties-fixed")
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        let mut row = |name: &str, value: String| {
                            ui.label(
                                egui::RichText::new(name).color(theme.faint).size(11.0),
                            );
                            ui.label(egui::RichText::new(value).size(11.0));
                            ui.end_row();
                        };
                        row("Sheets", showing.sheets.to_string());
                        row("Sheet sizes", sizes);
                        row("Markups", showing.markups.to_string());
                        row("Size on disk", megabytes(showing.bytes));
                        row("PDF version", showing.version.clone());
                        row(
                            "Made with",
                            if showing.about.creator.is_empty() {
                                "—".into()
                            } else {
                                showing.about.creator.clone()
                            },
                        );
                        row(
                            "Written by",
                            if showing.about.producer.is_empty() {
                                "—".into()
                            } else {
                                showing.about.producer.clone()
                            },
                        );
                        row(
                            "Created",
                            if showing.about.created.is_empty() {
                                "—".into()
                            } else {
                                showing.about.created.clone()
                            },
                        );
                        row(
                            "Last changed",
                            if showing.about.changed.is_empty() {
                                "—".into()
                            } else {
                                showing.about.changed.clone()
                            },
                        );
                        // The whole truth about the lock, because the
                        // difference between AES-256 and RC4 is the difference
                        // between a set that is safe to send and one that only
                        // looks it — and nobody can tell by looking.
                        row(
                            "Security",
                            match showing.lock {
                                Some(lock) => lock.in_words().to_string(),
                                None if showing.encrypted => {
                                    "Locked, and not opened.".to_string()
                                }
                                None => "None — anybody can open it.".to_string(),
                            },
                        );
                        if matches!(showing.lock, Some(lock) if lock.weak()) {
                            row(
                                "",
                                "Anybody determined can get into this file. Markups \
                                 saved into it keep this lock, because changing it \
                                 would lock out whoever else has the password — use \
                                 Document → Security on a copy to put a real one on it."
                                    .to_string(),
                            );
                        }
                    });

                if let Some(why) = &showing.error {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new(why).color(theme.warn).size(11.0));
                }

                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    let changed = showing.about != showing.was;
                    if ui
                        .add_enabled(changed, egui::Button::new("Save these"))
                        .clicked()
                    {
                        save = true;
                    }
                    if !changed {
                        ui.label(
                            egui::RichText::new("Nothing has been changed.")
                                .color(theme.faint)
                                .size(11.0),
                        );
                    }
                });
            });

        if save {
            match self.write_properties(&showing) {
                Ok(said) => {
                    self.status = said;
                    showing.was = showing.about.clone();
                }
                Err(why) => showing.error = Some(why),
            }
        }
        if keep {
            self.properties = Some(showing);
        }
    }

    /// Writes the changed information back, as an incremental update.
    ///
    /// The same way markups are saved: the original bytes stay exactly where
    /// they were and the change is appended, so a file whose properties were
    /// corrected is still byte-for-byte the drawing that was issued.
    fn write_properties(&mut self, showing: &Properties) -> Result<String, String> {
        let Some(doc) = self.doc() else {
            return Err("No drawing is open.".into());
        };
        if doc.read_only {
            return Err(
                "This drawing cannot be written to. Save a copy first and change that."
                    .into(),
            );
        }
        let file = &doc.file;
        let mut info = file
            .xref
            .trailer
            .get("Info")
            .map(|o| file.follow(o))
            .and_then(|o| o.as_dict().cloned())
            .unwrap_or_default();
        for (key, value) in [
            ("Title", &showing.about.title),
            ("Author", &showing.about.author),
            ("Subject", &showing.about.subject),
            ("Keywords", &showing.about.keywords),
        ] {
            if value.trim().is_empty() {
                info.remove(key);
            } else {
                info.set(key, pdf::Object::text(value));
            }
        }
        info.set("ModDate", pdf::Object::text(&annot::place::now()));

        let mut update = pdf::write::Update::new(file);
        let info_ref = match file.xref.trailer.get("Info").and_then(|o| o.as_ref()) {
            Some(reference) => {
                update.replace(reference, pdf::Object::Dict(info));
                reference
            }
            None => update.add(pdf::Object::Dict(info)),
        };
        update.set_trailer("Info", pdf::Object::Ref(info_ref));

        let written = update.apply(file);
        let path = doc.path.clone();
        std::fs::write(&path, written).map_err(|e| format!("could not save: {e}"))?;
        Ok("What the file says about itself has been saved.".into())
    }
}

fn megabytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{} KB", (bytes / 1024).max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pdf_date_reads_the_way_a_person_would_say_it() {
        assert_eq!(
            readable_date("D:20260918143000-06'00'"),
            "18 September 2026, 14:30"
        );
        assert_eq!(readable_date("D:20260918"), "18 September 2026");
    }

    #[test]
    fn a_date_that_is_not_a_date_is_shown_as_it_stands() {
        // Rather than "1 January 1970", which somebody would take for a fact.
        assert_eq!(readable_date("sometime last year"), "sometime last year");
        assert_eq!(readable_date(""), "");
    }

    #[test]
    fn the_paper_sizes_a_drawing_office_uses_are_named() {
        assert!(paper_named(42.0 * 72.0, 30.0 * 72.0).starts_with("ARCH E1"));
        assert!(paper_named(36.0 * 72.0, 24.0 * 72.0).starts_with("ARCH D"));
        assert!(paper_named(8.5 * 72.0, 11.0 * 72.0).starts_with("ANSI A"));
        // Turned round is the same paper.
        assert_eq!(
            paper_named(30.0 * 72.0, 42.0 * 72.0),
            paper_named(42.0 * 72.0, 30.0 * 72.0)
        );
    }

    #[test]
    fn an_odd_size_is_given_in_inches_rather_than_called_something_it_is_not() {
        let odd = paper_named(19.0 * 72.0, 13.0 * 72.0);
        assert!(odd.contains("19"), "{odd}");
        assert!(!odd.contains("ARCH"), "{odd}");
    }
}
