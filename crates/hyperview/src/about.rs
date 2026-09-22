//! The About box, the shortcut list, and selecting every markup on a sheet.
//!
//! Small things, and the About box is not one of them: it is where somebody
//! looks when six seats are supposed to be on the same build and one of them is
//! behaving differently. So it says the version plainly, says which server this
//! seat is talking to, and says what it is rendering with — the three things
//! that differ between two machines that should be the same.

use egui::RichText;

use crate::app::App;

impl App {
    /// Selects every markup on the sheet being looked at.
    pub fn select_all_on_this_sheet(&mut self) {
        let Some(doc) = self.doc() else {
            return;
        };
        let page = doc.page;
        let all: Vec<usize> = doc
            .marks
            .iter()
            .enumerate()
            .filter(|(_, mark)| mark.page == page && !mark.gone)
            .map(|(at, _)| at)
            .collect();
        let count = all.len();
        if let Some(doc) = self.doc_mut() {
            doc.selected = all.first().copied();
            doc.also = all.into_iter().skip(1).collect();
        }
        self.status = if count == 0 {
            "There are no markups on this sheet.".into()
        } else {
            format!(
                "{count} markup{} selected. Delete removes them; Escape lets them go.",
                if count == 1 { "" } else { "s" }
            )
        };
    }

