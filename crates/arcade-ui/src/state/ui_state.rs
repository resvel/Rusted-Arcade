use std::collections::HashMap;

use arcade_domain::RomCard;

use crate::app::AppView;

use super::{
    ControllerMappingState, LibraryState, ManageState, MenuNavState, PlaySessionState,
    RomSelectionState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsScrollTarget {
    AppConfig,
    Dependencies,
    InputSettings,
    TheGamesDbConfig,
}

impl SettingsScrollTarget {
    pub(crate) const ALL: [Self; 4] = [
        Self::Dependencies,
        Self::AppConfig,
        Self::TheGamesDbConfig,
        Self::InputSettings,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Dependencies => "Runtime Setup",
            Self::AppConfig => "App Config",
            Self::TheGamesDbConfig => "Covers",
            Self::InputSettings => "Input",
        }
    }

    pub(crate) fn nav_index(self) -> usize {
        Self::ALL
            .iter()
            .position(|section| *section == self)
            .unwrap_or(0)
    }

    pub(crate) fn from_nav_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or(Self::Dependencies)
    }
}

#[derive(Default, Clone)]
pub(crate) struct ControllerInputButtonDebug {
    pub(crate) code: Option<u32>,
    pub(crate) gilrs_is_pressed: bool,
    pub(crate) is_pressed: bool,
    pub(crate) effective_is_pressed: bool,
    pub(crate) data_pressed: Option<bool>,
    pub(crate) data_value: Option<f32>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ControllerAssignmentSource {
    Assigned,
    UnassignedOverLimit,
    #[default]
    Unsupported,
}

impl ControllerAssignmentSource {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ControllerAssignmentSource::Assigned => "Assigned",
            ControllerAssignmentSource::UnassignedOverLimit => "Unassigned (Over 4)",
            ControllerAssignmentSource::Unsupported => "Unsupported",
        }
    }
}

#[derive(Default, Clone)]
pub(crate) struct ControllerInputDebugSnapshot {
    pub(crate) player_slot: Option<u8>,
    pub(crate) is_playable: bool,
    pub(crate) assignment_source: ControllerAssignmentSource,
    pub(crate) connect_seq: u64,
    pub(crate) name: String,
    pub(crate) vendor_id: Option<String>,
    pub(crate) product_id: Option<String>,
    pub(crate) mapping_name: Option<String>,
    pub(crate) runtime_system: Option<String>,
    pub(crate) runtime_mapping_key: Option<String>,
    pub(crate) runtime_mapping_source: Option<String>,
    pub(crate) dpad_up: bool,
    pub(crate) dpad_down: bool,
    pub(crate) dpad_left: bool,
    pub(crate) dpad_right: bool,
    pub(crate) south: bool,
    pub(crate) east: bool,
    pub(crate) north: bool,
    pub(crate) west: bool,
    pub(crate) left_shoulder: bool,
    pub(crate) right_shoulder: bool,
    pub(crate) left_trigger: f32,
    pub(crate) right_trigger: f32,
    pub(crate) select: bool,
    pub(crate) start: bool,
    pub(crate) left_thumb: bool,
    pub(crate) raw_dpad_x: f32,
    pub(crate) raw_dpad_y: f32,
    pub(crate) raw_left_x: f32,
    pub(crate) raw_left_y: f32,
    pub(crate) raw_right_x: f32,
    pub(crate) raw_right_y: f32,
    pub(crate) mapped_left_x: f32,
    pub(crate) mapped_left_y: f32,
    pub(crate) mapped_right_x: f32,
    pub(crate) mapped_right_y: f32,
    pub(crate) dpad_up_debug: ControllerInputButtonDebug,
    pub(crate) dpad_down_debug: ControllerInputButtonDebug,
    pub(crate) guide_debug: ControllerInputButtonDebug,
    pub(crate) right_thumb_debug: ControllerInputButtonDebug,
}

#[derive(Default)]
pub(crate) struct ControllerInputDebugState {
    pub(crate) open: bool,
    pub(crate) snapshots: Vec<ControllerInputDebugSnapshot>,
    pub(crate) connected_total: usize,
    pub(crate) assigned_playable_total: usize,
    pub(crate) unassigned_total: usize,
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
    pub(crate) controller_input_debug: ControllerInputDebugState,
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
            controller_input_debug: ControllerInputDebugState::default(),
            play: PlaySessionState::default(),
            settings_scroll_target: None,
        }
    }
}
