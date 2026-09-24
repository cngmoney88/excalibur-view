//! The window furniture: menu bar, toolbars, the panel rail and the navigation
//! bar along the bottom.
//!
//! The arrangement follows the user's own Revu profile — same toolbars in the
//! same order, same panels down the left, same bar along the bottom — so that
//! nothing has to be hunted for. The drawing of it is ours.

use std::collections::BTreeMap;

use egui::{Align2, Color32, CornerRadius, FontId, Rect, Response, Sense, Stroke, Ui, Vec2};

use crate::command::{self, Command, Kind};
use crate::icon;
use crate::menu::{self, Entry};

#[derive(Clone, Copy)]
pub struct Theme {
    pub chrome: Color32,
    pub bar: Color32,
    pub sunken: Color32,
    pub line: Color32,
    pub text: Color32,
    pub faint: Color32,
    pub glyph: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub accent: Color32,
    /// The accent as text on a panel, which needs more contrast than the
    /// accent used behind a selected row.
    pub accent_text: Color32,
    pub warn: Color32,
    pub surround: Color32,
}

impl Theme {
    /// The dark palette, taken from the shield: navy body, steel bracing,
    /// silver blade. Deliberately not Bluebeam's neutral grey — a drawing
    /// viewer is looked at all day, and navy holds a white sheet better than
    /// grey does, which is also why plan rooms were never painted grey.
    pub fn dark() -> Theme {
        Theme {
            chrome: Color32::from_rgb(22, 29, 40),
            bar: Color32::from_rgb(28, 37, 50),
            sunken: Color32::from_rgb(15, 22, 32),
            line: Color32::from_rgb(44, 57, 73),
            text: Color32::from_rgb(227, 233, 240),
            faint: Color32::from_rgb(138, 153, 172),
            glyph: Color32::from_rgb(201, 212, 225),
            hover: Color32::from_rgb(37, 49, 63),
            pressed: Color32::from_rgb(15, 22, 32),
            accent: Color32::from_rgb(60, 110, 159),
            accent_text: Color32::from_rgb(127, 176, 222),
            warn: Color32::from_rgb(217, 164, 65),
            surround: Color32::from_rgb(31, 42, 56),
        }
    }

    /// The same palette turned over: the shield's silver becomes the chrome and
    /// its navy becomes the type.
    pub fn light() -> Theme {
        Theme {
            chrome: Color32::from_rgb(237, 240, 244),
            bar: Color32::from_rgb(246, 248, 250),
            sunken: Color32::from_rgb(255, 255, 255),
            line: Color32::from_rgb(198, 207, 218),
            text: Color32::from_rgb(20, 28, 38),
            faint: Color32::from_rgb(94, 110, 130),
            glyph: Color32::from_rgb(43, 68, 97),
            hover: Color32::from_rgb(220, 229, 240),
            pressed: Color32::from_rgb(200, 213, 228),
            accent: Color32::from_rgb(43, 68, 97),
            accent_text: Color32::from_rgb(35, 72, 104),
            warn: Color32::from_rgb(168, 113, 15),
            surround: Color32::from_rgb(154, 167, 182),
        }
    }

    /// True when this palette is a dark one, which decides whether type and
    /// glyphs on an accent fill should be white or the palette's own light.
    pub fn is_dark(&self) -> bool {
        self.text.r() > 128
    }

    /// The colour for type sitting on an accent fill.
    pub fn on_accent(&self) -> Color32 {
        Color32::from_rgb(240, 245, 250)
    }

    /// A glyph for something that cannot be used just now: the faint colour,
    /// knocked back towards the toolbar behind it. Solid rather than see-through, so a
    /// line's round ends do not show as darker dots where they overlap it.
    pub fn unavailable(&self) -> Color32 {
        icon::mix(self.bar, self.faint, 0.6)
    }

    /// Applies the palette to egui's own widgets so dialogs and lists match.
    ///
    /// egui keeps **two** sets of visuals, a light one and a dark one, and
    /// swaps between them when it notices what the operating system is set to.
    /// Setting only the set in use looks right on the machine it was written on
    /// and then, on a Windows box in light mode, paints the panels in our
    /// colours and every dialog in egui's — a dark program with a white
    /// sign-in window sitting in the middle of it. So both sets are filled in
    /// with the same palette and the preference is pinned: whatever the system
    /// decides, the program looks like itself.
    pub fn apply(&self, ctx: &egui::Context) {
        let visuals = self.visuals();
        ctx.set_visuals_of(egui::Theme::Dark, visuals.clone());
        ctx.set_visuals_of(egui::Theme::Light, visuals.clone());
        ctx.set_visuals(visuals);
        ctx.options_mut(|options| {
            options.theme_preference = if self.is_dark() {
                egui::ThemePreference::Dark
            } else {
                egui::ThemePreference::Light
            };
        });
    }

    fn visuals(&self) -> egui::Visuals {
        let mut visuals = if self.is_dark() {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        visuals.panel_fill = self.chrome;
        visuals.window_fill = self.bar;
        visuals.extreme_bg_color = self.sunken;
        visuals.faint_bg_color = self.hover;
        visuals.selection.bg_fill = self.accent.gamma_multiply(0.55);
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.line);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.text);
        visuals.widgets.inactive.bg_fill = self.bar;
        visuals.widgets.inactive.weak_bg_fill = self.bar;
        visuals.widgets.hovered.bg_fill = self.hover;
        visuals.widgets.hovered.weak_bg_fill = self.hover;
        visuals.widgets.active.bg_fill = self.pressed;
        visuals.window_stroke = Stroke::new(1.0, self.line);
        visuals.window_corner_radius = egui::CornerRadius::same(6);
        visuals.menu_corner_radius = egui::CornerRadius::same(5);
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.corner_radius = egui::CornerRadius::same(4);
        }
        visuals
    }
}