    pub fn about_box(&mut self, ctx: &egui::Context) {
        if !self.about {
            return;
        }
        let theme = self.chrome.theme;
        let mut open = true;
        egui::Window::new("About Excalibur View")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .max_width(660.0)
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let (mark, _) =
                        ui.allocate_exact_size(egui::vec2(64.0, 64.0), egui::Sense::hover());
                    ui::mark::draw(ui.painter(), mark, theme);
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Excalibur View").strong().size(18.0));
                        ui.label(
                            RichText::new("Drawing viewer, markup and takeoff.")
                                .color(theme.faint)
                                .size(11.0),
                        );
                        ui.add_space(6.0);
                        // Selectable, because the first thing anybody is asked
                        // when something is wrong is which version they are on.
                        let mut version = format!("Version {}", env!("CARGO_PKG_VERSION"));
                        ui.add(
                            egui::TextEdit::singleline(&mut version)
                                .desired_width(180.0)
                                .font(egui::TextStyle::Monospace),
                        );
                    });
                });
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(10.0);

                // The labels sit in a column of their own width, so every value
                // starts at the same place: spaces cannot line up type that is
                // not all one width.
                let row = |ui: &mut egui::Ui, label: &str, value: String| {
                    ui.horizontal(|ui| {
                        gutter(ui, label, theme);
                        ui.label(RichText::new(value).size(11.0));
                    });
                };
                row(
                    ui,
                    "Server",
                    if self.standing.signed_in() {
                        format!("{} · {}", self.standing.name, self.standing.base)
                    } else {
                        "not connected".to_string()
                    },
                );
                row(
                    ui,
                    "Signed in as",
                    self.standing
                        .who
                        .as_ref()
                        .map(|who| who.name.clone())
                        .unwrap_or_else(|| "nobody".into()),
                );
                row(
                    ui,
                    "License",
                    match (&self.standing.license, self.standing.signed_in()) {
                        (Some(license), true) => license.about_line(),
                        _ => "Free for one person, for any work, forever.".to_string(),
                    },
                );
                if !self.standing.signed_in() || self.standing.license.is_none() {
                    ui.horizontal(|ui| {
                        gutter(ui, "", theme);
                        ui.label(
                            RichText::new(
                                "Offices that want shared projects, live markups and one tool \
                                 chest for everyone add Excalibur View Office.",
                            )
                            .color(theme.faint)
                            .size(10.0),
                        );
                    });
                    ui.horizontal(|ui| {
                        gutter(ui, "", theme);
                        ui.hyperlink_to(RichText::new("Learn about Office").size(11.0), hub::site::PRICING);
                    });
                }
                row(
                    ui,
                    "Tool chest",
                    match &self.profile {
                        Some(profile) => {
                            let tools: usize =
                                profile.sets.iter().map(|s| s.tools.len()).sum();
                            format!("{} · {tools} tools", profile.name)
                        }
                        None => "none loaded".to_string(),
                    },
                );
                row(
                    ui,
                    "Drawing with",
                    if cfg!(windows) {
                        "Direct3D, falling back to OpenGL".into()
                    } else {
                        "OpenGL".to_string()
                    },
                );
                row(ui, "Updates", match self.standing.update.as_ref() {
                    _ if crate::trust::KEYS.is_empty() => "off — no signing key yet".to_string(),
                    _ if self.standing.update_ready.is_some() => format!(
                        "version {} is installed and takes over next time",
                        self.standing.update_ready.clone().unwrap_or_default()
                    ),
                    _ if self.standing.update_refused.is_some() => format!(
                        "not installed: {}",
                        self.standing.update_refused.clone().unwrap_or_default()
                    ),
                    Some(release) if !self.updates_itself => {
                        format!("version {} is out", release.version)
                    }
                    Some(release) => format!("fetching version {}", release.version),
                    None if self.standing.pinned_to.is_some() => format!(
                        "held on {}",
                        self.standing.pinned_to.clone().unwrap_or_default()
                    ),
                    None => "up to date".to_string(),
                });
                if crate::trust::KEYS.is_empty() {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "This build will not install an update, because no signing \
                             key has been put in it yet — and a program that installs \
                             anything when it has been told to trust nothing is worse \
                             than one that installs nothing.",
                        )
                        .color(theme.warn)
                        .size(10.0),
                    );
                }

                ui.add_space(14.0);
                ui.label(
                    RichText::new(
                        "Nothing in this program is estimated. Every quantity traces back \
                         to something somebody clicked, and every weight comes out of your \
                         own tool chest.",
                    )
                    .color(theme.faint)
                    .size(10.0),
                );
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("Excalibur Construction Technologies · Pueblo, Colorado ·")
                            .color(theme.faint)
                            .size(10.0),
                    );
                    ui.hyperlink_to(RichText::new("excaliburct.com").size(10.0), hub::site::HOME);
                    ui.label(RichText::new("·").color(theme.faint).size(10.0));
                    ui.hyperlink_to(
                        RichText::new("Every connection it makes").size(10.0),
                        hub::site::CONNECTIONS,
                    );
                });
            });
        if !open {
            self.about = false;
        }
    }

    pub fn shortcut_list(&mut self, ctx: &egui::Context) {
        if !self.shortcuts_open {
            return;
        }
        let theme = self.chrome.theme;
        let mut open = true;
        egui::Window::new("Keyboard Shortcuts")
            .collapsible(false)
            .resizable(true)
            .default_width(460.0)
            .default_height(560.0)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Taken from the same list the menus and the toolbar are built \
                         from, so this cannot drift out of date.",
                    )
                    .color(theme.faint)
                    .size(10.0),
                );
                ui.add_space(8.0);
                let mut with_keys: Vec<(&str, String)> = ui::command::ALL
                    .iter()
                    .filter_map(|command| {
                        command.shortcut.map(|keys| (command.label, keys.to_string()))
                    })
                    .collect();
                with_keys.sort_by(|a, b| a.0.cmp(b.0));

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (label, keys) in with_keys {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(label).size(11.0));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(keys)
                                                .color(theme.accent_text)
                                                .font(egui::FontId::monospace(11.0)),
                                        );
                                    },
                                );
                            });
                        }
                    });
            });
        if !open {
            self.shortcuts_open = false;
        }
    }
}

// ---- recently opened, and going back to what is on disk ---------------------

