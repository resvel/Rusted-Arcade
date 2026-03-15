use eframe::egui;

use crate::app::{GridSource, NativeArcadeUiApp};

impl NativeArcadeUiApp {
    pub(crate) fn draw_home(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let palette = self.palette();

        let (system_changed, alpha_changed, apply_filters) = self.draw_library_filters_panel(
            ctx,
            ui,
            self.state.library.favorite_ids_filtered.len(),
            "favorites loaded",
        );
        self.apply_current_view_filter_change(system_changed, alpha_changed, apply_filters);

        ui.add_space(4.0);

        if self.state.library.favorite_ids_filtered.is_empty() {
            self.panel_frame().show(ui, |ui| {
                ui.label(
                    egui::RichText::new(
                        "No favorites yet. Mark titles in Library to build your shelf.",
                    )
                    .color(palette.text_muted),
                );
            });
            return;
        }

        self.draw_rom_grid(ctx, ui, GridSource::Favorites, "favorites-grid");
    }
}