/// A few symbols the default fonts do not have — → ✓ ✗ ▲ ▼ ′ ″ among them —
/// cut from DejaVu Sans (see `fonts/LICENSE-symbols.txt`). Without them each
/// shows as an empty box: in "Help → About", in the Claude check's ticks and
/// crosses, in a column's sort arrow.
static SYMBOLS: &[u8] = include_bytes!("../fonts/symbols.ttf");

/// Adds the symbols as the last fallback of every font family. Call once,
/// when the window is made.
pub fn fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "excalibur-symbols".into(),
        std::sync::Arc::new(egui::FontData::from_static(SYMBOLS)),
    );
    for family in fonts.families.values_mut() {
        family.push("excalibur-symbols".into());
    }
    ctx.set_fonts(fonts);
}

pub const BUTTON: f32 = 26.0;
pub const GLYPH: f32 = 17.0;
/// The accent tile behind a tool, and the glyph that sits on it: about two
/// thirds of the tile, so the tile shows round the icon.
pub const TILE: f32 = 21.0;
pub const GLYPH_ON_TILE: f32 = 14.5;

/// Whether a command's icon sits on an accent tile: the tools that draw or
/// measure on a sheet, and Set Scale, which every measurement depends on.
pub fn wears_a_tile(command: &Command) -> bool {
    command.is_tool() || command.id == "Measure.Calibrate"
}

/// What the chrome knows: which tool is up, which settings are on, and what was
/// clicked since the app last looked.
pub struct Chrome {
    pub theme: Theme,
    pub tool: String,
    toggles: BTreeMap<String, bool>,
    fired: Vec<String>,
    /// Commands that cannot be used right now, with the reason.
    disabled: BTreeMap<String, String>,
    /// What the Plugins menu lists: the office's own tools, which are not
    /// commands this program was built with.
    pub plugins: Vec<PluginItem>,
}

/// One entry in the Plugins menu.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginItem {
    /// What clicking it fires.
    pub fire: String,
    /// The plugin it belongs to, as a heading.
    pub group: String,
    pub label: String,
    pub help: String,
}

impl Default for Chrome {
    fn default() -> Chrome {
        Chrome::new()
    }
}

impl Chrome {
    pub fn new() -> Chrome {
        Chrome {
            theme: Theme::dark(),
            tool: "Select".into(),
            toggles: BTreeMap::new(),
            fired: Vec::new(),
            disabled: BTreeMap::new(),
            plugins: Vec::new(),
        }
    }

    /// Everything clicked since this was last called.
    pub fn take_fired(&mut self) -> Vec<String> {
        std::mem::take(&mut self.fired)
    }

    pub fn fire(&mut self, id: &str) {
        self.fired.push(id.to_string());
    }

    pub fn is_on(&self, id: &str) -> bool {
        *self.toggles.get(id).unwrap_or(&false)
    }

    pub fn set(&mut self, id: &str, on: bool) {
        self.toggles.insert(id.to_string(), on);
    }

    pub fn turn(&mut self, id: &str) -> bool {
        let next = !self.is_on(id);
        self.set(id, next);
        next
    }

    /// Greys a command out and says why on hover. Better than a button that
    /// looks ready and does nothing.
    pub fn disable(&mut self, id: &str, why: &str) {
        self.disabled.insert(id.to_string(), why.to_string());
    }

    pub fn enable(&mut self, id: &str) {
        self.disabled.remove(id);
    }

    pub fn why_disabled(&self, id: &str) -> Option<&str> {
        self.disabled.get(id).map(|s| s.as_str())
    }

    /// True when this command is the tool currently in the user's hand, or a
    /// setting that is switched on.
    pub fn looks_active(&self, command: &Command) -> bool {
        if command.is_tool() {
            self.tool == command.id
        } else {
            self.is_on(command.id)
        }
    }

