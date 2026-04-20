use crate::app::NativeArcadeUiApp;
use crate::input::ControllerMappingTarget;
use crate::render::fit_size;
use crate::state::MenuFocusRegion;
use crate::theme::{ThemePalette, SYSTEM_FILTERS};
use arcade_domain::{DetectedPadIdentity, N64PrimaryStick};
use eframe::egui;

const ALPHA_FILTERS: [&str; 28] = [
    "ALL", "0-9", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P",
    "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
];

impl NativeArcadeUiApp {
    pub(crate) fn alpha_filter_values() -> &'static [&'static str] {
        &ALPHA_FILTERS
    }

    pub(crate) fn current_system_filter_index(&self) -> usize {
        SYSTEM_FILTERS
            .iter()
            .position(|system| *system == self.state.library.system_filter)
            .unwrap_or(0)
    }

    pub(crate) fn current_alpha_filter_index(&self) -> usize {
        Self::alpha_filter_values()
            .iter()
            .position(|value| *value == self.state.library.alpha_filter)
            .unwrap_or(0)
    }

    pub(crate) fn apply_system_filter_by_index(&mut self, index: usize) -> bool {
        let Some(system) = SYSTEM_FILTERS.get(index).copied() else {
            return false;
        };
        if self.state.library.system_filter == system {
            return false;
        }
        self.state.library.system_filter = system.to_string();
        true
    }

    pub(crate) fn apply_alpha_filter_by_index(&mut self, index: usize) -> bool {
        let Some(value) = Self::alpha_filter_values().get(index).copied() else {
            return false;
        };
        if self.state.library.alpha_filter == value {
            return false;
        }
        self.state.library.alpha_filter = value.to_string();
        true
    }

    pub(crate) fn draw_system_toolbar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) -> bool {
        let mut changed = false;

        let row_width = ui.available_width();
        let mut draw_pills = |ui: &mut egui::Ui, spacing: egui::Vec2| {
            ui.spacing_mut().item_spacing = spacing;
            for (index, system) in SYSTEM_FILTERS.iter().copied().enumerate() {
                let pill_palette = Self::palette_for_system(system);
                let selected = self.state.library.system_filter == system;
                let focused = self.state.menu_nav.focus_region == MenuFocusRegion::FiltersSystem
                    && self.state.menu_nav.system_filter_index == index;
                let selected_t = ui
                    .ctx()
                    .animate_bool(ui.id().with(("system-pill", system)), selected);
                let focus_t = ui
                    .ctx()
                    .animate_bool(ui.id().with(("system-pill-focus", system)), focused);
                let frame = egui::Frame::new()
                    .shadow(egui::epaint::Shadow::NONE)
                    .fill(blend_color(
                        if selected {
                            blend_color(pill_palette.panel_alt, pill_palette.accent_soft, 0.5)
                        } else {
                            blend_color(pill_palette.panel, pill_palette.accent_soft, 0.18)
                        },
                        pill_palette.accent_soft,
                        if selected {
                            0.42 + selected_t * 0.48
                        } else {
                            0.02 + focus_t * 0.12
                        },
                    ))
                    .stroke(egui::Stroke::new(
                        if selected {
                            1.6 + selected_t * 0.35
                        } else if focused {
                            1.45
                        } else {
                            1.0
                        },
                        blend_color(
                            pill_palette.border,
                            pill_palette.accent,
                            if selected {
                                0.72 + selected_t * 0.28
                            } else {
                                0.04 + focus_t * 0.46
                            },
                        ),
                    ))
                    .corner_radius(egui::CornerRadius::same(255))
                    .inner_margin(egui::Margin::symmetric(6, 3));

                let rendered = frame.show(ui, |ui| {
                    if let Some(texture) = self.system_logo_texture(ctx, system) {
                        let max_size = Self::system_logo_size(system);
                        let draw_size = fit_size(texture.size_vec2(), max_size);
                        ui.allocate_ui_with_layout(
                            max_size,
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            |ui| {
                                ui.add(
                                    egui::Image::new((texture.id(), draw_size))
                                        .sense(egui::Sense::click()),
                                )
                            },
                        )
                        .inner
                        .on_hover_text(system)
                    } else {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(if system == "ALL" {
                                    String::from("ALL")
                                } else {
                                    system.to_string()
                                })
                                .size(11.0)
                                .strong(),
                            )
                            .sense(egui::Sense::click()),
                        )
                    }
                });
                let response = rendered.inner;
                let glow_t = if focused { 0.56 + focus_t * 0.44 } else { 0.0 };
                Self::paint_selection_glow(
                    ui,
                    rendered.response.rect,
                    255,
                    pill_palette.accent,
                    glow_t,
                );

                if response.clicked() {
                    self.state.menu_nav.focus_region = MenuFocusRegion::FiltersSystem;
                    self.state.menu_nav.system_filter_index = index;
                    if self.apply_system_filter_by_index(index) {
                        changed = true;
                    }
                }
            }
        };

        if use_wrapped_system_toolbar(row_width) {
            let spacing_x = 7.0;
            let pill_overhead = 14.0; // 2 * inner_margin(6) + ~2 stroke
            let content_w: f32 = SYSTEM_FILTERS
                .iter()
                .map(|s| Self::system_logo_size(s).x + pill_overhead)
                .sum::<f32>()
                + (SYSTEM_FILTERS.len() as f32 - 1.0) * spacing_x;
            let pad = ((row_width - content_w) * 0.5).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(pad);
                draw_pills(ui, egui::vec2(spacing_x, 3.0));
            });
        } else {
            egui::ScrollArea::horizontal()
                .max_height(24.0)
                .id_salt("systems-scroll")
                .show(ui, |ui| {
                    ui.set_min_width(row_width);
                    ui.horizontal_centered(|ui| {
                        draw_pills(ui, egui::vec2(8.0, 3.0));
                    });
                });
        }

        changed
    }

    pub(crate) fn draw_system_controller_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let active_system = self.state.controller_mapping.input_system.clone();
        let available_width = ui.available_width();
        let wide_layout = available_width >= 620.0;
        let max_image_height = if active_system == "ARCADE" {
            if wide_layout {
                120.0
            } else if available_width >= 360.0 {
                96.0
            } else {
                84.0
            }
        } else if wide_layout {
            76.0
        } else if available_width >= 360.0 {
            64.0
        } else {
            52.0
        };
        let palette = self.palette();
        let action_labels: &[&str] = if active_system == "ALL" {
            &[]
        } else {
            self.supported_mapping_actions(&active_system)
        };
        let action_count = action_labels.len();
        let preview_texture = self.system_controller_texture(ctx, &active_system);
        let (mapping_targets, selected_target, has_device, dirty) = if active_system == "ALL" {
            (Vec::new(), None, false, false)
        } else {
            let mapping_targets = self.connected_controller_mapping_targets();
            self.sync_controller_mapping_editor(&mapping_targets);
            let selected_target = self
                .state
                .controller_mapping
                .selected_device_key()
                .and_then(|selected| {
                    mapping_targets
                        .iter()
                        .find(|target| target.identity.device_key == selected)
                        .cloned()
                });
            let has_device = selected_target.is_some();
            let dirty = self.controller_mapping_is_dirty();
            (mapping_targets, selected_target, has_device, dirty)
        };
        let summary_line = if active_system == "ALL" {
            String::from("Choose a system to edit mappings.")
        } else if let Some(target) = selected_target.as_ref() {
            format!(
                "Editing {active_system} mapping for {}",
                target.identity.name
            )
        } else {
            format!("Connect a controller to edit {active_system} mappings.")
        };
        let min_panel_height = if !self.state.controller_mapping.expanded {
            84.0
        } else if active_system == "ALL" {
            190.0
        } else if wide_layout {
            302.0
        } else {
            342.0
        };
        ui.set_min_height(min_panel_height);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 3.0);
            ui.spacing_mut().button_padding = egui::vec2(8.0, 4.0);
            ui.horizontal(|ui| {
                let toggle_width = 104.0;
                let content_width = (ui.available_width() - toggle_width).max(180.0);

                ui.allocate_ui_with_layout(
                    egui::vec2(content_width, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.label(
                            egui::RichText::new("Controller Mapping")
                                .size(15.0)
                                .strong()
                                .color(palette.text),
                        );
                        ui.horizontal_wrapped(|ui| {
                            if action_count > 0 {
                                badge_chip(
                                    ui,
                                    &format!("{action_count} bindings"),
                                    palette.accent_soft,
                                    palette.accent,
                                    palette.text,
                                );
                            }
                            badge_chip(
                                ui,
                                "Device Mapping",
                                palette.panel,
                                palette.border,
                                palette.text_muted,
                            );
                            if dirty {
                                badge_chip(
                                    ui,
                                    "Unsaved changes",
                                    palette.accent_soft,
                                    palette.accent,
                                    palette.text,
                                );
                            }
                        });
                    },
                );

                ui.add_space(4.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(toggle_width, 0.0),
                    egui::Layout::right_to_left(egui::Align::Min),
                    |ui| {
                        let toggle_label = if self.state.controller_mapping.expanded {
                            "Collapse"
                        } else {
                            "Expand"
                        };
                        let toggle_button =
                            egui::Button::new(egui::RichText::new(toggle_label).size(11.5))
                                .fill(palette.panel)
                                .stroke(egui::Stroke::new(1.0, palette.border))
                                .corner_radius(egui::CornerRadius::same(255));
                        let response = ui.add(toggle_button);
                        if response.clicked() {
                            self.state.controller_mapping.toggle_expanded();
                        }
                    },
                );
            });
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(summary_line)
                    .size(11.5)
                    .color(palette.text_muted),
            );
            if system_uses_primary_stick_selector(&active_system) {
                let mut selected_stick = self.services.n64_primary_stick();
                let system_label = if active_system.trim().eq_ignore_ascii_case("DREAMCAST") {
                    "Dreamcast"
                } else {
                    "N64"
                };
                ui.add_space(2.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("Primary Analog Stick")
                            .size(11.5)
                            .color(palette.text_muted),
                    );
                    for (label, value) in [
                        ("Left Stick", N64PrimaryStick::Left),
                        ("Right Stick", N64PrimaryStick::Right),
                    ] {
                        if scope_chip(ui, label, true, selected_stick == value, &palette).clicked() {
                            if value != selected_stick {
                                match self.services.update_n64_primary_stick(value) {
                                    Ok(()) => {
                                        selected_stick = value;
                                        self.state.status = format!(
                                            "{system_label} primary stick set to {}.",
                                            if value == N64PrimaryStick::Left {
                                                "Left Stick"
                                            } else {
                                                "Right Stick"
                                            }
                                        );
                                    }
                                    Err(err) => {
                                        self.state.status =
                                            format!("Failed to save primary stick preference: {err}");
                                    }
                                }
                            }
                        }
                    }
                });
            }
            ui.label(
                egui::RichText::new(
                    "Use mouse or keyboard to edit bindings and save mappings. Controller navigation can browse this panel, but not operate the dropdown editor yet.",
                )
                .size(10.8)
                .color(palette.text_muted),
            );
            ui.add_space(3.0);

            if !self.state.controller_mapping.expanded {
                ui.label(
                    egui::RichText::new(
                        "Mappings hidden. Expand to edit button assignments and shortcuts.",
                    )
                    .size(11.5)
                    .color(palette.text_muted),
                );
                return;
            }

            if active_system == "ALL" {
                draw_controller_preview(ui, preview_texture.as_ref(), &palette, max_image_height);
                ui.add_space(3.0);
                ui.label(
                    egui::RichText::new("Pick a specific system to edit controller mappings.")
                        .size(11.0)
                        .color(palette.text_muted),
                );
                return;
            }

            if wide_layout {
                ui.columns(2, |columns| {
                    draw_controller_preview(
                        &mut columns[0],
                        preview_texture.as_ref(),
                        &palette,
                        max_image_height,
                    );
                    self.draw_mapping_device_tabs_card(
                        &mut columns[1],
                        &mapping_targets,
                        selected_target.as_ref(),
                    );
                });
            } else {
                draw_controller_preview(ui, preview_texture.as_ref(), &palette, max_image_height);
                self.draw_mapping_device_tabs_card(ui, &mapping_targets, selected_target.as_ref());
            }

            if !has_device {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Connect a controller to edit and save per-device mappings.")
                        .size(11.2)
                        .color(palette.text_muted),
                );
                return;
            }

            let options = self.mapping_entry_options();
            let grid_spacing = egui::vec2(5.0, 5.0);
            let target_binding_card_width = 260.0;
            let mapping_columns = ((ui.available_width() + grid_spacing.x)
                / (target_binding_card_width + grid_spacing.x))
                .floor()
                .max(1.0) as usize;
            let mapping_columns = mapping_columns.min(action_labels.len().max(1));
            let cell_width = ((ui.available_width()
                - grid_spacing.x * (mapping_columns.saturating_sub(1) as f32))
                / mapping_columns as f32)
                .max(164.0);
            let binding_dropdown_width = ((cell_width - 14.0).max(138.0) / 3.0).max(56.0);
            let binding_label_width = 82.0;
            let binding_card_width = (binding_label_width + binding_dropdown_width + 18.0).max(132.0);
            egui::Grid::new(format!("mapping-editor-{active_system}"))
                .num_columns(mapping_columns)
                .spacing(grid_spacing)
                .show(ui, |ui| {
                    for (index, action) in action_labels.iter().enumerate() {
                        let key = (*action).to_string();
                        let mut selected = self
                            .state
                            .controller_mapping
                            .actions
                            .get(&key)
                            .cloned()
                            .unwrap_or(None);

                        egui::Frame::new()
                            .fill(palette.panel)
                            .stroke(egui::Stroke::new(1.0, palette.border))
                            .corner_radius(egui::CornerRadius::same(9))
                            .inner_margin(egui::Margin::symmetric(5, 3))
                            .show(ui, |ui| {
                                ui.set_min_width(binding_card_width);
                                ui.horizontal(|ui| {
                                    ui.add_sized(
                                        [binding_label_width, 18.0],
                                        egui::Label::new(
                                            egui::RichText::new(*action)
                                                .strong()
                                                .size(11.5)
                                                .color(palette.accent),
                                        ),
                                    );
                                    egui::ComboBox::from_id_salt(format!(
                                        "mapping-{active_system}-{action}"
                                    ))
                                    .selected_text(
                                        egui::RichText::new(
                                            self.mapping_entry_label(selected.as_ref()),
                                        )
                                        .size(11.5),
                                    )
                                    .width(binding_dropdown_width)
                                    .show_ui(ui, |ui| {
                                        for (label, entry) in &options {
                                            ui.selectable_value(
                                                &mut selected,
                                                entry.clone(),
                                                egui::RichText::new(label).size(11.5),
                                            );
                                        }
                                    });
                                });
                            });

                        self.state.controller_mapping.set_action(key, selected);
                        if (index + 1) % mapping_columns == 0 {
                            ui.end_row();
                        }
                    }
                });

            ui.add_space(4.0);
            let advanced = egui::CollapsingHeader::new(
                egui::RichText::new("Advanced").size(11.5).strong(),
            )
            .id_salt(format!("mapping-advanced-{active_system}"))
            .open(Some(self.state.controller_mapping.advanced_expanded))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("Axis threshold")
                            .size(11.5)
                            .color(palette.text_muted),
                    );
                    ui.add_sized(
                        [64.0, 22.0],
                        egui::DragValue::new(&mut self.state.controller_mapping.threshold)
                            .speed(0.01)
                            .fixed_decimals(2)
                            .range(0.1..=0.95),
                    );
                });
            });
            if advanced.header_response.clicked() {
                self.state.controller_mapping.toggle_advanced_expanded();
            }

            ui.add_space(3.0);
            ui.horizontal_wrapped(|ui| {
                let reset_button =
                    egui::Button::new(egui::RichText::new("Reset To Built-in").size(11.5))
                        .fill(palette.panel)
                        .stroke(egui::Stroke::new(1.0, palette.border))
                        .corner_radius(egui::CornerRadius::same(255));
                if ui.add(reset_button).clicked() {
                    self.reset_controller_mapping_editor_to_defaults();
                }
                let clear_button =
                    egui::Button::new(egui::RichText::new("Clear Current Bindings").size(11.5))
                        .fill(palette.panel)
                        .stroke(egui::Stroke::new(1.0, palette.border))
                        .corner_radius(egui::CornerRadius::same(255));
                if ui.add(clear_button).clicked() {
                    self.clear_controller_mapping_editor_bindings();
                }
                let save_button = egui::Button::new(egui::RichText::new("Save Mapping").size(11.5))
                    .fill(palette.accent_soft)
                    .stroke(egui::Stroke::new(1.0, palette.accent))
                    .corner_radius(egui::CornerRadius::same(255));
                if ui.add(save_button).clicked() {
                    self.save_controller_mapping_from_editor();
                }
            });
        });
    }

    fn draw_mapping_device_tabs_card(
        &mut self,
        ui: &mut egui::Ui,
        targets: &[ControllerMappingTarget],
        selected_target: Option<&ControllerMappingTarget>,
    ) {
        let palette = self.palette();
        egui::Frame::new()
            .fill(palette.panel)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Controllers")
                        .size(11.5)
                        .color(palette.text_muted),
                );

                if targets.is_empty() {
                    ui.label(
                        egui::RichText::new("No controller connected.")
                            .size(11.5)
                            .color(palette.text_muted),
                    );
                    return;
                }

                ui.horizontal_wrapped(|ui| {
                    for target in targets {
                        let selected = selected_target.is_some_and(|selected| {
                            selected.identity.device_key == target.identity.device_key
                        });
                        let label = mapping_target_tab_label(target);
                        if scope_chip(ui, label.as_str(), true, selected, &palette).clicked() {
                            self.request_controller_mapping_target_switch(target);
                        }
                    }
                });

                let pending_label = self
                    .state
                    .controller_mapping
                    .pending_device_switch()
                    .map(|(_, label)| label.to_string());
                if let Some(label) = pending_label {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "Unsaved changes. Switch to {label} and discard current edits?"
                            ))
                            .size(11.0)
                            .color(palette.text_muted),
                        );
                        if scope_chip(ui, "Keep Editing", true, false, &palette).clicked() {
                            self.cancel_pending_controller_mapping_target_switch();
                        }
                        if scope_chip(ui, "Discard & Switch", true, false, &palette).clicked() {
                            self.confirm_pending_controller_mapping_target_switch();
                        }
                    });
                }

                if let Some(selected) = selected_target {
                    ui.add_space(1.0);
                    ui.label(
                        egui::RichText::new(selected.identity.name.as_str())
                            .size(12.0)
                            .strong()
                            .color(palette.text),
                    );
                    draw_device_preset_hint(ui, &selected.identity, &palette);
                }
            });
    }

    pub(crate) fn draw_alpha_toolbar(&mut self, ui: &mut egui::Ui, show_manage: bool) -> bool {
        let palette = self.palette();
        let mut changed = false;

        let row_width = ui.available_width();

        if use_wrapped_alpha_toolbar(row_width) {
            let spacing_x = 3.0;
            let single_w = 27.0_f32; // single-letter button: text + 2*button_padding + stroke
            let wider_count = 2.0; // "ALL" and "0-9" are wider
            let wider_w = 38.0; // approximate width of wider buttons
            let n = ALPHA_FILTERS.len() as f32;
            let content_w =
                wider_count * wider_w + (n - wider_count) * single_w + (n - 1.0) * spacing_x;
            let pad = ((row_width - content_w) * 0.5).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(pad);
                ui.spacing_mut().item_spacing = egui::vec2(spacing_x, 3.0);
                for (index, value) in Self::alpha_filter_values().iter().copied().enumerate() {
                    if self.draw_alpha_button(ui, value, index, &palette) {
                        changed = true;
                    }
                }
                self.draw_manage_button_inline(ui, show_manage);
            });
        } else {
            egui::ScrollArea::horizontal()
                .max_height(15.0)
                .id_salt("alpha-scroll")
                .show(ui, |ui| {
                    ui.set_min_width(row_width);
                    ui.horizontal_centered(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 3.0);
                        for (index, value) in
                            Self::alpha_filter_values().iter().copied().enumerate()
                        {
                            if self.draw_alpha_button(ui, value, index, &palette) {
                                changed = true;
                            }
                        }
                        self.draw_manage_button_inline(ui, show_manage);
                    });
                });
        }

        changed
    }

    fn draw_manage_button_inline(&mut self, ui: &mut egui::Ui, show: bool) {
        if !show {
            return;
        }
        let palette = self.palette();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let focused = self.state.menu_nav.focus_region == MenuFocusRegion::LibraryManageButton;
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
                self.state.manage.open = true;
                self.state.menu_nav.focus_region = MenuFocusRegion::ManageHeader;
                self.refresh_manage_rows();
                self.sync_manage_settings_from_services();
            }
        });
    }

    pub(crate) fn draw_alpha_button(
        &mut self,
        ui: &mut egui::Ui,
        value: &str,
        index: usize,
        palette: &ThemePalette,
    ) -> bool {
        let selected = self.state.library.alpha_filter == value;
        let focused = self.state.menu_nav.focus_region == MenuFocusRegion::FiltersAlpha
            && self.state.menu_nav.alpha_filter_index == index;
        let focus_t = ui
            .ctx()
            .animate_bool(ui.id().with(("alpha-pill-focus", value)), focused);
        let button = egui::Button::new(egui::RichText::new(value).size(10.0).strong())
            .min_size(egui::vec2(15.0, 11.0))
            .fill(if selected {
                palette.accent_soft
            } else if focused {
                blend_color(palette.panel_alt, palette.accent_soft, 0.28)
            } else {
                palette.panel_alt
            })
            .stroke(egui::Stroke::new(
                if focused { 1.6 } else { 1.0 },
                if selected {
                    palette.accent
                } else if focused {
                    blend_color(palette.border, palette.accent, 0.7)
                } else {
                    palette.border
                },
            ))
            .corner_radius(egui::CornerRadius::same(255));
        let response = ui.add(button);
        let glow_t = if focused { 0.52 + focus_t * 0.48 } else { 0.0 };
        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, glow_t);

        if response.clicked() {
            self.state.menu_nav.focus_region = MenuFocusRegion::FiltersAlpha;
            self.state.menu_nav.alpha_filter_index = index;
            return self.apply_alpha_filter_by_index(index);
        }
        false
    }
}

