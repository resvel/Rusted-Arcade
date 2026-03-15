use eframe::egui;

use crate::app::{AppView, GridSource, NativeArcadeUiApp};
use crate::state::MenuFocusRegion;

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

        if self.state.menu_nav.focus_region == MenuFocusRegion::ControllerMappingToggle
            && !self.controller_mapping_panel_visible()
        {
            self.state.menu_nav.focus_region = if self.filters_panel_expanded() {
                MenuFocusRegion::FiltersAlpha
            } else {
                MenuFocusRegion::FiltersToggle
            };
        }
    }

    pub(crate) fn controller_mapping_panel_visible(&self) -> bool {
        self.state.current_view == AppView::Library && self.active_system() != "ALL"
    }

    fn filters_panel_summary(&self) -> String {
        let mut parts = Vec::new();

        if self.state.library.system_filter != "ALL" {
            parts.push(self.state.library.system_filter.clone());
        }
        if self.state.library.alpha_filter != "ALL" {
            parts.push(format!("Starts with {}", self.state.library.alpha_filter));
        }
        let search = self.state.library.search.trim();
        if !search.is_empty() {
            parts.push(format!("Search: {search}"));
        }

        if parts.is_empty() {
            String::from("All systems, all titles")
        } else {
            parts.join(" • ")
        }
    }

    pub(crate) fn draw_library_filters_panel(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        _loaded_count: usize,
        _loaded_label: &str,
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
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let toggle_label = if self.state.library.filters_panel_collapsed {
                            "Expand"
                        } else {
                            "Collapse"
                        };
                        let focused =
                            self.state.menu_nav.focus_region == MenuFocusRegion::FiltersToggle;
                        let focus_t = ui
                            .ctx()
                            .animate_bool(ui.id().with("filters-panel-toggle-focus"), focused);
                        let button = egui::Button::new(toggle_label)
                            .fill(palette.accent_soft)
                            .stroke(egui::Stroke::new(
                                if focused { 1.6 } else { 1.0 },
                                palette.accent,
                            ))
                            .corner_radius(egui::CornerRadius::same(255));
                        let response = ui.add(button);
                        let glow_t = if focused { 0.58 + focus_t * 0.42 } else { 0.0 };
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, glow_t);
                        if response.clicked() {
                            self.state.menu_nav.focus_region = MenuFocusRegion::FiltersToggle;
                            self.state.library.filters_panel_collapsed =
                                !self.state.library.filters_panel_collapsed;
                            self.normalize_filters_panel_focus();
                        }
                    });

                    if self.state.library.filters_panel_collapsed {
                        ui.add_space(3.0);
                        ui.label(
                            egui::RichText::new(self.filters_panel_summary())
                                .small()
                                .color(palette.text_muted),
                        );
                        return;
                    }

                    ui.add_space(2.0);
                    system_changed = self.draw_system_toolbar(ctx, ui);
                    ui.add_space(2.0);
                    alpha_changed = self.draw_alpha_toolbar(ui);
                    ui.add_space(4.0);

                    let search_band_width = ui.available_width().min(620.0);
                    ui.horizontal(|ui| {
                        let side_pad = ((ui.available_width() - search_band_width) * 0.5).max(0.0);
                        if side_pad > 0.0 {
                            ui.add_space(side_pad);
                        }

                        ui.allocate_ui_with_layout(
                            egui::vec2(search_band_width, 0.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 5.0);
                                    ui.label(
                                        egui::RichText::new("Search")
                                            .small()
                                            .color(palette.text_muted),
                                    );
                                    let search_width =
                                        (ui.available_width() * 0.5).clamp(220.0, 360.0);
                                    let search_response = ui.add_sized(
                                        [search_width, 30.0],
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

                        if side_pad > 0.0 {
                            ui.add_space(side_pad);
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
        let palette = self.palette();
        ui.horizontal(|ui| {
            if !self.state.manage.open {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let focused =
                        self.state.menu_nav.focus_region == MenuFocusRegion::LibraryManageButton;
                    let focus_t = ui
                        .ctx()
                        .animate_bool(ui.id().with("library-manage-button-focus"), focused);
                    let button = egui::Button::new("Manage")
                        .fill(if focused {
                            palette.accent_soft
                        } else {
                            palette.panel
                        })
                        .stroke(egui::Stroke::new(
                            if focused { 1.6 } else { 1.0 },
                            if focused {
                                palette.accent
                            } else {
                                palette.border
                            },
                        ))
                        .corner_radius(egui::CornerRadius::same(255));
                    let response = ui.add(button);
                    if focused {
                        Self::paint_selection_glow(
                            ui,
                            response.rect,
                            255,
                            palette.accent,
                            0.58 + focus_t * 0.42,
                        );
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region = MenuFocusRegion::LibraryManageButton;
                        self.state.manage.open = true;
                        self.state.menu_nav.focus_region = MenuFocusRegion::ManageHeader;
                        self.refresh_manage_rows();
                        self.sync_manage_settings_from_services();
                    }
                });
            }
        });
        ui.add_space(6.0);

        if self.state.manage.open {
            self.draw_manage_library(ctx, ui);
            return;
        }

        let (system_changed, alpha_changed, apply_filters) = self.draw_library_filters_panel(
            ctx,
            ui,
            self.state.library.visible_rom_ids.len(),
            "titles loaded",
        );
        if self.active_system() != "ALL" {
            ui.add_space(6.0);
            let section_width = ui.available_width();
            let content_width = Self::content_band_width_for(section_width);
            let side_gutter = ((section_width - content_width) * 0.5).max(0.0);

            ui.horizontal(|ui| {
                if side_gutter > 0.0 {
                    ui.add_space(side_gutter);
                }

                ui.allocate_ui_with_layout(
                    egui::vec2(content_width, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.draw_system_controller_panel(ctx, ui);
                    },
                );

                if side_gutter > 0.0 {
                    ui.add_space(side_gutter);
                }
            });
        }

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

        ui.add_space(if self.active_system() == "ALL" {
            4.0
        } else {
            2.0
        });
        if self.state.library.visible_rom_ids.is_empty() {
            self.panel_frame().show(ui, |ui| {
                ui.label(
                    egui::RichText::new("No ROMs match current filters.").color(palette.text_muted),
                );
            });
            return;
        }

        self.draw_rom_grid(ctx, ui, GridSource::Library, "library-grid");
    }
}
