use crate::app::NativeArcadeUiApp;
use crate::controller_mapper::{
    action_for_visual_control, controller_mapper_art_for_device, controller_mapper_control_label,
    physical_input_controls, physical_input_for_action, system_action_is_native,
    system_action_label, system_controller_layout_for_system, ControllerMapperArt,
    OverlayHotspotShape, SystemControllerLayout, SystemOverlayHotspot,
};
use crate::input::ControllerMappingTarget;
use crate::render::{fit_size, fit_size_to_aspect};
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
            let mapper_help = if system_controller_layout_for_system(&active_system).is_some() {
                "Select a system control first, then choose which physical controller input should drive it."
            } else {
                "This system uses the text mapper. Select an action, then choose the physical controller input to bind."
            };
            ui.label(
                egui::RichText::new(mapper_help)
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

            self.draw_mapping_device_tabs_card(ui, &mapping_targets, selected_target.as_ref());

            if !has_device {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Connect a controller to edit and save per-device mappings.")
                        .size(11.2)
                        .color(palette.text_muted),
                );
                return;
            }
            ui.add_space(6.0);
            if let Some(target) = selected_target.as_ref() {
                self.draw_visual_controller_mapper(
                    ui,
                    ctx,
                    target,
                    action_labels,
                    wide_layout,
                );
            }

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

    fn draw_visual_controller_mapper(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        target: &ControllerMappingTarget,
        action_labels: &[&str],
        wide_layout: bool,
    ) {
        let palette = self.palette();
        let system = self.state.controller_mapping.input_system.clone();
        let physical_art = controller_mapper_art_for_device(Some(&target.identity));
        if ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && self.state.controller_mapping.is_listening()
        {
            self.cancel_controller_mapping_input_listening(true);
        }
        let layout = system_controller_layout_for_system(&system);
        let system_hotspots = layout.and_then(|value| self.system_mapper_hotspots(value));
        let extra_actions: Vec<&str> = action_labels
            .iter()
            .copied()
            .filter(|action| !system_action_is_native(&system, action))
            .collect();
        let uses_system_visual = layout
            .zip(system_hotspots.as_ref())
            .is_some_and(|(value, _)| self.system_mapper_art_available(value));

        egui::Frame::new()
            .fill(palette.panel)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("Visual Mapper")
                            .size(12.0)
                            .strong()
                            .color(palette.text),
                    );
                    let chip_label = if uses_system_visual {
                        "System-first mapping"
                    } else {
                        "Text fallback"
                    };
                    badge_chip(
                        ui,
                        chip_label,
                        palette.panel_alt,
                        palette.border,
                        palette.text_muted,
                    );
                });
                ui.add_space(4.0);

                let mapper_height_hint = if wide_layout { 720.0 } else { 620.0 };
                if wide_layout {
                    let spacing = 10.0;
                    let available_width = ui.available_width();
                    let art_width = (available_width * 0.56).clamp(420.0, 640.0);
                    let panel_width = (available_width - art_width - spacing).max(280.0);
                    ui.horizontal_top(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(art_width, 0.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                if uses_system_visual {
                                    let (layout, hotspots) = layout
                                        .zip(system_hotspots.as_deref())
                                        .expect("visual mapper resources checked above");
                                    self.draw_system_visual_mapper_column(
                                        ui,
                                        ctx,
                                        &target.identity,
                                        layout,
                                        hotspots,
                                        physical_art,
                                        &extra_actions,
                                        &palette,
                                        false,
                                    );
                                } else {
                                    self.draw_text_mapper_column(
                                        ui,
                                        &target.identity,
                                        layout,
                                        physical_art,
                                        action_labels,
                                        &palette,
                                        false,
                                    );
                                }
                            },
                        );
                        ui.add_space(spacing);
                        ui.allocate_ui_with_layout(
                            egui::vec2(panel_width, 0.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                self.draw_physical_input_assignment_panel(
                                    ui,
                                    &target.identity,
                                    physical_art,
                                    layout,
                                    &palette,
                                    mapper_height_hint,
                                );
                            },
                        );
                    });
                } else {
                    if uses_system_visual {
                        let (layout, hotspots) = layout
                            .zip(system_hotspots.as_deref())
                            .expect("visual mapper resources checked above");
                        self.draw_system_visual_mapper_column(
                            ui,
                            ctx,
                            &target.identity,
                            layout,
                            hotspots,
                            physical_art,
                            &extra_actions,
                            &palette,
                            true,
                        );
                    } else {
                        self.draw_text_mapper_column(
                            ui,
                            &target.identity,
                            layout,
                            physical_art,
                            action_labels,
                            &palette,
                            true,
                        );
                    }
                    ui.add_space(8.0);
                    self.draw_physical_input_assignment_panel(
                        ui,
                        &target.identity,
                        physical_art,
                        layout,
                        &palette,
                        mapper_height_hint,
                    );
                }
            });
    }

    fn draw_system_visual_mapper_column(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        device: &DetectedPadIdentity,
        layout: SystemControllerLayout,
        hotspots: &[SystemOverlayHotspot],
        physical_art: ControllerMapperArt,
        extra_actions: &[&str],
        palette: &ThemePalette,
        compact: bool,
    ) {
        let visual_height = if compact { 360.0 } else { 500.0 };

        egui::Frame::new()
            .fill(palette.panel_alt)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("System Controls")
                            .size(11.2)
                            .strong()
                            .color(palette.text),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Click the original controller layout for the system action you want to bind.",
                        )
                            .size(10.8)
                            .color(palette.text_muted),
                    );
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            let debug_enabled =
                                self.state.controller_mapping.show_system_hotspot_debug;
                            let debug_button = egui::Button::new(
                                egui::RichText::new(if debug_enabled {
                                    "Hide Hotspots"
                                } else {
                                    "Show Hotspots"
                                })
                                .size(10.6),
                            )
                            .fill(if debug_enabled {
                                palette.accent_soft
                            } else {
                                palette.panel
                            })
                            .stroke(egui::Stroke::new(
                                1.0,
                                if debug_enabled {
                                    palette.accent
                                } else {
                                    palette.border
                                },
                            ))
                            .corner_radius(egui::CornerRadius::same(255));
                            if ui.add(debug_button).clicked() {
                                self.state.controller_mapping.toggle_system_hotspot_debug();
                            }
                        },
                    );
                });
                ui.add_space(4.0);
                self.draw_system_controller_mapper_image(
                    ui,
                    ctx,
                    device,
                    layout,
                    hotspots,
                    physical_art,
                    palette,
                    visual_height,
                );

                if !extra_actions.is_empty() {
                    ui.add_space(8.0);
                        self.draw_mapping_action_list(
                            ui,
                            device,
                            "Extra Inputs + Shortcuts",
                            Some(
                                "Map modern-only controls and frontend shortcuts outside the original controller layout.",
                        ),
                        physical_art,
                        extra_actions,
                        palette,
                        if compact { 180.0 } else { 210.0 },
                    );
                }
            });
    }

    fn draw_text_mapper_column(
        &mut self,
        ui: &mut egui::Ui,
        device: &DetectedPadIdentity,
        layout: Option<SystemControllerLayout>,
        physical_art: ControllerMapperArt,
        action_labels: &[&str],
        palette: &ThemePalette,
        compact: bool,
    ) {
        egui::Frame::new()
            .fill(palette.panel_alt)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("System Controls")
                        .size(11.8)
                        .strong()
                        .color(palette.text),
                );
                ui.add_space(4.0);
                let subtitle = if layout.is_some() {
                    "System art or hotspot overlay is unavailable for this system, so bindings are shown as a text list."
                } else {
                    "This system uses the non-visual mapper. Select an action to bind a physical input."
                };
                ui.label(
                    egui::RichText::new(subtitle)
                        .size(10.8)
                        .color(palette.text_muted),
                );
                ui.add_space(6.0);
                self.draw_mapping_action_list(
                    ui,
                    device,
                    "All Bindings",
                    None,
                    physical_art,
                    action_labels,
                    palette,
                    if compact { 360.0 } else { 520.0 },
                );
            });
    }

    fn draw_physical_input_assignment_panel(
        &mut self,
        ui: &mut egui::Ui,
        device: &DetectedPadIdentity,
        art: ControllerMapperArt,
        layout: Option<SystemControllerLayout>,
        palette: &ThemePalette,
        min_height: f32,
    ) {
        egui::Frame::new()
            .fill(palette.panel_alt)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_min_height(min_height);
                ui.label(
                    egui::RichText::new("Assignments")
                        .size(12.0)
                        .strong()
                        .color(palette.text),
                );
                ui.add_space(4.0);

                let Some(selected_action) =
                    self.state.controller_mapping.selected_mapping_action.clone()
                else {
                    ui.label(
                        egui::RichText::new(
                            "Select a system control to choose which physical controller input should drive it.",
                        )
                        .size(11.2)
                        .color(palette.text_muted),
                    );
                    return;
                };

                let action_label = layout
                    .and_then(|value| system_action_label(value, &selected_action))
                    .unwrap_or(selected_action.as_str());
                let current_control =
                    physical_input_for_action(&self.state.controller_mapping.actions, &selected_action);
                let current_input_label = current_control
                    .map(|control| controller_mapper_control_label(art, control))
                    .unwrap_or("Unassigned");
                let listening = self.state.controller_mapping.is_listening()
                    && self.state.controller_mapping.listening_action()
                        == Some(selected_action.as_str())
                    && self.state.controller_mapping.listening_device_key()
                        == Some(device.device_key.as_str());

                egui::Frame::new()
                    .fill(blend_color(palette.panel, palette.accent_soft, 0.22))
                    .stroke(egui::Stroke::new(1.0, palette.border))
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("Selected Action")
                                .size(10.8)
                                .strong()
                                .color(palette.text_muted),
                        );
                        ui.label(
                            egui::RichText::new(action_label)
                                .size(15.0)
                                .strong()
                                .color(palette.accent),
                        );
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(format!("Physical Input: {current_input_label}"))
                                .size(12.0)
                                .color(palette.text),
                        );
                    });
                ui.add_space(6.0);

                if listening {
                    egui::Frame::new()
                        .fill(blend_color(palette.panel, palette.accent_soft, 0.28))
                        .stroke(egui::Stroke::new(1.0, palette.accent))
                        .corner_radius(egui::CornerRadius::same(10))
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Listening for next input on {}",
                                    device.name.as_str()
                                ))
                                .size(12.0)
                                .strong()
                                .color(palette.accent),
                            );
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new(format!("Action: {action_label}"))
                                    .size(11.2)
                                    .color(palette.text),
                            );
                            ui.label(
                                egui::RichText::new(
                                    "Press any button, trigger, or stick direction.",
                                )
                                .size(10.8)
                                .color(palette.text_muted),
                            );
                            ui.add_space(4.0);
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("Cancel").size(10.8).strong(),
                                    )
                                    .fill(palette.panel)
                                    .stroke(egui::Stroke::new(1.0, palette.border))
                                    .corner_radius(egui::CornerRadius::same(255)),
                                )
                                .clicked()
                            {
                                self.cancel_controller_mapping_input_listening(true);
                            }
                        });
                    ui.add_space(6.0);
                }

                let unassign_selected = current_control.is_none();
                let unassign_button =
                    egui::Button::new(egui::RichText::new("Unassigned").size(11.5))
                        .fill(if unassign_selected {
                            palette.accent_soft
                        } else {
                            palette.panel
                        })
                        .stroke(egui::Stroke::new(
                            1.0,
                            if unassign_selected {
                                palette.accent
                            } else {
                                palette.border
                            },
                        ))
                        .corner_radius(egui::CornerRadius::same(255));
                if ui.add(unassign_button).clicked() {
                    self.unassign_selected_mapping_action(&selected_action);
                }

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Choose a physical input from the connected controller")
                        .size(10.8)
                        .color(palette.text_muted),
                );
                ui.add_space(4.0);
                let actions_height = (min_height - 190.0).max(320.0);
                egui::Frame::new()
                    .fill(blend_color(palette.panel, palette.panel_alt, 0.35))
                    .stroke(egui::Stroke::new(1.0, palette.border))
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_min_height(actions_height);
                        egui::ScrollArea::vertical()
                            .id_salt("controller-mapper-physical-inputs")
                            .max_height(actions_height - 8.0)
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(0.0, 6.0);
                                for hotspot in physical_input_controls(art) {
                                    let is_selected = current_control == Some(hotspot.control);
                                    let assigned_elsewhere = action_for_visual_control(
                                        &self.state.controller_mapping.actions,
                                        hotspot.control,
                                    )
                                    .filter(|mapped| *mapped != selected_action);
                                    let response = egui::Frame::new()
                                        .fill(if is_selected {
                                            palette.accent_soft
                                        } else {
                                            palette.panel
                                        })
                                        .stroke(egui::Stroke::new(
                                            if is_selected { 1.4 } else { 1.0 },
                                            if is_selected {
                                                palette.accent
                                            } else {
                                                palette.border
                                            },
                                        ))
                                        .corner_radius(egui::CornerRadius::same(8))
                                        .inner_margin(egui::Margin::symmetric(10, 7))
                                        .show(ui, |ui| {
                                            ui.set_min_width(ui.available_width());
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new(hotspot.label)
                                                        .size(12.0)
                                                        .strong()
                                                        .color(if is_selected {
                                                            palette.text
                                                        } else {
                                                            palette.accent
                                                        }),
                                                );
                                                if is_selected {
                                                    ui.add_space(6.0);
                                                    ui.label(
                                                        egui::RichText::new("Current")
                                                            .size(10.5)
                                                            .strong()
                                                            .color(palette.text_muted),
                                                    );
                                                } else if let Some(other_action) = assigned_elsewhere {
                                                    ui.add_space(6.0);
                                                    ui.label(
                                                        egui::RichText::new(format!(
                                                            "Used by {other_action}"
                                                        ))
                                                        .size(10.5)
                                                        .color(palette.text_muted),
                                                    );
                                                }
                                            });
                                        })
                                        .response
                                        .interact(egui::Sense::click());
                                    if response.clicked() {
                                        self.assign_selected_mapping_action_to_control(
                                            &selected_action,
                                            hotspot.control,
                                        );
                                    }
                                }
                            });
                    });
            });
    }

    fn draw_mapping_action_list(
        &mut self,
        ui: &mut egui::Ui,
        device: &DetectedPadIdentity,
        title: &str,
        subtitle: Option<&str>,
        physical_art: ControllerMapperArt,
        actions: &[&str],
        palette: &ThemePalette,
        min_height: f32,
    ) {
        egui::Frame::new()
            .fill(blend_color(palette.panel, palette.panel_alt, 0.30))
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_min_height(min_height);
                ui.label(
                    egui::RichText::new(title)
                        .size(11.5)
                        .strong()
                        .color(palette.text),
                );
                if let Some(subtitle) = subtitle {
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(subtitle)
                            .size(10.8)
                            .color(palette.text_muted),
                    );
                }
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .id_salt(format!("mapping-action-list-{title}"))
                    .max_height((min_height - 18.0).max(120.0))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 6.0);
                        for action in actions {
                            let selected = self
                                .state
                                .controller_mapping
                                .selected_mapping_action
                                .as_deref()
                                == Some(*action);
                            let mapped_label = physical_input_for_action(
                                &self.state.controller_mapping.actions,
                                action,
                            )
                            .map(|control| controller_mapper_control_label(physical_art, control))
                            .unwrap_or("Unassigned");
                            let response = egui::Frame::new()
                                .fill(if selected {
                                    palette.accent_soft
                                } else {
                                    palette.panel
                                })
                                .stroke(egui::Stroke::new(
                                    if selected { 1.4 } else { 1.0 },
                                    if selected {
                                        palette.accent
                                    } else {
                                        palette.border
                                    },
                                ))
                                .corner_radius(egui::CornerRadius::same(8))
                                .inner_margin(egui::Margin::symmetric(10, 7))
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(*action).size(11.8).strong().color(
                                                if selected {
                                                    palette.text
                                                } else {
                                                    palette.accent
                                                },
                                            ),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new(mapped_label)
                                                        .size(10.8)
                                                        .color(palette.text_muted),
                                                );
                                            },
                                        );
                                    });
                                })
                                .response
                                .interact(egui::Sense::click());
                            if response.clicked() {
                                self.begin_controller_mapping_input_listening(
                                    (*action).to_string(),
                                    Some(device),
                                );
                            }
                        }
                    });
            });
    }

    fn draw_system_controller_mapper_image(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        device: &DetectedPadIdentity,
        layout: SystemControllerLayout,
        hotspots: &[SystemOverlayHotspot],
        physical_art: ControllerMapperArt,
        palette: &ThemePalette,
        max_height: f32,
    ) {
        let available_width = (ui.available_width() - 2.0).max(180.0);
        let texture =
            self.system_mapper_texture(ctx, layout, egui::vec2(available_width, max_height));
        let Some(texture) = texture else {
            ui.label(
                egui::RichText::new("System controller artwork is missing for this mapper.")
                    .size(11.0)
                    .color(palette.text_muted),
            );
            return;
        };

        let aspect_ratio = texture.size_vec2().x / texture.size_vec2().y.max(1.0);
        let draw_size = fit_size_to_aspect(egui::vec2(available_width, max_height), aspect_ratio);
        let (rect, response) = ui.allocate_exact_size(draw_size, egui::Sense::click());
        let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
        ui.painter()
            .image(texture.id(), rect, uv, egui::Color32::WHITE);

        let hovered_hotspot = response.hover_pos().and_then(|pointer_pos| {
            hotspots
                .iter()
                .find(|hotspot| hotspot.contains(rect, pointer_pos))
        });

        if hovered_hotspot.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        if response.clicked() {
            if let Some(hotspot) = hovered_hotspot {
                self.begin_controller_mapping_input_listening(hotspot.action.clone(), Some(device));
            }
        }

        let tooltip_text = hovered_hotspot.map(|hotspot| {
            let mapped_input =
                physical_input_for_action(&self.state.controller_mapping.actions, &hotspot.action)
                    .map(|control| controller_mapper_control_label(physical_art, control))
                    .unwrap_or("Unassigned");
            format!("{}: {mapped_input}", hotspot.label)
        });
        if let Some(text) = tooltip_text {
            response.on_hover_text(text);
        }

        let selected_action = self
            .state
            .controller_mapping
            .selected_mapping_action
            .as_deref();
        let overlay_hotspot = hovered_hotspot.or_else(|| {
            selected_action
                .and_then(|selected| hotspots.iter().find(|hotspot| hotspot.action == selected))
        });

        let show_debug = self.state.controller_mapping.show_system_hotspot_debug;
        for hotspot in hotspots {
            let is_selected = selected_action == Some(hotspot.action.as_str());
            let is_hovered = hovered_hotspot.is_some_and(|hovered| hovered.id == hotspot.id);
            if show_debug || is_selected || is_hovered {
                paint_system_action_hotspot(ui, rect, hotspot, palette, is_selected, is_hovered);
                if show_debug {
                    paint_system_action_hotspot_debug_label(ui, rect, hotspot, palette);
                }
            }
        }

        if let Some(hotspot) = overlay_hotspot {
            let mapped_input =
                physical_input_for_action(&self.state.controller_mapping.actions, &hotspot.action)
                    .map(|control| controller_mapper_control_label(physical_art, control));
            paint_system_action_hotspot_overlay(ui, rect, hotspot, palette, mapped_input);
        }
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

fn paint_system_action_hotspot(
    ui: &egui::Ui,
    image_rect: egui::Rect,
    hotspot: &SystemOverlayHotspot,
    palette: &ThemePalette,
    is_selected: bool,
    is_hovered: bool,
) {
    let fill_alpha = if is_selected {
        78
    } else if is_hovered {
        42
    } else {
        14
    };
    let stroke_width = if is_selected {
        2.0
    } else if is_hovered {
        1.3
    } else {
        1.0
    };
    let stroke_color = if is_selected {
        palette.accent
    } else if is_hovered {
        blend_color(palette.accent, palette.text, 0.35)
    } else {
        blend_color(palette.accent, palette.border, 0.35)
    };
    let fill_color = egui::Color32::from_rgba_premultiplied(
        palette.accent.r(),
        palette.accent.g(),
        palette.accent.b(),
        fill_alpha,
    );
    let painter = ui.painter();

    match hotspot.shape {
        OverlayHotspotShape::Circle { .. } => {
            let rect = hotspot.paint_rect(image_rect);
            let radius = rect.width().min(rect.height()) * 0.5;
            painter.circle_filled(rect.center(), radius, fill_color);
            painter.circle_stroke(
                rect.center(),
                radius,
                egui::Stroke::new(stroke_width, stroke_color),
            );
        }
        OverlayHotspotShape::Rect { .. } => {
            let rect = hotspot.paint_rect(image_rect);
            painter.rect_filled(rect, egui::CornerRadius::same(8), fill_color);
            painter.rect_stroke(
                rect,
                egui::CornerRadius::same(8),
                egui::Stroke::new(stroke_width, stroke_color),
                egui::StrokeKind::Outside,
            );
        }
        OverlayHotspotShape::Polygon { ref points } => {
            let screen_points: Vec<_> = points
                .iter()
                .map(|point| {
                    egui::pos2(
                        image_rect.left() + point[0] * image_rect.width(),
                        image_rect.top() + point[1] * image_rect.height(),
                    )
                })
                .collect();
            painter.add(egui::Shape::convex_polygon(
                screen_points.clone(),
                fill_color,
                egui::Stroke::new(stroke_width, stroke_color),
            ));
            painter.add(egui::Shape::closed_line(
                screen_points,
                egui::Stroke::new(stroke_width, stroke_color),
            ));
        }
    }
}

fn paint_system_action_hotspot_overlay(
    ui: &egui::Ui,
    image_rect: egui::Rect,
    hotspot: &SystemOverlayHotspot,
    palette: &ThemePalette,
    mapped_input: Option<&str>,
) {
    let overlay_width = (image_rect.width() * 0.54).clamp(150.0, 260.0);
    let overlay_height = 42.0;
    let overlay_rect = egui::Rect::from_min_size(
        egui::pos2(
            image_rect.left() + 10.0,
            image_rect.bottom() - overlay_height - 10.0,
        ),
        egui::vec2(overlay_width, overlay_height),
    );
    let fill = egui::Color32::from_rgba_premultiplied(
        palette.panel.r(),
        palette.panel.g(),
        palette.panel.b(),
        224,
    );
    ui.painter()
        .rect_filled(overlay_rect, egui::CornerRadius::same(10), fill);
    ui.painter().rect_stroke(
        overlay_rect,
        egui::CornerRadius::same(10),
        egui::Stroke::new(1.0, blend_color(palette.border, palette.accent, 0.45)),
        egui::StrokeKind::Outside,
    );

    let hotspot_center = hotspot.paint_rect(image_rect).center();
    ui.painter().line_segment(
        [
            hotspot_center,
            egui::pos2(overlay_rect.left() + 18.0, overlay_rect.top() + 18.0),
        ],
        egui::Stroke::new(1.2, blend_color(palette.accent, palette.text, 0.2)),
    );

    let text_rect = overlay_rect.shrink2(egui::vec2(10.0, 6.0));
    let title = hotspot.label.as_str();
    let subtitle = match mapped_input {
        Some(input) => format!("Assigned to {input}"),
        None => String::from("Unassigned"),
    };
    ui.painter().text(
        text_rect.left_top(),
        egui::Align2::LEFT_TOP,
        title,
        egui::FontId::proportional(12.5),
        palette.text,
    );
    ui.painter().text(
        egui::pos2(text_rect.left(), text_rect.top() + 16.0),
        egui::Align2::LEFT_TOP,
        subtitle,
        egui::FontId::proportional(11.0),
        palette.accent,
    );
}

fn paint_system_action_hotspot_debug_label(
    ui: &egui::Ui,
    image_rect: egui::Rect,
    hotspot: &SystemOverlayHotspot,
    palette: &ThemePalette,
) {
    let rect = hotspot.paint_rect(image_rect);
    let anchor = egui::pos2(
        rect.center().x,
        (rect.top() - 3.0).max(image_rect.top() + 6.0),
    );
    ui.painter().text(
        anchor,
        egui::Align2::CENTER_BOTTOM,
        hotspot.label.as_str(),
        egui::FontId::proportional(9.5),
        blend_color(palette.text, palette.accent, 0.2),
    );
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