    /// One toolbar button. A dropdown gets a little chevron, and a combo gets a
    /// box wide enough to read what is in it, because a font name or a scale
    /// squeezed into a square tells nobody anything.
    pub fn button(&mut self, ui: &mut Ui, id: &str) -> Response {
        let known = command::find(id);
        let width = match known.map(|c| c.kind) {
            Some(Kind::Combo) => 110.0,
            Some(Kind::DropDown) => BUTTON + 9.0,
            Some(Kind::Label) => return self.readout(ui, known.unwrap()),
            _ => BUTTON,
        };
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, BUTTON), Sense::click());
        let Some(command) = known else {
            // A profile from elsewhere may name something we have never heard
            // of. Show it rather than leaving a hole in the row.
            let painter = ui.painter();
            painter.rect_stroke(
                rect.shrink(3.0),
                CornerRadius::same(3),
                Stroke::new(1.0, self.theme.line),
                egui::StrokeKind::Inside,
            );
            icon::letter(painter, id, rect, self.theme.faint);
            return response.on_hover_text(format!("{id}\nThis command is not in Excalibur View."));
        };

        let off = self.why_disabled(id).map(|s| s.to_string());
        let active = self.looks_active(command);
        let painter = ui.painter();
        let tiled = wears_a_tile(command) && command.glyph.is_some();
        let colour = if tiled {
            // The drawing and measuring tools sit on an accent tile, so the
            // things that put marks on a drawing stand apart from the rest of
            // the row at a glance.
            let look = match (&off, active) {
                (Some(_), _) => icon::Tile::Off,
                (None, true) => icon::Tile::Active,
                (None, false) if response.is_pointer_button_down_on() => icon::Tile::Pressed,
                (None, false) if response.hovered() => icon::Tile::Hover,
                (None, false) => icon::Tile::Rest,
            };
            let face = Rect::from_center_size(rect.center(), Vec2::splat(TILE));
            if active {
                painter.rect_stroke(
                    face.expand(1.5),
                    CornerRadius::same((TILE * 0.25 + 1.5).round() as u8),
                    Stroke::new(1.25, self.theme.accent_text),
                    egui::StrokeKind::Middle,
                );
            }
            icon::tile(painter, face, look, self.theme.bar);
            icon::glyph_on_tile(look, self.theme.bar, self.theme.glyph, self.theme.unavailable())
        } else {
            if active {
                // A setting that is on is marked by a tint and a bar beneath
                // it, not a solid block. A row of solid blocks is what makes
                // a busy toolbar hard to read at a glance.
                painter.rect_filled(
                    rect.shrink(1.0),
                    CornerRadius::same(4),
                    self.theme.accent.gamma_multiply(0.30),
                );
                painter.rect_filled(
                    Rect::from_min_max(
                        egui::pos2(rect.left() + 4.0, rect.bottom() - 3.0),
                        egui::pos2(rect.right() - 4.0, rect.bottom() - 1.0),
                    ),
                    CornerRadius::same(1),
                    self.theme.accent_text,
                );
            } else if response.is_pointer_button_down_on() {
                painter.rect_filled(rect.shrink(1.0), CornerRadius::same(4), self.theme.pressed);
            } else if response.hovered() && off.is_none() {
                painter.rect_filled(rect.shrink(1.0), CornerRadius::same(4), self.theme.hover);
            }
            match (&off, active) {
                (Some(_), _) => self.theme.unavailable(),
                (None, true) => self.theme.on_accent(),
                (None, false) => self.theme.glyph,
            }
        };
        match command.kind {
            Kind::Combo => {
                painter.rect_stroke(
                    rect.shrink(2.0),
                    CornerRadius::same(3),
                    Stroke::new(1.0, self.theme.line),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    egui::pos2(rect.left() + 8.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    command.label,
                    FontId::proportional(12.0),
                    colour,
                );
                chevron(painter, egui::pos2(rect.right() - 10.0, rect.center().y), colour);
            }
            Kind::DropDown => {
                let inner = Rect::from_center_size(
                    egui::pos2(rect.center().x - 4.0, rect.center().y),
                    Vec2::splat(GLYPH),
                );
                match command.glyph {
                    Some(glyph) => icon::draw(painter, glyph, inner, colour, 1.0),
                    None => icon::letter(painter, command.id, inner, colour),
                }
                chevron(painter, egui::pos2(rect.right() - 7.0, rect.center().y + 4.0), colour);
            }
            _ => {
                let size = if tiled { GLYPH_ON_TILE } else { GLYPH };
                let inner = Rect::from_center_size(rect.center(), Vec2::splat(size));
                match command.glyph {
                    Some(glyph) => icon::draw(painter, glyph, inner, colour, 1.0),
                    None => icon::letter(painter, command.id, rect, colour),
                }
            }
        }

        let response = match &off {
            Some(why) => response.on_hover_text(format!("{}\n{why}", command.label)),
            None => response.on_hover_text(command.tooltip()),
        };
        if response.clicked() && off.is_none() {
            if command.is_tool() {
                self.tool = command.id.to_string();
            } else if command.kind == Kind::Toggle {
                self.turn(command.id);
            }
            self.fired.push(command.id.to_string());
        }
        response
    }

    /// A read-only strip such as the page size, which shows rather than acts.
    fn readout(&self, ui: &mut Ui, command: &Command) -> Response {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(70.0, BUTTON), Sense::hover());
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            command.label,
            FontId::proportional(11.0),
            self.theme.faint,
        );
        response.on_hover_text(command.tooltip())
    }

    fn separator(&self, ui: &mut Ui) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(7.0, BUTTON), Sense::hover());
        let x = rect.center().x.round();
        ui.painter().line_segment(
            [
                egui::pos2(x, rect.top() + 4.0),
                egui::pos2(x, rect.bottom() - 4.0),
            ],
            Stroke::new(1.0, self.theme.line),
        );
    }

    /// Lays out one toolbar from a profile, keeping the order and skipping the
    /// buttons the profile marks hidden.
    pub fn toolbar(&mut self, ui: &mut Ui, bar: &chest::ToolBar) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            let mut last_was_gap = true;
            for item in &bar.items {
                match item {
                    chest::profile::Item::Separator => {
                        if !last_was_gap {
                            self.separator(ui);
                            last_was_gap = true;
                        }
                    }
                    chest::profile::Item::Command { id, visible } => {
                        if !visible {
                            continue;
                        }
                        self.button(ui, id);
                        last_was_gap = false;
                    }
                }
            }
        });
    }

    /// Every visible toolbar, laid out in the rows the profile puts them in.
    ///
    /// A row that is wider than the window carries on underneath rather than
    /// running off the edge. Revu's rows were laid out on somebody's wide
    /// monitor; on a laptop at 150% the right-hand third of them would simply
    /// not be there, and a tool you cannot see is a tool you do not have.
    /// Whole toolbars move down together, so a group is never split.
    pub fn toolbars(&mut self, ui: &mut Ui, bars: &[chest::ToolBar]) {
        let mut rows: BTreeMap<i32, Vec<&chest::ToolBar>> = BTreeMap::new();
        for bar in bars.iter().filter(|b| b.visible && !b.items.is_empty()) {
            rows.entry(bar.y).or_default().push(bar);
        }
        let room = ui.available_width().max(BUTTON * 4.0);
        for (_, mut row) in rows {
            row.sort_by_key(|b| b.x);
            for line in flow(&row, room) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (i, bar) in line.iter().enumerate() {
                        if i > 0 {
                            self.separator(ui);
                        }
                        self.toolbar(ui, bar);
                    }
                });
            }
        }
    }

    /// The menu bar.
    pub fn menu_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for m in menu::BAR {
                if m.name == "Help" {
                    ui.menu_button("Plugins", |ui| {
                        ui.set_min_width(260.0);
                        self.plugins_menu(ui);
                    });
                }
                ui.menu_button(m.name, |ui| {
                    ui.set_min_width(260.0);
                    self.entries(ui, m.entries);
                });
            }
        });
    }

    /// The office's plugins, each under its own name, then the two ways to
    /// look after them.
    fn plugins_menu(&mut self, ui: &mut Ui) {
        if self.plugins.is_empty() {
            ui.add_enabled(false, egui::Button::new("No plugins on this computer"))
                .on_disabled_hover_text(
                    "An office's own tools arrive here from its server, or are added below.",
                );
        }
        let mut heading: Option<&str> = None;
        let mut clicked: Option<String> = None;
        for item in &self.plugins {
            if heading != Some(item.group.as_str()) {
                if heading.is_some() {
                    ui.separator();
                }
                ui.label(
                    egui::RichText::new(&item.group)
                        .color(self.theme.faint)
                        .size(11.0),
                );
                heading = Some(item.group.as_str());
            }
            let mut button = ui.button(&item.label);
            if !item.help.is_empty() {
                button = button.on_hover_text(&item.help);
            }
            if button.clicked() {
                clicked = Some(item.fire.clone());
            }
        }
        ui.separator();
        if ui.button("Add Plugin…").clicked() {
            clicked = Some("Plugins.Add".into());
        }
        if ui.button("Manage Plugins…").clicked() {
            clicked = Some("Plugins.Manage".into());
        }
        if ui
            .button("Save This Sheet for a Plugin Test…")
            .on_hover_text(
                "Writes what a plugin would be handed for the sheet on screen — its words, \
                 line-work, scale and markups — to a JSON file, for somebody writing a plugin \
                 to test against.",
            )
            .clicked()
        {
            clicked = Some("Plugins.SaveInput".into());
        }
        if ui
            .button("Save This Drawing for a Plugin Test…")
            .on_hover_text("The same, for every sheet in the drawing, for a command that looks at them all.")
            .clicked()
        {
            clicked = Some("Plugins.SaveInputAll".into());
        }
        if let Some(fire) = clicked {
            self.fired.push(fire);
            ui.close();
        }
    }

    fn entries(&mut self, ui: &mut Ui, entries: &'static [Entry]) {
        for entry in entries {
            match entry {
                Entry::Line => {
                    ui.separator();
                }
                Entry::More(name, inner) => {
                    ui.menu_button(*name, |ui| {
                        ui.set_min_width(240.0);
                        self.entries(ui, inner);
                    });
                }
                Entry::Later(label, why) => {
                    ui.add_enabled(false, egui::Button::new(*label))
                        .on_disabled_hover_text(*why);
                }
                Entry::Item(id) => self.menu_item(ui, id, None),
                Entry::Named(id, label) => self.menu_item(ui, id, Some(label)),
            }
        }
    }

    fn menu_item(&mut self, ui: &mut Ui, id: &str, label: Option<&str>) {
        let Some(command) = command::find(id) else {
            return;
        };
        let name = label.unwrap_or(command.label);
        let off = self.why_disabled(id).map(|s| s.to_string());
        let ticked = self.looks_active(command);

        let response = ui.add_enabled_ui(off.is_none(), |ui| {
            ui.horizontal(|ui| {
                let (mark, _) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::hover());
                if ticked {
                    // A drawn tick, because the character is missing from
                    // plenty of fonts and shows as an empty box.
                    let c = mark.center();
                    ui.painter().add(egui::Shape::line(
                        vec![
                            egui::pos2(c.x - 4.0, c.y),
                            egui::pos2(c.x - 1.0, c.y + 3.5),
                            egui::pos2(c.x + 4.5, c.y - 4.0),
                        ],
                        Stroke::new(1.8, self.theme.accent),
                    ));
                }
                let clicked = ui.button(name).clicked();
                if let Some(keys) = command.shortcut {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(keys).color(self.theme.faint).size(11.0));
                    });
                }
                clicked
            })
            .inner
        });
        if response.inner {
            if command.is_tool() {
                self.tool = command.id.to_string();
            } else if command.kind == Kind::Toggle {
                self.turn(command.id);
            }
            self.fired.push(command.id.to_string());
            ui.close();
        }
    }
}

