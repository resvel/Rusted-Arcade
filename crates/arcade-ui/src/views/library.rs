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
                    let search_width = (toolbar_width * 0.5).clamp(176.0, 308.0);
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

        self.draw_selected_launch_options(ui);

        ui.add_space(4.0);
        if self.state.library.visible_rom_ids.is_empty() {
            self.panel_frame().show(ui, |ui| {
                ui.label(
                    egui::RichText::new("No ROMs match current filters.")
                        .color(self.palette().text_muted),
                );
            });
            return;
        }

        self.draw_rom_grid(ctx, ui, GridSource::Library, "library-grid");
    }

    pub(crate) fn draw_selected_launch_options(&mut self, ui: &mut egui::Ui) {
        let Some(rom) = self.current_selected_rom().cloned() else {
            return;
        };
        let config = self.services.config();
        let profiles = arcade_domain::configurable_core_profiles_with_dynamic(
            &config.emulation.discovered_core_profiles,
        )
        .into_iter()
        .filter(|profile| profile.system.eq_ignore_ascii_case(&rom.rom.system))
        .collect::<Vec<_>>();
        if profiles.is_empty() {
            return;
        }

        let palette = self.palette();
        let current_choice = self
            .state
            .library
            .launch_core_choices
            .entry(rom.rom.id.clone())
            .or_insert_with(|| String::from("auto"))
            .clone();
        let auto_core = arcade_domain::resolve_core_with_dynamic(
            &rom.rom.system,
            rom.rom.emulator_core.as_deref(),
            &config.emulation.discovered_core_profiles,
        );
        let chosen_label = if current_choice == "auto" {
            format!(
                "Auto ({})",
                display_core_name_for_launch(&auto_core, &profiles)
            )
        } else {
            display_core_name_for_launch(&current_choice, &profiles).to_string()
        };

        self.panel_frame().fill(palette.panel).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("Launch")
                        .strong()
                        .color(palette.text),
                );
                ui.label(
                    egui::RichText::new(&rom.display_title)
                        .small()
                        .color(palette.text_muted),
                );
                ui.add_space(8.0);
                if ui.button("Play").clicked() {
                    self.launch_selected_rom();
                }
                let toggle_label = if self.state.library.launch_options_open {
                    "Hide Core Options"
                } else {
                    "Core Options"
                };
                if ui.button(toggle_label).clicked() {
                    self.state.library.launch_options_open = !self.state.library.launch_options_open;
                }
                ui.label(
                    egui::RichText::new(format!("Core: {chosen_label}"))
                        .small()
                        .color(palette.text_muted),
                );
            });

            if self.state.library.launch_options_open {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Use core").small().color(palette.text_muted));
                    egui::ComboBox::from_id_salt(("launch-core", rom.rom.id.as_str()))
                        .selected_text(chosen_label)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                self.state
                                    .library
                                    .launch_core_choices
                                    .entry(rom.rom.id.clone())
                                    .or_insert_with(|| String::from("auto")),
                                String::from("auto"),
                                format!(
                                    "Auto ({})",
                                    display_core_name_for_launch(&auto_core, &profiles)
                                ),
                            );
                            for profile in &profiles {
                                ui.selectable_value(
                                    self.state
                                        .library
                                        .launch_core_choices
                                        .entry(rom.rom.id.clone())
                                        .or_insert_with(|| String::from("auto")),
                                    profile.core_name.to_string(),
                                    profile.display_name,
                                );
                            }
                        });
                    if ui.button("Always use for this game").clicked() {
                        if let Some(core) = self
                            .state
                            .library
                            .launch_core_choices
                            .get(&rom.rom.id)
                            .filter(|core| !core.eq_ignore_ascii_case("auto"))
                            .cloned()
                        {
                            match self.services.set_rom_core_override(&rom.rom.id, &core) {
                                Ok(()) => self.state.status = format!(
                                    "Saved {} for {}.",
                                    display_core_name_for_launch(&core, &profiles),
                                    rom.display_title
                                ),
                                Err(err) => {
                                    self.state.status = format!("Failed to save core choice: {err}")
                                }
                            }
                        }
                    }
                });
                ui.label(
                    egui::RichText::new(
                        "Core knobs auto-populate under Settings → Core Settings after a core exposes libretro options.",
                    )
                    .small()
                    .color(palette.text_muted),
                );
            }
        });
    }
}

fn display_core_name_for_launch<'a>(
    core_name: &str,
    profiles: &'a [arcade_domain::CoreProfile],
) -> &'a str {
    profiles
        .iter()
        .find(|profile| profile.core_name.eq_ignore_ascii_case(core_name))
        .map(|profile| profile.display_name)
        .unwrap_or("Selected Core")
}
