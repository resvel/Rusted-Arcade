use eframe::egui;

use crate::app::{GridSource, NativeArcadeUiApp};

impl NativeArcadeUiApp {
    pub(crate) fn filters_panel_expanded(&self) -> bool {
        !self.state.library.filters_panel_collapsed
    }

    pub(crate) fn normalize_filters_panel_focus(&mut self) {
        self.state.menu_nav.normalize_filter_focus(
            self.state.current_view,
            self.filters_panel_expanded(),
            self.current_browse_grid_source().is_some(),
        );
    }

    pub(crate) fn draw_library_filters_panel(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        _loaded_count: usize,
        _loaded_label: &str,
        show_manage: bool,
    ) -> (bool, bool, bool) {
        let palette = self.palette();
        let mut system_changed = false;
        let mut alpha_changed = false;
        let mut apply_filters = false;

        let section_width = ui.available_width();
        let toolbar_width = Self::content_band_width_for(section_width);
        let side_gutter = ((section_width - toolbar_width) * 0.5).max(0.0);

        ui.horizontal(|ui| {
            if side_gutter > 0.0 {
                ui.add_space(side_gutter);
            }

            ui.allocate_ui_with_layout(
                egui::vec2(toolbar_width, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    system_changed = self.draw_system_toolbar(ctx, ui);
                    ui.add_space(1.0);
                    alpha_changed = self.draw_alpha_toolbar(ui, show_manage);
                    ui.add_space(1.0);

                    // Estimate search row width to center it
                    let search_width =
                        (toolbar_width * 0.5).clamp(176.0, 308.0);
                    let search_row_w = 40.0 + search_width + 4.0 + 80.0; // label + input + gap + button
                    let pad = ((toolbar_width - search_row_w) * 0.5).max(0.0);
                    ui.horizontal(|ui| {
                        ui.add_space(pad);
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 3.0);
                        ui.label(
                            egui::RichText::new("Search")
                                .size(11.0)
                                .color(palette.text_muted),
                        );
                        let search_response = ui.add_sized(
                            [search_width, 18.0],
                            egui::TextEdit::singleline(&mut self.state.library.search)
                                .hint_text("Title, manufacturer, genre"),
                        );
                        let submit = search_response.lost_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let button = egui::Button::new("Apply Filters")
                            .fill(palette.accent_soft)
                            .stroke(egui::Stroke::new(1.0, palette.accent))
                            .corner_radius(egui::CornerRadius::same(255));
                        if ui.add(button).clicked() || submit {
                            apply_filters = true;
                        }
                    });
                },
            );

            if side_gutter > 0.0 {
                ui.add_space(side_gutter);
            }
        });

        (system_changed, alpha_changed, apply_filters)
    }

    pub(crate) fn draw_library(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {

        if self.state.manage.open {
            self.draw_manage_library(ctx, ui);
            return;
        }

        let show_manage = !self.state.manage.open;
        let (system_changed, alpha_changed, apply_filters) = self.draw_library_filters_panel(
            ctx,
            ui,
            self.state.library.visible_rom_ids.len(),
            "titles loaded",
            show_manage,
        );

        let desired_page_size =
            self.desired_library_page_size(ui.available_width(), ui.available_height());
        let previous_page_size = self.state.library.page_size;
        if self.state.library.set_page_size(desired_page_size)
            && desired_page_size > previous_page_size
        {
            self.state.library.loaded = false;
        }

        if !self.state.library.loaded {
            self.try_refresh_roms();
            self.apply_favorites_filters();
            self.repair_grid_selection(GridSource::Library);
        }
        self.apply_current_view_filter_change(system_changed, alpha_changed, apply_filters);

        ui.add_space(4.0);
        if self.state.library.visible_rom_ids.is_empty() {
            self.panel_frame().show(ui, |ui| {
                ui.label(
                    egui::RichText::new("No ROMs match current filters.").color(self.palette().text_muted),
                );
            });
            return;
        }

        self.draw_rom_grid(ctx, ui, GridSource::Library, "library-grid");
    }
}