impl App {
    /// The list of drawings opened lately.
    pub fn recent_window(&mut self, ctx: &egui::Context) {
        if !self.recent_open {
            return;
        }
        let theme = self.chrome.theme;
        let mut open = true;
        let mut chosen: Option<std::path::PathBuf> = None;
        let mut forget = false;
        let recent = self.prefs.recent_that_exist();

        egui::Window::new("Open Recent")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .max_width(700.0)
            .show(ctx, |ui| {
                ui.set_min_width(460.0);
                if recent.is_empty() {
                    ui.label(
                        RichText::new(
                            "Nothing yet. Drawings you open turn up here — and drop off \
                             again if they are moved or deleted.",
                        )
                        .color(theme.faint)
                        .size(11.0),
                    );
                    return;
                }
                for path in &recent {
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let response = ui.add(
                        egui::Button::new(RichText::new(name).size(12.0))
                            .frame(false)
                            .min_size(egui::vec2(ui.available_width(), 20.0)),
                    );
                    if response.clicked() {
                        chosen = Some(path.clone());
                    }
                    // The folder underneath, small, because two jobs often have
                    // a drawing with the same name in them.
                    ui.label(
                        RichText::new(
                            path.parent()
                                .map(|p| p.display().to_string())
                                .unwrap_or_default(),
                        )
                        .color(theme.faint)
                        .size(10.0),
                    );
                    ui.add_space(4.0);
                }
                ui.add_space(6.0);
                if ui.small_button("Forget these").clicked() {
                    forget = true;
                }
            });

        if let Some(path) = chosen {
            self.recent_open = false;
            self.open(path);
        }
        if forget {
            self.prefs.recent.clear();
            let _ = self.prefs.save();
            self.status = "The recent list is empty again.".into();
        }
        if !open {
            self.recent_open = false;
        }
    }

    /// Throws away everything unsaved and reopens the file from disk.
    ///
    /// Only offered when there is something to throw away, and it says how much
    /// before it does it — "revert" is one of the few words in a program that
    /// means "destroy my afternoon" if it is misread.
    pub fn revert(&mut self) {
        let Some(doc) = self.doc() else {
            self.status = "No drawing is open.".into();
            return;
        };
        let unsaved = doc.marks.iter().filter(|m| m.reference.is_none() && !m.gone).count();
        if unsaved == 0 && !doc.dirty {
            self.status = "This drawing is already exactly as it is on disk.".into();
            return;
        }
        self.reverting = Some(unsaved);
    }

    pub fn revert_dialog(&mut self, ctx: &egui::Context) {
        let Some(unsaved) = self.reverting else {
            return;
        };
        let theme = self.chrome.theme;
        let mut keep = true;
        let mut go = false;

        egui::Window::new("Revert")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .max_width(620.0)
            .show(ctx, |ui| {
                ui.set_min_width(380.0);
                ui.label(
                    RichText::new(if unsaved == 0 {
                        "This will reopen the drawing from disk.".to_string()
                    } else {
                        format!(
                            "{unsaved} markup{} on this drawing {} not been saved yet. \
                             Reverting throws {} away.",
                            if unsaved == 1 { "" } else { "s" },
                            if unsaved == 1 { "has" } else { "have" },
                            if unsaved == 1 { "it" } else { "them" }
                        )
                    })
                    .size(12.0),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new("There is no undo for this.")
                        .color(theme.warn)
                        .size(11.0),
                );
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui.button("Revert").clicked() {
                        go = true;
                    }
                    if ui.button("Keep what I have").clicked() {
                        keep = false;
                    }
                });
            });

        if go {
            keep = false;
            let path = self.doc().map(|d| d.path.clone());
            if let Some(path) = path {
                // Closed without saving, which is the whole point, then opened
                // again from what is actually on disk.
                let at = self.current;
                if at < self.docs.len() {
                    self.docs.remove(at);
                    self.current = self.current.min(self.docs.len().saturating_sub(1));
                }
                self.open(path);
                self.status = "Back to the drawing as it is saved on disk.".into();
            }
        }
        if !keep {
            self.reverting = None;
        }
    }
}

/// The label column on the About window.
fn gutter(ui: &mut egui::Ui, label: &str, theme: ui::chrome::Theme) {
    ui.allocate_ui_with_layout(
        egui::vec2(92.0, 16.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(92.0);
            ui.label(RichText::new(label).color(theme.faint).size(11.0));
        },
    );
}