fn chevron(painter: &egui::Painter, at: egui::Pos2, colour: Color32) {
    let points = vec![
        egui::pos2(at.x - 3.5, at.y - 2.0),
        egui::pos2(at.x, at.y + 2.0),
        egui::pos2(at.x + 3.5, at.y - 2.0),
    ];
    painter.add(egui::epaint::PathShape {
        points,
        closed: true,
        fill: colour,
        stroke: Stroke::NONE.into(),
    });
}

/// How wide one toolbar draws, in points: what `toolbar` will allocate.
/// The room left in a row for a text box that has a button after it —
/// exactly, so the row is never a pixel wider than the window.
///
/// A dialog sizes itself to what it held last frame. A row that asks for
/// even a pixel more than there is makes the window a pixel wider, which
/// gives the row a pixel more to ask for: the window creeps outward every
/// frame until it runs off the screen.
pub fn room_beside(ui: &Ui, button: &str) -> f32 {
    let text = ui.fonts(|f| {
        f.layout_no_wrap(
            button.to_string(),
            egui::TextStyle::Button.resolve(ui.style()),
            Color32::WHITE,
        )
        .size()
        .x
    });
    let spacing = ui.spacing();
    let button = text + spacing.button_padding.x * 2.0;
    (ui.available_width() - button - spacing.item_spacing.x - 1.0).max(120.0)
}

