use std::collections::HashMap;

use arcade_domain::RomCard;

use crate::app::AppView;

use super::{
    ControllerMappingState, LibraryState, ManageState, MenuNavState, PlaySessionState,
    RomSelectionState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsScrollTarget {
    InputSettings,
    TheGamesDbConfig,
}

pub(crate) struct ArcadeUiState {
    pub(crate) current_view: AppView,
    pub(crate) rom_catalog: HashMap<String, RomCard>,
    pub(crate) selection: RomSelectionState,
    pub(crate) status: String,
    pub(crate) input_debug: String,
    pub(crate) library: LibraryState,
    pub(crate) manage: ManageState,
    pub(crate) menu_nav: MenuNavState,
    pub(crate) controller_mapping: ControllerMappingState,
    pub(crate) play: PlaySessionState,
    pub(crate) settings_scroll_target: Option<SettingsScrollTarget>,
}

impl ArcadeUiState {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

impl Default for ArcadeUiState {
    fn default() -> Self {
        Self {
            current_view: AppView::Home,
            rom_catalog: HashMap::new(),
            selection: RomSelectionState::default(),
            status: String::new(),
            input_debug: String::new(),
            library: LibraryState::default(),
            manage: ManageState::default(),
            menu_nav: MenuNavState::default(),
            controller_mapping: ControllerMappingState::default(),
            play: PlaySessionState::default(),
            settings_scroll_target: None,
        }
    }
}
