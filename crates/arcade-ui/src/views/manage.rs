use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use arcade_domain::{
    CoreCatalogCompatibility, CoreCatalogEntry, CoreCatalogInstallPolicy, CoreCatalogInstallState,
    CoverScrapeRunOptions, CoverScrapeSettingsInput, DependencyComponentKind, DependencySetupView,
    DependencySource, DependencyState, DependencySystemReadiness, LocalCoverRelinkRunOptions,
    ManageOperationKind, ManageOperationSummary, ManageProgressEvent, ManageScope, N64CpuCoreMode,
    PathsConfig,
};
use arcade_services::{CatalogCoreInstallRequest, DependencyInstallRequest};
use eframe::egui;

use crate::app::{ManageUiMessage, NativeArcadeUiApp};
use crate::state::MenuFocusRegion;
use crate::theme::SYSTEM_FILTERS;

const TGDB_PLATFORM_IDS_NES: &[u32] = &[7];
const TGDB_PLATFORM_IDS_SNES: &[u32] = &[6];
const TGDB_PLATFORM_IDS_GENESIS: &[u32] = &[18];
const TGDB_PLATFORM_IDS_GB: &[u32] = &[4];
const TGDB_PLATFORM_IDS_GBA: &[u32] = &[5];
const TGDB_PLATFORM_IDS_N64: &[u32] = &[3];
const TGDB_PLATFORM_IDS_ARCADE: &[u32] = &[23];
// Additional platform IDs used for cover scraping
const TGDB_PLATFORM_IDS_PSX: &[u32] = &[1];
const TGDB_PLATFORM_IDS_PS2: &[u32] = &[11];
const TGDB_PLATFORM_IDS_DREAMCAST: &[u32] = &[8];
const TGDB_PLATFORM_IDS_GAMECUBE: &[u32] = &[2];
const TGDB_PLATFORM_IDS_SATURN: &[u32] = &[22];
const TGDB_PLATFORM_IDS_DOS: &[u32] = &[9];
const TGDB_PLATFORM_IDS_PCECD: &[u32] = &[4955];

impl NativeArcadeUiApp {
    pub(crate) fn sync_manage_settings_from_services(&mut self) {
        let config = self.services.config();
        let scrape = &config.management.cover_scraping;
        self.state.manage.settings_rom_root = config.paths.rom_root.display().to_string();
        self.state.manage.settings_db_path = config.paths.db_path.display().to_string();
        self.state.manage.settings_save_state_root =
            config.paths.save_state_root.display().to_string();
        self.state.manage.settings_core_root = config.paths.core_root.display().to_string();
        self.state.manage.settings_bios_root = config.paths.bios_root.display().to_string();

        // Populate the generic core_values map from the persisted core_settings,
        // resolving defaults from the registry for any keys not yet stored.
        let profiles = arcade_domain::configurable_core_profiles_with_dynamic(
            &config.emulation.discovered_core_profiles,
        );
        let configurable_profiles = profiles.clone();
        let mut core_values: HashMap<String, HashMap<String, String>> = HashMap::new();
        for profile in &profiles {
            let mut vars = HashMap::new();
            for var_def in &profile.variables {
                let value = arcade_domain::resolve_core_variable(
                    &config.emulation.core_settings,
                    profile.core_name,
                    var_def,
                );
                vars.insert(var_def.key.to_string(), value);
            }
            core_values.insert(profile.core_name.to_string(), vars);
        }
        self.state.manage.settings_core_values = core_values;

        // Default to the first visible core tab if the current selection is empty
        // or no longer belongs to this platform's supported core matrix.
        let selected_core_is_configurable = configurable_profiles
            .iter()
            .any(|profile| profile.core_name == self.state.manage.settings_selected_core);
        if self.state.manage.settings_selected_core.is_empty() || !selected_core_is_configurable {
            if let Some(first) = configurable_profiles.first() {
                self.state.manage.settings_selected_core = first.core_name.to_string();
            }
        }

        self.state.manage.settings_api_key = scrape.tgdb_api_key.clone().unwrap_or_default();
        self.state.manage.settings_limit = scrape.default_limit.to_string();
        self.state.manage.settings_delay_ms = scrape.default_delay_ms.to_string();
        self.state.manage.scrape_limit = scrape.default_limit.to_string();
        self.state.manage.scrape_delay_ms = scrape.default_delay_ms.to_string();
        self.refresh_runtime_setup_state();
    }

    pub(crate) fn settings_core_profiles_for_selected_system(
        &self,
    ) -> Vec<arcade_domain::CoreProfile> {
        let selected = self.state.manage.settings_selected_system.as_str();
        let profiles = arcade_domain::configurable_core_profiles_with_dynamic(
            &self.services.config().emulation.discovered_core_profiles,
        );
        if selected == "ALL" {
            profiles
        } else {
            profiles
                .into_iter()
                .filter(|profile| profile.system.eq_ignore_ascii_case(selected))
                .collect()
        }
    }

    pub(crate) fn sync_selected_core_for_settings_system(&mut self) {
        let profiles = self.settings_core_profiles_for_selected_system();
        let selected_valid = profiles
            .iter()
            .any(|profile| profile.core_name == self.state.manage.settings_selected_core);
        if !selected_valid {
            self.state.manage.settings_selected_core = profiles
                .first()
                .map(|profile| profile.core_name.to_string())
                .unwrap_or_default();
        }
    }

    pub(crate) fn apply_settings_system_index(&mut self, index: usize) -> bool {
        let index = index.min(SYSTEM_FILTERS.len().saturating_sub(1));
        let Some(system) = SYSTEM_FILTERS.get(index).copied() else {
            return false;
        };
        if self
            .state
            .settings_scroll_target
            .is_some_and(|target| target == crate::state::SettingsScrollTarget::InputSettings)
            && system != self.state.manage.settings_selected_system
            && self.controller_mapping_is_dirty()
        {
            self.state.status =
                String::from("Save or reset controller mapping changes before switching systems.");
            return false;
        }
        self.state.manage.settings_system_index = index;
        self.state.manage.settings_selected_system = system.to_string();
        self.state.manage.runtime_setup_focus_index = 0;
        self.state.manage.runtime_setup_advanced_open = false;
        self.state.manage.runtime_setup_core_browser_open = false;
        self.state.menu_nav.settings_core_tab_index = 0;
        self.state.menu_nav.settings_core_variable_index = 0;
        self.state.menu_nav.settings_core_option_index = 0;
        self.sync_selected_core_for_settings_system();
        if system != "ALL" {
            self.state
                .controller_mapping
                .set_input_system(system.to_string());
        }
        true
    }

    pub(crate) fn draw_settings_system_chrome(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
        title: &str,
        subtitle: &str,
    ) {
        let system = self.state.manage.settings_selected_system.clone();
        self.draw_settings_system_identity_header(ui, palette, &system, title, subtitle);
        ui.add_space(8.0);
        self.draw_settings_system_toolbar(ui);
        ui.add_space(10.0);
    }

    pub(crate) fn refresh_manage_rows(&mut self) {
        let scope = self.current_manage_scope();
        match self.services.list_manage_roms(&scope) {
            Ok(rows) => {
                self.state.manage.rows = rows;
                self.state.manage.selected_rom_ids.retain(|rom_id| {
                    self.state
                        .manage
                        .rows
                        .iter()
                        .any(|row| &row.rom_id == rom_id)
                });
                self.state.manage.remove_confirmation_armed = false;
                self.sync_manage_nav_state();
            }
            Err(err) => {
                self.state.manage.status_message = format!("Failed to load manage rows: {err}");
            }
        }
    }

    pub(crate) fn poll_manage_jobs(&mut self) {
        let mut finished = None;
        if let Some(receiver) = self.manage_job_rx.as_ref() {
            while let Ok(message) = receiver.try_recv() {
                match message {
                    ManageUiMessage::Progress(progress) => {
                        self.state.manage.progress_processed = progress.processed;
                        self.state.manage.progress_total = progress.total;
                        self.state.manage.status_message = progress.message;
                    }
                    ManageUiMessage::Finished(result) => {
                        finished = Some(result);
                    }
                }
            }
        }

        if let Some(result) = finished {
            self.manage_job_rx = None;
            self.state.manage.job_running = false;
            match result {
                Ok(summary) => {
                    self.state.manage.status_message = summary.message.clone();
                    self.state.manage.last_summary = Some(summary.clone());
                    if summary.kind == Some(ManageOperationKind::RemoveFromLibrary) {
                        self.state.manage.clear_selection();
                    }
                    self.state.status = summary.message;
                    if let Err(err) = self.refresh_all() {
                        self.state.status = format!("Refresh warning: {err}");
                    }
                    self.sync_manage_settings_from_services();
                    self.refresh_manage_rows();
                }
                Err(err) => {
                    self.state.manage.status_message = format!("Manage action failed: {err}");
                    self.state.status = self.state.manage.status_message.clone();
                    self.refresh_runtime_setup_state();
                }
            }
        }
    }

