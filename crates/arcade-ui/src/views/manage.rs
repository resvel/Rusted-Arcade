use std::path::PathBuf;
use std::sync::mpsc;

use arcade_domain::{
    CoverScrapeRunOptions, CoverScrapeSettingsInput, ManageOperationKind, ManageScope,
    N64CopyColorToRdram, N64CountPerOp, N64FbEmulation, N64ParallelProfile,
    N64ParallelRdpFilter, N64ParallelRdpToggle, N64ParallelRdpUpscaling,
    N64PreferredCore, N64FrameDuplication, N64Framerate, N64ViRefresh,
    N64AspectRatio, N64CountPerOpDenomPot, PathsConfig,
};
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
        #[cfg(target_os = "macos")]
        {
            if arcade_domain::is_running_under_rosetta() {
                self.state.manage.settings_n64_preferred_core = config
                    .emulation
                    .n64
                    .preferred_core
                    .as_core_name()
                    .to_string();
            } else {
                self.state.manage.settings_n64_preferred_core =
                    String::from("mupen64plus_next");
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.state.manage.settings_n64_preferred_core = config
                .emulation
                .n64
                .preferred_core
                .as_core_name()
                .to_string();
        }
        self.state.manage.settings_n64_parallel_rdp_upscaling = config
            .emulation
            .n64
            .parallel_rdp_upscaling
            .as_core_value()
            .to_string();
        self.state.manage.settings_n64_parallel_profile = config
            .emulation
            .n64
            .parallel_profile
            .as_config_value()
            .to_string();
        self.state.manage.settings_n64_parallel_rdp_synchronous = config
            .emulation
            .n64
            .parallel_rdp_synchronous
            .as_core_value()
            .to_string();
        self.state.manage.settings_n64_parallel_rdp_super_sampled_read_back = config
            .emulation
            .n64
            .parallel_rdp_super_sampled_read_back
            .as_core_value()
            .to_string();
        self.state.manage.settings_n64_parallel_rdp_vi_aa = config
            .emulation
            .n64
            .parallel_rdp_vi_aa
            .as_core_value()
            .to_string();
        self.state.manage.settings_n64_parallel_rdp_vi_bilinear = config
            .emulation
            .n64
            .parallel_rdp_vi_bilinear
            .as_core_value()
            .to_string();
        self.state.manage.settings_n64_parallel_rdp_dither_filter = config
            .emulation
            .n64
            .parallel_rdp_dither_filter
            .as_core_value()
            .to_string();
        self.state.manage.settings_n64_parallel_rdp_divot_filter = config
            .emulation
            .n64
            .parallel_rdp_divot_filter
            .as_core_value()
            .to_string();
        self.state.manage.settings_n64_parallel_rdp_gamma_dither = config
            .emulation
            .n64
            .parallel_rdp_gamma_dither
            .as_core_value()
            .to_string();
        self.state.manage.settings_n64_count_per_op = config
            .emulation
            .n64
            .count_per_op
            .as_core_value()
            .to_string();
        self.state.manage.settings_n64_fb_emulation = config
            .emulation
            .n64
            .fb_emulation
            .as_core_value()
            .to_string();
        self.state.manage.settings_n64_copy_color_to_rdram = match config
            .emulation
            .n64
            .copy_color_to_rdram
        {
            arcade_domain::N64CopyColorToRdram::Off => String::from("Off"),
            arcade_domain::N64CopyColorToRdram::Async => String::from("Async"),
            arcade_domain::N64CopyColorToRdram::Sync => String::from("Sync"),
        };
        self.state.manage.settings_n64_frame_duplication = config
            .emulation
            .n64
            .frame_duplication
            .as_core_value()
            .to_string();
        self.state.manage.settings_n64_framerate = config
            .emulation
            .n64
            .framerate
            .as_core_value()
            .to_string();
        self.state.manage.settings_n64_vi_refresh = config
            .emulation
            .n64
            .vi_refresh
            .as_core_value()
            .to_string();
        self.state.manage.settings_n64_count_per_op_denom_pot = config
            .emulation
            .n64
            .count_per_op_denom_pot
            .as_core_value()
            .to_string();
        self.state.manage.settings_n64_aspect_ratio = config
            .emulation
            .n64
            .aspect_ratio
            .as_core_value()
            .to_string();
        self.state.manage.settings_api_key = scrape.tgdb_api_key.clone().unwrap_or_default();
        self.state.manage.settings_limit = scrape.default_limit.to_string();
        self.state.manage.settings_delay_ms = scrape.default_delay_ms.to_string();
        self.state.manage.scrape_limit = scrape.default_limit.to_string();
        self.state.manage.scrape_delay_ms = scrape.default_delay_ms.to_string();
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
                    self.refresh_manage_rows();
                }
                Err(err) => {
                    self.state.manage.status_message = format!("Manage action failed: {err}");
                    self.state.status = self.state.manage.status_message.clone();
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
        palette: crate::theme::ThemePalette,
    ) {
        self.panel_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("App Configuration");
            ui.add_space(6.0);

            draw_path_row(ui, "ROM Root", &mut self.state.manage.settings_rom_root);
            draw_path_row(ui, "DB Path", &mut self.state.manage.settings_db_path);
            draw_path_row(
                ui,
                "Save State Root",
                &mut self.state.manage.settings_save_state_root,
            );
            draw_path_row(ui, "Core Root", &mut self.state.manage.settings_core_root);
            draw_path_row(ui, "BIOS Root", &mut self.state.manage.settings_bios_root);

            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("N64 Core: mupen64plus_next")
                    .small()
                    .color(palette.text_muted),
            );
            ui.label(
                egui::RichText::new("N64 Renderer: ParaLLEl RDP (Vulkan)")
                    .small()
                    .color(palette.text_muted),
            );

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("N64 Internal Resolution")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, value) in ["1x", "2x", "4x", "8x"].into_iter().enumerate() {
                    let selected = self.state.manage.settings_n64_parallel_rdp_upscaling == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigUpscaling
                        && self.state.menu_nav.settings_app_upscaling_index == index;
                    let response = ui.add(manage_button(value, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigUpscaling;
                        self.state.menu_nav.settings_app_upscaling_index = index;
                        self.state.manage.settings_n64_parallel_rdp_upscaling = value.to_string();
                    }
                }
            });

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Parallel Profile")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, value) in ["balanced", "performance"].into_iter().enumerate() {
                    let selected = self.state.manage.settings_n64_parallel_profile == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigParallelProfile
                        && self.state.menu_nav.settings_app_parallel_profile_index == index;
                    let response = ui.add(manage_button(value, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigParallelProfile;
                        self.state.menu_nav.settings_app_parallel_profile_index = index;
                        self.state.manage.settings_n64_parallel_profile = value.to_string();
                    }
                }
            });

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("ParaLLEl RDP Settings")
                    .small()
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(4.0);

            // Synchronous Rendering
            ui.label(
                egui::RichText::new("Synchronous Rendering")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, value) in ["false", "true"].into_iter().enumerate() {
                    let selected =
                        self.state.manage.settings_n64_parallel_rdp_synchronous == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigSynchronous
                        && self.state.menu_nav.settings_app_synchronous_index == index;
                    let response = ui.add(manage_button(value, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigSynchronous;
                        self.state.menu_nav.settings_app_synchronous_index = index;
                        self.state.manage.settings_n64_parallel_rdp_synchronous =
                            value.to_string();
                    }
                }
            });

            ui.add_space(4.0);
            // Super-Sampled Read-Back
            ui.label(
                egui::RichText::new("Super-Sampled Read-Back")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, value) in ["false", "true"].into_iter().enumerate() {
                    let selected =
                        self.state.manage.settings_n64_parallel_rdp_super_sampled_read_back
                            == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigSsReadBack
                        && self.state.menu_nav.settings_app_ss_read_back_index == index;
                    let response = ui.add(manage_button(value, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigSsReadBack;
                        self.state.menu_nav.settings_app_ss_read_back_index = index;
                        self.state
                            .manage
                            .settings_n64_parallel_rdp_super_sampled_read_back =
                            value.to_string();
                    }
                }
            });

            ui.add_space(4.0);
            // VI Anti-Aliasing
            ui.label(
                egui::RichText::new("VI Anti-Aliasing")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, value) in ["disabled", "enabled"].into_iter().enumerate() {
                    let selected =
                        self.state.manage.settings_n64_parallel_rdp_vi_aa == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigViAa
                        && self.state.menu_nav.settings_app_vi_aa_index == index;
                    let response = ui.add(manage_button(value, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigViAa;
                        self.state.menu_nav.settings_app_vi_aa_index = index;
                        self.state.manage.settings_n64_parallel_rdp_vi_aa = value.to_string();
                    }
                }
            });

            ui.add_space(4.0);
            // VI Bilinear Filtering
            ui.label(
                egui::RichText::new("VI Bilinear Filtering")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, value) in ["disabled", "enabled"].into_iter().enumerate() {
                    let selected =
                        self.state.manage.settings_n64_parallel_rdp_vi_bilinear == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigViBilinear
                        && self.state.menu_nav.settings_app_vi_bilinear_index == index;
                    let response = ui.add(manage_button(value, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigViBilinear;
                        self.state.menu_nav.settings_app_vi_bilinear_index = index;
                        self.state.manage.settings_n64_parallel_rdp_vi_bilinear =
                            value.to_string();
                    }
                }
            });

            ui.add_space(4.0);
            // Dither Filter
            ui.label(
                egui::RichText::new("Dither Filter")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, value) in ["disabled", "enabled"].into_iter().enumerate() {
                    let selected =
                        self.state.manage.settings_n64_parallel_rdp_dither_filter == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigDitherFilter
                        && self.state.menu_nav.settings_app_dither_filter_index == index;
                    let response = ui.add(manage_button(value, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigDitherFilter;
                        self.state.menu_nav.settings_app_dither_filter_index = index;
                        self.state.manage.settings_n64_parallel_rdp_dither_filter =
                            value.to_string();
                    }
                }
            });

            ui.add_space(4.0);
            // Divot Filter
            ui.label(
                egui::RichText::new("Divot Filter")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, value) in ["disabled", "enabled"].into_iter().enumerate() {
                    let selected =
                        self.state.manage.settings_n64_parallel_rdp_divot_filter == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigDivotFilter
                        && self.state.menu_nav.settings_app_divot_filter_index == index;
                    let response = ui.add(manage_button(value, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigDivotFilter;
                        self.state.menu_nav.settings_app_divot_filter_index = index;
                        self.state.manage.settings_n64_parallel_rdp_divot_filter =
                            value.to_string();
                    }
                }
            });

            ui.add_space(4.0);
            // Gamma Dither
            ui.label(
                egui::RichText::new("Gamma Dither")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, value) in ["disabled", "enabled"].into_iter().enumerate() {
                    let selected =
                        self.state.manage.settings_n64_parallel_rdp_gamma_dither == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigGammaDither
                        && self.state.menu_nav.settings_app_gamma_dither_index == index;
                    let response = ui.add(manage_button(value, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigGammaDither;
                        self.state.menu_nav.settings_app_gamma_dither_index = index;
                        self.state.manage.settings_n64_parallel_rdp_gamma_dither =
                            value.to_string();
                    }
                }
            });

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Performance Settings")
                    .small()
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(4.0);

            // Count Per Op
            ui.label(
                egui::RichText::new("Count Per Op (CPU timing hack)")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, (value, label)) in
                    [("0", "Auto"), ("1", "1"), ("2", "2"), ("3", "3")]
                        .into_iter()
                        .enumerate()
                {
                    let selected = self.state.manage.settings_n64_count_per_op == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigCountPerOp
                        && self.state.menu_nav.settings_app_count_per_op_index == index;
                    let response = ui.add(manage_button(label, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigCountPerOp;
                        self.state.menu_nav.settings_app_count_per_op_index = index;
                        self.state.manage.settings_n64_count_per_op = value.to_string();
                    }
                }
            });

            ui.add_space(4.0);
            // FB Emulation
            ui.label(
                egui::RichText::new("Framebuffer Emulation")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, value) in ["True", "False"].into_iter().enumerate() {
                    let selected = self.state.manage.settings_n64_fb_emulation == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigFbEmulation
                        && self.state.menu_nav.settings_app_fb_emulation_index == index;
                    let response = ui.add(manage_button(value, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigFbEmulation;
                        self.state.menu_nav.settings_app_fb_emulation_index = index;
                        self.state.manage.settings_n64_fb_emulation = value.to_string();
                    }
                }
            });

            ui.add_space(4.0);
            // Copy Color to RDRAM
            ui.label(
                egui::RichText::new("Copy Color to RDRAM")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, value) in ["Off", "Async", "Sync"].into_iter().enumerate() {
                    let selected = self.state.manage.settings_n64_copy_color_to_rdram == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigCopyColorToRdram
                        && self.state.menu_nav.settings_app_copy_color_to_rdram_index == index;
                    let response = ui.add(manage_button(value, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigCopyColorToRdram;
                        self.state.menu_nav.settings_app_copy_color_to_rdram_index = index;
                        self.state.manage.settings_n64_copy_color_to_rdram = value.to_string();
                    }
                }
            });

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Frame Pacing")
                    .small()
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(4.0);

            // Frame Duplication
            ui.label(
                egui::RichText::new("Frame Duplication")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, value) in ["False", "True"].into_iter().enumerate() {
                    let selected = self.state.manage.settings_n64_frame_duplication == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigFrameDuplication
                        && self.state.menu_nav.settings_app_frame_duplication_index == index;
                    let response = ui.add(manage_button(value, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigFrameDuplication;
                        self.state.menu_nav.settings_app_frame_duplication_index = index;
                        self.state.manage.settings_n64_frame_duplication = value.to_string();
                    }
                }
            });

            ui.add_space(4.0);
            // Framerate
            ui.label(
                egui::RichText::new("Framerate")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, value) in ["Original", "Fullspeed"].into_iter().enumerate() {
                    let selected = self.state.manage.settings_n64_framerate == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigFramerate
                        && self.state.menu_nav.settings_app_framerate_index == index;
                    let response = ui.add(manage_button(value, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigFramerate;
                        self.state.menu_nav.settings_app_framerate_index = index;
                        self.state.manage.settings_n64_framerate = value.to_string();
                    }
                }
            });

            ui.add_space(4.0);
            // VI Refresh (Overclock)
            ui.label(
                egui::RichText::new("VI Refresh (Overclock)")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, value) in ["Auto", "1500", "2200"].into_iter().enumerate() {
                    let selected = self.state.manage.settings_n64_vi_refresh == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigViRefresh
                        && self.state.menu_nav.settings_app_vi_refresh_index == index;
                    let response = ui.add(manage_button(value, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigViRefresh;
                        self.state.menu_nav.settings_app_vi_refresh_index = index;
                        self.state.manage.settings_n64_vi_refresh = value.to_string();
                    }
                }
            });

            ui.add_space(4.0);
            // Count Per Op Divider (Overclock)
            ui.label(
                egui::RichText::new("Count Per Op Divider (Overclock)")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, value) in ["0", "1", "2", "3", "4"].into_iter().enumerate() {
                    let selected = self.state.manage.settings_n64_count_per_op_denom_pot == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigCountPerOpDenomPot
                        && self.state.menu_nav.settings_app_count_per_op_denom_pot_index == index;
                    let response = ui.add(manage_button(value, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigCountPerOpDenomPot;
                        self.state.menu_nav.settings_app_count_per_op_denom_pot_index = index;
                        self.state.manage.settings_n64_count_per_op_denom_pot =
                            value.to_string();
                    }
                }
            });

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Display")
                    .small()
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(4.0);

            // Aspect Ratio
            ui.label(
                egui::RichText::new("Aspect Ratio")
                    .small()
                    .color(palette.text_muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (index, value) in ["4:3", "16:9", "16:9 adjusted"].into_iter().enumerate() {
                    let selected = self.state.manage.settings_n64_aspect_ratio == value;
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::SettingsAppConfigAspectRatio
                        && self.state.menu_nav.settings_app_aspect_ratio_index == index;
                    let response = ui.add(manage_button(value, focused, selected, palette));
                    if focused {
                        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, 0.78);
                    }
                    if response.clicked() {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigAspectRatio;
                        self.state.menu_nav.settings_app_aspect_ratio_index = index;
                        self.state.manage.settings_n64_aspect_ratio = value.to_string();
                    }
                }
            });

            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "Changing database or save-state storage paths is saved immediately, but a restart is recommended.",
                )
                .small()
                .color(palette.text_muted),
            );

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
        palette: crate::theme::ThemePalette,
    ) {
        self.panel_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("TheGamesDB Settings");
            ui.add_space(6.0);
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
                    self.state.manage.settings_show_api_key =
                        !self.state.manage.settings_show_api_key;
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
            let save_settings_response = ui.add_enabled(
                !self.state.manage.job_running,
                manage_button(
                    "Save Settings",
                    self.state.menu_nav.focus_region == MenuFocusRegion::SettingsCoverSettings
                        && self.state.menu_nav.settings_cover_action_index == 1,
                    false,
                    palette,
                ),
            );
            if self.state.menu_nav.focus_region == MenuFocusRegion::SettingsCoverSettings
                && self.state.menu_nav.settings_cover_action_index == 1
            {
                Self::paint_selection_glow(
                    ui,
                    save_settings_response.rect,
                    255,
                    palette.accent,
                    0.78,
                );
            }
            if save_settings_response.clicked() {
                self.state.menu_nav.focus_region = MenuFocusRegion::SettingsCoverSettings;
                self.state.menu_nav.settings_cover_action_index = 1;
                self.save_manage_settings();
            }
        });
    }

    fn draw_manage_scrape_panel(&mut self, ui: &mut egui::Ui, palette: crate::theme::ThemePalette) {
        self.panel_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("Game Cover Scraper");
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for (index, system) in SYSTEM_FILTERS.iter().copied().enumerate() {
                    let mut selected = self.is_manage_scrape_system_selected(system);
                    let label = if system == "ALL" { "All" } else { system };
                    let focused = self.state.menu_nav.focus_region
                        == MenuFocusRegion::ManageScrapeSystems
                        && self.state.menu_nav.manage_scrape_system_index == index;
                    let response = manage_toggle_chip(
                        ui,
                        label,
                        focused,
                        &mut selected,
                        palette,
                        egui::vec2(88.0, 34.0),
                    );
                    if response.clicked() {
                        self.state.menu_nav.focus_region = MenuFocusRegion::ManageScrapeSystems;
                        self.state.menu_nav.manage_scrape_system_index = index;
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
        #[cfg(target_os = "macos")]
        let preferred_core = if arcade_domain::is_running_under_rosetta() {
            match parse_n64_preferred_core(&self.state.manage.settings_n64_preferred_core) {
                Ok(value) => value,
                Err(err) => {
                    self.state.manage.status_message = err;
                    self.state.status = self.state.manage.status_message.clone();
                    return;
                }
            }
        } else {
            N64PreferredCore::Mupen64plusNext
        };
        #[cfg(not(target_os = "macos"))]
        let preferred_core =
            match parse_n64_preferred_core(&self.state.manage.settings_n64_preferred_core) {
                Ok(value) => value,
                Err(err) => {
                    self.state.manage.status_message = err;
                    self.state.status = self.state.manage.status_message.clone();
                    return;
                }
            };
        let parallel_rdp_upscaling = match parse_n64_parallel_rdp_upscaling(
            &self.state.manage.settings_n64_parallel_rdp_upscaling,
        ) {
            Ok(value) => value,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };
        let parallel_profile = match parse_n64_parallel_profile(
            &self.state.manage.settings_n64_parallel_profile,
        ) {
            Ok(value) => value,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };
        let parallel_rdp_synchronous = match parse_n64_parallel_rdp_toggle(
            &self.state.manage.settings_n64_parallel_rdp_synchronous,
        ) {
            Ok(value) => value,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };
        let parallel_rdp_super_sampled_read_back = match parse_n64_parallel_rdp_toggle(
            &self.state.manage.settings_n64_parallel_rdp_super_sampled_read_back,
        ) {
            Ok(value) => value,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };
        let parallel_rdp_vi_aa = match parse_n64_parallel_rdp_filter(
            &self.state.manage.settings_n64_parallel_rdp_vi_aa,
        ) {
            Ok(value) => value,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };
        let parallel_rdp_vi_bilinear = match parse_n64_parallel_rdp_filter(
            &self.state.manage.settings_n64_parallel_rdp_vi_bilinear,
        ) {
            Ok(value) => value,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };
        let parallel_rdp_dither_filter = match parse_n64_parallel_rdp_filter(
            &self.state.manage.settings_n64_parallel_rdp_dither_filter,
        ) {
            Ok(value) => value,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };
        let parallel_rdp_divot_filter = match parse_n64_parallel_rdp_filter(
            &self.state.manage.settings_n64_parallel_rdp_divot_filter,
        ) {
            Ok(value) => value,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };
        let parallel_rdp_gamma_dither = match parse_n64_parallel_rdp_filter(
            &self.state.manage.settings_n64_parallel_rdp_gamma_dither,
        ) {
            Ok(value) => value,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };

        let count_per_op = match parse_n64_count_per_op(
            &self.state.manage.settings_n64_count_per_op,
        ) {
            Ok(value) => value,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };
        let fb_emulation = match parse_n64_fb_emulation(
            &self.state.manage.settings_n64_fb_emulation,
        ) {
            Ok(value) => value,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };
        let copy_color_to_rdram = match parse_n64_copy_color_to_rdram(
            &self.state.manage.settings_n64_copy_color_to_rdram,
        ) {
            Ok(value) => value,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };
        let frame_duplication = match parse_n64_frame_duplication(
            &self.state.manage.settings_n64_frame_duplication,
        ) {
            Ok(value) => value,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };
        let framerate = match parse_n64_framerate(
            &self.state.manage.settings_n64_framerate,
        ) {
            Ok(value) => value,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };
        let vi_refresh = match parse_n64_vi_refresh(
            &self.state.manage.settings_n64_vi_refresh,
        ) {
            Ok(value) => value,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };
        let count_per_op_denom_pot = match parse_n64_count_per_op_denom_pot(
            &self.state.manage.settings_n64_count_per_op_denom_pot,
        ) {
            Ok(value) => value,
            Err(err) => {
                self.state.manage.status_message = err;
                self.state.status = self.state.manage.status_message.clone();
                return;
            }
        };

        let n64_config = arcade_domain::N64EmulationConfig {
            preferred_core,
            parallel_rdp_upscaling,
            parallel_profile,
            parallel_rdp_synchronous,
            parallel_rdp_super_sampled_read_back,
            parallel_rdp_vi_aa,
            parallel_rdp_vi_bilinear,
            parallel_rdp_dither_filter,
            parallel_rdp_divot_filter,
            parallel_rdp_gamma_dither,
            count_per_op,
            fb_emulation,
            copy_color_to_rdram,
            frame_duplication,
            framerate,
            vi_refresh,
            count_per_op_denom_pot,
            aspect_ratio: parse_n64_aspect_ratio(&self.state.manage.settings_n64_aspect_ratio).unwrap_or_default(),
        };

        match self.services.update_app_config_settings(
            paths,
            n64_config,
        ) {
            Ok(outcome) => {
                if !outcome.restart_required {
                    self.sync_manage_settings_from_services();
                    self.assets = crate::assets::AssetCache::new(
                        self.services.config().paths.rom_root.as_path(),
                    );
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
            self.state.menu_nav.manage_scrape_action_index.min(0);
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
                    | MenuFocusRegion::ControllerMappingToggle
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

fn manage_toggle_chip(
    ui: &mut egui::Ui,
    label: &str,
    focused: bool,
    selected: &mut bool,
    palette: crate::theme::ThemePalette,
    size: egui::Vec2,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        *selected = !*selected;
    }

    let fill = if *selected {
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
    };
    let stroke = egui::Stroke::new(
        if focused { 1.6 } else { 1.0 },
        if *selected || focused {
            palette.accent
        } else {
            palette.border
        },
    );

    ui.painter().rect(
        rect,
        egui::CornerRadius::same(255),
        fill,
        stroke,
        egui::StrokeKind::Middle,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::TextStyle::Button.resolve(ui.style()),
        palette.text,
    );

    if focused {
        NativeArcadeUiApp::paint_selection_glow(ui, rect, 255, palette.accent, 0.78);
    }

    response
}

fn scrape_system_values() -> [&'static str; 7] {
    ["NES", "SNES", "GENESIS", "GB", "GBA", "N64", "ARCADE"]
}

fn draw_path_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        let width = ui.available_width().max(280.0);
        ui.add_sized([width, 28.0], egui::TextEdit::singleline(value));
    });
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

fn parse_n64_parallel_rdp_upscaling(value: &str) -> Result<N64ParallelRdpUpscaling, String> {
    match value.trim() {
        "1x" => Ok(N64ParallelRdpUpscaling::X1),
        "2x" => Ok(N64ParallelRdpUpscaling::X2),
        "4x" => Ok(N64ParallelRdpUpscaling::X4),
        "8x" => Ok(N64ParallelRdpUpscaling::X8),
        _ => Err(String::from(
            "N64 internal resolution must be one of: 1x, 2x, 4x, 8x.",
        )),
    }
}

fn parse_n64_parallel_profile(value: &str) -> Result<N64ParallelProfile, String> {
    match value.trim() {
        "balanced" => Ok(N64ParallelProfile::Balanced),
        "performance" => Ok(N64ParallelProfile::Performance),
        _ => Err(String::from(
            "N64 parallel profile must be either balanced or performance.",
        )),
    }
}

fn parse_n64_parallel_rdp_toggle(value: &str) -> Result<N64ParallelRdpToggle, String> {
    match value.trim() {
        "false" => Ok(N64ParallelRdpToggle::False),
        "true" => Ok(N64ParallelRdpToggle::True),
        _ => Err(String::from("Value must be false or true.")),
    }
}

fn parse_n64_parallel_rdp_filter(value: &str) -> Result<N64ParallelRdpFilter, String> {
    match value.trim() {
        "disabled" => Ok(N64ParallelRdpFilter::Disabled),
        "enabled" => Ok(N64ParallelRdpFilter::Enabled),
        _ => Err(String::from("Value must be disabled or enabled.")),
    }
}

fn parse_n64_preferred_core(value: &str) -> Result<N64PreferredCore, String> {
    match value.trim() {
        "mupen64plus_next" => Ok(N64PreferredCore::Mupen64plusNext),
        "parallel_n64" => Ok(N64PreferredCore::ParallelN64),
        _ => Err(String::from(
            "N64 preferred core must be either mupen64plus_next or parallel_n64.",
        )),
    }
}

fn parse_n64_count_per_op(value: &str) -> Result<N64CountPerOp, String> {
    match value.trim() {
        "0" => Ok(N64CountPerOp::Auto),
        "1" => Ok(N64CountPerOp::One),
        "2" => Ok(N64CountPerOp::Two),
        "3" => Ok(N64CountPerOp::Three),
        _ => Err(String::from("Count Per Op must be one of: Auto, 1, 2, 3.")),
    }
}

fn parse_n64_fb_emulation(value: &str) -> Result<N64FbEmulation, String> {
    match value.trim() {
        "True" => Ok(N64FbEmulation::True),
        "False" => Ok(N64FbEmulation::False),
        _ => Err(String::from("FB Emulation must be True or False.")),
    }
}

fn parse_n64_copy_color_to_rdram(value: &str) -> Result<N64CopyColorToRdram, String> {
    match value.trim() {
        "Off" => Ok(N64CopyColorToRdram::Off),
        "Async" => Ok(N64CopyColorToRdram::Async),
        "Sync" => Ok(N64CopyColorToRdram::Sync),
        _ => Err(String::from(
            "Copy Color to RDRAM must be one of: Off, Async, Sync.",
        )),
    }
}

fn parse_n64_frame_duplication(value: &str) -> Result<N64FrameDuplication, String> {
    match value.trim() {
        "False" => Ok(N64FrameDuplication::False),
        "True" => Ok(N64FrameDuplication::True),
        _ => Err(String::from("Frame Duplication must be True or False.")),
    }
}

fn parse_n64_framerate(value: &str) -> Result<N64Framerate, String> {
    match value.trim() {
        "Original" => Ok(N64Framerate::Original),
        "Fullspeed" => Ok(N64Framerate::Fullspeed),
        _ => Err(String::from("Framerate must be one of: Original, Fullspeed.")),
    }
}

fn parse_n64_vi_refresh(value: &str) -> Result<N64ViRefresh, String> {
    match value.trim() {
        "Auto" => Ok(N64ViRefresh::Auto),
        "1500" => Ok(N64ViRefresh::V1500),
        "2200" => Ok(N64ViRefresh::V2200),
        _ => Err(String::from("VI Refresh must be one of: Auto, 1500, 2200.")),
    }
}

fn parse_n64_aspect_ratio(value: &str) -> Result<N64AspectRatio, String> {
    match value.trim() {
        "4:3" => Ok(N64AspectRatio::Ratio43),
        "16:9" => Ok(N64AspectRatio::Ratio169),
        "16:9 adjusted" => Ok(N64AspectRatio::Ratio169Adjusted),
        _ => Err(String::from("Aspect Ratio must be one of: 4:3, 16:9, 16:9 adjusted.")),
    }
}

fn parse_n64_count_per_op_denom_pot(value: &str) -> Result<N64CountPerOpDenomPot, String> {
    match value.trim() {
        "0" => Ok(N64CountPerOpDenomPot::Zero),
        "1" => Ok(N64CountPerOpDenomPot::One),
        "2" => Ok(N64CountPerOpDenomPot::Two),
        "3" => Ok(N64CountPerOpDenomPot::Three),
        "4" => Ok(N64CountPerOpDenomPot::Four),
        "5" => Ok(N64CountPerOpDenomPot::Five),
        "6" => Ok(N64CountPerOpDenomPot::Six),
        "7" => Ok(N64CountPerOpDenomPot::Seven),
        "8" => Ok(N64CountPerOpDenomPot::Eight),
        "9" => Ok(N64CountPerOpDenomPot::Nine),
        "10" => Ok(N64CountPerOpDenomPot::Ten),
        "11" => Ok(N64CountPerOpDenomPot::Eleven),
        _ => Err(String::from(
            "Count Per Op Divider must be between 0 and 11.",
        )),
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
