use crate::app::NativeArcadeUiApp;
use crate::render::fit_size;
use crate::state::{MappingEditorScope, MenuFocusRegion};
use crate::theme::{ThemePalette, SYSTEM_FILTERS};
use arcade_domain::DetectedPadIdentity;
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
        let palette = self.palette();
        let mut changed = false;

        if self.active_system() == "ARCADE" {
            self.draw_arcade_toolbar_banner(ctx, ui, &palette);
            ui.add_space(4.0);
        }
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
                    .inner_margin(egui::Margin::symmetric(14, 9));

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
                                .size(12.0)
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
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_width(row_width);
                ui.horizontal_wrapped(|ui| {
                    draw_pills(ui, egui::vec2(12.0, 6.0));
                });
            });
        } else {
            egui::ScrollArea::horizontal()
                .max_height(54.0)
                .id_salt("systems-scroll")
                .show(ui, |ui| {
                    ui.set_min_width(row_width);
                    ui.horizontal_centered(|ui| {
                        draw_pills(ui, egui::vec2(14.0, 6.0));
                    });
                });
        }

        changed
    }

    fn draw_arcade_toolbar_banner(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        _palette: &ThemePalette,
    ) {
        let banner_height = if ui.available_width() >= 720.0 {
            44.0
        } else if ui.available_width() >= 420.0 {
            38.0
        } else {
            32.0
        };
        let width = ui.available_width();
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(width, banner_height), egui::Sense::hover());
        let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));

        for (path, anchor_x, max_w, max_h, alpha) in [
            (
                "/system-logos-web/Neo_Geo_logo.svg",
                0.16_f32,
                0.26_f32,
                0.82_f32,
                42_u8,
            ),
            (
                "/system-logos-web/SNK_logo.svg",
                0.5_f32,
                0.16_f32,
                0.5_f32,
                32_u8,
            ),
            (
                "/system-logos-web/SNK_Playmore_logo.svg",
                0.82_f32,
                0.3_f32,
                0.62_f32,
                38_u8,
            ),
        ] {
            let Some(asset_path) = self.resolve_db_asset_path(path) else {
                continue;
            };
            let Some(texture) = self.themed_art_texture(ctx, asset_path) else {
                continue;
            };
            let max_size = egui::vec2((rect.width() * max_w).max(32.0), rect.height() * max_h);
            let draw_size = fit_size(texture.size_vec2(), max_size);
            let center = egui::pos2(rect.left() + rect.width() * anchor_x, rect.center().y);
            let draw_rect = egui::Rect::from_center_size(center, draw_size);
            ui.painter().image(
                texture.id(),
                draw_rect,
                uv,
                egui::Color32::from_rgba_premultiplied(255, 255, 255, alpha),
            );
        }
    }

    pub(crate) fn draw_system_controller_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let active_system = self.active_system().to_string();
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
        let action_labels = if active_system == "ALL" {
            Vec::new()
        } else {
            self.supported_mapping_actions(&active_system).to_vec()
        };
        let action_count = action_labels.len();
        let preview_texture = self.system_controller_texture(ctx, &active_system);
        let (active_device, has_device, dirty) = if active_system == "ALL" {
            (None, false, false)
        } else {
            self.sync_controller_mapping_editor();
            let active_device = self.active_detected_gamepad_identity();
            let has_device = active_device.is_some();
            let dirty = self.controller_mapping_is_dirty();
            (active_device, has_device, dirty)
        };
        let using_device_override = self.state.controller_mapping.selected_scope
            == MappingEditorScope::ActiveDevice
            && has_device;
        let summary_line = if active_system == "ALL" {
            String::from("Choose a system to edit mappings.")
        } else if using_device_override {
            let device_name = active_device
                .as_ref()
                .map(|device| device.name.as_str())
                .unwrap_or("connected controller");
            format!("Editing {active_system} override for {device_name}")
        } else {
            format!("Editing {active_system} system defaults")
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
                                if using_device_override {
                                    "Device Override"
                                } else {
                                    "System Default"
                                },
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
                        let focused =
                            self.state.menu_nav.focus_region == MenuFocusRegion::ControllerMappingToggle;
                        let focus_t = ui
                            .ctx()
                            .animate_bool(ui.id().with("controller-mapping-toggle-focus"), focused);
                        let toggle_button =
                            egui::Button::new(egui::RichText::new(toggle_label).size(11.5))
                                .fill(if focused {
                                    blend_color(palette.panel, palette.accent_soft, 0.32)
                                } else {
                                    palette.panel
                                })
                                .stroke(egui::Stroke::new(
                                    if focused { 1.6 } else { 1.0 },
                                    if focused { palette.accent } else { palette.border },
                                ))
                                .corner_radius(egui::CornerRadius::same(255));
                        let response = ui.add(toggle_button);
                        let glow_t = if focused { 0.58 + focus_t * 0.42 } else { 0.0 };
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, glow_t);
                        if response.clicked() {
                            self.state.menu_nav.focus_region = MenuFocusRegion::ControllerMappingToggle;
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

                    egui::Frame::new()
                        .fill(palette.panel)
                        .stroke(egui::Stroke::new(1.0, palette.border))
                        .corner_radius(egui::CornerRadius::same(10))
                        .inner_margin(egui::Margin::same(6))
                        .show(&mut columns[1], |ui| {
                            ui.label(
                                egui::RichText::new("Profile")
                                    .size(11.5)
                                    .color(palette.text_muted),
                            );
                            ui.horizontal_wrapped(|ui| {
                                let system_default_selected = self.state.controller_mapping.selected_scope
                                    == MappingEditorScope::SystemDefault;
                                if scope_chip(
                                    ui,
                                    "System Default",
                                    true,
                                    system_default_selected,
                                    &palette,
                                )
                                .clicked()
                                {
                                    self.state
                                        .controller_mapping
                                        .select_scope(MappingEditorScope::SystemDefault);
                                }

                                let device_selected = self.state.controller_mapping.selected_scope
                                    == MappingEditorScope::ActiveDevice;
                                if scope_chip(
                                    ui,
                                    "Active Device Override",
                                    has_device,
                                    device_selected,
                                    &palette,
                                )
                                .clicked()
                                {
                                    self.state
                                        .controller_mapping
                                        .select_scope(MappingEditorScope::ActiveDevice);
                                }
                            });

                            ui.add_space(1.0);
                            if let Some(device) = active_device.as_ref() {
                                ui.label(
                                    egui::RichText::new(device.name.as_str())
                                        .size(12.0)
                                        .strong()
                                        .color(palette.text),
                                );
                                draw_device_preset_hint(ui, device, &palette);
                            } else {
                                ui.label(
                                    egui::RichText::new(
                                        "No controller connected. Device overrides are unavailable.",
                                    )
                                    .size(11.5)
                                    .color(palette.text_muted),
                                );
                            }
                        });
                });
            } else {
                draw_controller_preview(ui, preview_texture.as_ref(), &palette, max_image_height);

                egui::Frame::new()
                    .fill(palette.panel)
                    .stroke(egui::Stroke::new(1.0, palette.border))
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("Profile")
                                .size(11.5)
                                .color(palette.text_muted),
                        );
                        ui.horizontal_wrapped(|ui| {
                            let system_default_selected = self.state.controller_mapping.selected_scope
                                == MappingEditorScope::SystemDefault;
                            if scope_chip(
                                ui,
                                "System Default",
                                true,
                                system_default_selected,
                                &palette,
                            )
                            .clicked()
                            {
                                self.state
                                    .controller_mapping
                                    .select_scope(MappingEditorScope::SystemDefault);
                            }

                            let device_selected = self.state.controller_mapping.selected_scope
                                == MappingEditorScope::ActiveDevice;
                            if scope_chip(
                                ui,
                                "Active Device Override",
                                has_device,
                                device_selected,
                                &palette,
                            )
                            .clicked()
                            {
                                self.state
                                    .controller_mapping
                                    .select_scope(MappingEditorScope::ActiveDevice);
                            }
                        });

                        ui.add_space(1.0);
                        if let Some(device) = active_device.as_ref() {
                            ui.label(
                                egui::RichText::new(device.name.as_str())
                                    .size(12.0)
                                    .strong()
                                    .color(palette.text),
                            );
                            draw_device_preset_hint(ui, device, &palette);
                        } else {
                            ui.label(
                                egui::RichText::new(
                                    "No controller connected. Device overrides are unavailable.",
                                )
                                .size(11.5)
                                .color(palette.text_muted),
                            );
                        }
                    });
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

    pub(crate) fn draw_alpha_toolbar(&mut self, ui: &mut egui::Ui) -> bool {
        let palette = self.palette();
        let mut changed = false;

        let row_width = ui.available_width();
        let mut draw_buttons = |ui: &mut egui::Ui, spacing: egui::Vec2| {
            ui.spacing_mut().item_spacing = spacing;
            for (index, value) in Self::alpha_filter_values().iter().copied().enumerate() {
                if self.draw_alpha_button(ui, value, index, &palette) {
                    changed = true;
                }
            }
        };

        if use_wrapped_alpha_toolbar(row_width) {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_width(row_width);
                ui.horizontal_wrapped(|ui| {
                    draw_buttons(ui, egui::vec2(6.0, 6.0));
                });
            });
        } else {
            egui::ScrollArea::horizontal()
                .max_height(34.0)
                .id_salt("alpha-scroll")
                .show(ui, |ui| {
                    ui.set_min_width(row_width);
                    ui.horizontal_centered(|ui| {
                        draw_buttons(ui, egui::vec2(8.0, 6.0));
                    });
                });
        }

        changed
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
        let button = egui::Button::new(egui::RichText::new(value).size(11.0).strong())
            .min_size(egui::vec2(32.0, 24.0))
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