fn use_wrapped_system_toolbar(width: f32) -> bool {
    width >= 680.0
}

fn use_wrapped_alpha_toolbar(width: f32) -> bool {
    width >= 860.0
}

fn mapping_target_tab_label(target: &ControllerMappingTarget) -> String {
    if let Some(slot) = target.player_slot {
        format!("P{} {}", slot + 1, target.identity.name)
    } else {
        target.identity.name.clone()
    }
}

fn badge_chip(
    ui: &mut egui::Ui,
    label: &str,
    fill: egui::Color32,
    stroke: egui::Color32,
    text: egui::Color32,
) {
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(egui::CornerRadius::same(255))
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).size(11.5).strong().color(text));
        });
}

fn scope_chip(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    selected: bool,
    palette: &ThemePalette,
) -> egui::Response {
    let animation = ui
        .ctx()
        .animate_bool(ui.id().with(("scope-chip", label)), selected);
    let fill = if enabled {
        blend_color(palette.panel, palette.accent_soft, animation)
    } else {
        blend_color(
            palette.panel,
            egui::Color32::from_rgba_premultiplied(
                palette.panel.r(),
                palette.panel.g(),
                palette.panel.b(),
                120,
            ),
            0.55,
        )
    };
    let stroke = if enabled {
        blend_color(palette.border, palette.accent, animation)
    } else {
        palette.border
    };

    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(label).size(11.5))
            .min_size(egui::vec2(0.0, 20.0))
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, stroke))
            .corner_radius(egui::CornerRadius::same(255)),
    )
}