pub fn toolbar_width(bar: &chest::ToolBar) -> f32 {
    let mut width = 0.0;
    let mut last_was_gap = true;
    let mut count = 0usize;
    for item in &bar.items {
        match item {
            chest::profile::Item::Separator => {
                if !last_was_gap {
                    width += 7.0;
                    count += 1;
                    last_was_gap = true;
                }
            }
            chest::profile::Item::Command { id, visible } => {
                if !visible {
                    continue;
                }
                width += match command::find(id).map(|c| c.kind) {
                    Some(Kind::Combo) => 110.0,
                    Some(Kind::DropDown) => BUTTON + 9.0,
                    Some(Kind::Label) => 70.0,
                    _ => BUTTON,
                };
                count += 1;
                last_was_gap = false;
            }
        }
    }
    // One point between items, as `toolbar` spaces them.
    width + count.saturating_sub(1) as f32
}

/// Splits one row of toolbars into lines no wider than `room`, keeping each
/// toolbar whole. A toolbar wider than the room on its own gets a line to
/// itself.
pub fn flow<'a>(row: &[&'a chest::ToolBar], room: f32) -> Vec<Vec<&'a chest::ToolBar>> {
    // Between toolbars: the spacing either side of a separator.
    const BETWEEN: f32 = 7.0 + 2.0 * 2.0;
    let mut lines: Vec<Vec<&chest::ToolBar>> = Vec::new();
    let mut used = 0.0;
    for bar in row {
        let width = toolbar_width(bar);
        match lines.last_mut() {
            Some(line) if used + BETWEEN + width <= room => {
                line.push(bar);
                used += BETWEEN + width;
            }
            _ => {
                lines.push(vec![bar]);
                used = width;
            }
        }
    }
    lines
}

