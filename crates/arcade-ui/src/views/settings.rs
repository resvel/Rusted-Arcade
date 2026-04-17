use eframe::egui;

use crate::app::NativeArcadeUiApp;
use crate::state::{
    ControllerInputButtonDebug, ControllerInputDebugSnapshot, SettingsScrollTarget,
};
use crate::theme::SYSTEM_FILTERS;

impl NativeArcadeUiApp {
    pub(crate) fn sync_settings_navigation_indices(&mut self) {
        // Navigation indices are now computed dynamically from the core
        // registry and the generic settings_core_* fields on MenuNavState.
        // Nothing to sync.
        self.state.menu_nav.settings_cover_action_index =
            self.state.menu_nav.settings_cover_action_index.min(1);
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
                                    let button =
                                        egui::Button::new(egui::RichText::new(label).size(11.0))
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
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Input Settings")
                            .size(15.0)
                            .strong()
                            .color(palette.text),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let toggle_label = if self.state.controller_input_debug.open {
                            "Back To Mapping"
                        } else {
                            "Debug"
                        };
                        let button =
                            egui::Button::new(egui::RichText::new(toggle_label).size(11.0))
                                .min_size(egui::vec2(0.0, 24.0))
                                .fill(palette.panel_alt)
                                .stroke(egui::Stroke::new(1.0, palette.border))
                                .corner_radius(egui::CornerRadius::same(255));
                        if ui.add(button).clicked() {
                            self.state.controller_input_debug.open =
                                !self.state.controller_input_debug.open;
                        }
                    });
                });
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(if self.state.controller_input_debug.open {
                        "Live debug view for the connected controller. No env vars required."
                    } else {
                        "Configure controller button mappings and shortcuts per system."
                    })
                    .size(11.5)
                    .color(palette.text_muted),
                );
                ui.add_space(6.0);

                if self.state.controller_input_debug.open {
                    self.draw_controller_input_debug_view(ui);
                    return;
                }

                // System selector pills (skip "ALL")
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 5.0);
                    ui.label(
                        egui::RichText::new("System")
                            .size(11.5)
                            .color(palette.text_muted),
                    );
                    for system in SYSTEM_FILTERS.iter().copied().filter(|s| *s != "ALL") {
                        let selected = self.state.controller_mapping.input_system == system;
                        let pill_palette = Self::palette_for_system(system);
                        let button =
                            egui::Button::new(egui::RichText::new(system).size(11.0).strong())
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
                            self.state.controller_mapping.input_system = system.to_string();
                            self.state.controller_mapping.loaded_key.clear();
                        }
                    }
                });

                ui.add_space(6.0);
                self.draw_system_controller_panel(ctx, ui);
            });
    }

    fn draw_controller_input_debug_view(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let Some(snapshot) = self.state.controller_input_debug.snapshots.first() else {
            ui.label(
                egui::RichText::new(
                    "No playable controller is connected. Connect one and press buttons to see live input.",
                )
                .size(11.5)
                .color(palette.text_muted),
            );
            return;
        };

        let snapshot = snapshot.clone();
        if self.state.controller_input_debug.snapshots.len() > 1 {
            ui.label(
                egui::RichText::new(format!(
                    "Multiple controllers connected ({}). Showing port {}.",
                    self.state.controller_input_debug.snapshots.len(),
                    snapshot.port + 1
                ))
                .size(11.0)
                .color(palette.text_muted),
            );
            ui.add_space(4.0);
        }

        egui::Frame::new()
            .fill(palette.panel_alt)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                self.draw_controller_debug_header(ui, &snapshot);
                ui.add_space(6.0);

                egui::Grid::new("controller-input-debug-buttons")
                    .num_columns(2)
                    .spacing(egui::vec2(14.0, 6.0))
                    .show(ui, |ui| {
                        self.draw_input_flag(ui, "Square (West)", snapshot.west);
                        self.draw_input_flag(ui, "Triangle (North)", snapshot.north);
                        ui.end_row();

                        self.draw_input_flag(ui, "Cross/X (South)", snapshot.south);
                        self.draw_input_flag(ui, "Circle/O (East)", snapshot.east);
                        ui.end_row();

                        self.draw_input_flag(ui, "D-Pad Up", snapshot.dpad_up);
                        self.draw_input_flag(ui, "D-Pad Down", snapshot.dpad_down);
                        ui.end_row();

                        self.draw_input_flag(ui, "D-Pad Left", snapshot.dpad_left);
                        self.draw_input_flag(ui, "D-Pad Right", snapshot.dpad_right);
                        ui.end_row();

                        self.draw_input_flag(ui, "L1 / Left Shoulder", snapshot.left_shoulder);
                        self.draw_input_flag(ui, "R1 / Right Shoulder", snapshot.right_shoulder);
                        ui.end_row();

                        self.draw_input_flag(ui, "L3 / Left Stick Click", snapshot.left_thumb);
                        self.draw_input_flag(
                            ui,
                            "R3 / Right Stick Click",
                            snapshot.right_thumb_debug.effective_is_pressed,
                        );
                        ui.end_row();

                        self.draw_input_flag(
                            ui,
                            "PS / Guide",
                            snapshot.guide_debug.effective_is_pressed,
                        );
                        self.draw_input_flag(ui, "Start", snapshot.start);
                        ui.end_row();

                        self.draw_input_flag(ui, "Select / Share", snapshot.select);
                        ui.label("");
                        ui.end_row();
                    });

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Low-level suspect diagnostics")
                        .size(11.0)
                        .strong()
                        .color(palette.text),
                );
                self.draw_low_level_button_debug_line(ui, "DPad Up", &snapshot.dpad_up_debug);
                self.draw_low_level_button_debug_line(ui, "DPad Down", &snapshot.dpad_down_debug);
                self.draw_low_level_button_debug_line(ui, "PS / Guide", &snapshot.guide_debug);
                self.draw_low_level_button_debug_line(ui, "R3", &snapshot.right_thumb_debug);

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!(
                        "L2 / Left Trigger: {:.2}{}",
                        snapshot.left_trigger,
                        if snapshot.left_trigger >= 0.1 {
                            " (active)"
                        } else {
                            ""
                        }
                    ))
                    .size(11.0)
                    .color(palette.text),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "R2 / Right Trigger: {:.2}{}",
                        snapshot.right_trigger,
                        if snapshot.right_trigger >= 0.1 {
                            " (active)"
                        } else {
                            ""
                        }
                    ))
                    .size(11.0)
                    .color(palette.text),
                );

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!(
                        "D-Pad Axis Raw: X {:.2}  Y {:.2}",
                        snapshot.raw_dpad_x, snapshot.raw_dpad_y
                    ))
                    .size(11.0)
                    .color(palette.text_muted),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "Left Stick: raw ({:.2}, {:.2}) mapped ({:.2}, {:.2})",
                        snapshot.raw_left_x,
                        snapshot.raw_left_y,
                        snapshot.mapped_left_x,
                        snapshot.mapped_left_y
                    ))
                    .size(11.0)
                    .color(palette.text_muted),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "Right Stick: raw ({:.2}, {:.2}) mapped ({:.2}, {:.2})",
                        snapshot.raw_right_x,
                        snapshot.raw_right_y,
                        snapshot.mapped_right_x,
                        snapshot.mapped_right_y
                    ))
                    .size(11.0)
                    .color(palette.text_muted),
                );
            });
    }

    fn draw_low_level_button_debug_line(
        &self,
        ui: &mut egui::Ui,
        label: &str,
        debug: &ControllerInputButtonDebug,
    ) {
        let palette = self.palette();
        let code = debug
            .code
            .map(|value| format!("0x{value:08x}"))
            .unwrap_or_else(|| String::from("n/a"));
        let data_pressed = debug
            .data_pressed
            .map(|value| if value { "1" } else { "0" })
            .unwrap_or("-");
        let data_value = debug
            .data_value
            .map(|value| format!("{value:.3}"))
            .unwrap_or_else(|| String::from("-"));
        ui.label(
            egui::RichText::new(format!(
                "{label}: code {code} | gilrs_is_pressed {} | raw_logical {} | effective_logical {} | data_pressed {data_pressed} | data_value {data_value}",
                if debug.gilrs_is_pressed { "1" } else { "0" },
                if debug.is_pressed { "1" } else { "0" },
                if debug.effective_is_pressed { "1" } else { "0" }
            ))
            .size(10.5)
            .color(palette.text_muted),
        );
    }

    fn draw_controller_debug_header(
        &self,
        ui: &mut egui::Ui,
        snapshot: &ControllerInputDebugSnapshot,
    ) {
        let palette = self.palette();
        ui.label(
            egui::RichText::new(format!("Controller (Port {})", snapshot.port + 1))
                .size(12.5)
                .strong()
                .color(palette.text),
        );
        ui.label(
            egui::RichText::new(snapshot.name.clone())
                .size(11.0)
                .color(palette.text_muted),
        );

        let mut meta = Vec::new();
        if let Some(vendor_id) = snapshot.vendor_id.as_deref() {
            meta.push(format!("VID {vendor_id}"));
        }
        if let Some(product_id) = snapshot.product_id.as_deref() {
            meta.push(format!("PID {product_id}"));
        }
        if let Some(mapping_name) = snapshot.mapping_name.as_deref() {
            meta.push(format!("map {mapping_name}"));
        }
        if !meta.is_empty() {
            ui.label(
                egui::RichText::new(meta.join("  |  "))
                    .size(10.8)
                    .color(palette.text_muted),
            );
        }
        if let (Some(system), Some(mapping_key), Some(source)) = (
            snapshot.runtime_system.as_deref(),
            snapshot.runtime_mapping_key.as_deref(),
            snapshot.runtime_mapping_source.as_deref(),
        ) {
            ui.label(
                egui::RichText::new(format!(
                    "Runtime Profile: {system} | key {mapping_key} | source {source}"
                ))
                .size(10.8)
                .color(palette.text_muted),
            );
        }
    }

    fn draw_input_flag(&self, ui: &mut egui::Ui, label: &str, active: bool) {
        let palette = self.palette();
        let (fill, stroke, text) = if active {
            (palette.accent_soft, palette.accent, palette.text)
        } else {
            (palette.panel, palette.border, palette.text_muted)
        };
        egui::Frame::new()
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, stroke))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(format!("{label}: {}", if active { "ON" } else { "off" }))
                        .size(10.8)
                        .color(text),
                );
            });
    }
}
