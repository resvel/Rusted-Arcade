use std::collections::{BTreeSet, HashMap};

use arcade_domain::{DependencyReport, ManageOperationSummary, ManageRomStatus};

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

    /// Currently selected core name in the settings panel.
    pub(crate) settings_selected_core: String,
    /// Per-core settings being edited: core_name → (variable_key → value).
    pub(crate) settings_core_values: HashMap<String, HashMap<String, String>>,

    pub(crate) status_message: String,
    pub(crate) remove_confirmation_armed: bool,
    pub(crate) job_running: bool,
    pub(crate) progress_processed: usize,
    pub(crate) progress_total: Option<usize>,
    pub(crate) last_summary: Option<ManageOperationSummary>,
    pub(crate) dependency_report: Option<DependencyReport>,
    pub(crate) runtime_setup_selected_system: String,
    pub(crate) runtime_setup_system_index: usize,
    pub(crate) runtime_setup_advanced_open: bool,
    pub(crate) runtime_setup_focus_index: usize,
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
        let scrape_systems = crate::theme::SYSTEM_FILTERS
            .iter()
            .copied()
            .filter(|system| *system != "ALL")
            .map(String::from)
            .collect();

        let default_core = arcade_domain::configurable_core_profiles()
            .first()
            .map(|p| p.core_name.to_string())
            .unwrap_or_default();

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
            settings_selected_core: default_core,
            settings_core_values: HashMap::new(),
            status_message: String::new(),
            remove_confirmation_armed: false,
            job_running: false,
            progress_processed: 0,
            progress_total: None,
            last_summary: None,
            dependency_report: None,
            runtime_setup_selected_system: String::from("ALL"),
            runtime_setup_system_index: 0,
            runtime_setup_advanced_open: false,
            runtime_setup_focus_index: 0,
        }
    }
}