fn draw_device_preset_hint(
    ui: &mut egui::Ui,
    device: &DetectedPadIdentity,
    palette: &ThemePalette,
) {
    let Some(hint) = arcade_domain::device_preset_hint(device) else {
        return;
    };

    ui.add_space(1.0);
    ui.label(egui::RichText::new(hint).size(11.0).color(palette.accent));
}

fn draw_controller_preview(
    ui: &mut egui::Ui,
    texture: Option<&egui::TextureHandle>,
    palette: &ThemePalette,
    max_image_height: f32,
) {
    egui::Frame::new()
        .fill(palette.panel)
        .stroke(egui::Stroke::new(1.0, palette.border))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                if let Some(texture) = texture {
                    let max_size =
                        egui::vec2((ui.available_width() - 2.0).max(180.0), max_image_height);
                    let draw_size = fit_size(texture.size_vec2(), max_size);
                    ui.add(egui::Image::new((texture.id(), draw_size)));
                }
            });
        });
}

fn blend_color(from: egui::Color32, to: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| -> u8 { (a as f32 + (b as f32 - a as f32) * t).round() as u8 };

    egui::Color32::from_rgba_premultiplied(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
        mix(from.a(), to.a()),
    )
}

fn system_uses_primary_stick_selector(system: &str) -> bool {
    let normalized = system.trim().to_ascii_uppercase();
    matches!(normalized.as_str(), "N64" | "DREAMCAST")
}
