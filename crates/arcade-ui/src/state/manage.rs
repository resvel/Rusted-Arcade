use std::collections::BTreeSet;

use arcade_domain::{ManageOperationSummary, ManageRomStatus};

pub(crate) struct ManageState {
    pub(crate) open: bool,
    pub(crate) scope: String,
    pub(crate) rows: Vec<ManageRomStatus>,
    pub(crate) selected_rom_ids: BTreeSet<String>,
    pub(crate) settings_api_key: String,
    pub(crate) settings_show_api_key: bool,
    pub(crate) settings_limit: String,
    pub(crate) settings_delay_ms: String,
    pub(crate) scrape_systems: BTreeSet<String>,
    pub(crate) scrape_limit: String,
    pub(crate) scrape_delay_ms: String,
    pub(crate) settings_rom_root: String,
    pub(crate) settings_db_path: String,
    pub(crate) settings_save_state_root: String,
    pub(crate) settings_core_root: String,
    pub(crate) settings_bios_root: String,
    pub(crate) settings_n64_preferred_core: String,
    pub(crate) settings_n64_parallel_rdp_upscaling: String,
    pub(crate) settings_n64_parallel_profile: String,
    pub(crate) settings_n64_parallel_rdp_synchronous: String,
    pub(crate) settings_n64_parallel_rdp_super_sampled_read_back: String,
    pub(crate) settings_n64_parallel_rdp_vi_aa: String,
    pub(crate) settings_n64_parallel_rdp_vi_bilinear: String,
    pub(crate) settings_n64_parallel_rdp_dither_filter: String,
    pub(crate) settings_n64_parallel_rdp_divot_filter: String,
    pub(crate) settings_n64_parallel_rdp_gamma_dither: String,
    pub(crate) settings_n64_count_per_op: String,
    pub(crate) settings_n64_fb_emulation: String,
    pub(crate) settings_n64_copy_color_to_rdram: String,
    pub(crate) settings_n64_frame_duplication: String,
    pub(crate) settings_n64_framerate: String,
    pub(crate) settings_n64_vi_refresh: String,
    pub(crate) settings_n64_count_per_op_denom_pot: String,
    pub(crate) settings_n64_aspect_ratio: String,
    pub(crate) status_message: String,
    pub(crate) remove_confirmation_armed: bool,
    pub(crate) job_running: bool,
    pub(crate) progress_processed: usize,
    pub(crate) progress_total: Option<usize>,
    pub(crate) last_summary: Option<ManageOperationSummary>,
}

impl ManageState {
    pub(crate) fn clear_selection(&mut self) {
        self.selected_rom_ids.clear();
        self.remove_confirmation_armed = false;
    }

    pub(crate) fn selected_count(&self) -> usize {
        self.selected_rom_ids.len()
    }
}

impl Default for ManageState {
    fn default() -> Self {
        let scrape_systems = ["NES", "SNES", "GENESIS", "GB", "GBA", "N64", "ARCADE"]
            .into_iter()
            .map(String::from)
            .collect();
        Self {
            open: false,
            scope: String::from("ALL"),
            rows: Vec::new(),
            selected_rom_ids: BTreeSet::new(),
            settings_api_key: String::new(),
            settings_show_api_key: false,
            settings_limit: String::from("200"),
            settings_delay_ms: String::from("150"),
            scrape_systems,
            scrape_limit: String::from("200"),
            scrape_delay_ms: String::from("150"),
            settings_rom_root: String::new(),
            settings_db_path: String::new(),
            settings_save_state_root: String::new(),
            settings_core_root: String::new(),
            settings_bios_root: String::new(),
            settings_n64_preferred_core: String::from("mupen64plus_next"),
            settings_n64_parallel_rdp_upscaling: String::from("1x"),
            settings_n64_parallel_profile: String::from("balanced"),
            settings_n64_parallel_rdp_synchronous: String::from("false"),
            settings_n64_parallel_rdp_super_sampled_read_back: String::from("false"),
            settings_n64_parallel_rdp_vi_aa: String::from("disabled"),
            settings_n64_parallel_rdp_vi_bilinear: String::from("disabled"),
            settings_n64_parallel_rdp_dither_filter: String::from("disabled"),
            settings_n64_parallel_rdp_divot_filter: String::from("disabled"),
            settings_n64_frame_duplication: String::from("False"),
            settings_n64_framerate: String::from("Original"),
            settings_n64_vi_refresh: String::from("Auto"),
            settings_n64_count_per_op_denom_pot: String::from("0"),
            settings_n64_aspect_ratio: String::from("4:3"),
            settings_n64_parallel_rdp_gamma_dither: String::from("disabled"),
            settings_n64_count_per_op: String::from("0"),
            settings_n64_fb_emulation: String::from("True"),
            settings_n64_copy_color_to_rdram: String::from("Async"),
            status_message: String::new(),
            remove_confirmation_armed: false,
            job_running: false,
            progress_processed: 0,
            progress_total: None,
            last_summary: None,
        }
    }
}
