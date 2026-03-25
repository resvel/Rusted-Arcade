use eframe::egui;

use crate::app::NativeArcadeUiApp;

impl NativeArcadeUiApp {
    pub(crate) fn sync_settings_navigation_indices(&mut self) {
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
        self.state.menu_nav.settings_app_parallel_profile_index = match self
            .state
            .manage
            .settings_n64_parallel_profile
            .as_str()
        {
            "performance" => 1,
            _ => 0,
        };
        self.state.menu_nav.settings_app_synchronous_index = match self
            .state
            .manage
            .settings_n64_parallel_rdp_synchronous
            .as_str()
        {
            "true" => 1,
            _ => 0,
        };
        self.state.menu_nav.settings_app_ss_read_back_index = match self
            .state
            .manage
            .settings_n64_parallel_rdp_super_sampled_read_back
            .as_str()
        {
            "true" => 1,
            _ => 0,
        };
        self.state.menu_nav.settings_app_vi_aa_index = match self
            .state
            .manage
            .settings_n64_parallel_rdp_vi_aa
            .as_str()
        {
            "enabled" => 1,
            _ => 0,
        };
        self.state.menu_nav.settings_app_vi_bilinear_index = match self
            .state
            .manage
            .settings_n64_parallel_rdp_vi_bilinear
            .as_str()
        {
            "enabled" => 1,
            _ => 0,
        };
        self.state.menu_nav.settings_app_dither_filter_index = match self
            .state
            .manage
            .settings_n64_parallel_rdp_dither_filter
            .as_str()
        {
            "enabled" => 1,
            _ => 0,
        };
        self.state.menu_nav.settings_app_divot_filter_index = match self
            .state
            .manage
            .settings_n64_parallel_rdp_divot_filter
            .as_str()
        {
            "enabled" => 1,
            _ => 0,
        };
        self.state.menu_nav.settings_app_gamma_dither_index = match self
            .state
            .manage
            .settings_n64_parallel_rdp_gamma_dither
            .as_str()
        {
            "enabled" => 1,
            _ => 0,
        };
        self.state.menu_nav.settings_app_count_per_op_index = match self
            .state
            .manage
            .settings_n64_count_per_op
            .as_str()
        {
            "1" => 1,
            "2" => 2,
            "3" => 3,
            _ => 0,
        };
        self.state.menu_nav.settings_app_fb_emulation_index = match self
            .state
            .manage
            .settings_n64_fb_emulation
            .as_str()
        {
            "False" => 1,
            _ => 0,
        };
        self.state.menu_nav.settings_app_copy_color_to_rdram_index = match self
            .state
            .manage
            .settings_n64_copy_color_to_rdram
            .as_str()
        {
            "Async" => 1,
            "Sync" => 2,
            _ => 0,
        };
        self.state.menu_nav.settings_app_frame_duplication_index = match self
            .state
            .manage
            .settings_n64_frame_duplication
            .as_str()
        {
            "True" => 1,
            _ => 0,
        };
        self.state.menu_nav.settings_app_framerate_index = match self
            .state
            .manage
            .settings_n64_framerate
            .as_str()
        {
            "Fullspeed" => 1,
            _ => 0,
        };
        self.state.menu_nav.settings_app_vi_refresh_index = match self
            .state
            .manage
            .settings_n64_vi_refresh
            .as_str()
        {
            "1500" => 1,
            "2200" => 2,
            _ => 0,
        };
        self.state.menu_nav.settings_app_count_per_op_denom_pot_index = match self
            .state
            .manage
            .settings_n64_count_per_op_denom_pot
            .as_str()
        {
            "1" => 1,
            "2" => 2,
            "3" => 3,
            "4" => 4,
            _ => 0,
        };
        self.state.menu_nav.settings_app_aspect_ratio_index = match self
            .state
            .manage
            .settings_n64_aspect_ratio
            .as_str()
        {
            "16:9" => 1,
            "16:9 adjusted" => 2,
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
