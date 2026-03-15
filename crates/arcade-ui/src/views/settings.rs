use eframe::egui;

use crate::app::NativeArcadeUiApp;

impl NativeArcadeUiApp {
    pub(crate) fn sync_settings_navigation_indices(&mut self) {
        self.state.menu_nav.settings_app_core_index =
            if self.state.manage.settings_n64_preferred_core == "parallel_n64" {
                1
            } else {
                0
            };
        self.state.menu_nav.settings_app_upscaling_index = match self
            .state
            .manage
            .settings_n64_parallel_rdp_upscaling
            .as_str()
        {
            "2x" => 1,
            "4x" => 2,
            "8x" => 3,
            _ => 0,
        };
        self.state.menu_nav.settings_cover_action_index =
            self.state.menu_nav.settings_cover_action_index.min(1);
    }

    pub(crate) fn draw_settings(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        let palette = self.palette();
        let section_width = ui.available_width();
        let content_width = Self::content_band_width_for(section_width).min(section_width);
        let side_gutter = ((section_width - content_width) * 0.5).max(0.0);

        egui::ScrollArea::vertical()
            .id_salt("settings-view-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if side_gutter > 0.0 {
                        ui.add_space(side_gutter);
                    }

                    ui.allocate_ui_with_layout(
                        egui::vec2(content_width, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.label(
                                egui::RichText::new("Settings")
                                    .heading()
                                    .strong()
                                    .color(palette.text),
                            );
                            ui.add_space(8.0);
                            self.draw_manage_app_config_panel(ui, palette);
                            ui.add_space(10.0);
                            self.draw_manage_settings_panel(ui, palette);
                            ui.add_space(12.0);
                        },
                    );

                    if side_gutter > 0.0 {
                        ui.add_space(side_gutter);
                    }
                });
            });
    }
}