/// Hyperview's own arrangement, for a seat that has no Revu profile, or whose
/// profile is a tool set on its own with no toolbars in it. Revu's rows, with
/// every tool Hyperview has on them — not a cut-down set to get started with.
pub fn standard_toolbars() -> Vec<chest::ToolBar> {
    const BARS: &[(&str, i32, i32, &[&str])] = &[
        ("toolStripFile", 0, 3, &["Document.New", "File.CreatePDF", "File.CombinePDFs", "File.Open", "Document.Save", "Document.Print", "Split.Email", "Document.NewPackage", "Search", "SpellCheck"]),
        ("toolStripAdvancedText", 0, 383, &["Edit.Text", "|", "Markup.ReviewText", "|", "Markup.Underline", "Markup.Squiggly", "Markup.Strikethrough"]),
        ("toolStripMeasure", 0, 586, &["Measure.Tool", "Measure.Calibrate", "|", "Measure.Length", "Measure.Polylength", "Measure.Area", "Measure.Perimeter", "Measure.Diameter", "Measure.Angle", "Measure.Radius", "Measure.Volume", "Measure.Count", "|", "Measure.AreaCutout", "Measure.AreaEllipseCutout", "Measure.DynamicFill"]),
        ("toolStripText", 0, 1113, &["Markup.TextBox", "Markup.Typewriter", "Markup.Note", "Markup.Callout", "Markup.Flag", "|", "Markup.Highlight", "Markup.Pen", "Eraser", "|", "Markup.Cloud9", "Markup.Cloud", "Button.Stamp", "Markup.Image", "Snapshot"]),
        ("toolStripLine", 0, 1457, &["Markup.Line", "Markup.Arrow", "Markup.Arc", "Markup.Polyline", "Markup.Dimension", "Markup.Rectangle", "Markup.Ellipse", "Markup.Polygon"]),
        ("toolStripRotateDocument", 0, 1756, &["Document.RotateCounterclockwise", "Document.RotateClockwise"]),
        ("toolStripDigitalSignature", 0, 1839, &["Markup.DigitalSignature"]),
        ("toolStripEdit", 36, 0, &["Edit.Undo", "Edit.Redo", "|", "Edit.Cut", "Edit.Copy", "Edit.Paste", "Delete"]),
        ("toolStripFont", 36, 3, &["Combo.Font", "Combo.FontSize", "|", "DropDown.ColorText", "|", "Button.Bold", "Button.Italic", "Button.Underline", "Button.Strikethrough", "Button.Superscript", "Button.Subscript", "|", "Button.TextAlignLeft", "Button.TextAlignCenter", "Button.TextAlignRight", "|", "Button.TextVAlignTop", "Button.TextVAlignMiddle", "Button.TextVAlignBottom"]),
        ("toolStripPanZoom", 36, 529, &["Select", "Pan", "Zoom", "Measure.Tool", "Lasso"]),
        ("toolStripAnnotation", 36, 720, &["Markup.Hyperlink", "Markup.FileAttachment", "Document.Flatten", "Edit.EraseContent"]),
        ("toolStripAppearance", 36, 875, &["DropDown.ColorFill", "DropDown.Opacity", "DropDown.HatchPattern"]),
        ("toolStripSketch", 36, 1021, &["Markup.PolygonSketchToScale", "Markup.RectangleSketchToScale", "Markup.EllipseSketchToScale", "Markup.PolylineSketchToScale"]),
        // What a markup looks like once it is drawn, and where it sits.
        ("toolStripProperties", 36, 1150, &["DropDown.ColorLine", "DropDown.LineWidth", "DropDown.LineStyle", "DropDown.LineStart", "DropDown.LineEnd", "Markup.FormatPainter"]),
        ("toolStripArrange", 36, 1320, &["Markup.BringToFront", "Markup.BringForward", "Markup.SendBackward", "Markup.SendToBack"]),
        ("toolStripAlign", 36, 1440, &["Align.Left", "Align.Center", "Align.Right", "Align.Top", "Align.Middle", "Align.Bottom", "|", "Align.SpacingHorizontal", "Align.SpacingVertical", "|", "Align.Width", "Align.Height", "Align.Size", "|", "Align.FlipHorizontal", "Align.FlipVertical", "Align.CenterDocument"]),
        ("toolStripPoints", 36, 1900, &["ControlPoint.AddMode", "ControlPoint.SubtractMode", "ControlPoint.ConvertMode"]),
        ("toolStripDocumentTools", 36, 2000, &["Edit.SelectText", "Markup.Redaction", "Document.ApplyRedactions", "Document.Ocr", "Document.HeadersAndFooters", "CropImage", "Toggle.FullScreen"]),
    ];
    BARS.iter()
        .map(|(name, y, x, items)| chest::ToolBar {
            name: name.to_string(),
            module: None,
            x: *x,
            y: *y,
            visible: true,
            location: chest::profile::Dock::Top,
            items: items
                .iter()
                .map(|id| match *id {
                    "|" => chest::profile::Item::Separator,
                    id => chest::profile::Item::Command {
                        id: id.to_string(),
                        visible: true,
                    },
                })
                .collect(),
        })
        .collect()
}

/// A profile's toolbars with every Hyperview tool it leaves out added back.
///
/// A Revu profile is somebody's arrangement, and Revu keeps some of its most
/// used tools — Select, Pan, Undo — in places Hyperview does not copy. A seat
/// should never be missing a tool because of how somebody else's Revu was
/// set up, so whatever the standard bars carry that the profile does not
/// show goes on its second row: the arrow, the hand and Undo first, where a
/// hand reaches for them, and the rest after the profile's own bars.
pub fn completed(bars: &[chest::ToolBar]) -> Vec<chest::ToolBar> {
    let mut shown: std::collections::HashSet<String> = bars
        .iter()
        .filter(|b| b.visible)
        .flat_map(|b| b.shown())
        .map(|id| id.to_string())
        .collect();
    let mut rows: Vec<i32> = bars.iter().filter(|b| b.visible && !b.items.is_empty()).map(|b| b.y).collect();
    rows.sort_unstable();
    rows.dedup();
    let second = match (rows.first(), rows.get(1)) {
        (_, Some(y)) => *y,
        (Some(y), None) => y + 36,
        (None, None) => 36,
    };
    let mut out: Vec<chest::ToolBar> = bars.to_vec();
    for (k, standard) in standard_toolbars().into_iter().enumerate() {
        let missing: Vec<chest::profile::Item> = standard
            .items
            .iter()
            .filter(|item| match item {
                chest::profile::Item::Command { id, .. } => shown.insert(id.clone()),
                _ => false,
            })
            .cloned()
            .collect();
        if missing.is_empty() {
            continue;
        }
        let first = matches!(standard.name.as_str(), "toolStripEdit" | "toolStripPanZoom");
        out.push(chest::ToolBar {
            name: format!("hyperview:{}", standard.name),
            module: None,
            x: if first { -10_000 + k as i32 } else { 100_000 + k as i32 },
            y: second,
            visible: true,
            location: chest::profile::Dock::Top,
            items: missing,
        });
    }
    out
}

/// The panels on the rail for a seat whose profile does not say.
pub const STANDARD_PANELS: &[&str] = &[
    "Thumbnails", "Bookmarks", "Layers", "Tool Chest", "Measurements", "Properties", "Search",
    "Signatures", "Studio", "Flags",
];

