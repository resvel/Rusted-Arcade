use eframe::egui;

use crate::app::NativeArcadeUiApp;
use crate::state::SettingsScrollTarget;
use crate::theme::SYSTEM_FILTERS;

impl NativeArcadeUiApp {
    pub(crate) fn sync_settings_navigation_indices(&mut self) {
        // Navigation indices are now computed dynamically from the core
        // registry and the generic settings_core_* fields on MenuNavState.
        // Nothing to sync.
        self.state
            .menu_nav
            .settings_cover_action_index = self.state.menu_nav.settings_cover_action_index.min(1);
    }

    pub(crate) fn draw_settings(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
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
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Settings")
                                        .heading()
                                        .strong()
                                        .color(palette.text),
                                );
                                ui.add_space(8.0);
                                ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
                                for (label, target) in [
                                    ("Input Settings", SettingsScrollTarget::InputSettings),
                                    ("TheGamesDB Config", SettingsScrollTarget::TheGamesDbConfig),
                                ] {
                                    let button = egui::Button::new(
                                        egui::RichText::new(label).size(11.0),
                                    )
                                    .min_size(egui::vec2(0.0, 24.0))
                                    .fill(palette.panel_alt)
                                    .stroke(egui::Stroke::new(1.0, palette.border))
                                    .corner_radius(egui::CornerRadius::same(255));
                                    if ui.add(button).clicked() {
                                        self.state.settings_scroll_target = Some(target);
                                    }
                                }
                            });
                            ui.add_space(8.0);
                            self.draw_manage_app_config_panel(ui, palette);
                            ui.add_space(10.0);
                            if self.state.settings_scroll_target
                                == Some(SettingsScrollTarget::TheGamesDbConfig)
                            {
                                self.state.settings_scroll_target = None;
                                ui.scroll_to_cursor(Some(egui::Align::TOP));
                            }
                            self.draw_manage_settings_panel(ui, palette);
                            ui.add_space(10.0);
                            if self.state.settings_scroll_target
                                == Some(SettingsScrollTarget::InputSettings)
                            {
                                self.state.settings_scroll_target = None;
                                ui.scroll_to_cursor(Some(egui::Align::TOP));
                            }
                            self.draw_input_settings_section(ctx, ui);
                            ui.add_space(12.0);
                        },
                    );

                    if side_gutter > 0.0 {
                        ui.add_space(side_gutter);
                    }
                });
            });
    }

    fn draw_input_settings_section(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let palette = self.palette();

        egui::Frame::new()
            .fill(palette.panel)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Input Settings")
                        .size(15.0)
                        .strong()
                        .color(palette.text),
                );
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(
                        "Configure controller button mappings and shortcuts per system.",
                    )
                    .size(11.5)
                    .color(palette.text_muted),
                );
                ui.add_space(6.0);

                // System selector pills (skip "ALL")
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 5.0);
                    ui.label(
                        egui::RichText::new("System")
                            .size(11.5)
                            .color(palette.text_muted),
                    );
                    for system in SYSTEM_FILTERS.iter().copied().filter(|s| *s != "ALL") {
                        let selected =
                            self.state.controller_mapping.input_system == system;
                        let pill_palette = Self::palette_for_system(system);
                        let button = egui::Button::new(
                            egui::RichText::new(system).size(11.0).strong(),
                        )
                        .min_size(egui::vec2(0.0, 22.0))
                        .fill(if selected {
                            pill_palette.accent_soft
                        } else {
                            palette.panel_alt
                        })
                        .stroke(egui::Stroke::new(
                            if selected { 1.4 } else { 1.0 },
                            if selected {
                                pill_palette.accent
                            } else {
                                palette.border
                            },
                        ))
                        .corner_radius(egui::CornerRadius::same(255));
                        if ui.add(button).clicked() {
                            self.state.controller_mapping.input_system =
                                system.to_string();
                            self.state.controller_mapping.loaded_key.clear();
                        }
                    }
                });

                ui.add_space(6.0);
                self.draw_system_controller_panel(ctx, ui);
            });
    }
}
