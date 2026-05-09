use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rom {
    pub id: String,
    pub system: String,
    pub slug: String,
    pub title: String,
    pub file_path: String,
    pub emulator_core: Option<String>,
    pub cover_path: Option<String>,
    pub preview_video_path: Option<String>,
    pub preview_poster_path: Option<String>,
    pub preview_duration_sec: Option<i64>,
    pub preview_updated_at: Option<DateTime<Utc>>,
    pub added_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RomCard {
    pub rom: Rom,
    pub display_title: String,
    pub release_year: Option<i64>,
    pub manufacturer: Option<String>,
    pub genre: Option<String>,
    pub is_favorite: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RomQuery {
    pub system: Option<String>,
    pub search: Option<String>,
    pub alpha: Option<String>,
    pub favorites_only: bool,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ManageScope {
    AllSystems,
    System(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoverScrapeSettingsInput {
    pub tgdb_api_key: Option<String>,
    pub default_limit: u32,
    pub default_delay_ms: u32,
    pub nes_platform_ids: Vec<u32>,
    pub snes_platform_ids: Vec<u32>,
    pub genesis_platform_ids: Vec<u32>,
    pub gb_platform_ids: Vec<u32>,
    pub gba_platform_ids: Vec<u32>,
    pub n64_platform_ids: Vec<u32>,
    pub arcade_platform_ids: Vec<u32>,
    pub psx_platform_ids: Vec<u32>,
    pub ps2_platform_ids: Vec<u32>,
    pub dreamcast_platform_ids: Vec<u32>,
    pub gamecube_platform_ids: Vec<u32>,
    pub saturn_platform_ids: Vec<u32>,
    pub dos_platform_ids: Vec<u32>,
    pub pcecd_platform_ids: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoverScrapeRunOptions {
    pub systems: Vec<String>,
    pub limit: usize,
    pub delay_ms: u64,
    pub missing_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalCoverRelinkRunOptions {
    pub systems: Vec<String>,
    pub missing_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ManageOperationKind {
    SmartScan,
    RemoveFromLibrary,
    ScrapeMissingCovers,
    RelinkLocalCovers,
    InstallDependency,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManageProgressEvent {
    pub kind: ManageOperationKind,
    pub processed: usize,
    pub total: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManageOperationSummary {
    pub kind: Option<ManageOperationKind>,
    pub created: usize,
    pub matched: usize,
    pub updated: usize,
    pub removed: usize,
    pub missing: usize,
    pub unchanged: usize,
    pub skipped: usize,
    pub failed: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManageRomStatus {
    pub rom_id: String,
    pub title: String,
    pub system: String,
    pub managed_path: String,
    pub present_on_disk: bool,
    pub has_cover: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveSlotSummary {
    pub slot: i32,
    pub has_state: bool,
    pub label: Option<String>,
    pub size_bytes: i64,
    pub updated_at: Option<DateTime<Utc>>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveLimits {
    pub slot_count: i32,
    pub slot_max_bytes: i64,
    pub profile_max_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveSlotData {
    pub slot: i32,
    pub label: Option<String>,
    pub size_bytes: i64,
    pub updated_at: DateTime<Utc>,
    pub version: String,
    pub bytes: Vec<u8>,
}

pub const SYSTEM_DEFAULT_MAPPING_KEY: &str = "__system__";
pub const QUICK_SAVE_ACTION: &str = "Quick Save";
pub const QUICK_LOAD_ACTION: &str = "Quick Load";
pub const NEXT_SAVE_SLOT_ACTION: &str = "Next Save Slot";
pub const EXIT_ACTION: &str = "Exit";
pub const RESET_ACTION: &str = "Reset";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControllerFamily {
    PlayStation,
    Xbox,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum CanonicalButton {
    South,
    East,
    North,
    West,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    Start,
    Select,
    LeftShoulder,
    RightShoulder,
    Guide,
    LeftThumb,
    RightThumb,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum CanonicalAxis {
    LeftStickX,
    LeftStickY,
    RightStickX,
    RightStickY,
    LeftTrigger,
    RightTrigger,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum MappingEntry {
    Button { button: CanonicalButton },
    Axis { axis: CanonicalAxis, direction: i8 },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredDeviceMeta {
    pub id: Option<String>,
    pub mapping: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredGamepadMapping {
    pub actions: BTreeMap<String, Option<MappingEntry>>,
    pub threshold: f32,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<String>,
    pub device: Option<StoredDeviceMeta>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedPadIdentity {
    pub device_key: String,
    pub name: String,
    pub vendor_id: Option<String>,
    pub product_id: Option<String>,
    pub mapping_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedGamepadMappingSummary {
    pub id: String,
    pub system: String,
    pub name: String,
    pub vendor_id: Option<String>,
    pub product_id: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

const NES_GAMEPAD_ACTIONS: [&str; 13] = [
    "Up",
    "Down",
    "Left",
    "Right",
    "A",
    "B",
    "Start",
    "Select",
    EXIT_ACTION,
    QUICK_SAVE_ACTION,
    QUICK_LOAD_ACTION,
    NEXT_SAVE_SLOT_ACTION,
    RESET_ACTION,
];

const SNES_GAMEPAD_ACTIONS: [&str; 17] = [
    "Up",
    "Down",
    "Left",
    "Right",
    "A",
    "B",
    "X",
    "Y",
    "L",
    "R",
    "Start",
    "Select",
    EXIT_ACTION,
    QUICK_SAVE_ACTION,
    QUICK_LOAD_ACTION,
    NEXT_SAVE_SLOT_ACTION,
    RESET_ACTION,
];

const GENESIS_GAMEPAD_ACTIONS: [&str; 13] = [
    "Up",
    "Down",
    "Left",
    "Right",
    "A",
    "B",
    "C",
    "Start",
    EXIT_ACTION,
    QUICK_SAVE_ACTION,
    QUICK_LOAD_ACTION,
    NEXT_SAVE_SLOT_ACTION,
    RESET_ACTION,
];

const GB_GAMEPAD_ACTIONS: [&str; 13] = [
    "Up",
    "Down",
    "Left",
    "Right",
    "A",
    "B",
    "Start",
    "Select",
    EXIT_ACTION,
    QUICK_SAVE_ACTION,
    QUICK_LOAD_ACTION,
    NEXT_SAVE_SLOT_ACTION,
    RESET_ACTION,
];

const GBA_GAMEPAD_ACTIONS: [&str; 15] = [
    "Up",
    "Down",
    "Left",
    "Right",
    "A",
    "B",
    "L",
    "R",
    "Start",
    "Select",
    EXIT_ACTION,
    QUICK_SAVE_ACTION,
    QUICK_LOAD_ACTION,
    NEXT_SAVE_SLOT_ACTION,
    RESET_ACTION,
];

const N64_GAMEPAD_ACTIONS: [&str; 23] = [
    "Up",
    "Down",
    "Left",
    "Right",
    "Stick Up",
    "Stick Down",
    "Stick Left",
    "Stick Right",
    "A",
    "B",
    "L",
    "R",
    "Z",
    "Start",
    "C-Up",
    "C-Down",
    "C-Left",
    "C-Right",
    EXIT_ACTION,
    QUICK_SAVE_ACTION,
    QUICK_LOAD_ACTION,
    NEXT_SAVE_SLOT_ACTION,
    RESET_ACTION,
];

const ARCADE_GAMEPAD_ACTIONS: [&str; 15] = [
    "Up",
    "Down",
    "Left",
    "Right",
    "A",
    "B",
    "C",
    "D",
    "Start",
    "Coin",
    EXIT_ACTION,
    QUICK_SAVE_ACTION,
    QUICK_LOAD_ACTION,
    NEXT_SAVE_SLOT_ACTION,
    RESET_ACTION,
];

const PSX_GAMEPAD_ACTIONS: [&str; 21] = [
    "Up",
    "Down",
    "Left",
    "Right",
    "Cross",
    "Circle",
    "Square",
    "Triangle",
    "L1",
    "R1",
    "L2",
    "R2",
    "L3",
    "R3",
    "Start",
    "Select",
    EXIT_ACTION,
    QUICK_SAVE_ACTION,
    QUICK_LOAD_ACTION,
    NEXT_SAVE_SLOT_ACTION,
    RESET_ACTION,
];

const PS2_GAMEPAD_ACTIONS: [&str; 29] = [
    "Up",
    "Down",
    "Left",
    "Right",
    "Cross",
    "Circle",
    "Square",
    "Triangle",
    "L1",
    "R1",
    "L2",
    "R2",
    "L3",
    "R3",
    "Left Stick Up",
    "Left Stick Down",
    "Left Stick Left",
    "Left Stick Right",
    "Right Stick Up",
    "Right Stick Down",
    "Right Stick Left",
    "Right Stick Right",
    "Start",
    "Select",
    EXIT_ACTION,
    QUICK_SAVE_ACTION,
    QUICK_LOAD_ACTION,
    NEXT_SAVE_SLOT_ACTION,
    RESET_ACTION,
];

const DREAMCAST_GAMEPAD_ACTIONS: [&str; 14] = [
    "Up",
    "Down",
    "Left",
    "Right",
    "A",
    "B",
    "X",
    "Y",
    "Start",
    EXIT_ACTION,
    QUICK_SAVE_ACTION,
    QUICK_LOAD_ACTION,
    NEXT_SAVE_SLOT_ACTION,
    RESET_ACTION,
];

const GAMECUBE_GAMEPAD_ACTIONS: [&str; 25] = [
    "Up",
    "Down",
    "Left",
    "Right",
    "A",
    "B",
    "X",
    "Y",
    "Z",
    "L",
    "R",
    "Start",
    "Main Stick Up",
    "Main Stick Down",
    "Main Stick Left",
    "Main Stick Right",
    "C Stick Up",
    "C Stick Down",
    "C Stick Left",
    "C Stick Right",
    EXIT_ACTION,
    QUICK_SAVE_ACTION,
    QUICK_LOAD_ACTION,
    NEXT_SAVE_SLOT_ACTION,
    RESET_ACTION,
];

const DOS_GAMEPAD_ACTIONS: [&str; 17] = [
    "Up",
    "Down",
    "Left",
    "Right",
    "A",
    "B",
    "X",
    "Y",
    "L",
    "R",
    "Start",
    "Select",
    EXIT_ACTION,
    QUICK_SAVE_ACTION,
    QUICK_LOAD_ACTION,
    NEXT_SAVE_SLOT_ACTION,
    RESET_ACTION,
];

const SATURN_GAMEPAD_ACTIONS: [&str; 18] = [
    "Up",
    "Down",
    "Left",
    "Right",
    "A",
    "B",
    "C",
    "X",
    "Y",
    "Z",
    "L",
    "R",
    "Start",
    EXIT_ACTION,
    QUICK_SAVE_ACTION,
    QUICK_LOAD_ACTION,
    NEXT_SAVE_SLOT_ACTION,
    RESET_ACTION,
];

const PCECD_GAMEPAD_ACTIONS: [&str; 13] = [
    "Up",
    "Down",
    "Left",
    "Right",
    "I",
    "II",
    "Run",
    "Select",
    EXIT_ACTION,
    QUICK_SAVE_ACTION,
    QUICK_LOAD_ACTION,
    NEXT_SAVE_SLOT_ACTION,
    RESET_ACTION,
];

pub fn supported_gamepad_actions(system: &str) -> &'static [&'static str] {
    match system.trim().to_ascii_uppercase().as_str() {
        "NES" => &NES_GAMEPAD_ACTIONS,
        "SNES" => &SNES_GAMEPAD_ACTIONS,
        "GENESIS" => &GENESIS_GAMEPAD_ACTIONS,
        "GB" => &GB_GAMEPAD_ACTIONS,
        "GBA" => &GBA_GAMEPAD_ACTIONS,
        "N64" => &N64_GAMEPAD_ACTIONS,
        "ARCADE" => &ARCADE_GAMEPAD_ACTIONS,
        "PSX" => &PSX_GAMEPAD_ACTIONS,
        "PS2" => &PS2_GAMEPAD_ACTIONS,
        "DREAMCAST" => &DREAMCAST_GAMEPAD_ACTIONS,
        "GAMECUBE" => &GAMECUBE_GAMEPAD_ACTIONS,
        "SATURN" => &SATURN_GAMEPAD_ACTIONS,
        "PCECD" => &PCECD_GAMEPAD_ACTIONS,
        "DOS" => &DOS_GAMEPAD_ACTIONS,
        _ => &NES_GAMEPAD_ACTIONS,
    }
}

pub fn default_gamepad_mapping_for_system(system: &str) -> StoredGamepadMapping {
    let system = system.trim().to_ascii_uppercase();
    let mut actions = base_direction_actions();

    match system.as_str() {
        "NES" | "GB" => {
            insert_button(&mut actions, "A", CanonicalButton::South);
            insert_button(&mut actions, "B", CanonicalButton::East);
            insert_button(&mut actions, "Start", CanonicalButton::Start);
            insert_button(&mut actions, "Select", CanonicalButton::Select);
        }
        "SNES" => {
            insert_button(&mut actions, "A", CanonicalButton::South);
            insert_button(&mut actions, "B", CanonicalButton::East);
            insert_button(&mut actions, "X", CanonicalButton::North);
            insert_button(&mut actions, "Y", CanonicalButton::West);
            insert_button(&mut actions, "L", CanonicalButton::LeftShoulder);
            insert_button(&mut actions, "R", CanonicalButton::RightShoulder);
            insert_button(&mut actions, "Start", CanonicalButton::Start);
            insert_button(&mut actions, "Select", CanonicalButton::Select);
        }
        "GENESIS" => {
            insert_button(&mut actions, "A", CanonicalButton::South);
            insert_button(&mut actions, "B", CanonicalButton::East);
            insert_button(&mut actions, "C", CanonicalButton::North);
            insert_button(&mut actions, "Start", CanonicalButton::Start);
        }
        "GBA" => {
            insert_button(&mut actions, "A", CanonicalButton::South);
            insert_button(&mut actions, "B", CanonicalButton::East);
            insert_button(&mut actions, "L", CanonicalButton::LeftShoulder);
            insert_button(&mut actions, "R", CanonicalButton::RightShoulder);
            insert_button(&mut actions, "Start", CanonicalButton::Start);
            insert_button(&mut actions, "Select", CanonicalButton::Select);
        }
        "N64" => {
            insert_unassigned(&mut actions, "Stick Up");
            insert_unassigned(&mut actions, "Stick Down");
            insert_unassigned(&mut actions, "Stick Left");
            insert_unassigned(&mut actions, "Stick Right");
            insert_button(&mut actions, "A", CanonicalButton::South);
            insert_button(&mut actions, "B", CanonicalButton::West);
            insert_button(&mut actions, "L", CanonicalButton::LeftShoulder);
            insert_button(&mut actions, "R", CanonicalButton::RightShoulder);
            insert_axis(&mut actions, "Z", CanonicalAxis::LeftTrigger, 1);
            insert_button(&mut actions, "Start", CanonicalButton::Start);
            insert_button(&mut actions, "C-Up", CanonicalButton::North);
            insert_axis(&mut actions, "C-Down", CanonicalAxis::RightStickY, 1);
            insert_axis(&mut actions, "C-Left", CanonicalAxis::RightStickX, -1);
            insert_button(&mut actions, "C-Right", CanonicalButton::East);
        }
        "ARCADE" => {
            insert_button(&mut actions, "A", CanonicalButton::South);
            insert_button(&mut actions, "B", CanonicalButton::East);
            insert_button(&mut actions, "C", CanonicalButton::North);
            insert_button(&mut actions, "D", CanonicalButton::West);
            insert_button(&mut actions, "Start", CanonicalButton::Start);
            insert_button(&mut actions, "Coin", CanonicalButton::Select);
        }
        "PSX" => {
            insert_button(&mut actions, "Cross", CanonicalButton::South);
            insert_button(&mut actions, "Circle", CanonicalButton::East);
            insert_button(&mut actions, "Square", CanonicalButton::West);
            insert_button(&mut actions, "Triangle", CanonicalButton::North);
            insert_button(&mut actions, "L1", CanonicalButton::LeftShoulder);
            insert_button(&mut actions, "R1", CanonicalButton::RightShoulder);
            insert_axis(&mut actions, "L2", CanonicalAxis::LeftTrigger, 1);
            insert_axis(&mut actions, "R2", CanonicalAxis::RightTrigger, 1);
            insert_button(&mut actions, "L3", CanonicalButton::LeftThumb);
            insert_button(&mut actions, "R3", CanonicalButton::RightThumb);
            insert_button(&mut actions, "Start", CanonicalButton::Start);
            insert_button(&mut actions, "Select", CanonicalButton::Select);
        }
        "PS2" => {
            insert_button(&mut actions, "Cross", CanonicalButton::South);
            insert_button(&mut actions, "Circle", CanonicalButton::East);
            insert_button(&mut actions, "Square", CanonicalButton::West);
            insert_button(&mut actions, "Triangle", CanonicalButton::North);
            insert_button(&mut actions, "L1", CanonicalButton::LeftShoulder);
            insert_button(&mut actions, "R1", CanonicalButton::RightShoulder);
            insert_axis(&mut actions, "L2", CanonicalAxis::LeftTrigger, 1);
            insert_axis(&mut actions, "R2", CanonicalAxis::RightTrigger, 1);
            insert_button(&mut actions, "L3", CanonicalButton::LeftThumb);
            insert_button(&mut actions, "R3", CanonicalButton::RightThumb);
            insert_axis(&mut actions, "Left Stick Up", CanonicalAxis::LeftStickY, -1);
            insert_axis(
                &mut actions,
                "Left Stick Down",
                CanonicalAxis::LeftStickY,
                1,
            );
            insert_axis(
                &mut actions,
                "Left Stick Left",
                CanonicalAxis::LeftStickX,
                -1,
            );
            insert_axis(
                &mut actions,
                "Left Stick Right",
                CanonicalAxis::LeftStickX,
                1,
            );
            insert_axis(
                &mut actions,
                "Right Stick Up",
                CanonicalAxis::RightStickY,
                -1,
            );
            insert_axis(
                &mut actions,
                "Right Stick Down",
                CanonicalAxis::RightStickY,
                1,
            );
            insert_axis(
                &mut actions,
                "Right Stick Left",
                CanonicalAxis::RightStickX,
                -1,
            );
            insert_axis(
                &mut actions,
                "Right Stick Right",
                CanonicalAxis::RightStickX,
                1,
            );
            insert_button(&mut actions, "Start", CanonicalButton::Start);
            insert_button(&mut actions, "Select", CanonicalButton::Select);
        }
        "DREAMCAST" => {
            insert_button(&mut actions, "A", CanonicalButton::South);
            insert_button(&mut actions, "B", CanonicalButton::East);
            insert_button(&mut actions, "X", CanonicalButton::West);
            insert_button(&mut actions, "Y", CanonicalButton::North);
            insert_button(&mut actions, "Start", CanonicalButton::Start);
        }
        "GAMECUBE" => {
            insert_button(&mut actions, "A", CanonicalButton::South);
            insert_button(&mut actions, "B", CanonicalButton::East);
            insert_button(&mut actions, "X", CanonicalButton::West);
            insert_button(&mut actions, "Y", CanonicalButton::North);
            insert_axis(&mut actions, "Z", CanonicalAxis::LeftTrigger, 1);
            insert_button(&mut actions, "L", CanonicalButton::LeftShoulder);
            insert_button(&mut actions, "R", CanonicalButton::RightShoulder);
            insert_button(&mut actions, "Start", CanonicalButton::Start);
            insert_axis(&mut actions, "Main Stick Up", CanonicalAxis::LeftStickY, -1);
            insert_axis(
                &mut actions,
                "Main Stick Down",
                CanonicalAxis::LeftStickY,
                1,
            );
            insert_axis(
                &mut actions,
                "Main Stick Left",
                CanonicalAxis::LeftStickX,
                -1,
            );
            insert_axis(
                &mut actions,
                "Main Stick Right",
                CanonicalAxis::LeftStickX,
                1,
            );
            insert_axis(&mut actions, "C Stick Up", CanonicalAxis::RightStickY, -1);
            insert_axis(&mut actions, "C Stick Down", CanonicalAxis::RightStickY, 1);
            insert_axis(&mut actions, "C Stick Left", CanonicalAxis::RightStickX, -1);
            insert_axis(&mut actions, "C Stick Right", CanonicalAxis::RightStickX, 1);
        }
        "SATURN" => {
            insert_button(&mut actions, "A", CanonicalButton::South);
            insert_button(&mut actions, "B", CanonicalButton::East);
            insert_axis(&mut actions, "C", CanonicalAxis::LeftTrigger, 1);
            insert_button(&mut actions, "X", CanonicalButton::West);
            insert_button(&mut actions, "Y", CanonicalButton::North);
            insert_axis(&mut actions, "Z", CanonicalAxis::RightTrigger, 1);
            insert_button(&mut actions, "L", CanonicalButton::LeftShoulder);
            insert_button(&mut actions, "R", CanonicalButton::RightShoulder);
            insert_button(&mut actions, "Start", CanonicalButton::Start);
        }
        "PCECD" => {
            insert_button(&mut actions, "I", CanonicalButton::South);
            insert_button(&mut actions, "II", CanonicalButton::East);
            insert_button(&mut actions, "Run", CanonicalButton::Start);
            insert_button(&mut actions, "Select", CanonicalButton::Select);
        }
        "DOS" => {
            insert_button(&mut actions, "A", CanonicalButton::South);
            insert_button(&mut actions, "B", CanonicalButton::East);
            insert_button(&mut actions, "X", CanonicalButton::West);
            insert_button(&mut actions, "Y", CanonicalButton::North);
            insert_button(&mut actions, "L", CanonicalButton::LeftShoulder);
            insert_button(&mut actions, "R", CanonicalButton::RightShoulder);
            insert_button(&mut actions, "Start", CanonicalButton::Start);
            insert_button(&mut actions, "Select", CanonicalButton::Select);
        }
        _ => {
            insert_button(&mut actions, "A", CanonicalButton::South);
            insert_button(&mut actions, "B", CanonicalButton::East);
            insert_button(&mut actions, "Start", CanonicalButton::Start);
            insert_button(&mut actions, "Select", CanonicalButton::Select);
        }
    }

    insert_button(&mut actions, EXIT_ACTION, CanonicalButton::RightShoulder);
    insert_unassigned(&mut actions, QUICK_SAVE_ACTION);
    insert_unassigned(&mut actions, QUICK_LOAD_ACTION);
    insert_unassigned(&mut actions, NEXT_SAVE_SLOT_ACTION);
    insert_button(&mut actions, RESET_ACTION, CanonicalButton::LeftShoulder);

    StoredGamepadMapping {
        actions,
        threshold: 0.6,
        updated_at: None,
        device: None,
    }
}

pub fn apply_device_preset_defaults(
    mapping: &mut StoredGamepadMapping,
    device: &DetectedPadIdentity,
) -> bool {
    let Some(family) = detect_controller_family(device) else {
        return false;
    };

    let mut changed = false;
    match family {
        ControllerFamily::PlayStation | ControllerFamily::Xbox => {
            changed |=
                fill_unassigned_button(mapping, QUICK_SAVE_ACTION, CanonicalButton::LeftThumb);
            changed |=
                fill_unassigned_button(mapping, QUICK_LOAD_ACTION, CanonicalButton::RightThumb);
            changed |=
                fill_unassigned_button(mapping, NEXT_SAVE_SLOT_ACTION, CanonicalButton::Guide);
        }
    }

    changed
}

pub fn device_preset_hint(device: &DetectedPadIdentity) -> Option<&'static str> {
    match detect_controller_family(device)? {
        ControllerFamily::PlayStation | ControllerFamily::Xbox => {
            Some(
                "Defaults: Left Shoulder reset, Right Shoulder exit, L3 quick save, R3 quick load, PS/Guide next save slot.",
            )
        }
    }
}

// Frontend controller-port limit for runtime autoconfiguration and input routing.
pub const MAX_GAMEPAD_PLAYERS: u8 = 4;

fn base_direction_actions() -> BTreeMap<String, Option<MappingEntry>> {
    let mut actions = BTreeMap::new();
    insert_button(&mut actions, "Up", CanonicalButton::DPadUp);
    insert_button(&mut actions, "Down", CanonicalButton::DPadDown);
    insert_button(&mut actions, "Left", CanonicalButton::DPadLeft);
    insert_button(&mut actions, "Right", CanonicalButton::DPadRight);
    actions
}

fn insert_button(
    actions: &mut BTreeMap<String, Option<MappingEntry>>,
    action: &str,
    button: CanonicalButton,
) {
    actions.insert(action.to_string(), Some(MappingEntry::Button { button }));
}

fn insert_unassigned(actions: &mut BTreeMap<String, Option<MappingEntry>>, action: &str) {
    actions.insert(action.to_string(), None);
}

fn fill_unassigned_button(
    mapping: &mut StoredGamepadMapping,
    action: &str,
    button: CanonicalButton,
) -> bool {
    match mapping.actions.get(action) {
        Some(Some(_)) => false,
        _ => {
            mapping
                .actions
                .insert(action.to_string(), Some(MappingEntry::Button { button }));
            true
        }
    }
}

fn detect_controller_family(device: &DetectedPadIdentity) -> Option<ControllerFamily> {
    match device.vendor_id.as_deref() {
        Some("054c") => return Some(ControllerFamily::PlayStation),
        Some("045e") => return Some(ControllerFamily::Xbox),
        _ => {}
    }

    let name = device.name.trim().to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }

    if name.contains("dualsense")
        || name.contains("dualshock")
        || name.contains("playstation")
        || name == "wireless controller"
    {
        return Some(ControllerFamily::PlayStation);
    }

    if name.contains("xbox") || name.contains("x-input") || name.contains("xinput") {
        return Some(ControllerFamily::Xbox);
    }

    None
}

fn insert_axis(
    actions: &mut BTreeMap<String, Option<MappingEntry>>,
    action: &str,
    axis: CanonicalAxis,
    direction: i8,
) {
    actions.insert(
        action.to_string(),
        Some(MappingEntry::Axis { axis, direction }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_mapping_roundtrips_as_json() {
        let mapping = default_gamepad_mapping_for_system("SNES");
        let json = serde_json::to_string(&mapping).expect("serialize");
        let parsed: StoredGamepadMapping = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, mapping);
    }

    #[test]
    fn supported_actions_include_arcade_coin() {
        assert!(supported_gamepad_actions("ARCADE").contains(&"Coin"));
    }

    #[test]
    fn supported_actions_include_frontend_shortcuts() {
        let actions = supported_gamepad_actions("SNES");
        assert!(actions.contains(&EXIT_ACTION));
        assert!(actions.contains(&QUICK_SAVE_ACTION));
        assert!(actions.contains(&QUICK_LOAD_ACTION));
        assert!(actions.contains(&NEXT_SAVE_SLOT_ACTION));
        assert!(actions.contains(&RESET_ACTION));
    }

    #[test]
    fn pcecd_actions_include_face_buttons_and_run_select() {
        let actions = supported_gamepad_actions("PCECD");
        assert!(actions.contains(&"I"));
        assert!(actions.contains(&"II"));
        assert!(actions.contains(&"Run"));
        assert!(actions.contains(&"Select"));
        assert!(actions.contains(&EXIT_ACTION));
    }

    #[test]
    fn saturn_actions_include_six_face_buttons_and_shoulders() {
        let actions = supported_gamepad_actions("SATURN");
        assert!(actions.contains(&"A"));
        assert!(actions.contains(&"B"));
        assert!(actions.contains(&"C"));
        assert!(actions.contains(&"X"));
        assert!(actions.contains(&"Y"));
        assert!(actions.contains(&"Z"));
        assert!(actions.contains(&"L"));
        assert!(actions.contains(&"R"));
        assert!(actions.contains(&"Start"));
        assert!(actions.contains(&EXIT_ACTION));
    }

    #[test]
    fn gamecube_actions_include_faces_shoulders_and_sticks() {
        let actions = supported_gamepad_actions("GAMECUBE");
        for action in [
            "A",
            "B",
            "X",
            "Y",
            "Z",
            "L",
            "R",
            "Start",
            "Main Stick Up",
            "C Stick Right",
            EXIT_ACTION,
        ] {
            assert!(actions.contains(&action));
        }
    }

    #[test]
    fn gamecube_default_mapping_assigns_faces_shoulders_and_sticks() {
        let mapping = default_gamepad_mapping_for_system("GAMECUBE");

        assert_eq!(
            mapping.actions.get("A"),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::South,
            }))
        );
        assert_eq!(
            mapping.actions.get("B"),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::East,
            }))
        );
        assert_eq!(
            mapping.actions.get("Z"),
            Some(&Some(MappingEntry::Axis {
                axis: CanonicalAxis::LeftTrigger,
                direction: 1,
            }))
        );
        assert_eq!(
            mapping.actions.get("Main Stick Up"),
            Some(&Some(MappingEntry::Axis {
                axis: CanonicalAxis::LeftStickY,
                direction: -1,
            }))
        );
        assert_eq!(
            mapping.actions.get("C Stick Right"),
            Some(&Some(MappingEntry::Axis {
                axis: CanonicalAxis::RightStickX,
                direction: 1,
            }))
        );
    }

    #[test]
    fn saturn_default_mapping_assigns_faces_shoulders_and_triggers() {
        let mapping = default_gamepad_mapping_for_system("SATURN");

        assert_eq!(
            mapping.actions.get("A"),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::South,
            }))
        );
        assert_eq!(
            mapping.actions.get("B"),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::East,
            }))
        );
        assert_eq!(
            mapping.actions.get("C"),
            Some(&Some(MappingEntry::Axis {
                axis: CanonicalAxis::LeftTrigger,
                direction: 1,
            }))
        );
        assert_eq!(
            mapping.actions.get("Z"),
            Some(&Some(MappingEntry::Axis {
                axis: CanonicalAxis::RightTrigger,
                direction: 1,
            }))
        );
        assert_eq!(
            mapping.actions.get("L"),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::LeftShoulder,
            }))
        );
        assert_eq!(
            mapping.actions.get("R"),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::RightShoulder,
            }))
        );
    }

    #[test]
    fn n64_supported_actions_include_control_stick_directions() {
        let actions = supported_gamepad_actions("N64");
        assert!(actions.contains(&"Stick Up"));
        assert!(actions.contains(&"Stick Down"));
        assert!(actions.contains(&"Stick Left"));
        assert!(actions.contains(&"Stick Right"));
    }

    #[test]
    fn default_mapping_assigns_exit_and_reset_but_leaves_other_shortcuts_unassigned() {
        let mapping = default_gamepad_mapping_for_system("NES");
        assert_eq!(
            mapping.actions.get(EXIT_ACTION),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::RightShoulder,
            }))
        );
        assert_eq!(mapping.actions.get(QUICK_SAVE_ACTION), Some(&None));
        assert_eq!(mapping.actions.get(QUICK_LOAD_ACTION), Some(&None));
        assert_eq!(mapping.actions.get(NEXT_SAVE_SLOT_ACTION), Some(&None));
        assert_eq!(
            mapping.actions.get(RESET_ACTION),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::LeftShoulder,
            }))
        );
    }

    #[test]
    fn n64_default_mapping_keeps_a_b_and_primary_c_buttons_in_physical_positions() {
        let mapping = default_gamepad_mapping_for_system("N64");

        assert_eq!(mapping.actions.get("Stick Up"), Some(&None));
        assert_eq!(mapping.actions.get("Stick Down"), Some(&None));
        assert_eq!(mapping.actions.get("Stick Left"), Some(&None));
        assert_eq!(mapping.actions.get("Stick Right"), Some(&None));
        assert_eq!(
            mapping.actions.get("A"),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::South,
            }))
        );
        assert_eq!(
            mapping.actions.get("B"),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::West,
            }))
        );
        assert_eq!(
            mapping.actions.get("C-Up"),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::North,
            }))
        );
        assert_eq!(
            mapping.actions.get("C-Down"),
            Some(&Some(MappingEntry::Axis {
                axis: CanonicalAxis::RightStickY,
                direction: 1,
            }))
        );
        assert_eq!(
            mapping.actions.get("C-Left"),
            Some(&Some(MappingEntry::Axis {
                axis: CanonicalAxis::RightStickX,
                direction: -1,
            }))
        );
        assert_eq!(
            mapping.actions.get("C-Right"),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::East,
            }))
        );
    }

    #[test]
    fn playstation_preset_fills_unassigned_shortcuts_without_overriding_defaults() {
        let mut mapping = default_gamepad_mapping_for_system("SNES");
        let changed = apply_device_preset_defaults(
            &mut mapping,
            &DetectedPadIdentity {
                device_key: String::from("054c:0ce6:DualSense"),
                name: String::from("DualSense Wireless Controller"),
                vendor_id: Some(String::from("054c")),
                product_id: Some(String::from("0ce6")),
                mapping_name: None,
            },
        );

        assert!(changed);
        assert_eq!(
            mapping.actions.get(EXIT_ACTION),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::RightShoulder,
            }))
        );
        assert_eq!(
            mapping.actions.get(QUICK_SAVE_ACTION),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::LeftThumb,
            }))
        );
        assert_eq!(
            mapping.actions.get(QUICK_LOAD_ACTION),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::RightThumb,
            }))
        );
        assert_eq!(
            mapping.actions.get(NEXT_SAVE_SLOT_ACTION),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::Guide,
            }))
        );
    }

    #[test]
    fn device_preset_does_not_override_existing_shortcuts() {
        let mut mapping = default_gamepad_mapping_for_system("NES");
        mapping.actions.insert(
            QUICK_SAVE_ACTION.to_string(),
            Some(MappingEntry::Button {
                button: CanonicalButton::North,
            }),
        );

        let changed = apply_device_preset_defaults(
            &mut mapping,
            &DetectedPadIdentity {
                device_key: String::from("045e:02ea:Xbox"),
                name: String::from("Xbox Wireless Controller"),
                vendor_id: Some(String::from("045e")),
                product_id: Some(String::from("02ea")),
                mapping_name: None,
            },
        );

        assert!(changed);
        assert_eq!(
            mapping.actions.get(QUICK_SAVE_ACTION),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::North,
            }))
        );
    }

    #[test]
    fn recognized_device_returns_preset_hint() {
        let hint = device_preset_hint(&DetectedPadIdentity {
            device_key: String::from("045e:02ea:Xbox"),
            name: String::from("Xbox Wireless Controller"),
            vendor_id: Some(String::from("045e")),
            product_id: Some(String::from("02ea")),
            mapping_name: None,
        });

        assert_eq!(
            hint,
            Some(
                "Defaults: Left Shoulder reset, Right Shoulder exit, L3 quick save, R3 quick load, PS/Guide next save slot."
            )
        );
    }
}