/// Whether a profile's own toolbars are enough to work with. A tool set on
/// its own (a `.btx`) has none; neither does a profile with every bar hidden.
pub fn has_toolbars(bars: &[chest::ToolBar]) -> bool {
    bars.iter()
        .filter(|b| b.visible)
        .map(|b| b.shown().iter().filter(|id| command::find(id).is_some()).count())
        .sum::<usize>()
        >= 12
}

/// The icon for a panel in the left rail.
pub fn panel_glyph(name: &str) -> icon::Glyph {
    match name {
        "Thumbnails" => icon::THUMBNAILS,
        "Tool Chest" => icon::CHEST,
        "Markups" => icon::GRID,
        "Measurements" => icon::RULE,
        "Layers" => icon::LAYERS,
        "Bookmarks" => icon::BOOKMARK,
        "Properties" => icon::PROPERTIES,
        "Search" => icon::SEARCH,
        "Signatures" => icon::SIGN,
        "Flags" => icon::FLAG,
        "File Manager" => icon::OPEN,
        "Studio" => icon::LINK,
        "Server" => icon::LINK,
        "Spaces" => icon::PERIMETER,
        "Hyperlinks" => icon::LINK,
        "Sets" => icon::COPY,
        "Acroforms" => icon::TEXT_BOX,
        _ => icon::PANEL,
    }
}

/// Panels Hyperview can actually show, and what the others would need.
pub fn panel_ready(name: &str) -> Result<(), &'static str> {
    match name {
        "Thumbnails" | "Tool Chest" | "Markups" | "Measurements" | "Search" | "Properties"
        | "Server" | "Spaces" | "Layers" | "Bookmarks" | "Flags" | "Hyperlinks"
        | "Acroforms" | "Forms" | "Signatures" | "Studio" => Ok(()),
        "Sets" => Err("Open a folder of drawings from File \u{2192} Open instead; each one \
                       gets its own tab."),
        "File Manager" => Err("Use the Server panel for the company's drawings, or \
                               File \u{2192} Open for anything else."),
        _ => Err("This panel is not built yet."),
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn every_symbol_the_program_shows_has_a_glyph() {
        // Each character here is in a message, a label or a report somewhere
        // in the program. Without the symbols font they drew as empty boxes.
        let ctx = egui::Context::default();
        fonts(&ctx);
        let _ = ctx.run(Default::default(), |_| {});
        for family in [egui::FontId::proportional(12.0), egui::FontId::monospace(12.0)] {
            for c in "→✓✗▲▼′″°²³·½×—“”…".chars() {
                assert!(ctx.fonts(|f| f.has_glyph(&family, c)), "{c} in {family:?}");
            }
        }
    }

    use super::*;

    #[test]
    fn picking_a_tool_puts_down_the_one_before_it() {
        let mut chrome = Chrome::new();
        assert_eq!(chrome.tool, "Select");
        chrome.tool = "Measure.Length".into();
        assert!(chrome.looks_active(command::find("Measure.Length").unwrap()));
        assert!(!chrome.looks_active(command::find("Select").unwrap()));
    }

    #[test]
    fn a_setting_stays_on_once_switched_on() {
        let mut chrome = Chrome::new();
        let invert = command::find("View.InvertColors").unwrap();
        assert!(!chrome.looks_active(invert));
        chrome.turn("View.InvertColors");
        assert!(chrome.looks_active(invert));
        chrome.turn("View.InvertColors");
        assert!(!chrome.looks_active(invert));
    }

    #[test]
    fn a_disabled_command_says_why() {
        let mut chrome = Chrome::new();
        assert_eq!(chrome.why_disabled("Document.Save"), None);
        chrome.disable("Document.Save", "Nothing has changed.");
        assert_eq!(chrome.why_disabled("Document.Save"), Some("Nothing has changed."));
        chrome.enable("Document.Save");
        assert_eq!(chrome.why_disabled("Document.Save"), None);
    }

    #[test]
    fn what_was_clicked_is_handed_over_once() {
        let mut chrome = Chrome::new();
        chrome.fire("Document.Save");
        chrome.fire("View.FitPage");
        assert_eq!(chrome.take_fired(), vec!["Document.Save", "View.FitPage"]);
        assert!(chrome.take_fired().is_empty());
    }

    #[test]
    fn the_standard_toolbars_carry_every_tool_and_nothing_hyperview_lacks() {
        let bars = standard_toolbars();
        assert!(has_toolbars(&bars));
        let shown: Vec<&str> = bars.iter().flat_map(|b| b.shown()).collect();
        for id in &shown {
            assert!(command::find(id).is_some(), "{id} is on the standard toolbars but not in Excalibur View");
        }
        for must in [
            "Measure.Calibrate", "Measure.Length", "Measure.Area", "Measure.Count",
            "Measure.Polylength", "Measure.Perimeter", "Measure.Volume", "Measure.DynamicFill",
            "Markup.Cloud", "Markup.TextBox", "Markup.Callout", "Markup.Pen", "Markup.Dimension",
            "Select", "Pan", "Document.Save", "Document.Print",
        ] {
            assert!(shown.contains(&must), "{must} is missing from the standard toolbars");
        }
        assert!(shown.len() >= 70, "only {} buttons", shown.len());
    }

    #[test]
    fn a_row_too_wide_for_the_window_carries_on_underneath_in_whole_toolbars() {
        let bars = standard_toolbars();
        let top: Vec<&chest::ToolBar> = bars.iter().filter(|b| b.y == 0).collect();
        let total: f32 = top.iter().map(|b| toolbar_width(b)).sum();
        // Wide enough: one line.
        assert_eq!(flow(&top, total + 200.0).len(), 1);
        // A laptop at 150%: more lines, every toolbar still there, none split.
        let lines = flow(&top, 1200.0);
        assert!(lines.len() >= 2);
        assert_eq!(lines.iter().map(|l| l.len()).sum::<usize>(), top.len());
        for line in &lines {
            let used: f32 = line.iter().map(|b| toolbar_width(b)).sum();
            assert!(line.len() == 1 || used <= 1200.0);
        }
    }

    #[test]
    fn a_tool_set_on_its_own_is_not_a_set_of_toolbars() {
        assert!(!has_toolbars(&[]));
    }

    #[test]
    fn a_profile_that_hides_the_arrow_and_the_hand_still_gets_them_first_on_its_second_row() {
        // Two rows of his own: the file tools and the font bar. Nothing to
        // select with, pan with or undo with.
        let mine: Vec<chest::ToolBar> = standard_toolbars()
            .into_iter()
            .filter(|b| b.name == "toolStripFile" || b.name == "toolStripFont")
            .collect();
        let all = completed(&mine);
        let everything: Vec<String> = all.iter().flat_map(|b| b.shown()).map(|s| s.to_string()).collect();
        for id in ["Select", "Pan", "Zoom", "Edit.Undo", "Edit.Redo", "Delete", "Measure.Length", "Markup.Line"] {
            assert!(everything.iter().any(|e| e == id), "{id} is missing");
        }
        let second = all.iter().filter(|b| b.y == 36).min_by_key(|b| b.x).unwrap();
        assert!(second.shown().contains(&"Edit.Undo") || second.shown().contains(&"Select"));
        // Nothing he already shows is shown twice.
        let mut seen = std::collections::HashSet::new();
        assert!(everything.iter().all(|id| seen.insert(id.clone())), "a tool appears twice");
        // A full set needs nothing added.
        assert_eq!(completed(&standard_toolbars()).len(), standard_toolbars().len());
        let _ = &mine;
    }

    #[test]
    fn every_panel_in_his_profile_has_an_icon_and_an_honest_answer() {
        for name in [
            "Measurements", "File Manager", "Thumbnails", "Bookmarks", "Layers",
            "Tool Chest", "Properties", "Search", "Signatures", "Studio", "Flags",
            "Markups", "Spaces", "Sets", "Acroforms", "Hyperlinks",
        ] {
            let _ = panel_glyph(name);
            if let Err(why) = panel_ready(name) {
                assert!(why.ends_with('.'), "{name}: {why}");
            }
        }
    }

    #[test]
    fn the_dark_and_light_themes_both_keep_text_readable() {
        for theme in [Theme::dark(), Theme::light()] {
            let contrast = |a: Color32, b: Color32| {
                let l = |c: Color32| {
                    0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32
                };
                (l(a) - l(b)).abs()
            };
            assert!(
                contrast(theme.text, theme.chrome) > 100.0,
                "text on chrome is too close"
            );
            assert!(
                contrast(theme.glyph, theme.bar) > 90.0,
                "icons on the toolbar are too close"
            );
            assert!(
                contrast(Color32::WHITE, theme.accent) > 60.0,
                "a selected tool's icon has to show against the highlight"
            );
        }
    }
}