    pub(crate) fn draw_manage_library(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        let palette = self.palette();
        self.sync_manage_nav_state();
        let section_width = ui.available_width();
        let content_width = Self::content_band_width_for(section_width).min(section_width);
        let side_gutter = ((section_width - content_width) * 0.5).max(0.0);

        egui::ScrollArea::vertical()
            .id_salt("manage-library-scroll")
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
                            self.draw_manage_header(ui, palette);
                            ui.add_space(8.0);
                            self.draw_manage_scrape_panel(ui, palette);
                            ui.add_space(12.0);
                            self.draw_manage_actions_panel(ui, palette);
                            ui.add_space(10.0);
                            self.draw_manage_inventory_panel(ui, palette);
                            ui.add_space(12.0);
                        },
                    );

                    if side_gutter > 0.0 {
                        ui.add_space(side_gutter);
                    }
                });
            });
    }

    fn draw_manage_header(&mut self, ui: &mut egui::Ui, palette: crate::theme::ThemePalette) {
        ui.horizontal(|ui| {
            let back_focus = self.state.menu_nav.focus_region == MenuFocusRegion::ManageHeader;
            let back_button = manage_button("Back", back_focus, false, palette);
            let back_response = ui.add(back_button);
            if back_focus {
                Self::paint_selection_glow(ui, back_response.rect, 255, palette.accent, 0.78);
            }
            if back_response.clicked() {
                self.close_manage_view();
            }
        });
    }

    pub(crate) fn draw_manage_app_config_panel(
        &mut self,
        ui: &mut egui::Ui,
        _palette: crate::theme::ThemePalette,
    ) {
        self.sync_selected_core_for_settings_system();
        let selected_system = self.state.manage.settings_selected_system.clone();
        let palette = Self::palette_for_system(&selected_system);
        let panel_width = ui.available_width();
        self.panel_frame().fill(palette.panel).show(ui, |ui| {
            let inner_width = (panel_width - 24.0).max(0.0);
            ui.set_width(inner_width);
            ui.set_max_width(inner_width);
            let title = if selected_system == "ALL" {
                String::from("App Configuration")
            } else {
                format!(
                    "{} Core Settings",
                    runtime_system_display_name(&selected_system)
                )
            };
            let subtitle = if selected_system == "ALL" {
                String::from("Global paths and a system-by-system core settings overview.")
            } else {
                String::from("Core options for the selected system.")
            };
            self.draw_settings_system_chrome(ui, palette, &title, &subtitle);

            if selected_system == "ALL" {
                self.draw_app_config_global_page(ui, palette);
            } else {
                self.draw_app_config_system_core_page(ui, palette, &selected_system);
            }
        });
    }

    fn draw_app_config_global_page(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
    ) {
        draw_runtime_section_heading(ui, "Storage Paths", palette);
        draw_path_row(
            ui,
            "ROM Root",
            &mut self.state.manage.settings_rom_root,
            PathPickerKind::Directory,
        );
        draw_path_row(
            ui,
            "DB Path",
            &mut self.state.manage.settings_db_path,
            PathPickerKind::File,
        );
        draw_path_row(
            ui,
            "Save State Root",
            &mut self.state.manage.settings_save_state_root,
            PathPickerKind::Directory,
        );
        draw_path_row(
            ui,
            "Core Root",
            &mut self.state.manage.settings_core_root,
            PathPickerKind::Directory,
        );
        draw_path_row(
            ui,
            "BIOS Root",
            &mut self.state.manage.settings_bios_root,
            PathPickerKind::Directory,
        );

        ui.add_space(8.0);
        draw_runtime_section_heading(ui, "Core Settings", palette);
        let profiles = arcade_domain::configurable_core_profiles_with_dynamic(
            &self.services.config().emulation.discovered_core_profiles,
        );
        for system in SYSTEM_FILTERS
            .iter()
            .copied()
            .filter(|system| *system != "ALL")
        {
            let count = profiles
                .iter()
                .filter(|profile| profile.system.eq_ignore_ascii_case(system))
                .count();
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(runtime_system_display_name(system))
                        .strong()
                        .color(palette.text),
                );
                ui.label(
                    egui::RichText::new(if count == 0 {
                        "No configurable core settings"
                    } else if count == 1 {
                        "1 configurable profile"
                    } else {
                        "Multiple configurable profiles"
                    })
                    .small()
                    .color(palette.text_muted),
                );
            });
        }

        ui.add_space(6.0);
        if ui.button("Discover Installed Core Options").clicked() {
            match self.services.discover_installed_core_options() {
                Ok(count) => {
                    self.sync_manage_settings_from_services();
                    self.state.manage.status_message = if count == 0 {
                        String::from("No core option profile changes found.")
                    } else {
                        format!("Updated core option profiles for {count} core(s).")
                    };
                    self.state.status = self.state.manage.status_message.clone();
                }
                Err(err) => {
                    self.state.manage.status_message = format!("Core discovery failed: {err}");
                    self.state.status = self.state.manage.status_message.clone();
                }
            }
        }
        ui.label(
            egui::RichText::new(
                "Runs the out-of-process core probe helper so newly installed cores can populate Core Settings before launch.",
            )
            .small()
            .color(palette.text_muted),
        );

        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "Changing database or save-state storage paths is saved immediately, but a restart is recommended.",
            )
            .small()
            .color(palette.text_muted),
        );
        self.draw_app_config_save_button(ui, palette);
    }

    fn draw_app_config_system_core_page(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
        system: &str,
    ) {
        let profiles = self.settings_core_profiles_for_selected_system();
        if profiles.is_empty() {
            draw_runtime_section_heading(ui, "Core Settings", palette);
            ui.label(
                egui::RichText::new("This system does not expose configurable core settings yet.")
                    .small()
                    .color(palette.text_muted),
            );
            self.draw_app_config_save_button(ui, palette);
            return;
        }

        let tab_idx = self
            .state
            .menu_nav
            .settings_core_tab_index
            .min(profiles.len().saturating_sub(1));
        draw_runtime_section_heading(ui, "Core Profile", palette);
        ui.horizontal_wrapped(|ui| {
            for (index, profile) in profiles.iter().enumerate() {
                let selected = self.state.manage.settings_selected_core == profile.core_name;
                let focused = self.state.menu_nav.focus_region
                    == MenuFocusRegion::SettingsAppConfigCoreTab
                    && tab_idx == index;
                let response = ui.add(manage_button(
                    profile.display_name,
                    focused,
                    selected,
                    palette,
                ));
                if focused {
                    Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                }
                if response.clicked() {
                    self.state.menu_nav.focus_region = MenuFocusRegion::SettingsAppConfigCoreTab;
                    self.state.menu_nav.settings_core_tab_index = index;
                    self.state.menu_nav.settings_core_variable_index = 0;
                    self.state.menu_nav.settings_core_option_index = 0;
                    self.state.manage.settings_selected_core = profile.core_name.to_string();
                }
            }
        });
        ui.add_space(6.0);

        if let Some(profile) = profiles.get(tab_idx) {
            draw_runtime_section_heading(ui, profile.display_name, palette);
            self.draw_core_profile_settings(ui, palette, profile, system);
        }
        self.draw_app_config_save_button(ui, palette);
    }

    fn draw_core_profile_settings(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
        profile: &arcade_domain::CoreProfile,
        system: &str,
    ) {
        let core_name = profile.core_name.to_string();
        let mut last_group: Option<&str> = None;

        if system == "N64" && profile.core_name == "mupen64plus_next" {
            let mut selected_cpu_core = self.services.n64_cpu_core_mode();
            draw_runtime_section_heading(ui, "CPU Core Lane", palette);
            ui.horizontal_wrapped(|ui| {
                for (label, value) in [
                    ("Stable Cached", N64CpuCoreMode::CachedInterpreter),
                    ("Experimental Dynarec", N64CpuCoreMode::DynamicRecompiler),
                ] {
                    let selected = selected_cpu_core == value;
                    let response = ui.add(manage_button(label, false, selected, palette));
                    if response.clicked() && value != selected_cpu_core {
                        match self.services.update_n64_cpu_core_mode(value) {
                            Ok(()) => {
                                selected_cpu_core = value;
                                self.state.status = format!(
                                    "N64 CPU core lane set to {}.",
                                    if value == N64CpuCoreMode::CachedInterpreter {
                                        "Stable Cached"
                                    } else {
                                        "Experimental Dynarec"
                                    }
                                );
                            }
                            Err(err) => {
                                self.state.status =
                                    format!("Failed to save N64 CPU core lane: {err}");
                            }
                        }
                    }
                }
            });
            ui.add_space(4.0);
        }

        for (var_idx, var_def) in profile.variables.iter().enumerate() {
            if last_group != Some(var_def.group) {
                if last_group.is_some() {
                    ui.add_space(6.0);
                }
                draw_runtime_section_heading(ui, var_def.group, palette);
                last_group = Some(var_def.group);
            }
            ui.label(
                egui::RichText::new(var_def.label)
                    .small()
                    .color(palette.text_muted),
            );
            let current_value = self
                .state
                .manage
                .settings_core_values
                .get(&core_name)
                .and_then(|m| m.get(var_def.key))
                .cloned()
                .unwrap_or_default();
            ui.horizontal_wrapped(|ui| {
                for (opt_idx, opt) in var_def.options.iter().enumerate() {
                    let opt_selected = current_value == opt.value;
                    let opt_focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigCoreVariable
                        && self.state.menu_nav.settings_core_variable_index == var_idx
                        && self.state.menu_nav.settings_core_option_index == opt_idx;
                    let display = opt.display.unwrap_or(opt.value);
                    let response =
                        ui.add(manage_button(display, opt_focused, opt_selected, palette));
                    if opt_focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigCoreVariable;
                        self.state.menu_nav.settings_core_variable_index = var_idx;
                        self.state.menu_nav.settings_core_option_index = opt_idx;
                        self.state
                            .manage
                            .settings_core_values
                            .entry(core_name.clone())
                            .or_default()
                            .insert(var_def.key.to_string(), opt.value.to_string());
                    }
                }
            });
            ui.add_space(2.0);
        }
    }

    fn draw_app_config_save_button(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
    ) {
        ui.add_space(6.0);
        let save_response = ui.add_enabled(
            !self.state.manage.job_running,
            manage_button(
                "Save App Config",
                self.state.menu_nav.focus_region == MenuFocusRegion::SettingsAppConfigSave,
                false,
                palette,
            ),
        );
        if self.state.menu_nav.focus_region == MenuFocusRegion::SettingsAppConfigSave {
            Self::paint_selection_glow(ui, save_response.rect, 255, palette.accent, 0.78);
        }
        if save_response.clicked() {
            self.state.menu_nav.focus_region = MenuFocusRegion::SettingsAppConfigSave;
            self.save_manage_app_settings();
        }
    }

    pub(crate) fn draw_dependency_installer_panel(
        &mut self,
        ui: &mut egui::Ui,
        _palette: crate::theme::ThemePalette,
    ) {
        let setup_system = self.state.manage.settings_selected_system.clone();
        let setup_palette = Self::palette_for_system(&setup_system);
        let panel_width = ui.available_width();
        self.panel_frame().fill(setup_palette.panel).show(ui, |ui| {
            let inner_width = (panel_width - 24.0).max(0.0);
            ui.set_width(inner_width);
            ui.set_max_width(inner_width);

            let Some(report) = self.state.manage.dependency_report.clone() else {
                ui.heading("Runtime Setup");
                ui.label(
                    egui::RichText::new("Runtime dependency status has not been scanned yet.")
                        .small(),
                );
                return;
            };

            let view = DependencySetupView::from_report(&report);
            let focus_count = self.runtime_setup_focus_count(&view);
            self.state.manage.runtime_setup_focus_index = self
                .state
                .manage
                .runtime_setup_focus_index
                .min(focus_count.saturating_sub(1));

            let runtime_title = if setup_system == "ALL" {
                String::from("Welcome to Rusted Arcade")
            } else {
                runtime_system_display_name(&setup_system).to_string()
            };
            let runtime_subtitle = if setup_system == "ALL" {
                format!(
                    "{} · {} ready · {} need files",
                    runtime_setup_lane_label(),
                    view.ready_system_count,
                    view.blocked_system_count
                )
            } else {
                runtime_system_summary(&setup_system, &view)
            };
            self.draw_settings_system_chrome(ui, setup_palette, &runtime_title, &runtime_subtitle);

            if setup_system == "ALL" {
                self.draw_runtime_setup_all_page(ui, setup_palette, &view);
            } else {
                self.draw_runtime_setup_system_page(ui, setup_palette, &setup_system, &view);
            }
        });
    }

    fn draw_settings_system_identity_header(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
        system: &str,
        title: &str,
        subtitle: &str,
    ) {
        let width = ui.available_width();
        let height = 104.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 12.0, palette.panel_alt);
        let bg = if system == "ALL" {
            self.all_systems_background_texture(ui.ctx())
        } else {
            self.launch_system_background_texture(ui.ctx(), Some(system))
        };
        if let Some(texture) = bg {
            let size = texture.size_vec2();
            if size.x > 0.0 && size.y > 0.0 {
                let scale = (rect.width() / size.x).max(rect.height() / size.y);
                let draw_size = egui::vec2(size.x * scale, size.y * scale);
                let draw_rect = egui::Rect::from_center_size(rect.center(), draw_size);
                painter.image(
                    texture.id(),
                    draw_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::from_rgba_premultiplied(255, 255, 255, 58),
                );
            }
        }
        let overlay_top = egui::Color32::from_rgba_premultiplied(
            palette.bg_top.r(),
            palette.bg_top.g(),
            palette.bg_top.b(),
            172,
        );
        let overlay_bottom = egui::Color32::from_rgba_premultiplied(
            palette.bg_bottom.r(),
            palette.bg_bottom.g(),
            palette.bg_bottom.b(),
            218,
        );
        paint_rect_gradient(ui, rect, overlay_top, overlay_bottom);

        let mut cursor = rect.left_top() + egui::vec2(16.0, 14.0);
        if system != "ALL" {
            if let Some(texture) = self.system_logo_texture(ui.ctx(), system) {
                let draw_size =
                    crate::render::fit_size(texture.size_vec2(), egui::vec2(160.0, 34.0));
                let logo_rect = egui::Rect::from_min_size(cursor, draw_size);
                painter.image(
                    texture.id(),
                    logo_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
                cursor.y += draw_size.y + 8.0;
            }
        }

        painter.text(
            cursor,
            egui::Align2::LEFT_TOP,
            title,
            egui::FontId::proportional(22.0),
            palette.text,
        );
        painter.text(
            cursor + egui::vec2(0.0, 30.0),
            egui::Align2::LEFT_TOP,
            subtitle,
            egui::FontId::proportional(12.0),
            palette.text_muted,
        );
    }

    fn draw_settings_system_toolbar(&mut self, ui: &mut egui::Ui) {
        let row_width = ui.available_width().max(1.0);
        let gap_x = runtime_setup_pill_gap(row_width);
        let pill_size = egui::vec2(runtime_setup_pill_width(row_width, gap_x), 27.0);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(gap_x, 0.0);
            for (index, system) in SYSTEM_FILTERS.iter().copied().enumerate() {
                let selected = self.state.manage.settings_selected_system == system;
                let focused = self.state.menu_nav.focus_region
                    == MenuFocusRegion::SettingsSystemToolbar
                    && self.state.manage.settings_system_index == index;
                let response = self.settings_system_pill(ui, system, selected, focused, pill_size);
                if response.clicked() {
                    self.state.menu_nav.focus_region = MenuFocusRegion::SettingsSystemToolbar;
                    self.apply_settings_system_index(index);
                }
            }
        });
    }

    fn settings_system_pill(
        &mut self,
        ui: &mut egui::Ui,
        system: &str,
        selected: bool,
        focused: bool,
        size: egui::Vec2,
    ) -> egui::Response {
        let pill_palette = Self::palette_for_system(system);
        let fill = if selected {
            blend_runtime_color(pill_palette.panel_alt, pill_palette.accent_soft, 0.65)
        } else {
            blend_runtime_color(pill_palette.panel, pill_palette.accent_soft, 0.16)
        };
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        let stroke = egui::Stroke::new(
            if selected || focused { 1.6 } else { 1.0 },
            if selected || focused {
                pill_palette.accent
            } else {
                pill_palette.border
            },
        );
        ui.painter().rect(
            rect,
            egui::CornerRadius::same(255),
            fill,
            stroke,
            egui::StrokeKind::Middle,
        );

        let content_rect = rect.shrink2(egui::vec2(8.0, 4.0));
        let content_painter = ui.painter().with_clip_rect(content_rect);
        let logo_space = content_rect.width().max(16.0);
        let logo_max = egui::vec2(
            Self::system_logo_size(system).x.min(logo_space),
            Self::system_logo_size(system).y.min(16.0),
        );
        if let Some(texture) = self.system_logo_texture(ui.ctx(), system) {
            let draw_size = crate::render::fit_size(texture.size_vec2(), logo_max);
            let logo_rect = egui::Rect::from_center_size(content_rect.center(), draw_size);
            content_painter.image(
                texture.id(),
                logo_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        } else {
            content_painter.text(
                content_rect.center(),
                egui::Align2::CENTER_CENTER,
                system,
                egui::FontId::proportional(11.0),
                pill_palette.text,
            );
        }

        if focused {
            Self::paint_selection_glow(ui, rect, 255, pill_palette.accent, 0.78);
        }
        response.on_hover_text(runtime_system_display_name(system))
    }

    fn draw_runtime_setup_all_page(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
        view: &DependencySetupView,
    ) {
        let mut focus_cursor = 0usize;
        self.draw_runtime_setup_primary_actions(ui, palette, view, "ALL", &mut focus_cursor);
        ui.add_space(10.0);

        self.draw_runtime_setup_all_cores_summary(ui, palette, &mut focus_cursor);
        ui.add_space(10.0);

        if self.runtime_setup_welcome_visible() {
            draw_runtime_section_heading(ui, "First Run", palette);
            ui.label(
                egui::RichText::new(
                    "Rusted Arcade can prepare folders, install standard cores, apply safe defaults, and scan your game folders. You stay in control of BIOS files, ROMs, and compatibility packages.",
                )
                .small()
                .color(palette.text_muted),
            );
            ui.add_space(6.0);
        }

        draw_runtime_section_heading(ui, "Systems", palette);
        for group in &view.system_groups {
            let line = match group.readiness {
                DependencySystemReadiness::Ready => "Ready to play",
                DependencySystemReadiness::Blocked => "Needs your files",
                DependencySystemReadiness::ReadyWithOptionalUpgrades => "Ready, optional upgrade",
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(runtime_system_display_name(&group.system))
                        .strong()
                        .color(palette.text),
                );
                draw_status_chip(ui, line, runtime_readiness_color(group.readiness, palette));
                if group.missing_required > 0 {
                    ui.label(
                        egui::RichText::new(format!("{} blocker(s)", group.missing_required))
                            .small()
                            .color(palette.accent),
                    );
                }
            });
        }

        if !view.optional_upgrades.is_empty() {
            ui.add_space(8.0);
            draw_runtime_section_heading(ui, "Optional Upgrades", palette);
            for status in &view.optional_upgrades {
                ui.label(
                    egui::RichText::new(format!(
                        "{}: {}",
                        status.component.system, status.component.title
                    ))
                    .small()
                    .color(palette.text_muted),
                );
            }
        }
    }

    fn draw_runtime_setup_system_page(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
        system: &str,
        view: &DependencySetupView,
    ) {
        let Some(setup) = view.system_setup(system) else {
            ui.label(
                egui::RichText::new("This system is not in the runtime setup manifest.")
                    .color(palette.text_muted),
            );
            return;
        };
        let mut focus_cursor = 0usize;
        self.draw_runtime_setup_primary_actions(ui, palette, view, system, &mut focus_cursor);
        ui.add_space(10.0);

        self.draw_runtime_setup_cores_section(ui, palette, system, &mut focus_cursor);
        ui.add_space(10.0);

        draw_runtime_section_heading(ui, "Rusted Arcade Can", palette);
        if setup.automatic_actions.is_empty() {
            ui.label(
                egui::RichText::new("No automatic dependency work is required for this system.")
                    .small()
                    .color(palette.text_muted),
            );
        } else {
            for status in &setup.automatic_actions {
                self.draw_runtime_dependency_row(ui, status, palette, &mut focus_cursor, false);
                ui.separator();
            }
        }

        ui.add_space(8.0);
        draw_runtime_section_heading(ui, "You Provide", palette);
        if setup.user_actions.is_empty() {
            ui.label(
                egui::RichText::new("No user-provided BIOS or resource files are missing.")
                    .small()
                    .color(palette.text_muted),
            );
        } else {
            for status in &setup.user_actions {
                self.draw_runtime_dependency_row(ui, status, palette, &mut focus_cursor, false);
                ui.separator();
            }
        }

        ui.add_space(8.0);
        draw_runtime_section_heading(ui, "Ready Check", palette);
        let ready_line = match setup.readiness {
            DependencySystemReadiness::Ready => "Ready to play.",
            DependencySystemReadiness::Blocked => {
                "Needs user-provided files before games can launch."
            }
            DependencySystemReadiness::ReadyWithOptionalUpgrades => {
                "Ready to play. Optional upgrades are available."
            }
        };
        ui.label(
            egui::RichText::new(ready_line)
                .small()
                .color(palette.text_muted),
        );
        if !setup.optional_upgrades.is_empty() {
            ui.add_space(4.0);
            for status in &setup.optional_upgrades {
                self.draw_runtime_dependency_row(ui, status, palette, &mut focus_cursor, true);
                ui.separator();
            }
        }

        ui.add_space(8.0);
        self.draw_runtime_advanced_inventory_toggle(ui, palette, &mut focus_cursor);
        if self.state.manage.runtime_setup_advanced_open {
            for status in &setup.advanced_components {
                self.draw_runtime_dependency_row(ui, status, palette, &mut focus_cursor, true);
                ui.separator();
            }
        }
    }

    fn draw_runtime_setup_all_cores_summary(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
        focus_cursor: &mut usize,
    ) {
        draw_runtime_section_heading(ui, "Cores", palette);
        let Some(catalog) = self.state.manage.core_catalog.as_ref() else {
            ui.label(
                egui::RichText::new("Core catalog has not been scanned yet. Use Rescan to refresh runtime core status.")
                    .small()
                    .color(palette.text_muted),
            );
            return;
        };

        let mut missing_recommended = 0usize;
        let mut installed_recommended = 0usize;
        for group in catalog.system_groups() {
            for entry in group.recommended {
                if entry.installed() {
                    installed_recommended += 1;
                } else {
                    missing_recommended += 1;
                }
            }
        }
        let remote_entries = catalog.remote_buildbot_entries();
        ui.label(
            egui::RichText::new(format!(
                "Recommended core status: {installed_recommended} installed · {missing_recommended} missing. Pick a system above to download or repair individual curated cores."
            ))
            .small()
            .color(palette.text_muted),
        );
        ui.label(
            egui::RichText::new(
                "ALL stays conservative: no download-all action. Unclassified or ambiguous remote-only buildbot cores remain in the explicit Advanced Buildbot Browser below.",
            )
            .small()
            .color(palette.text_muted),
        );
        if !remote_entries.is_empty() {
            ui.add_space(4.0);
            let label = if self.state.manage.runtime_setup_core_browser_open {
                "Hide Advanced Buildbot Browser"
            } else {
                "Show Advanced Buildbot Browser"
            };
            self.draw_runtime_action_button(ui, palette, focus_cursor, label, |app| {
                app.state.manage.runtime_setup_core_browser_open =
                    !app.state.manage.runtime_setup_core_browser_open;
            });
            if self.state.manage.runtime_setup_core_browser_open {
                ui.label(
                    egui::RichText::new("These live buildbot cores could not be safely assigned to one supported system from metadata. They may be untested, incompatible, or inappropriate for this platform lane.")
                        .small()
                        .color(palette.accent),
                );
                for entry in &remote_entries {
                    self.draw_runtime_core_catalog_row(ui, entry, palette, focus_cursor);
                    ui.separator();
                }
            }
        }
    }

    fn draw_runtime_setup_cores_section(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
        system: &str,
        focus_cursor: &mut usize,
    ) {
        draw_runtime_section_heading(ui, "Cores", palette);
        let Some(group) = self
            .state
            .manage
            .core_catalog
            .as_ref()
            .and_then(|catalog| catalog.system_group(system))
        else {
            ui.label(
                egui::RichText::new("No catalog entries are available for this system yet.")
                    .small()
                    .color(palette.text_muted),
            );
            return;
        };

        if group.recommended.is_empty() {
            ui.label(
                egui::RichText::new("No downloadable recommended core is listed for this system.")
                    .small()
                    .color(palette.text_muted),
            );
        } else {
            for entry in &group.recommended {
                self.draw_runtime_core_catalog_row(ui, entry, palette, focus_cursor);
                ui.separator();
            }
        }

        if !group.compatible.is_empty() || !group.protected.is_empty() {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Other compatible / protected cores")
                    .size(12.0)
                    .strong()
                    .color(palette.text),
            );
            for entry in group.compatible.iter().chain(group.protected.iter()) {
                self.draw_runtime_core_catalog_row(ui, entry, palette, focus_cursor);
                ui.separator();
            }
        }

        if !group.advanced.is_empty() {
            ui.add_space(4.0);
            let label = if self.state.manage.runtime_setup_core_browser_open {
                "Hide Advanced Core Browser"
            } else {
                "Show Advanced Core Browser"
            };
            self.draw_runtime_action_button(ui, palette, focus_cursor, label, |app| {
                app.state.manage.runtime_setup_core_browser_open =
                    !app.state.manage.runtime_setup_core_browser_open;
            });
            if self.state.manage.runtime_setup_core_browser_open {
                ui.label(
                    egui::RichText::new("Advanced buildbot cores include metadata-classified candidates and may still be untested, incompatible, or inappropriate for this platform lane.")
                        .small()
                        .color(palette.text_muted),
                );
                for entry in &group.advanced {
                    self.draw_runtime_core_catalog_row(ui, entry, palette, focus_cursor);
                    ui.separator();
                }
            }
        }
    }

    fn draw_runtime_core_catalog_row(
        &mut self,
        ui: &mut egui::Ui,
        entry: &CoreCatalogEntry,
        palette: crate::theme::ThemePalette,
        focus_cursor: &mut usize,
    ) {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(&entry.display_name)
                    .strong()
                    .color(palette.text),
            );
            draw_status_chip(
                ui,
                entry.compatibility.label(),
                catalog_compatibility_color(entry.compatibility, palette),
            );
            draw_status_chip(
                ui,
                catalog_install_state_label(&entry.install_state),
                catalog_install_state_color(&entry.install_state, palette),
            );
            draw_status_chip(ui, entry.source.label(), palette.text_muted);
            if entry.is_default_core_for_system {
                draw_status_chip(ui, "Current default", palette.accent);
            }
        });
        ui.label(
            egui::RichText::new(format!(
                "Core: {} · File: {}",
                entry.core_name, entry.expected_archive_member
            ))
            .monospace()
            .small()
            .color(palette.text_muted),
        );
        if let Some(file_name) = entry.buildbot_file_name.as_deref() {
            ui.label(
                egui::RichText::new(format!("Archive: {file_name}"))
                    .small()
                    .color(palette.text_muted),
            );
        }
        for note in &entry.notes {
            ui.label(egui::RichText::new(note).small().color(palette.text_muted));
        }
        for warning in &entry.warnings {
            ui.label(
                egui::RichText::new(format!("Warning: {warning}"))
                    .small()
                    .color(palette.accent),
            );
        }
        if entry.compatibility == CoreCatalogCompatibility::Protected {
            ui.label(
                egui::RichText::new("Protected lane: Rusted Arcade will not replace this with a stock buildbot core automatically.")
                    .small()
                    .color(palette.accent),
            );
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(format!("Policy: {}", entry.install_policy.label()))
                    .small()
                    .color(palette.text_muted),
            );
            if entry.install_policy.can_install_in_app() && entry.source.can_install_in_app() {
                let focused = self.runtime_setup_action_focused(*focus_cursor);
                let label = if entry.installed() {
                    "Repair"
                } else {
                    "Download"
                };
                let response = ui.add_enabled(
                    !self.state.manage.job_running,
                    manage_button(label, focused, false, palette),
                );
                if focused {
                    Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                }
                *focus_cursor += 1;
                if response.clicked() {
                    self.start_catalog_core_install_job(entry.id.clone());
                }
            } else if entry.install_policy == CoreCatalogInstallPolicy::ManualProtected {
                ui.label(
                    egui::RichText::new("Manual import/resource setup only")
                        .small()
                        .color(palette.text_muted),
                );
            }
        });
    }

    fn draw_runtime_setup_primary_actions(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
        view: &DependencySetupView,
        system: &str,
        focus_cursor: &mut usize,
    ) {
        ui.horizontal_wrapped(|ui| {
            self.draw_runtime_action_button(ui, palette, focus_cursor, "Rescan", |app| {
                app.sync_manage_settings_from_services();
            });
            self.draw_runtime_action_button(ui, palette, focus_cursor, "Get What We Can", |app| {
                app.start_safe_runtime_setup_job(system.to_string());
            });
            self.draw_runtime_action_button(ui, palette, focus_cursor, "Open ROM Folder", |app| {
                app.open_runtime_rom_folder(system);
            });
            self.draw_runtime_action_button(ui, palette, focus_cursor, "Scan Games", |app| {
                app.start_runtime_scan_job(system.to_string());
            });
            if system == "ALL" && self.runtime_setup_welcome_visible() {
                self.draw_runtime_action_button(
                    ui,
                    palette,
                    focus_cursor,
                    "Continue To Favorites",
                    |app| {
                        app.complete_runtime_setup_welcome();
                    },
                );
            }
        });
        if system == "ALL" && !view.missing_required_standard_core_ids().is_empty() {
            ui.label(
                egui::RichText::new(format!(
                    "Safe setup can install {} missing standard core(s).",
                    view.missing_required_standard_core_ids().len()
                ))
                .small()
                .color(palette.text_muted),
            );
        }
    }

    fn draw_runtime_action_button(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
        focus_cursor: &mut usize,
        label: &str,
        action: impl FnOnce(&mut Self),
    ) {
        let focused = self.runtime_setup_action_focused(*focus_cursor);
        let response = ui.add_enabled(
            !self.state.manage.job_running,
            manage_button(label, focused, false, palette),
        );
        if focused {
            Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
        }
        *focus_cursor += 1;
        if response.clicked() {
            action(self);
        }
    }

    fn draw_runtime_advanced_inventory_toggle(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
        focus_cursor: &mut usize,
    ) {
        let advanced_label = if self.state.manage.runtime_setup_advanced_open {
            "Hide Advanced"
        } else {
            "Show Advanced"
        };
        self.draw_runtime_action_button(ui, palette, focus_cursor, advanced_label, |app| {
            app.state.manage.runtime_setup_advanced_open =
                !app.state.manage.runtime_setup_advanced_open;
        });
    }

    fn draw_runtime_dependency_row(
        &mut self,
        ui: &mut egui::Ui,
        status: &arcade_domain::DependencyComponentStatus,
        palette: crate::theme::ThemePalette,
        focus_cursor: &mut usize,
        compact: bool,
    ) {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(&status.component.title)
                    .strong()
                    .color(palette.text),
            );
            draw_status_chip(ui, &status.component.system, palette.text_muted);
            draw_status_chip(ui, status.component.kind.label(), palette.text_muted);
            draw_status_chip(
                ui,
                if status.component.required {
                    "Required"
                } else {
                    "Optional"
                },
                if status.component.required {
                    palette.accent
                } else {
                    palette.text_muted
                },
            );
            draw_status_chip(
                ui,
                match status.state {
                    DependencyState::Ready => "Ready",
                    DependencyState::Missing => "Missing",
                },
                match status.state {
                    DependencyState::Ready => palette.text_muted,
                    DependencyState::Missing => palette.accent,
                },
            );
        });
        if !compact {
            ui.label(
                egui::RichText::new(&status.component.description)
                    .small()
                    .color(palette.text_muted),
            );
        }
        ui.label(
            egui::RichText::new(&status.detail)
                .monospace()
                .small()
                .color(palette.text_muted),
        );
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(format!("Source: {}", status.component.source.label()))
                    .small()
                    .color(palette.text_muted),
            );
            if matches!(
                status.component.kind,
                DependencyComponentKind::Bios | DependencyComponentKind::Romset
            ) {
                ui.label(
                    egui::RichText::new("User-owned files only")
                        .small()
                        .color(palette.text_muted),
                );
            }
        });
        ui.horizontal_wrapped(|ui| {
            let open_focused = self.runtime_setup_action_focused(*focus_cursor);
            let response = ui.add_enabled(
                !self.state.manage.job_running,
                manage_button("Open Folder", open_focused, false, palette),
            );
            if open_focused {
                Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
            }
            *focus_cursor += 1;
            if response.clicked() {
                self.open_dependency_target(&status.component.id);
            }

            match &status.component.source {
                DependencySource::LibretroBuildbot { .. }
                | DependencySource::Homebrew { .. }
                | DependencySource::UpstreamDownload { .. } => {
                    let action_focused = self.runtime_setup_action_focused(*focus_cursor);
                    let label = if status.ready() { "Repair" } else { "Install" };
                    let response = ui.add_enabled(
                        !self.state.manage.job_running,
                        manage_button(label, action_focused, false, palette),
                    );
                    if action_focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    *focus_cursor += 1;
                    if response.clicked() {
                        self.start_dependency_install_job(status.component.id.clone(), None);
                    }
                }
                DependencySource::LocalImport => {
                    let action_focused = self.runtime_setup_action_focused(*focus_cursor);
                    let label = if status.ready() { "Reimport" } else { "Import" };
                    let response = ui.add_enabled(
                        !self.state.manage.job_running,
                        manage_button(label, action_focused, false, palette),
                    );
                    if action_focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    *focus_cursor += 1;
                    if response.clicked() {
                        self.start_dependency_import_job(&status.component);
                    }
                }
                DependencySource::ExternalGuided { .. } => {
                    let action_focused = self.runtime_setup_action_focused(*focus_cursor);
                    let response = ui.add_enabled(
                        !self.state.manage.job_running,
                        manage_button("Open Source", action_focused, false, palette),
                    );
                    if action_focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    *focus_cursor += 1;
                    if response.clicked() {
                        self.start_dependency_install_job(status.component.id.clone(), None);
                    }
                }
                DependencySource::UserProvided => {}
            }
        });
    }

    fn draw_manage_actions_panel(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
    ) {
        self.panel_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("Actions");
            ui.add_space(6.0);
            let disabled = self.state.manage.job_running;
            let selected_count = self.state.manage.selected_count();
            let remove_label = if self.state.manage.remove_confirmation_armed {
                format!("Confirm Remove ({selected_count})")
            } else {
                format!("Remove Selected ({selected_count})")
            };

            ui.horizontal_wrapped(|ui| {
                ui.label("Scope");
                for (index, system) in SYSTEM_FILTERS.iter().copied().enumerate() {
                    let focused = self.state.menu_nav.focus_region == MenuFocusRegion::ManageScope
                        && self.state.menu_nav.manage_scope_index == index;
                    let selected = self.state.manage.scope == system;
                    let label = if system == "ALL" {
                        "All systems"
                    } else {
                        system
                    };
                    let scope_response =
                        ui.add_enabled(!disabled, manage_button(label, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(
                            ui,
                            scope_response.rect,
                            255,
                            palette.accent,
                            0.78,
                        );
                    }
                    if scope_response.clicked() {
                        self.state.menu_nav.focus_region = MenuFocusRegion::ManageScope;
                        self.apply_manage_scope_index(index);
                    }
                }

                let refresh_response = ui.add_enabled(
                    !disabled,
                    manage_button(
                        "Refresh",
                        self.state.menu_nav.focus_region == MenuFocusRegion::ManageActions
                            && self.state.menu_nav.manage_action_index == 0,
                        false,
                        palette,
                    ),
                );
                if self.state.menu_nav.focus_region == MenuFocusRegion::ManageActions
                    && self.state.menu_nav.manage_action_index == 0
                {
                    Self::paint_selection_glow(
                        ui,
                        refresh_response.rect,
                        255,
                        palette.accent,
                        0.78,
                    );
                }
                if refresh_response.clicked() {
                    self.state.menu_nav.focus_region = MenuFocusRegion::ManageActions;
                    self.state.menu_nav.manage_action_index = 0;
                    self.refresh_manage_rows();
                }

                let scan_response = ui.add_enabled(
                    !disabled,
                    manage_button(
                        "Smart Scan",
                        self.state.menu_nav.focus_region == MenuFocusRegion::ManageActions
                            && self.state.menu_nav.manage_action_index == 1,
                        false,
                        palette,
                    ),
                );
                if self.state.menu_nav.focus_region == MenuFocusRegion::ManageActions
                    && self.state.menu_nav.manage_action_index == 1
                {
                    Self::paint_selection_glow(ui, scan_response.rect, 255, palette.accent, 0.78);
                }
                if scan_response.clicked() {
                    self.state.menu_nav.focus_region = MenuFocusRegion::ManageActions;
                    self.state.menu_nav.manage_action_index = 1;
                    self.start_smart_scan_job();
                }

                let remove_response = ui.add_enabled(
                    !disabled && selected_count > 0,
                    manage_button(
                        &remove_label,
                        self.state.menu_nav.focus_region == MenuFocusRegion::ManageActions
                            && self.state.menu_nav.manage_action_index == 2,
                        self.state.manage.remove_confirmation_armed,
                        palette,
                    ),
                );
                if self.state.menu_nav.focus_region == MenuFocusRegion::ManageActions
                    && self.state.menu_nav.manage_action_index == 2
                {
                    Self::paint_selection_glow(ui, remove_response.rect, 255, palette.accent, 0.78);
                }
                if remove_response.clicked() {
                    self.state.menu_nav.focus_region = MenuFocusRegion::ManageActions;
                    self.state.menu_nav.manage_action_index = 2;
                    self.start_remove_job();
                }
            });
        });
    }

    pub(crate) fn draw_manage_settings_panel(
        &mut self,
        ui: &mut egui::Ui,
        _palette: crate::theme::ThemePalette,
    ) {
        let selected_system = self.state.manage.settings_selected_system.clone();
        let palette = Self::palette_for_system(&selected_system);
        let panel_width = ui.available_width();
        self.panel_frame().fill(palette.panel).show(ui, |ui| {
            let inner_width = (panel_width - 24.0).max(0.0);
            ui.set_width(inner_width);
            ui.set_max_width(inner_width);
            let title = if selected_system == "ALL" {
                String::from("Cover Setup")
            } else {
                format!("{} Covers", runtime_system_display_name(&selected_system))
            };
            let subtitle = if selected_system == "ALL" {
                String::from("TheGamesDB defaults and global cover actions.")
            } else {
                self.cover_stats_summary_for_system(&selected_system)
            };
            self.draw_settings_system_chrome(ui, palette, &title, &subtitle);

            if selected_system == "ALL" {
                self.draw_cover_global_settings(ui, palette);
                ui.add_space(8.0);
            } else {
                draw_runtime_section_heading(ui, "Selected System", palette);
                ui.label(
                    egui::RichText::new(self.cover_stats_summary_for_system(&selected_system))
                        .small()
                        .color(palette.text_muted),
                );
                ui.add_space(8.0);
            }
            self.draw_cover_run_controls(ui, palette, &selected_system);
        });
    }

    fn draw_cover_global_settings(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
    ) {
        draw_runtime_section_heading(ui, "TheGamesDB", palette);
        ui.horizontal(|ui| {
            ui.label("API Key");
            ui.add_sized(
                [420.0, 30.0],
                egui::TextEdit::singleline(&mut self.state.manage.settings_api_key)
                    .password(!self.state.manage.settings_show_api_key),
            );
            if ui
                .add(manage_button(
                    if self.state.manage.settings_show_api_key {
                        "Hide"
                    } else {
                        "Show"
                    },
                    self.state.menu_nav.focus_region == MenuFocusRegion::SettingsCoverSettings
                        && self.state.menu_nav.settings_cover_action_index == 0,
                    false,
                    palette,
                ))
                .clicked()
            {
                self.state.menu_nav.focus_region = MenuFocusRegion::SettingsCoverSettings;
                self.state.menu_nav.settings_cover_action_index = 0;
                self.state.manage.settings_show_api_key = !self.state.manage.settings_show_api_key;
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Default Limit");
            ui.add_sized(
                [100.0, 28.0],
                egui::TextEdit::singleline(&mut self.state.manage.settings_limit),
            );
            ui.label("Default Delay (ms)");
            ui.add_sized(
                [100.0, 28.0],
                egui::TextEdit::singleline(&mut self.state.manage.settings_delay_ms),
            );
        });
        ui.label(egui::RichText::new("Platform IDs are fixed in-app.").small());
    }

    fn draw_cover_run_controls(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
        system: &str,
    ) {
        draw_runtime_section_heading(ui, "Actions", palette);
        ui.horizontal_wrapped(|ui| {
            ui.label("Limit");
            ui.add_sized(
                [100.0, 28.0],
                egui::TextEdit::singleline(&mut self.state.manage.scrape_limit),
            );
            ui.label("Delay (ms)");
            ui.add_sized(
                [100.0, 28.0],
                egui::TextEdit::singleline(&mut self.state.manage.scrape_delay_ms),
            );
        });
        ui.horizontal_wrapped(|ui| {
            let save_focused = self.state.menu_nav.focus_region
                == MenuFocusRegion::SettingsCoverSettings
                && self.state.menu_nav.settings_cover_action_index == 1;
            let save_response = ui.add_enabled(
                !self.state.manage.job_running,
                manage_button("Save Defaults", save_focused, false, palette),
            );
            if save_focused {
                Self::paint_selection_glow(ui, save_response.rect, 255, palette.accent, 0.78);
            }
            if save_response.clicked() {
                self.state.menu_nav.focus_region = MenuFocusRegion::SettingsCoverSettings;
                self.state.menu_nav.settings_cover_action_index = 1;
                self.save_manage_settings();
            }

            let scrape_response = ui.add_enabled(
                !self.state.manage.job_running,
                manage_button("Scrape Missing Covers", false, false, palette),
            );
            if scrape_response.clicked() {
                self.apply_settings_cover_scope(system);
                self.start_scrape_job();
            }
            let relink_response = ui.add_enabled(
                !self.state.manage.job_running,
                manage_button("Relink Local Covers", false, false, palette),
            );
            if relink_response.clicked() {
                self.apply_settings_cover_scope(system);
                self.start_relink_local_covers_job();
            }
        });
    }

    fn apply_settings_cover_scope(&mut self, system: &str) {
        self.set_manage_scrape_system_selected(system, true);
    }

    fn cover_stats_summary_for_system(&self, system: &str) -> String {
        let scope = if system == "ALL" {
            ManageScope::AllSystems
        } else {
            ManageScope::System(system.to_string())
        };
        match self.services.list_manage_roms(&scope) {
            Ok(rows) => {
                let missing = rows.iter().filter(|row| !row.has_cover).count();
                format!(
                    "{} tracked game(s), {} missing cover(s).",
                    rows.len(),
                    missing
                )
            }
            Err(_) => String::from("Cover status is unavailable until the library is refreshed."),
        }
    }

    fn draw_manage_scrape_panel(&mut self, ui: &mut egui::Ui, palette: crate::theme::ThemePalette) {
        self.panel_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("Game Cover Scraper");
            ui.add_space(6.0);
            let row_width = ui.available_width().max(1.0);
            let gap_x = runtime_setup_pill_gap(row_width);
            let pill_size = egui::vec2(runtime_setup_pill_width(row_width, gap_x), 27.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(gap_x, 0.0);
                for (index, system) in SYSTEM_FILTERS.iter().copied().enumerate() {
                    let mut selected = self.is_manage_scrape_system_selected(system);
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::ManageScrapeSystems
                        && self.state.menu_nav.manage_scrape_system_index == index;
                    let response =
                        self.settings_system_pill(ui, system, selected, focused, pill_size);
                    if response.clicked() {
                        self.state.menu_nav.focus_region = MenuFocusRegion::ManageScrapeSystems;
                        self.state.menu_nav.manage_scrape_system_index = index;
                        selected = true;
                        self.set_manage_scrape_system_selected(system, selected);
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Limit");
                ui.add_sized(
                    [100.0, 28.0],
                    egui::TextEdit::singleline(&mut self.state.manage.scrape_limit),
                );
                ui.label("Delay (ms)");
                ui.add_sized(
                    [100.0, 28.0],
                    egui::TextEdit::singleline(&mut self.state.manage.scrape_delay_ms),
                );
                let scrape_response = ui.add_enabled(
                    !self.state.manage.job_running,
                    manage_button(
                        "Scrape Missing Covers",
                        self.state.menu_nav.focus_region == MenuFocusRegion::ManageScrapeActions
                            && self.state.menu_nav.manage_scrape_action_index == 0,
                        false,
                        palette,
                    ),
                );
                if self.state.menu_nav.focus_region == MenuFocusRegion::ManageScrapeActions
                    && self.state.menu_nav.manage_scrape_action_index == 0
                {
                    Self::paint_selection_glow(ui, scrape_response.rect, 255, palette.accent, 0.78);
                }
                if scrape_response.clicked() {
                    self.state.menu_nav.focus_region = MenuFocusRegion::ManageScrapeActions;
                    self.state.menu_nav.manage_scrape_action_index = 0;
                    self.start_scrape_job();
                }
                let relink_response = ui.add_enabled(
                    !self.state.manage.job_running,
                    manage_button(
                        "Relink Local Covers",
                        self.state.menu_nav.focus_region == MenuFocusRegion::ManageScrapeActions
                            && self.state.menu_nav.manage_scrape_action_index == 1,
                        false,
                        palette,
                    ),
                );
                if self.state.menu_nav.focus_region == MenuFocusRegion::ManageScrapeActions
                    && self.state.menu_nav.manage_scrape_action_index == 1
                {
                    Self::paint_selection_glow(ui, relink_response.rect, 255, palette.accent, 0.78);
                }
                if relink_response.clicked() {
                    self.state.menu_nav.focus_region = MenuFocusRegion::ManageScrapeActions;
                    self.state.menu_nav.manage_scrape_action_index = 1;
                    self.start_relink_local_covers_job();
                }
            });
        });
    }

    fn draw_manage_inventory_panel(
        &mut self,
        ui: &mut egui::Ui,
        palette: crate::theme::ThemePalette,
    ) {
        self.panel_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let missing_files = self
                .state
                .manage
                .rows
                .iter()
                .filter(|row| !row.present_on_disk)
                .count();
            let missing_covers = self
                .state
                .manage
                .rows
                .iter()
                .filter(|row| !row.has_cover)
                .count();
            let selected_count = self.state.manage.selected_count();

            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Tracked ROMs: {}", self.state.manage.rows.len()));
                ui.label(format!("Selected: {selected_count}"));
                ui.label(format!("Missing files: {missing_files}"));
                ui.label(format!("Missing covers: {missing_covers}"));
                if self.state.manage.job_running {
                    if let Some(total) = self.state.manage.progress_total {
                        ui.label(format!(
                            "Progress: {}/{}",
                            self.state.manage.progress_processed, total
                        ));
                    }
                }
            });

            if self.state.manage.remove_confirmation_armed && selected_count > 0 {
                ui.label(
                    egui::RichText::new(format!(
                        "Remove is armed for {selected_count} selected ROMs. Activate remove again to confirm."
                    ))
                    .color(palette.accent),
                );
            }
            if !self.state.manage.status_message.is_empty() {
                ui.label(
                    egui::RichText::new(self.state.manage.status_message.clone())
                        .color(palette.text_muted),
                );
            }
            if let Some(summary) = self
                .state
                .manage
                .last_summary
                .as_ref()
                .filter(|summary| summary.message != self.state.manage.status_message)
            {
                ui.label(egui::RichText::new(summary.message.clone()).color(palette.text_muted));
            }
            ui.separator();
            for index in 0..self.state.manage.rows.len() {
                let row = self.state.manage.rows[index].clone();
                self.draw_manage_row(ui, index, &row, palette);
                ui.add_space(6.0);
            }
        });
    }

    fn draw_manage_row(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        row: &arcade_domain::ManageRomStatus,
        palette: crate::theme::ThemePalette,
    ) {
        let mut selected = self.state.manage.selected_rom_ids.contains(&row.rom_id);
        let row_focus = self.state.menu_nav.focus_region == MenuFocusRegion::ManageList
            && self.state.menu_nav.manage_list_index == index;
        let row_response = ui
            .vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.checkbox(&mut selected, "").changed() {
                        if selected {
                            self.state
                                .manage
                                .selected_rom_ids
                                .insert(row.rom_id.clone());
                        } else {
                            self.state.manage.selected_rom_ids.remove(&row.rom_id);
                        }
                        self.state.manage.remove_confirmation_armed = false;
                    }
                    ui.label(egui::RichText::new(&row.title).strong());
                    ui.label(
                        egui::RichText::new(&row.system)
                            .small()
                            .color(palette.text_muted),
                    );
                    ui.label(
                        egui::RichText::new(if row.present_on_disk {
                            "On disk"
                        } else {
                            "Missing file"
                        })
                        .small()
                        .color(if row.present_on_disk {
                            palette.text_muted
                        } else {
                            palette.accent
                        }),
                    );
                    ui.label(
                        egui::RichText::new(if row.has_cover { "Cover" } else { "No cover" })
                            .small()
                            .color(if row.has_cover {
                                palette.text_muted
                            } else {
                                palette.accent
                            }),
                    );
                });
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(compact_path(&row.managed_path, 72))
                        .monospace()
                        .small()
                        .color(palette.text_muted),
                );
            })
            .response;
        if row_focus {
            Self::paint_selection_glow(ui, row_response.rect, 12, palette.accent, 0.72);
            row_response.scroll_to_me(Some(egui::Align::Center));
        }
        if row_response.clicked() {
            self.state.menu_nav.focus_region = MenuFocusRegion::ManageList;
            self.state.menu_nav.manage_list_index = index;
        }
        ui.separator();
    }

    fn current_manage_scope(&self) -> ManageScope {
        if self.state.manage.scope == "ALL" {
            ManageScope::AllSystems
        } else {
            ManageScope::System(self.state.manage.scope.clone())
        }
    }

    pub(crate) fn save_manage_app_settings(&mut self) {
        let paths = match self.parse_manage_paths_config() {
            Ok(paths) => paths,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };

        let core_settings = self.state.manage.settings_core_values.clone();
        match self
            .services
            .update_app_config_settings(paths, core_settings)
        {
            Ok(outcome) => {
                if !outcome.restart_required {
                    self.sync_manage_settings_from_services();
                    self.assets = crate::assets::AssetCache::new(self.services.config().as_ref());
                    self.host
                        .update_core_variables(&self.services.config().emulation);
                }
                self.sync_settings_navigation_indices();
                self.state.manage.status_message = if outcome.restart_required {
                    String::from("Saved app configuration. Restart the app to apply path changes.")
                } else {
                    String::from("Saved app configuration.")
                };
                self.state.status = self.state.manage.status_message.clone();
                if !outcome.restart_required {
                    if let Err(err) = self.refresh_all() {
                        self.state.status = format!("Refresh warning: {err}");
                    }
                    self.refresh_manage_rows();
                }
            }
            Err(err) => {
                self.state.manage.status_message = format!("Failed to save app config: {err}");
                self.state.status = self.state.manage.status_message.clone();
            }
        }
    }

    pub(crate) fn save_manage_settings(&mut self) {
        let input = match self.parse_cover_scrape_settings_input() {
            Ok(input) => input,
            Err(err) => {
                self.state.manage.status_message = err;
                return;
            }
        };

        match self.services.update_cover_scrape_settings(input) {
            Ok(()) => {
                self.sync_manage_settings_from_services();
                self.sync_settings_navigation_indices();
                self.state.manage.status_message = String::from("Saved TheGamesDB settings.");
                self.state.status = self.state.manage.status_message.clone();
            }
            Err(err) => {
                self.state.manage.status_message = format!("Failed to save settings: {err}");
            }
        }
    }

    fn parse_manage_paths_config(&self) -> Result<PathsConfig, String> {
        Ok(PathsConfig {
            rom_root: parse_path_field(&self.state.manage.settings_rom_root, "ROM root")?,
            db_path: parse_path_field(&self.state.manage.settings_db_path, "DB path")?,
            save_state_root: parse_path_field(
                &self.state.manage.settings_save_state_root,
                "Save state root",
            )?,
            core_root: parse_path_field(&self.state.manage.settings_core_root, "Core root")?,
            bios_root: parse_path_field(&self.state.manage.settings_bios_root, "BIOS root")?,
        })
    }

    fn parse_cover_scrape_settings_input(&self) -> Result<CoverScrapeSettingsInput, String> {
        Ok(CoverScrapeSettingsInput {
            tgdb_api_key: if self.state.manage.settings_api_key.trim().is_empty() {
                None
            } else {
                Some(self.state.manage.settings_api_key.trim().to_string())
            },
            default_limit: parse_positive_u32(&self.state.manage.settings_limit, "Default limit")?,
            default_delay_ms: parse_u32(&self.state.manage.settings_delay_ms, "Default delay")?,
            nes_platform_ids: TGDB_PLATFORM_IDS_NES.to_vec(),
            snes_platform_ids: TGDB_PLATFORM_IDS_SNES.to_vec(),
            genesis_platform_ids: TGDB_PLATFORM_IDS_GENESIS.to_vec(),
            gb_platform_ids: TGDB_PLATFORM_IDS_GB.to_vec(),
            gba_platform_ids: TGDB_PLATFORM_IDS_GBA.to_vec(),
            n64_platform_ids: TGDB_PLATFORM_IDS_N64.to_vec(),
            arcade_platform_ids: TGDB_PLATFORM_IDS_ARCADE.to_vec(),
            psx_platform_ids: TGDB_PLATFORM_IDS_PSX.to_vec(),
            ps2_platform_ids: TGDB_PLATFORM_IDS_PS2.to_vec(),
            dreamcast_platform_ids: TGDB_PLATFORM_IDS_DREAMCAST.to_vec(),
            gamecube_platform_ids: TGDB_PLATFORM_IDS_GAMECUBE.to_vec(),
            saturn_platform_ids: TGDB_PLATFORM_IDS_SATURN.to_vec(),
            dos_platform_ids: TGDB_PLATFORM_IDS_DOS.to_vec(),
            pcecd_platform_ids: TGDB_PLATFORM_IDS_PCECD.to_vec(),
        })
    }

    fn build_scrape_run(&self) -> Result<CoverScrapeRunOptions, String> {
        if self.state.manage.scrape_systems.is_empty() {
            return Err(String::from("Select at least one system to scrape."));
        }

        Ok(CoverScrapeRunOptions {
            systems: self.state.manage.scrape_systems.iter().cloned().collect(),
            limit: parse_positive_usize(&self.state.manage.scrape_limit, "Scrape limit")?,
            delay_ms: parse_u64(&self.state.manage.scrape_delay_ms, "Scrape delay")?,
            missing_only: true,
        })
    }

    fn build_local_relink_run(&self) -> Result<LocalCoverRelinkRunOptions, String> {
        if self.state.manage.scrape_systems.is_empty() {
            return Err(String::from("Select at least one system to relink."));
        }

        Ok(LocalCoverRelinkRunOptions {
            systems: self.state.manage.scrape_systems.iter().cloned().collect(),
            missing_only: true,
        })
    }

    pub(crate) fn start_smart_scan_job(&mut self) {
        let Some(tx) = self.begin_manage_job("Running smart scan...") else {
            return;
        };
        let scope = self.current_manage_scope();
        let services = self.services.clone();
        std::thread::spawn(move || {
            let result = services.smart_scan_roms(&scope, |progress| {
                let _ = tx.send(ManageUiMessage::Progress(progress));
            });
            let _ = tx.send(ManageUiMessage::Finished(result));
        });
    }

    pub(crate) fn start_remove_job(&mut self) {
        let selected_count = self.state.manage.selected_count();
        if selected_count == 0 {
            self.state.manage.remove_confirmation_armed = false;
            self.state.manage.status_message = String::from("Select at least one ROM to remove.");
            self.state.status = self.state.manage.status_message.clone();
            return;
        }
        if !self.state.manage.remove_confirmation_armed {
            self.state.manage.remove_confirmation_armed = true;
            self.state.manage.status_message =
                format!("Press remove again to confirm removing {selected_count} selected ROMs.");
            self.state.status = self.state.manage.status_message.clone();
            return;
        }
        let Some(tx) = self.begin_manage_job("Removing selected ROMs from the library...") else {
            return;
        };
        let rom_ids = self
            .state
            .manage
            .selected_rom_ids
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let services = self.services.clone();
        std::thread::spawn(move || {
            let result = services.remove_roms_from_library(&rom_ids, |progress| {
                let _ = tx.send(ManageUiMessage::Progress(progress));
            });
            let _ = tx.send(ManageUiMessage::Finished(result));
        });
    }

    pub(crate) fn start_scrape_job(&mut self) {
        let run = match self.build_scrape_run() {
            Ok(run) => run,
            Err(err) => {
                self.state.manage.status_message = err;
                return;
            }
        };
        let Some(tx) = self.begin_manage_job("Scraping missing covers...") else {
            return;
        };
        let services = self.services.clone();
        std::thread::spawn(move || {
            let result = services.scrape_covers(run, |progress| {
                let _ = tx.send(ManageUiMessage::Progress(progress));
            });
            let _ = tx.send(ManageUiMessage::Finished(result));
        });
    }

    pub(crate) fn start_relink_local_covers_job(&mut self) {
        let run = match self.build_local_relink_run() {
            Ok(run) => run,
            Err(err) => {
                self.state.manage.status_message = err;
                return;
            }
        };
        let Some(tx) = self.begin_manage_job("Relinking local covers...") else {
            return;
        };
        let services = self.services.clone();
        std::thread::spawn(move || {
            let result = services.relink_local_covers(run, |progress| {
                let _ = tx.send(ManageUiMessage::Progress(progress));
            });
            let _ = tx.send(ManageUiMessage::Finished(result));
        });
    }

    pub(crate) fn refresh_runtime_setup_state(&mut self) {
        self.refresh_dependency_report();
        self.refresh_core_catalog();
    }

    pub(crate) fn refresh_dependency_report(&mut self) {
        match self.services.dependency_report() {
            Ok(report) => {
                self.state.manage.dependency_report = Some(report);
            }
            Err(err) => {
                self.state.manage.status_message = format!("Failed to scan dependencies: {err}");
                self.state.status = self.state.manage.status_message.clone();
            }
        }
    }

    pub(crate) fn refresh_core_catalog(&mut self) {
        match self.services.core_catalog() {
            Ok(catalog) => {
                self.state.manage.core_catalog = Some(catalog);
            }
            Err(err) => {
                self.state.manage.status_message = format!("Failed to scan core catalog: {err}");
                self.state.status = self.state.manage.status_message.clone();
            }
        }
    }

    pub(crate) fn runtime_setup_focus_count(&self, view: &DependencySetupView) -> usize {
        let selected = self.state.manage.settings_selected_system.as_str();
        let mut count = 4; // Rescan, safe setup, open ROM folder, scan games.
        if selected == "ALL" {
            if let Some(catalog) = self.state.manage.core_catalog.as_ref() {
                let remote_entries = catalog.remote_buildbot_entries();
                if !remote_entries.is_empty() {
                    count += 1; // Advanced Buildbot Browser toggle.
                    if self.state.manage.runtime_setup_core_browser_open {
                        count += remote_entries
                            .iter()
                            .filter(|entry| {
                                entry.install_policy.can_install_in_app()
                                    && entry.source.can_install_in_app()
                            })
                            .count();
                    }
                }
            }
        }
        if selected == "ALL" && self.runtime_setup_welcome_visible() {
            count += 1;
        }
        if selected != "ALL" {
            if let Some(catalog) = self.state.manage.core_catalog.as_ref() {
                if let Some(group) = catalog.system_group(selected) {
                    count += runtime_setup_catalog_group_action_count(
                        &group,
                        self.state.manage.runtime_setup_core_browser_open,
                    );
                }
            }
            if let Some(setup) = view.system_setup(selected) {
                count += setup
                    .automatic_actions
                    .iter()
                    .chain(setup.user_actions.iter())
                    .chain(setup.optional_upgrades.iter())
                    .map(runtime_setup_component_action_count)
                    .sum::<usize>();
                count += 1; // Advanced toggle.
                if self.state.manage.runtime_setup_advanced_open {
                    count += setup
                        .advanced_components
                        .iter()
                        .map(runtime_setup_component_action_count)
                        .sum::<usize>();
                }
            }
        }
        count
    }

    pub(crate) fn activate_runtime_setup_focus(&mut self) {
        let Some(report) = self.state.manage.dependency_report.clone() else {
            self.refresh_dependency_report();
            return;
        };
        let view = DependencySetupView::from_report(&report);
        let mut index = self.state.manage.runtime_setup_focus_index;
        let selected = self.state.manage.settings_selected_system.clone();

        if index == 0 {
            self.sync_manage_settings_from_services();
            return;
        }
        index -= 1;

        if index == 0 {
            self.start_safe_runtime_setup_job(selected.clone());
            return;
        }
        index -= 1;

        if index == 0 {
            self.open_runtime_rom_folder(&selected);
            return;
        }
        index -= 1;

        if index == 0 {
            self.start_runtime_scan_job(selected.clone());
            return;
        }
        index -= 1;

        if selected == "ALL" {
            if let Some(catalog) = self.state.manage.core_catalog.as_ref() {
                let remote_entries = catalog.remote_buildbot_entries();
                if !remote_entries.is_empty() {
                    if index == 0 {
                        self.state.manage.runtime_setup_core_browser_open =
                            !self.state.manage.runtime_setup_core_browser_open;
                        return;
                    }
                    index -= 1;
                    if self.state.manage.runtime_setup_core_browser_open {
                        for entry in &remote_entries {
                            if entry.install_policy.can_install_in_app()
                                && entry.source.can_install_in_app()
                            {
                                if index == 0 {
                                    self.start_catalog_core_install_job(entry.id.clone());
                                    return;
                                }
                                index -= 1;
                            }
                        }
                    }
                }
            }
        }

        if selected == "ALL" && self.runtime_setup_welcome_visible() {
            if index == 0 {
                self.complete_runtime_setup_welcome();
                return;
            }
            index -= 1;
        }

        if selected != "ALL" {
            if let Some(catalog) = self.state.manage.core_catalog.as_ref() {
                if let Some(group) = catalog.system_group(&selected) {
                    for entry in group
                        .recommended
                        .iter()
                        .chain(group.compatible.iter())
                        .chain(group.protected.iter())
                    {
                        if entry.install_policy.can_install_in_app()
                            && entry.source.can_install_in_app()
                        {
                            if index == 0 {
                                self.start_catalog_core_install_job(entry.id.clone());
                                return;
                            }
                            index -= 1;
                        }
                    }
                    if !group.advanced.is_empty() {
                        if index == 0 {
                            self.state.manage.runtime_setup_core_browser_open =
                                !self.state.manage.runtime_setup_core_browser_open;
                            return;
                        }
                        index -= 1;
                    }
                    if self.state.manage.runtime_setup_core_browser_open {
                        for entry in &group.advanced {
                            if entry.install_policy.can_install_in_app()
                                && entry.source.can_install_in_app()
                            {
                                if index == 0 {
                                    self.start_catalog_core_install_job(entry.id.clone());
                                    return;
                                }
                                index -= 1;
                            }
                        }
                    }
                }
            }
        }

        if let Some(setup) = view.system_setup(&selected) {
            for status in setup
                .automatic_actions
                .iter()
                .chain(setup.user_actions.iter())
                .chain(setup.optional_upgrades.iter())
            {
                if index == 0 {
                    self.open_dependency_target(&status.component.id);
                    return;
                }
                index -= 1;

                if runtime_setup_component_action_count(status) > 1 {
                    if index == 0 {
                        self.start_runtime_setup_component_action(status);
                        return;
                    }
                    index -= 1;
                }
            }

            if index == 0 {
                self.state.manage.runtime_setup_advanced_open =
                    !self.state.manage.runtime_setup_advanced_open;
                return;
            }
            index -= 1;

            if self.state.manage.runtime_setup_advanced_open {
                for status in &setup.advanced_components {
                    if index == 0 {
                        self.open_dependency_target(&status.component.id);
                        return;
                    }
                    index -= 1;

                    if runtime_setup_component_action_count(status) > 1 {
                        if index == 0 {
                            self.start_runtime_setup_component_action(status);
                            return;
                        }
                        index -= 1;
                    }
                }
            }
        }
    }

    fn runtime_setup_action_focused(&self, index: usize) -> bool {
        self.state.menu_nav.focus_region == MenuFocusRegion::SettingsRuntimeSetup
            && self.state.manage.runtime_setup_focus_index == index
    }

    fn runtime_setup_welcome_visible(&self) -> bool {
        self.state.runtime_setup_welcome_presented_this_session
            || !self.services.runtime_setup_welcome_completed()
    }

    fn open_dependency_target(&mut self, component_id: &str) {
        match self.services.open_dependency_target(component_id) {
            Ok(()) => {
                self.state.manage.status_message = String::from("Opened dependency folder.");
                self.state.status = self.state.manage.status_message.clone();
            }
            Err(err) => {
                self.state.manage.status_message =
                    format!("Failed to open dependency folder: {err}");
                self.state.status = self.state.manage.status_message.clone();
            }
        }
    }

    fn open_runtime_rom_folder(&mut self, system: &str) {
        let target = (system != "ALL").then_some(system);
        match self.services.open_rom_folder(target) {
            Ok(()) => {
                self.state.manage.status_message = String::from("Opened ROM folder.");
                self.state.status = self.state.manage.status_message.clone();
            }
            Err(err) => {
                self.state.manage.status_message = format!("Failed to open ROM folder: {err}");
                self.state.status = self.state.manage.status_message.clone();
            }
        }
    }

    fn complete_runtime_setup_welcome(&mut self) {
        match self.services.mark_runtime_setup_welcome_completed() {
            Ok(()) => {
                self.state.runtime_setup_welcome_presented_this_session = false;
                let system_changed = self.apply_system_filter_by_index(0);
                let alpha_changed = self.apply_alpha_filter_by_index(0);
                self.navigate_to_view(crate::app::AppView::Home);
                self.apply_current_view_filter_change(system_changed, alpha_changed, true);
                self.state.manage.status_message =
                    String::from("Runtime setup is available in Settings anytime.");
                self.state.status = self.state.manage.status_message.clone();
            }
            Err(err) => {
                self.state.manage.status_message =
                    format!("Failed to save runtime setup state: {err}");
                self.state.status = self.state.manage.status_message.clone();
            }
        }
    }

    fn start_runtime_setup_component_action(
        &mut self,
        status: &arcade_domain::DependencyComponentStatus,
    ) {
        match &status.component.source {
            DependencySource::LibretroBuildbot { .. }
            | DependencySource::Homebrew { .. }
            | DependencySource::UpstreamDownload { .. }
            | DependencySource::ExternalGuided { .. } => {
                self.start_dependency_install_job(status.component.id.clone(), None);
            }
            DependencySource::LocalImport => {
                self.start_dependency_import_job(&status.component);
            }
            DependencySource::UserProvided => {}
        }
    }

    fn start_dependency_import_job(&mut self, component: &arcade_domain::DependencyComponent) {
        let source = if component
            .target_path
            .extension()
            .and_then(|ext| ext.to_str())
            == Some("dylib")
        {
            rfd::FileDialog::new()
                .add_filter("Libretro Core", &["dylib"])
                .pick_file()
        } else {
            rfd::FileDialog::new().pick_folder()
        };

        let Some(source) = source else {
            self.state.manage.status_message = String::from("Import cancelled.");
            self.state.status = self.state.manage.status_message.clone();
            return;
        };

        self.start_dependency_install_job(component.id.clone(), Some(source));
    }

    fn start_safe_runtime_setup_job(&mut self, system: String) {
        let Some(report) = self.state.manage.dependency_report.clone() else {
            self.refresh_dependency_report();
            return;
        };
        let view = DependencySetupView::from_report(&report);
        let mut component_ids = if system == "ALL" {
            view.missing_required_standard_core_ids()
        } else {
            view.system_setup(&system)
                .map(|setup| {
                    setup
                        .automatic_actions
                        .iter()
                        .filter(|status| {
                            matches!(
                                status.component.source,
                                DependencySource::LibretroBuildbot { .. }
                            )
                        })
                        .map(|status| status.component.id.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        if let Some(catalog) = self.state.manage.core_catalog.as_ref() {
            component_ids.retain(|component_id| {
                let Some(status) = report
                    .components
                    .iter()
                    .find(|status| status.component.id == *component_id)
                else {
                    return false;
                };
                let Some(file_name) = status
                    .component
                    .target_path
                    .file_name()
                    .and_then(|name| name.to_str())
                else {
                    return true;
                };
                catalog.entries.iter().any(|entry| {
                    entry.expected_archive_member == file_name
                        && entry.install_policy.can_install_in_app()
                        && entry.source.can_install_in_app()
                })
            });
        }
        let scope = if system == "ALL" {
            ManageScope::AllSystems
        } else {
            ManageScope::System(system.clone())
        };

        let Some(tx) = self.begin_manage_job("Preparing runtime setup...") else {
            return;
        };
        let services = self.services.clone();
        std::thread::spawn(move || {
            let total = component_ids.len() + 2;
            let mut processed = 0usize;
            if let Err(err) = services.prepare_safe_runtime_setup() {
                let _ = tx.send(ManageUiMessage::Finished(Err(err)));
                return;
            }
            processed += 1;
            let _ = tx.send(ManageUiMessage::Progress(ManageProgressEvent {
                kind: ManageOperationKind::InstallDependency,
                processed,
                total: Some(total),
                message: String::from("Created runtime folders and applied safe defaults."),
            }));

            let mut updated = 0usize;
            for component_id in component_ids {
                let result = services.install_dependency(
                    DependencyInstallRequest {
                        component_id,
                        local_source: None,
                    },
                    |progress| {
                        let _ = tx.send(ManageUiMessage::Progress(ManageProgressEvent {
                            kind: ManageOperationKind::InstallDependency,
                            processed,
                            total: Some(total),
                            message: progress.message,
                        }));
                    },
                );
                match result {
                    Ok(_) => {
                        updated += 1;
                        processed += 1;
                    }
                    Err(err) => {
                        let _ = tx.send(ManageUiMessage::Finished(Err(err)));
                        return;
                    }
                }
            }

            let scan_result = services.smart_scan_roms(&scope, |progress| {
                let _ = tx.send(ManageUiMessage::Progress(ManageProgressEvent {
                    kind: ManageOperationKind::SmartScan,
                    processed,
                    total: Some(total),
                    message: progress.message,
                }));
            });
            match scan_result {
                Ok(scan_summary) => {
                    processed += 1;
                    let _ = tx.send(ManageUiMessage::Progress(ManageProgressEvent {
                        kind: ManageOperationKind::SmartScan,
                        processed,
                        total: Some(total),
                        message: String::from("Runtime setup finished."),
                    }));
                    let _ = tx.send(ManageUiMessage::Finished(Ok(ManageOperationSummary {
                        kind: Some(ManageOperationKind::InstallDependency),
                        updated: updated + scan_summary.updated,
                        skipped: scan_summary.skipped,
                        failed: scan_summary.failed,
                        message: format!(
                            "Runtime setup finished. Installed {updated} standard core(s); scanned {} game(s).",
                            scan_summary.updated
                        ),
                        ..ManageOperationSummary::default()
                    })));
                }
                Err(err) => {
                    let _ = tx.send(ManageUiMessage::Finished(Err(err)));
                }
            }
        });
    }

    fn start_runtime_scan_job(&mut self, system: String) {
        let scope = if system == "ALL" {
            ManageScope::AllSystems
        } else {
            ManageScope::System(system)
        };
        let Some(tx) = self.begin_manage_job("Scanning games...") else {
            return;
        };
        let services = self.services.clone();
        std::thread::spawn(move || {
            let result = services.smart_scan_roms(&scope, |progress| {
                let _ = tx.send(ManageUiMessage::Progress(progress));
            });
            let _ = tx.send(ManageUiMessage::Finished(result));
        });
    }

    fn start_dependency_install_job(
        &mut self,
        component_id: String,
        local_source: Option<PathBuf>,
    ) {
        let Some(tx) = self.begin_manage_job("Installing dependency...") else {
            return;
        };
        let services = self.services.clone();
        std::thread::spawn(move || {
            let request = DependencyInstallRequest {
                component_id,
                local_source,
            };
            let result = services.install_dependency(request, |progress| {
                let _ = tx.send(ManageUiMessage::Progress(progress));
            });
            let _ = tx.send(ManageUiMessage::Finished(result));
        });
    }

    fn start_catalog_core_install_job(&mut self, catalog_id: String) {
        let Some(tx) = self.begin_manage_job("Installing catalog core...") else {
            return;
        };
        let services = self.services.clone();
        std::thread::spawn(move || {
            let request = CatalogCoreInstallRequest { catalog_id };
            let result = services.install_catalog_core(request, |progress| {
                let _ = tx.send(ManageUiMessage::Progress(progress));
            });
            let _ = tx.send(ManageUiMessage::Finished(result));
        });
    }

    pub(crate) fn sync_manage_nav_state(&mut self) {
        self.state.menu_nav.manage_scope_index = SYSTEM_FILTERS
            .iter()
            .position(|value| *value == self.state.manage.scope)
            .unwrap_or(0);
        self.state.menu_nav.manage_action_index = self.state.menu_nav.manage_action_index.min(2);
        self.state.menu_nav.manage_settings_index =
            self.state.menu_nav.manage_settings_index.min(1);
        self.state.menu_nav.manage_scrape_system_index = self
            .state
            .menu_nav
            .manage_scrape_system_index
            .min(SYSTEM_FILTERS.len().saturating_sub(1));
        self.state.menu_nav.manage_scrape_action_index =
            self.state.menu_nav.manage_scrape_action_index.min(1);
        let row_len = self.state.manage.rows.len();
        self.state.menu_nav.manage_list_index = if row_len == 0 {
            0
        } else {
            self.state
                .menu_nav
                .manage_list_index
                .min(row_len.saturating_sub(1))
        };
        if self.state.manage.open
            && matches!(
                self.state.menu_nav.focus_region,
                MenuFocusRegion::FiltersToggle
                    | MenuFocusRegion::FiltersSystem
                    | MenuFocusRegion::FiltersAlpha
                    | MenuFocusRegion::Grid
            )
        {
            self.state.menu_nav.focus_region = MenuFocusRegion::ManageHeader;
        }
    }

    pub(crate) fn close_manage_view(&mut self) {
        self.state.manage.open = false;
        self.state.manage.status_message.clear();
        self.state.manage.remove_confirmation_armed = false;
        self.state.menu_nav.focus_region = MenuFocusRegion::FiltersToggle;
    }

    fn begin_manage_job(&mut self, status_message: &str) -> Option<mpsc::Sender<ManageUiMessage>> {
        if self.state.manage.job_running {
            self.state.manage.status_message =
                String::from("A manage task is already running. Wait for it to finish.");
            self.state.status = self.state.manage.status_message.clone();
            return None;
        }

        let (tx, rx) = mpsc::channel();
        self.manage_job_rx = Some(rx);
        self.state.manage.job_running = true;
        self.state.manage.progress_processed = 0;
        self.state.manage.progress_total = None;
        self.state.manage.last_summary = None;
        self.state.manage.remove_confirmation_armed = false;
        self.state.manage.status_message = status_message.to_string();
        self.state.status = self.state.manage.status_message.clone();
        Some(tx)
    }

    pub(crate) fn apply_manage_scope_index(&mut self, index: usize) {
        let index = index.min(SYSTEM_FILTERS.len().saturating_sub(1));
        self.state.manage.remove_confirmation_armed = false;
        self.state.manage.scope = SYSTEM_FILTERS[index].to_string();
        self.state.menu_nav.manage_scope_index = index;
        if !self.state.manage.job_running {
            self.refresh_manage_rows();
        }
    }

    pub(crate) fn toggle_manage_scrape_system_by_index(&mut self, index: usize) {
        let Some(system) = SYSTEM_FILTERS.get(index).copied() else {
            return;
        };

        self.set_manage_scrape_system_selected(system, true);
    }

    fn set_manage_scrape_system_selected(&mut self, system: &str, selected: bool) {
        if !selected {
            return;
        }

        if system == "ALL" {
            self.state.manage.scrape_systems = scrape_system_values()
                .into_iter()
                .map(String::from)
                .collect();
            return;
        }

        if scrape_system_values().contains(&system) {
            self.state.manage.scrape_systems.clear();
            self.state.manage.scrape_systems.insert(system.to_string());
        }
    }

    fn is_manage_scrape_system_selected(&self, system: &str) -> bool {
        if system == "ALL" {
            return self.is_manage_scrape_all_selected();
        }

        self.state.manage.scrape_systems.len() == 1
            && self.state.manage.scrape_systems.contains(system)
    }

    fn is_manage_scrape_all_selected(&self) -> bool {
        scrape_system_values()
            .iter()
            .all(|system| self.state.manage.scrape_systems.contains(*system))
    }

    pub(crate) fn toggle_manage_row_selection(&mut self, index: usize) {
        let Some(row) = self.state.manage.rows.get(index) else {
            return;
        };
        self.state.manage.remove_confirmation_armed = false;
        if self.state.manage.selected_rom_ids.contains(&row.rom_id) {
            self.state.manage.selected_rom_ids.remove(&row.rom_id);
        } else {
            self.state
                .manage
                .selected_rom_ids
                .insert(row.rom_id.clone());
        }
    }
}

fn manage_button(
    label: &str,
    focused: bool,
    selected: bool,
    palette: crate::theme::ThemePalette,
) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(label.to_owned()).strong())
        .fill(if selected {
            palette.accent_soft
        } else if focused {
            egui::Color32::from_rgba_premultiplied(
                palette.accent.r(),
                palette.accent.g(),
                palette.accent.b(),
                26,
            )
        } else {
            palette.panel_alt
        })
        .stroke(egui::Stroke::new(
            if focused { 1.6 } else { 1.0 },
            if selected || focused {
                palette.accent
            } else {
                palette.border
            },
        ))
        .corner_radius(egui::CornerRadius::same(255))
}

fn draw_runtime_section_heading(
    ui: &mut egui::Ui,
    label: &str,
    palette: crate::theme::ThemePalette,
) {
    ui.label(
        egui::RichText::new(label)
            .small()
            .strong()
            .color(palette.text),
    );
    ui.add_space(3.0);
}

fn draw_status_chip(ui: &mut egui::Ui, label: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(label).small().color(color));
}

