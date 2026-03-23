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
            status_message: String::new(),
            remove_confirmation_armed: false,
            job_running: false,
            progress_processed: 0,
            progress_total: None,
            last_summary: None,
        }
    }
}
