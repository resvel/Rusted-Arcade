use eframe::egui;

use super::NativeArcadeUiApp;

impl NativeArcadeUiApp {
    pub(super) fn draw_status_bar(&mut self, ctx: &egui::Context) {
        let width = Self::viewport_width(ctx);
        let chrome_margin = if width >= 1300.0 { 6 } else { 5 };
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(self.palette().panel_alt)
                    .inner_margin(egui::Margin::same(chrome_margin)),
            )
            .show(ctx, |ui| {
                let panel_width = ui.available_width();
                let content_width = Self::content_band_width_for(panel_width);
                let side_gutter = ((panel_width - content_width) * 0.5).max(0.0);

                ui.horizontal(|ui| {
                    if side_gutter > 0.0 {
                        ui.add_space(side_gutter);
                    }

                    ui.allocate_ui_with_layout(
                        egui::vec2(content_width, 0.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.horizontal_wrapped(|ui| {
                                if !self.state.input_debug.is_empty() {
                                    ui.label(
                                        egui::RichText::new(self.state.input_debug.clone())
                                            .color(self.palette().accent),
                                    );
                                }
                                if !self.state.status.is_empty() {
                                    ui.label(
                                        egui::RichText::new(self.state.status.clone())
                                            .color(self.palette().text_muted),
                                    );
                                }
                                if !self.state.play.status.is_empty() {
                                    ui.label(
                                        egui::RichText::new(self.state.play.status.clone())
                                            .color(self.palette().text),
                                    );
                                }
                            });
                        },
                    );

                    if side_gutter > 0.0 {
                        ui.add_space(side_gutter);
                    }
                });
            });
    }
}
