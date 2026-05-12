use arcade_domain::MAX_GAMEPAD_PLAYERS;
use eframe::egui;

use crate::app::NativeArcadeUiApp;
use crate::state::{
    ControllerAssignmentSource, ControllerInputButtonDebug, ControllerInputDebugSnapshot,
    MenuFocusRegion, SettingsScrollTarget,
};

use super::manage::runtime_system_display_name;

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
                                let active_section = self
                                    .state
                                    .settings_scroll_target
                                    .unwrap_or(SettingsScrollTarget::AppConfig);
                                for (index, target) in
                                    SettingsScrollTarget::ALL.iter().copied().enumerate()
                                {
                                    let selected = active_section == target;
                                    let focused = self.state.menu_nav.focus_region
                                        == MenuFocusRegion::SettingsSectionNav
                                        && self.state.menu_nav.settings_section_index == index;
                                    let button = egui::Button::new(
                                        egui::RichText::new(target.label()).size(11.0).strong(),
                                    )
                                    .min_size(egui::vec2(0.0, 24.0))
                                    .fill(if selected {
                                        palette.accent_soft
                                    } else {
                                        palette.panel_alt
                                    })
                                    .stroke(egui::Stroke::new(
                                        if selected { 1.4 } else { 1.0 },
                                        if selected {
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
                                            0.78,
                                        );
                                    }
                                    if response.clicked() {
                                        self.state.menu_nav.focus_region =
                                            MenuFocusRegion::SettingsSectionNav;
                                        self.state.menu_nav.settings_section_index = index;
                                        self.state.settings_scroll_target = Some(target);
                                    }
                                }
                            });
                            ui.add_space(8.0);
                            match self
                                .state
                                .settings_scroll_target
                                .unwrap_or(SettingsScrollTarget::AppConfig)
                            {
                                SettingsScrollTarget::Dependencies => {
                                    self.draw_dependency_installer_panel(ui, palette);
                                }
                                SettingsScrollTarget::AppConfig => {
                                    self.draw_manage_app_config_panel(ui, palette);
                                }
                                SettingsScrollTarget::TheGamesDbConfig => {
                                    self.draw_manage_settings_panel(ui, palette);
                                }
                                SettingsScrollTarget::InputSettings => {
                                    self.draw_input_settings_section(ctx, ui);
                                }
                            }
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
        let selected_system = self.state.manage.settings_selected_system.clone();
        let palette = Self::palette_for_system(&selected_system);

        egui::Frame::new()
            .fill(palette.panel)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                let title = if selected_system == "ALL" {
                    String::from("Input Settings")
                } else {
                    format!(
                        "{} Controller Mapper",
                        runtime_system_display_name(&selected_system)
                    )
                };
                let subtitle = if selected_system == "ALL" {
                    String::from("Connected controller status and per-system mapping entry point.")
                } else {
                    String::from("Button mappings and shortcuts for the selected system.")
                };
                self.draw_settings_system_chrome(ui, palette, &title, &subtitle, |_| "");

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(if selected_system == "ALL" {
                            "Controller Status"
                        } else {
                            "Controller Mapping"
                        })
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
                        "Live debug view for connected controllers. No env vars required."
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

                if selected_system == "ALL" {
                    ui.label(
                        egui::RichText::new("Pick a system above to edit its controller mapping.")
                            .small()
                            .color(palette.text_muted),
                    );
                    return;
                }

                if self.state.controller_mapping.input_system != selected_system {
                    self.state
                        .controller_mapping
                        .set_input_system(selected_system.clone());
                }
                ui.add_space(6.0);
                self.draw_system_controller_panel(ctx, ui);
            });
    }

    fn draw_controller_input_debug_view(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let mut snapshots = self.state.controller_input_debug.snapshots.clone();
        if snapshots.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No controller is connected. Connect one and press buttons to see live input.",
                )
                .size(11.5)
                .color(palette.text_muted),
            );
            return;
        }

        snapshots.sort_by(|left, right| {
            let left_rank = match left.assignment_source {
                ControllerAssignmentSource::Assigned => 0,
                ControllerAssignmentSource::UnassignedOverLimit => 1,
                ControllerAssignmentSource::Unsupported => 2,
            };
            let right_rank = match right.assignment_source {
                ControllerAssignmentSource::Assigned => 0,
                ControllerAssignmentSource::UnassignedOverLimit => 1,
                ControllerAssignmentSource::Unsupported => 2,
            };
            left_rank
                .cmp(&right_rank)
                .then(
                    left.player_slot
                        .unwrap_or(u8::MAX)
                        .cmp(&right.player_slot.unwrap_or(u8::MAX)),
                )
                .then(left.connect_seq.cmp(&right.connect_seq))
                .then(left.name.cmp(&right.name))
        });

        ui.label(
            egui::RichText::new(format!(
                "Connected {} | Players {}/{} | Unassigned {}",
                self.state.controller_input_debug.connected_total,
                self.state.controller_input_debug.assigned_playable_total,
                MAX_GAMEPAD_PLAYERS,
                self.state.controller_input_debug.unassigned_total
            ))
            .size(11.0)
            .color(palette.text_muted),
        );
        ui.add_space(6.0);

        let mut player_slots = vec![None; MAX_GAMEPAD_PLAYERS as usize];
        let mut other_snapshots = Vec::new();
        for snapshot in snapshots {
            if let Some(slot) = snapshot.player_slot {
                let slot_index = slot as usize;
                if slot_index < player_slots.len() && player_slots[slot_index].is_none() {
                    player_slots[slot_index] = Some(snapshot);
                    continue;
                }
            }
            other_snapshots.push(snapshot);
        }

        let column_spacing = 10.0;
        let row_spacing = 10.0;
        let quadrant_width = ((ui.available_width() - column_spacing).max(0.0)) * 0.5;
        let rows = (MAX_GAMEPAD_PLAYERS as usize + 1) / 2;
        for row in 0..rows {
            ui.horizontal_top(|ui| {
                for col in 0..2 {
                    let slot = row * 2 + col;
                    if slot >= MAX_GAMEPAD_PLAYERS as usize {
                        continue;
                    }
                    ui.allocate_ui_with_layout(
                        egui::vec2(quadrant_width, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            if let Some(snapshot) = player_slots[slot].as_ref() {
                                self.draw_controller_debug_card(ui, snapshot, slot);
                            } else {
                                self.draw_controller_debug_placeholder(ui, slot as u8);
                            }
                        },
                    );
                    if col == 0 {
                        ui.add_space(column_spacing);
                    }
                }
            });
            if row + 1 < rows {
                ui.add_space(row_spacing);
            }
        }

        if !other_snapshots.is_empty() {
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("Other Connected Controllers")
                    .size(11.5)
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(6.0);
            for (index, snapshot) in other_snapshots.iter().enumerate() {
                self.draw_controller_debug_card(ui, snapshot, 100 + index);
                if index + 1 < other_snapshots.len() {
                    ui.add_space(8.0);
                }
            }
        }
    }

    fn draw_controller_debug_card(
        &self,
        ui: &mut egui::Ui,
        snapshot: &ControllerInputDebugSnapshot,
        card_index: usize,
    ) {
        let palette = self.palette();
        egui::Frame::new()
            .fill(palette.panel_alt)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                self.draw_controller_debug_header(ui, snapshot);
                ui.add_space(6.0);

                egui::Grid::new(format!(
                    "controller-input-debug-buttons-{card_index}-{}",
                    snapshot.connect_seq
                ))
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

    fn draw_controller_debug_placeholder(&self, ui: &mut egui::Ui, slot: u8) {
        let palette = self.palette();
        egui::Frame::new()
            .fill(palette.panel_alt)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(format!("Controller (Player {})", slot + 1))
                        .size(12.5)
                        .strong()
                        .color(palette.text),
                );
                ui.label(
                    egui::RichText::new("Waiting for controller...")
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
        let assignment_label = if let Some(slot) = snapshot.player_slot {
            format!("Player {}", slot + 1)
        } else {
            snapshot.assignment_source.label().to_string()
        };
        ui.label(
            egui::RichText::new(format!("Controller ({assignment_label})"))
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
        if !snapshot.is_playable {
            meta.push(String::from("not playable"));
        }
        if snapshot.connect_seq != u64::MAX {
            meta.push(format!("order {}", snapshot.connect_seq + 1));
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