fn runtime_setup_lane_label() -> &'static str {
    match arcade_domain::runtime_arch() {
        "x86_64" => "Rosetta x86_64 Runtime",
        "arm64" | "aarch64" => "Apple Silicon Runtime",
        _ => "Runtime",
    }
}

pub(crate) fn runtime_system_display_name(system: &str) -> &'static str {
    match system {
        "ALL" => "All Systems",
        "NES" => "Nintendo Entertainment System",
        "SNES" => "Super Nintendo",
        "GENESIS" => "Genesis / Mega Drive",
        "GB" => "Game Boy",
        "GBA" => "Game Boy Advance",
        "N64" => "Nintendo 64",
        "ARCADE" => "Arcade",
        "PSX" => "PlayStation",
        "PS2" => "PlayStation 2",
        "DREAMCAST" => "Dreamcast",
        "GAMECUBE" => "GameCube",
        "SATURN" => "Saturn",
        "PCECD" => "PCE-CD",
        "DOS" => "DOS",
        _ => "System",
    }
}

fn runtime_system_summary(system: &str, view: &DependencySetupView) -> String {
    let Some(group) = view
        .system_groups
        .iter()
        .find(|group| group.system.eq_ignore_ascii_case(system))
    else {
        return String::from("No setup manifest entries for this system.");
    };
    match group.readiness {
        DependencySystemReadiness::Ready => String::from("Ready to play."),
        DependencySystemReadiness::Blocked => {
            format!(
                "{} required setup item(s) need your attention.",
                group.missing_required
            )
        }
        DependencySystemReadiness::ReadyWithOptionalUpgrades => {
            format!(
                "Ready to play. {} optional upgrade(s) available.",
                group.missing_optional
            )
        }
    }
}