#[cfg(test)]
mod reach {
    /// Every command the program carries out is on a toolbar or in a menu —
    /// a tool nobody can find is a tool the program does not have.
    #[test]
    fn nothing_the_program_can_do_is_out_of_reach() {
        let mut reachable: std::collections::HashSet<String> = std::collections::HashSet::new();
        for bar in super::standard_toolbars() {
            for id in bar.shown() {
                reachable.insert(id.to_string());
            }
        }
        for m in crate::menu::BAR {
            crate::menu::walk(m.entries, &mut |e| match e {
                crate::menu::Entry::Item(id) | crate::menu::Entry::Named(id, _) => {
                    reachable.insert(id.to_string());
                }
                _ => {}
            });
        }
        // Kept for a profile's own bars and the bottom bar, or not built.
        const ELSEWHERE: &[&str] = &[
            "Document.OCR", "Document.Script", "Split.Profiles", "Press.EscapeKey", "Toggle.ShiftKey", "Markup.ImageFromCamera",
            "Tab.Spaces", "View.OneFullPage", "View.ScrollingPages", "View.PageFirst", "View.PagePrevious",
            "View.PageNext", "View.PageLast", "View.PreviousView", "View.NextView", "TextBox.Page", "Combo.Scale",
            "Combo.DMS", "DMS.Login", "DMS.Open", "DMS.SaveAs", "DMS.CheckIn",
        ];
        let out: Vec<&str> = crate::command::ALL
            .iter()
            .filter(|c| c.kind != crate::command::Kind::Separator && c.kind != crate::command::Kind::Label)
            .map(|c| c.id)
            .filter(|id| !reachable.contains(*id) && !ELSEWHERE.contains(id))
            .collect();
        assert!(out.is_empty(), "out of reach: {out:?}");
    }
}
