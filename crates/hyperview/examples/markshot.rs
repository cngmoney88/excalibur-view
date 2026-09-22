//! Draws the mark at several sizes so it can be looked at before it ships.
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([760.0, 340.0]),
        ..Default::default()
    };
    eframe::run_native(
        "mark",
        options,
        Box::new(|cc| {
            ui::Theme::dark().apply(&cc.egui_ctx);
            Ok(Box::new(Shot { light: false }))
        }),
    )
}

struct Shot {
    light: bool,
}

impl eframe::App for Shot {
    fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) {
        let theme = if self.light {
            ui::Theme::light()
        } else {
            ui::Theme::dark()
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme.chrome))
            .show(ctx, |ui| {
                let painter = ui.painter();
                let mut x = 24.0;
                for size in [16.0f32, 24.0, 32.0, 48.0, 96.0, 160.0] {
                    ui::mark::draw(
                        painter,
                        egui::Rect::from_min_size(
                            egui::pos2(x, 30.0 + (160.0 - size) * 0.5),
                            egui::vec2(size, size),
                        ),
                        theme,
                    );
                    x += size + 22.0;
                }
                ui::mark::wordmark(painter, egui::pos2(24.0, 220.0), 84.0, theme);
                // And the same thing on the light palette, side by side.
                let light = ui::Theme::light();
                let plate = egui::Rect::from_min_size(
                    egui::pos2(380.0, 200.0),
                    egui::vec2(360.0, 120.0),
                );
                painter.rect_filled(plate, 6.0, light.chrome);
                ui::mark::wordmark(painter, egui::pos2(400.0, 218.0), 84.0, light);
            });
    }
}