fn runtime_setup_pill_gap(row_width: f32) -> f32 {
    if row_width >= 900.0 {
        5.0
    } else {
        3.0
    }
}

fn runtime_setup_pill_width(row_width: f32, gap_x: f32) -> f32 {
    let count = SYSTEM_FILTERS.len() as f32;
    ((row_width - gap_x * (count - 1.0)) / count).max(1.0)
}

fn runtime_readiness_color(
    readiness: DependencySystemReadiness,
    palette: crate::theme::ThemePalette,
) -> egui::Color32 {
    match readiness {
        DependencySystemReadiness::Ready => palette.text_muted,
        DependencySystemReadiness::Blocked => palette.accent,
        DependencySystemReadiness::ReadyWithOptionalUpgrades => palette.text_muted,
    }
}

fn catalog_install_state_label(state: &CoreCatalogInstallState) -> &'static str {
    match state {
        CoreCatalogInstallState::Installed { .. } => "Installed",
        CoreCatalogInstallState::Missing { .. } => "Missing",
        CoreCatalogInstallState::Empty { .. } => "Needs repair",
    }
}

fn catalog_install_state_color(
    state: &CoreCatalogInstallState,
    palette: crate::theme::ThemePalette,
) -> egui::Color32 {
    match state {
        CoreCatalogInstallState::Installed { .. } => palette.text_muted,
        CoreCatalogInstallState::Missing { .. } | CoreCatalogInstallState::Empty { .. } => {
            palette.accent
        }
    }
}

fn catalog_compatibility_color(
    compatibility: CoreCatalogCompatibility,
    palette: crate::theme::ThemePalette,
) -> egui::Color32 {
    match compatibility {
        CoreCatalogCompatibility::Recommended => palette.accent,
        CoreCatalogCompatibility::Protected | CoreCatalogCompatibility::Advanced => palette.accent,
        CoreCatalogCompatibility::Compatible | CoreCatalogCompatibility::Optional => {
            palette.text_muted
        }
    }
}

fn paint_rect_gradient(ui: &egui::Ui, rect: egui::Rect, top: egui::Color32, bottom: egui::Color32) {
    let mut mesh = egui::epaint::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    ui.painter()
        .with_clip_rect(rect)
        .add(egui::Shape::mesh(mesh));
}

fn blend_runtime_color(from: egui::Color32, to: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let lerp = |a: u8, b: u8| -> u8 {
        (a as f32 + (b as f32 - a as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    egui::Color32::from_rgba_premultiplied(
        lerp(from.r(), to.r()),
        lerp(from.g(), to.g()),
        lerp(from.b(), to.b()),
        lerp(from.a(), to.a()),
    )
}

fn runtime_setup_component_action_count(
    status: &arcade_domain::DependencyComponentStatus,
) -> usize {
    let mut count = 1; // Open Folder.
    if !matches!(status.component.source, DependencySource::UserProvided) {
        count += 1;
    }
    count
}

fn runtime_setup_catalog_group_action_count(
    group: &arcade_domain::CoreCatalogSystemGroup,
    advanced_open: bool,
) -> usize {
    let mut count = group
        .recommended
        .iter()
        .chain(group.compatible.iter())
        .chain(group.protected.iter())
        .filter(|entry| {
            entry.install_policy.can_install_in_app() && entry.source.can_install_in_app()
        })
        .count();
    if !group.advanced.is_empty() {
        count += 1; // Advanced Core Browser toggle.
        if advanced_open {
            count += group
                .advanced
                .iter()
                .filter(|entry| {
                    entry.install_policy.can_install_in_app() && entry.source.can_install_in_app()
                })
                .count();
        }
    }
    count
}

fn scrape_system_values() -> Vec<&'static str> {
    SYSTEM_FILTERS
        .iter()
        .copied()
        .filter(|system| *system != "ALL")
        .collect()
}

#[derive(Clone, Copy)]
enum PathPickerKind {
    Directory,
    File,
}

fn draw_path_row(ui: &mut egui::Ui, label: &str, value: &mut String, picker_kind: PathPickerKind) {
    ui.horizontal(|ui| {
        let label_width = 120.0;
        let browse_width = 88.0;
        let spacing = ui.spacing().item_spacing.x;

        ui.add_sized([label_width, 28.0], egui::Label::new(label));
        let field_width = (ui.available_width() - browse_width - spacing).max(160.0);
        ui.add_sized([field_width, 28.0], egui::TextEdit::singleline(value));
        let browse = ui.add_sized([browse_width, 28.0], egui::Button::new("Browse..."));
        if browse.clicked() {
            if let Some(path) = pick_path_with_dialog(value, picker_kind) {
                *value = path.display().to_string();
            }
        }
    });
}

fn pick_path_with_dialog(current_value: &str, picker_kind: PathPickerKind) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new();
    let current = current_value.trim();
    if !current.is_empty() {
        let current_path = PathBuf::from(current);
        let initial_dir = initial_dialog_dir(&current_path, picker_kind);
        if let Some(dir) = initial_dir {
            dialog = dialog.set_directory(dir);
        }
        if matches!(picker_kind, PathPickerKind::File) {
            if let Some(name) = current_path.file_name().and_then(|name| name.to_str()) {
                dialog = dialog.set_file_name(name);
            }
        }
    }

    match picker_kind {
        PathPickerKind::Directory => dialog.pick_folder(),
        // DB path may not exist yet; save-file mode supports choosing or creating one.
        PathPickerKind::File => dialog.save_file(),
    }
}

fn initial_dialog_dir(current_path: &Path, picker_kind: PathPickerKind) -> Option<PathBuf> {
    match picker_kind {
        PathPickerKind::Directory => {
            if current_path.is_dir() {
                Some(current_path.to_path_buf())
            } else {
                current_path.parent().map(Path::to_path_buf)
            }
        }
        PathPickerKind::File => current_path.parent().map(Path::to_path_buf),
    }
}

fn compact_path(value: &str, max_chars: usize) -> String {
    let length = value.chars().count();
    if length <= max_chars || max_chars <= 10 {
        return value.to_string();
    }

    let left_len = max_chars / 2;
    let right_len = max_chars.saturating_sub(left_len + 3);
    let left = value.chars().take(left_len).collect::<String>();
    let right = value
        .chars()
        .rev()
        .take(right_len)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("{left}...{right}")
}

fn parse_path_field(value: &str, label: &str) -> Result<PathBuf, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{label} cannot be empty."))
    } else {
        Ok(PathBuf::from(trimmed))
    }
}

fn parse_positive_u32(value: &str, label: &str) -> Result<u32, String> {
    let parsed = parse_u32(value, label)?;
    if parsed == 0 {
        Err(format!("{label} must be greater than zero."))
    } else {
        Ok(parsed)
    }
}

fn parse_u32(value: &str, label: &str) -> Result<u32, String> {
    value
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("{label} must be a valid number."))
}

fn parse_positive_usize(value: &str, label: &str) -> Result<usize, String> {
    let parsed = value
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("{label} must be a valid number."))?;
    if parsed == 0 {
        Err(format!("{label} must be greater than zero."))
    } else {
        Ok(parsed)
    }
}

fn parse_u64(value: &str, label: &str) -> Result<u64, String> {
    value
        .trim()
        .parse::<u64>()
        .map_err(|_| format!("{label} must be a valid number."))
}
