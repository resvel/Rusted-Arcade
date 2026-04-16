use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
#[cfg(feature = "gamepad")]
use tracing::debug;

#[cfg(feature = "gamepad")]
use arcade_domain::MAX_GAMEPAD_PLAYERS;
use arcade_domain::{
    default_gamepad_mapping_for_system, supported_gamepad_actions, CanonicalAxis, CanonicalButton,
    DetectedPadIdentity, MappingEntry, N64PrimaryStick, StoredDeviceMeta, StoredGamepadMapping,
    EXIT_ACTION, NEXT_SAVE_SLOT_ACTION, QUICK_LOAD_ACTION, QUICK_SAVE_ACTION, RESET_ACTION,
    SYSTEM_DEFAULT_MAPPING_KEY,
};

use crate::{
    app::NativeArcadeUiApp,
    state::{ControllerMappingCacheKey, MappingEditorScope, MenuFocusRegion, MenuNavDirection},
};
#[cfg(feature = "gamepad")]
use gilrs::{ev::Code, Axis, Button, GamepadId};
#[cfg(feature = "gamepad")]
use std::collections::HashMap;

const RETRO_DEVICE_JOYPAD: u32 = 1;
const RETRO_DEVICE_ANALOG: u32 = 5;
const RETRO_BUTTON_PRESSED_VALUE: i16 = i16::MAX;

const RETRO_DEVICE_ID_JOYPAD_B: u32 = 0;
const RETRO_DEVICE_ID_JOYPAD_Y: u32 = 1;
const RETRO_DEVICE_ID_JOYPAD_SELECT: u32 = 2;
const RETRO_DEVICE_ID_JOYPAD_START: u32 = 3;
const RETRO_DEVICE_ID_JOYPAD_UP: u32 = 4;
const RETRO_DEVICE_ID_JOYPAD_DOWN: u32 = 5;
const RETRO_DEVICE_ID_JOYPAD_LEFT: u32 = 6;
const RETRO_DEVICE_ID_JOYPAD_RIGHT: u32 = 7;
const RETRO_DEVICE_ID_JOYPAD_A: u32 = 8;
const RETRO_DEVICE_ID_JOYPAD_X: u32 = 9;
const RETRO_DEVICE_ID_JOYPAD_L: u32 = 10;
const RETRO_DEVICE_ID_JOYPAD_R: u32 = 11;
const RETRO_DEVICE_ID_JOYPAD_L2: u32 = 12;

const RETRO_DEVICE_INDEX_ANALOG_LEFT: u32 = 0;
const RETRO_DEVICE_INDEX_ANALOG_RIGHT: u32 = 1;
const RETRO_DEVICE_INDEX_ANALOG_BUTTON: u32 = 2;
const RETRO_DEVICE_ID_ANALOG_X: u32 = 0;
const RETRO_DEVICE_ID_ANALOG_Y: u32 = 1;

#[cfg(feature = "gamepad")]
const DIGITAL_FALLBACK_THRESHOLD: f32 = 0.45;
#[cfg(feature = "gamepad")]
const BUTTON_ACTIVE_THRESHOLD: f32 = 0.10;
#[cfg(feature = "gamepad")]
const SONY_VENDOR_ID: u16 = 0x054c;
#[cfg(feature = "gamepad")]
const HID_PAGE_GENERIC_DESKTOP: u16 = 0x0001;
#[cfg(feature = "gamepad")]
const HID_PAGE_BUTTON: u16 = 0x0009;
#[cfg(feature = "gamepad")]
const HID_USAGE_GD_HATSWITCH_X: u16 = 0x0039;
#[cfg(feature = "gamepad")]
const HID_USAGE_GD_HATSWITCH_Y: u16 = 0x003A;
#[cfg(feature = "gamepad")]
const HID_USAGE_GD_DPAD_UP: u16 = 0x0090;
#[cfg(feature = "gamepad")]
const HID_USAGE_GD_DPAD_DOWN: u16 = 0x0091;
#[cfg(feature = "gamepad")]
const HID_USAGE_GD_DPAD_RIGHT: u16 = 0x0092;
#[cfg(feature = "gamepad")]
const HID_USAGE_GD_DPAD_LEFT: u16 = 0x0093;
#[cfg(feature = "gamepad")]
const HID_USAGE_BTN_DPAD_UP: u16 = 0x000c;
#[cfg(feature = "gamepad")]
const HID_USAGE_BTN_DPAD_DOWN: u16 = 0x000d;
#[cfg(feature = "gamepad")]
const HID_USAGE_BTN_DPAD_LEFT: u16 = 0x000e;
#[cfg(feature = "gamepad")]
const HID_USAGE_BTN_DPAD_RIGHT: u16 = 0x000f;

#[derive(Default, Clone)]
struct CanonicalPadState {
    dpad_up: bool,
    dpad_down: bool,
    dpad_left: bool,
    dpad_right: bool,
    south: bool,
    east: bool,
    north: bool,
    west: bool,
    left_shoulder: bool,
    right_shoulder: bool,
    guide: bool,
    left_trigger: f32,
    right_trigger: f32,
    select: bool,
    start: bool,
    left_thumb: bool,
    right_thumb: bool,
    left_x: f32,
    left_y: f32,
    right_x: f32,
    right_y: f32,
}

#[derive(Default, Clone, Copy)]
struct FrontendShortcutState {
    reset: bool,
    quick_save: bool,
    quick_load: bool,
    next_save_slot: bool,
    return_pressed: bool,
}

#[derive(Default, Clone, Copy)]
struct FrontendNavInput {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    south: bool,
    east: bool,
    west: bool,
    left_trigger: bool,
    right_trigger: bool,
    left_shoulder: bool,
    right_shoulder: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum RetroActionBinding {
    Joypad(u32),
    Analog {
        index: u32,
        axis_id: u32,
        value: f32,
    },
}

#[cfg(feature = "gamepad")]
struct GamepadCapture {
    port: u32,
    identity: DetectedPadIdentity,
    state: CanonicalPadState,
}

#[cfg(feature = "gamepad")]
#[derive(Default, Clone, Copy)]
pub(crate) struct RawDpadState {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    axis_x: f32,
    axis_y: f32,
}

impl NativeArcadeUiApp {
    pub(crate) fn apply_keyboard_and_gamepad_input(&mut self, ctx: &egui::Context) {
        self.host.clear_input_state();

        let system = self.active_input_system();
        let n64_primary_stick = if normalize_system_name(&system) == "N64" {
            Some(self.services.n64_primary_stick())
        } else {
            None
        };
        let keyboard_state = self.capture_keyboard_state(ctx);
        let mut shortcuts =
            self.apply_system_mapping_to_host(0, &keyboard_state, &system, None, n64_primary_stick);
        shortcuts.return_pressed |= ctx.input(|i| i.key_down(egui::Key::Escape));

        #[cfg(feature = "gamepad")]
        for capture in self.capture_gamepad_state() {
            let capture_shortcuts = self.apply_system_mapping_to_host(
                capture.port,
                &capture.state,
                &system,
                Some(&capture.identity),
                n64_primary_stick,
            );
            shortcuts.merge(capture_shortcuts);
        }

        self.apply_frontend_shortcuts(shortcuts);
    }

    pub(crate) fn tick_frontend_navigation(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.focused) {
            self.clear_menu_repeat_state();
            return;
        }

        let input = self.capture_frontend_nav_input(ctx);
        let now = Instant::now();

        if !self.state.manage.open
            && !matches!(self.state.current_view, crate::app::AppView::Settings)
            && consume_rising_edge(
                &mut self.state.menu_nav.left_shoulder_held,
                input.left_shoulder,
            )
        {
            self.cycle_system_filter(-1, self.state.current_view);
        }
        if !self.state.manage.open
            && !matches!(self.state.current_view, crate::app::AppView::Settings)
            && consume_rising_edge(
                &mut self.state.menu_nav.right_shoulder_held,
                input.right_shoulder,
            )
        {
            self.cycle_system_filter(1, self.state.current_view);
        }

        if consume_rising_edge(&mut self.state.menu_nav.east_held, input.east) {
            self.step_back_menu_focus();
        }

        if consume_rising_edge(&mut self.state.menu_nav.south_held, input.south) {
            self.activate_menu_focus();
        }

        if consume_rising_edge(&mut self.state.menu_nav.west_held, input.west) {
            self.handle_menu_west_action();
        }

        if consume_rising_edge(
            &mut self.state.menu_nav.left_trigger_held,
            input.left_trigger,
        ) {
            self.handle_grid_page(-1);
        }
        if consume_rising_edge(
            &mut self.state.menu_nav.right_trigger_held,
            input.right_trigger,
        ) {
            self.handle_grid_page(1);
        }

        if let Some(direction) = self.consume_menu_nav_direction(now, input) {
            self.handle_menu_direction(direction);
        }

        ctx.request_repaint_after(Duration::from_millis(50));
    }

    pub(crate) fn supported_mapping_actions(&self, system: &str) -> &'static [&'static str] {
        supported_gamepad_actions(system)
    }

    pub(crate) fn mapping_entry_options(&self) -> Vec<(String, Option<MappingEntry>)> {
        let mut options = Vec::new();
        options.push((String::from("Unassigned"), None));
        for button in [
            CanonicalButton::South,
            CanonicalButton::East,
            CanonicalButton::North,
            CanonicalButton::West,
            CanonicalButton::DPadUp,
            CanonicalButton::DPadDown,
            CanonicalButton::DPadLeft,
            CanonicalButton::DPadRight,
            CanonicalButton::Start,
            CanonicalButton::Select,
            CanonicalButton::LeftShoulder,
            CanonicalButton::RightShoulder,
            CanonicalButton::Guide,
            CanonicalButton::LeftThumb,
            CanonicalButton::RightThumb,
        ] {
            options.push((
                self.mapping_entry_label(Some(&MappingEntry::Button { button })),
                Some(MappingEntry::Button { button }),
            ));
        }

        for (axis, direction) in [
            (CanonicalAxis::LeftStickX, -1),
            (CanonicalAxis::LeftStickX, 1),
            (CanonicalAxis::LeftStickY, -1),
            (CanonicalAxis::LeftStickY, 1),
            (CanonicalAxis::RightStickX, -1),
            (CanonicalAxis::RightStickX, 1),
            (CanonicalAxis::RightStickY, -1),
            (CanonicalAxis::RightStickY, 1),
            (CanonicalAxis::LeftTrigger, 1),
            (CanonicalAxis::RightTrigger, 1),
        ] {
            let entry = MappingEntry::Axis { axis, direction };
            options.push((self.mapping_entry_label(Some(&entry)), Some(entry)));
        }

        options
    }

    pub(crate) fn mapping_entry_label(&self, entry: Option<&MappingEntry>) -> String {
        match entry {
            None => String::from("Unassigned"),
            Some(MappingEntry::Button { button }) => match button {
                CanonicalButton::South => String::from("South"),
                CanonicalButton::East => String::from("East"),
                CanonicalButton::North => String::from("North"),
                CanonicalButton::West => String::from("West"),
                CanonicalButton::DPadUp => String::from("D-Pad Up"),
                CanonicalButton::DPadDown => String::from("D-Pad Down"),
                CanonicalButton::DPadLeft => String::from("D-Pad Left"),
                CanonicalButton::DPadRight => String::from("D-Pad Right"),
                CanonicalButton::Start => String::from("Start"),
                CanonicalButton::Select => String::from("Select"),
                CanonicalButton::LeftShoulder => String::from("Left Shoulder"),
                CanonicalButton::RightShoulder => String::from("Right Shoulder"),
                CanonicalButton::Guide => String::from("Guide / PS"),
                CanonicalButton::LeftThumb => String::from("L3 (Left Stick Click)"),
                CanonicalButton::RightThumb => String::from("R3 (Right Stick Click)"),
            },
            Some(MappingEntry::Axis { axis, direction }) => {
                let axis_label = match axis {
                    CanonicalAxis::LeftStickX => "Left Stick X",
                    CanonicalAxis::LeftStickY => "Left Stick Y",
                    CanonicalAxis::RightStickX => "Right Stick X",
                    CanonicalAxis::RightStickY => "Right Stick Y",
                    CanonicalAxis::LeftTrigger => "Left Trigger",
                    CanonicalAxis::RightTrigger => "Right Trigger",
                };
                if *direction < 0 {
                    format!("{axis_label} -")
                } else {
                    format!("{axis_label} +")
                }
            }
        }
    }

    pub(crate) fn sync_controller_mapping_editor(&mut self) {
        let system = self.state.controller_mapping.input_system.clone();
        if system == "ALL" || system.is_empty() {
            return;
        }

        let active_device = self.active_detected_gamepad_identity();
        self.state
            .controller_mapping
            .normalize_scope(active_device.is_some());
        let desired_key = self.state.controller_mapping.desired_mapping_key(
            active_device
                .as_ref()
                .map(|device| device.device_key.as_str()),
        );

        if !self
            .state
            .controller_mapping
            .needs_reload(&system, &desired_key)
        {
            return;
        }

        let mapping = self.resolved_mapping_for(
            &system,
            if self.state.controller_mapping.selected_scope == MappingEditorScope::ActiveDevice {
                active_device.as_ref()
            } else {
                None
            },
        );
        let threshold = normalize_mapping_threshold(mapping.threshold);

        self.state.controller_mapping.load_from_mapping(
            system,
            desired_key,
            mapping.as_ref().clone(),
            threshold,
        );
    }

    pub(crate) fn reset_controller_mapping_editor_to_defaults(&mut self) {
        let system = self.state.controller_mapping.input_system.clone();
        if system == "ALL" || system.is_empty() {
            return;
        }

        let normalized_threshold =
            normalize_mapping_threshold(default_gamepad_mapping_for_system(&system).threshold);
        self.state
            .controller_mapping
            .reset_to_defaults(&system, normalized_threshold);
    }

    pub(crate) fn save_controller_mapping_from_editor(&mut self) {
        let system = self.state.controller_mapping.input_system.clone();
        if system == "ALL" || system.is_empty() {
            self.state.status =
                String::from("Choose a specific system before saving a controller mapping.");
            return;
        }

        let active_device = self.active_detected_gamepad_identity();
        let (mapping_key, vendor_id, product_id, device_meta) =
            if self.state.controller_mapping.selected_scope == MappingEditorScope::ActiveDevice {
                let Some(device) = active_device.as_ref() else {
                    self.state.status =
                        String::from("No controller connected for a device override.");
                    return;
                };
                (
                    device.device_key.as_str(),
                    device.vendor_id.as_deref(),
                    device.product_id.as_deref(),
                    Some(StoredDeviceMeta {
                        id: Some(device.device_key.clone()),
                        mapping: device.mapping_name.clone(),
                    }),
                )
            } else {
                (SYSTEM_DEFAULT_MAPPING_KEY, None, None, None)
            };
        let normalized_threshold =
            normalize_mapping_threshold(self.state.controller_mapping.threshold);

        let payload = StoredGamepadMapping {
            actions: self.state.controller_mapping.actions.clone(),
            threshold: normalized_threshold,
            updated_at: None,
            device: device_meta,
        };

        match self.services.save_gamepad_mapping(
            &system,
            mapping_key,
            vendor_id,
            product_id,
            &payload,
        ) {
            Ok(()) => {
                self.invalidate_gamepad_mapping_cache_for_system(&system);
                self.state
                    .controller_mapping
                    .mark_saved(normalized_threshold);
                self.state.status = if mapping_key == SYSTEM_DEFAULT_MAPPING_KEY {
                    format!("Saved {} controller defaults.", system)
                } else {
                    format!("Saved {} controller override for {}.", system, mapping_key)
                };
            }
            Err(err) => {
                self.state.status = format!("Could not save controller mapping: {err}");
            }
        }
    }

    pub(crate) fn controller_mapping_is_dirty(&self) -> bool {
        self.state
            .controller_mapping
            .is_dirty(normalize_mapping_threshold(
                self.state.controller_mapping.threshold,
            ))
    }

    pub(crate) fn active_detected_gamepad_identity(&mut self) -> Option<DetectedPadIdentity> {
        #[cfg(feature = "gamepad")]
        {
            let gilrs = self.gilrs.as_ref()?;
            let mut connected = gilrs
                .gamepads()
                .filter(|(_, gamepad)| gamepad.is_connected() && is_playable_gamepad(gamepad))
                .collect::<Vec<_>>();
            connected.sort_by(|left, right| left.0.to_string().cmp(&right.0.to_string()));
            let (id, gamepad) = connected.into_iter().next()?;
            Some(build_detected_pad_identity(id, &gamepad))
        }

        #[cfg(not(feature = "gamepad"))]
        {
            None
        }
    }

    pub(crate) fn invalidate_gamepad_mapping_cache_for_system(&mut self, system: &str) {
        let system = normalize_system_name(system);
        self.state
            .controller_mapping
            .invalidate_cache_for_system(&system);
    }

    fn active_input_system(&self) -> String {
        if let Some(system) = self.state.play.active_system.as_deref() {
            return normalize_system_name(system);
        }

        let active = self.active_system();
        if active != "ALL" {
            return normalize_system_name(active);
        }

        String::from("NES")
    }

    fn capture_frontend_nav_input(&mut self, ctx: &egui::Context) -> FrontendNavInput {
        let keyboard_navigation_enabled = !ctx.wants_keyboard_input();
        let input = FrontendNavInput {
            left: keyboard_navigation_enabled && ctx.input(|i| i.key_down(egui::Key::ArrowLeft)),
            right: keyboard_navigation_enabled && ctx.input(|i| i.key_down(egui::Key::ArrowRight)),
            up: keyboard_navigation_enabled && ctx.input(|i| i.key_down(egui::Key::ArrowUp)),
            down: keyboard_navigation_enabled && ctx.input(|i| i.key_down(egui::Key::ArrowDown)),
            south: keyboard_navigation_enabled && ctx.input(|i| i.key_down(egui::Key::Z)),
            east: keyboard_navigation_enabled && ctx.input(|i| i.key_down(egui::Key::C)),
            west: keyboard_navigation_enabled && ctx.input(|i| i.key_down(egui::Key::X)),
            left_trigger: keyboard_navigation_enabled
                && ctx.input(|i| i.key_down(egui::Key::PageUp)),
            right_trigger: keyboard_navigation_enabled
                && ctx.input(|i| i.key_down(egui::Key::PageDown)),
            left_shoulder: keyboard_navigation_enabled && ctx.input(|i| i.key_down(egui::Key::Q)),
            right_shoulder: keyboard_navigation_enabled && ctx.input(|i| i.key_down(egui::Key::E)),
        };

        #[cfg(feature = "gamepad")]
        let mut input = input;

        #[cfg(feature = "gamepad")]
        for capture in self.capture_gamepad_state() {
            input.left |= capture.state.dpad_left;
            input.right |= capture.state.dpad_right;
            input.up |= capture.state.dpad_up;
            input.down |= capture.state.dpad_down;
            input.south |= capture.state.south;
            input.east |= capture.state.east;
            input.west |= capture.state.west;
            input.left_trigger |= capture.state.left_trigger >= BUTTON_ACTIVE_THRESHOLD;
            input.right_trigger |= capture.state.right_trigger >= BUTTON_ACTIVE_THRESHOLD;
            input.left_shoulder |= capture.state.left_shoulder;
            input.right_shoulder |= capture.state.right_shoulder;
        }

        input
    }

    fn capture_keyboard_state(&self, ctx: &egui::Context) -> CanonicalPadState {
        CanonicalPadState {
            dpad_up: ctx.input(|i| i.key_down(egui::Key::ArrowUp)),
            dpad_down: ctx.input(|i| i.key_down(egui::Key::ArrowDown)),
            dpad_left: ctx.input(|i| i.key_down(egui::Key::ArrowLeft)),
            dpad_right: ctx.input(|i| i.key_down(egui::Key::ArrowRight)),
            south: ctx.input(|i| i.key_down(egui::Key::Z)),
            east: ctx.input(|i| i.key_down(egui::Key::X)),
            start: ctx.input(|i| i.key_down(egui::Key::Enter)),
            select: ctx.input(|i| i.key_down(egui::Key::Space)),
            ..CanonicalPadState::default()
        }
    }

    #[cfg(feature = "gamepad")]
    fn capture_gamepad_state(&mut self) -> Vec<GamepadCapture> {
        let Some(gilrs) = self.gilrs.as_mut() else {
            self.state.input_debug.clear();
            return Vec::new();
        };
        let identity_cache = &mut self.gamepad_identity_cache;
        let raw_dpad_state_cache = &mut self.raw_dpad_state_cache;

        while let Some(event) = gilrs.next_event() {
            update_raw_dpad_state_from_event(raw_dpad_state_cache, event);
        }

        let mut connected = gilrs
            .gamepads()
            .filter(|(_, gamepad)| gamepad.is_connected() && is_playable_gamepad(gamepad))
            .map(|(id, _)| (usize::from(id), id))
            .collect::<Vec<_>>();
        connected.sort_by(|left, right| left.0.cmp(&right.0));
        raw_dpad_state_cache.retain(|id, _| connected.iter().any(|(connected_id, _)| connected_id == id));

        let mut captures = Vec::with_capacity(connected.len().min(MAX_GAMEPAD_PLAYERS as usize));
        let mut input_debug = String::new();
        for (port, (_, id)) in connected
            .into_iter()
            .take(MAX_GAMEPAD_PLAYERS as usize)
            .enumerate()
        {
            let gamepad = gilrs.gamepad(id);
            let raw_dpad = raw_dpad_state_cache
                .get(&usize::from(id))
                .copied()
                .unwrap_or_default();
            let invert_vertical = should_invert_vertical_axis(gamepad.vendor_id(), gamepad.name());
            let has_dpad_buttons = gamepad_has_dpad_buttons(&gamepad);
            let has_mapped_dpad_axes =
                gamepad.axis_code(Axis::DPadX).is_some() || gamepad.axis_code(Axis::DPadY).is_some();
            let has_raw_dpad_state = raw_dpad.up
                || raw_dpad.down
                || raw_dpad.left
                || raw_dpad.right
                || raw_dpad.axis_x.abs() >= DIGITAL_FALLBACK_THRESHOLD
                || raw_dpad.axis_y.abs() >= DIGITAL_FALLBACK_THRESHOLD;
            let has_explicit_dpad_input = has_dpad_buttons || has_mapped_dpad_axes || has_raw_dpad_state;
            let raw_dpad_x = gamepad.value(Axis::DPadX);
            let raw_dpad_y = gamepad.value(Axis::DPadY);
            let effective_raw_dpad_x = merge_dpad_axis_value(raw_dpad_x, raw_dpad.axis_x);
            let effective_raw_dpad_y = merge_dpad_axis_value(raw_dpad_y, raw_dpad.axis_y);
            let raw_left_x = gamepad.value(Axis::LeftStickX);
            let raw_left_y = gamepad.value(Axis::LeftStickY);
            let dpad_up_pressed = (has_dpad_buttons && button_down(&gamepad, Button::DPadUp))
                || raw_dpad.up;
            let dpad_down_pressed = (has_dpad_buttons && button_down(&gamepad, Button::DPadDown))
                || raw_dpad.down;
            let dpad_left_pressed = (has_dpad_buttons && button_down(&gamepad, Button::DPadLeft))
                || raw_dpad.left;
            let dpad_right_pressed = (has_dpad_buttons && button_down(&gamepad, Button::DPadRight))
                || raw_dpad.right;
            let dpad_x = effective_raw_dpad_x;
            // gilrs already normalizes DPadY with platform reversal rules; applying
            // our Sony stick inversion here would double-invert on macOS.
            let dpad_y = normalize_dpad_vertical_axis(effective_raw_dpad_y);
            let left_x = raw_left_x;
            let left_y = normalize_vertical_axis(raw_left_y, invert_vertical);
            let right_y = normalize_vertical_axis(gamepad.value(Axis::RightStickY), invert_vertical);
            let dpad_fallback_x = if has_explicit_dpad_input { 0.0 } else { left_x };
            let dpad_fallback_y = if has_explicit_dpad_input { 0.0 } else { left_y };
            let state = CanonicalPadState {
                dpad_up: direction_active(dpad_up_pressed, dpad_y, dpad_fallback_y, false),
                dpad_down: direction_active(dpad_down_pressed, dpad_y, dpad_fallback_y, true),
                dpad_left: direction_active(dpad_left_pressed, dpad_x, dpad_fallback_x, false),
                dpad_right: direction_active(dpad_right_pressed, dpad_x, dpad_fallback_x, true),
                south: button_down(&gamepad, Button::South),
                east: button_down(&gamepad, Button::East),
                north: button_down(&gamepad, Button::North),
                west: button_down(&gamepad, Button::West),
                left_shoulder: button_down(&gamepad, Button::LeftTrigger),
                right_shoulder: button_down(&gamepad, Button::RightTrigger),
                guide: button_down(&gamepad, Button::Mode),
                left_trigger: trigger_axis_value(&gamepad, Axis::LeftZ, Button::LeftTrigger2),
                right_trigger: trigger_axis_value(&gamepad, Axis::RightZ, Button::RightTrigger2),
                select: button_down(&gamepad, Button::Select),
                start: button_down(&gamepad, Button::Start),
                left_thumb: button_down(&gamepad, Button::LeftThumb),
                right_thumb: button_down(&gamepad, Button::RightThumb),
                left_x,
                left_y,
                right_x: gamepad.value(Axis::RightStickX),
                right_y,
            };

            debug_gamepad_capture(
                &gamepad,
                has_dpad_buttons,
                effective_raw_dpad_x,
                effective_raw_dpad_y,
                raw_left_x,
                raw_left_y,
                &state,
            );
            if input_debug.is_empty() && std::env::var_os("ARCADE_INPUT_DEBUG").is_some() {
                input_debug = format_gamepad_capture_debug_line(
                    &gamepad,
                    has_dpad_buttons,
                    effective_raw_dpad_x,
                    effective_raw_dpad_y,
                    raw_left_x,
                    raw_left_y,
                    &state,
                );
            }

            let cache_key = usize::from(id);
            let identity = if let Some(existing) = identity_cache.get(&cache_key) {
                existing.clone()
            } else {
                let built = build_detected_pad_identity(id, &gamepad);
                identity_cache.insert(cache_key, built.clone());
                built
            };

            captures.push(GamepadCapture {
                port: port as u32,
                identity,
                state,
            });
        }

        self.state.input_debug = input_debug;
        captures
    }

    fn resolved_mapping_for(
        &mut self,
        system: &str,
        device: Option<&DetectedPadIdentity>,
    ) -> Arc<StoredGamepadMapping> {
        let system = normalize_system_name(system);
        let mapping_key = device
            .map(|device| device.device_key.clone())
            .unwrap_or_else(|| SYSTEM_DEFAULT_MAPPING_KEY.to_string());
        let cache_key = ControllerMappingCacheKey {
            system: system.clone(),
            mapping_key,
        };

        if let Some(mapping) = self.state.controller_mapping.cache_get(&cache_key) {
            return mapping;
        }

        let mut mapping = match self.services.resolve_gamepad_mapping(&system, device) {
            Ok(mapping) => mapping,
            Err(err) => {
                self.state.status = format!("Controller mapping load failed: {err}");
                default_gamepad_mapping_for_system(&system)
            }
        };
        let default_mapping = default_gamepad_mapping_for_system(&system);
        for action in supported_gamepad_actions(&system) {
            if mapping.actions.contains_key(*action) {
                continue;
            }
            mapping.actions.insert(
                (*action).to_string(),
                default_mapping
                    .actions
                    .get(*action)
                    .cloned()
                    .unwrap_or(None),
            );
        }
        self.state
            .controller_mapping
            .cache_put(cache_key.clone(), mapping);
        self.state
            .controller_mapping
            .cache_get(&cache_key)
            .expect("mapping must exist in cache after cache_put")
    }

    fn apply_system_mapping_to_host(
        &mut self,
        port: u32,
        state: &CanonicalPadState,
        system: &str,
        device: Option<&DetectedPadIdentity>,
        n64_primary_stick: Option<N64PrimaryStick>,
    ) -> FrontendShortcutState {
        let mapping = self.resolved_mapping_for(system, device);

        for action in supported_gamepad_actions(system) {
            let Some(Some(entry)) = mapping.actions.get(*action) else {
                continue;
            };
            if !mapping_entry_is_active(state, entry, mapping.threshold) {
                continue;
            }

            if let Some(binding) = action_to_retro_binding(system, action) {
                match binding {
                    RetroActionBinding::Joypad(joypad_id) => {
                        set_button(self, port, joypad_id, true)
                    }
                    RetroActionBinding::Analog {
                        index,
                        axis_id,
                        value,
                    } => set_analog(self, port, index, axis_id, value),
                }
            }
        }

        if system_supports_native_analog(system) {
            let preference = n64_primary_stick.unwrap_or(N64PrimaryStick::Left);
            let (primary_x, primary_y) = n64_primary_stick_axes_for_preference(state, preference);
            if primary_x.abs() > f32::EPSILON {
                set_analog(
                    self,
                    port,
                    RETRO_DEVICE_INDEX_ANALOG_LEFT,
                    RETRO_DEVICE_ID_ANALOG_X,
                    primary_x,
                );
            }
            if primary_y.abs() > f32::EPSILON {
                set_analog(
                    self,
                    port,
                    RETRO_DEVICE_INDEX_ANALOG_LEFT,
                    RETRO_DEVICE_ID_ANALOG_Y,
                    primary_y,
                );
            }
        }

        frontend_shortcuts_from_mapping(&mapping, state)
    }

    fn apply_frontend_shortcuts(&mut self, shortcuts: FrontendShortcutState) {
        let any_shortcut_pressed = shortcuts.reset
            || shortcuts.quick_save
            || shortcuts.quick_load
            || shortcuts.next_save_slot
            || shortcuts.return_pressed;
        if self
            .state
            .play
            .should_block_frontend_shortcuts_until_release(any_shortcut_pressed)
        {
            return;
        }

        self.handle_session_return_shortcut(shortcuts.return_pressed);
        if !self.host.is_loaded() {
            return;
        }
        self.handle_session_reset_shortcut(shortcuts.reset);
        if !self.host.is_loaded() {
            return;
        }
        if self
            .state
            .play
            .consume_quick_save_press(shortcuts.quick_save)
        {
            self.quick_save_play_session();
        }
        if self
            .state
            .play
            .consume_quick_load_press(shortcuts.quick_load)
        {
            self.quick_load_play_session();
        }
        if self
            .state
            .play
            .consume_next_save_slot_press(shortcuts.next_save_slot)
        {
            self.cycle_save_slot_forward();
        }
    }

    fn clear_menu_repeat_state(&mut self) {
        self.state.menu_nav.clear_repeat_state();
    }

    fn consume_menu_nav_direction(
        &mut self,
        now: Instant,
        input: FrontendNavInput,
    ) -> Option<MenuNavDirection> {
        let direction = if input.left {
            Some(MenuNavDirection::Left)
        } else if input.right {
            Some(MenuNavDirection::Right)
        } else if input.up {
            Some(MenuNavDirection::Up)
        } else if input.down {
            Some(MenuNavDirection::Down)
        } else {
            None
        };

        self.state.menu_nav.consume_direction(now, direction)
    }

    fn handle_menu_direction(&mut self, direction: MenuNavDirection) {
        if self.state.manage.open {
            self.handle_manage_direction(direction);
            return;
        }

        if matches!(self.state.current_view, crate::app::AppView::Settings) {
            self.handle_settings_direction(direction);
            return;
        }

        self.normalize_filters_panel_focus();

        match self.state.menu_nav.focus_region {
            MenuFocusRegion::TopNav => {
                if matches!(self.state.current_view, crate::app::AppView::Library)
                    && matches!(direction, MenuNavDirection::Down)
                {
                    self.state.menu_nav.focus_region = MenuFocusRegion::LibraryManageButton;
                } else {
                    self.state
                        .menu_nav
                        .move_top_nav(direction, self.state.current_view);
                }
            }
            MenuFocusRegion::LibraryManageButton => match direction {
                MenuNavDirection::Up => {
                    self.state
                        .menu_nav
                        .focus_top_nav_for_view(self.state.current_view);
                }
                MenuNavDirection::Down | MenuNavDirection::Left => {
                    self.state.menu_nav.focus_region = MenuFocusRegion::FiltersToggle;
                }
                MenuNavDirection::Right => {}
            },
            MenuFocusRegion::FiltersToggle => match direction {
                MenuNavDirection::Up => {
                    self.state
                        .menu_nav
                        .focus_top_nav_for_view(self.state.current_view);
                }
                MenuNavDirection::Down => {
                    self.state.menu_nav.focus_region = if self.filters_panel_expanded() {
                        MenuFocusRegion::FiltersSystem
                    } else if self.current_browse_grid_source().is_some() {
                        MenuFocusRegion::Grid
                    } else {
                        MenuFocusRegion::TopNav
                    };
                }
                MenuNavDirection::Left => {}
                MenuNavDirection::Right => {
                    if matches!(self.state.current_view, crate::app::AppView::Library) {
                        self.state.menu_nav.focus_region = MenuFocusRegion::LibraryManageButton;
                    }
                }
            },
            MenuFocusRegion::FiltersSystem => match direction {
                MenuNavDirection::Up => {
                    self.state.menu_nav.focus_region = MenuFocusRegion::FiltersToggle;
                }
                _ => {
                    let system_last = crate::theme::SYSTEM_FILTERS.len().saturating_sub(1);
                    let alpha_last = Self::alpha_filter_values().len().saturating_sub(1);
                    self.state
                        .menu_nav
                        .move_system_filter(direction, system_last, alpha_last);
                }
            },
            MenuFocusRegion::FiltersAlpha => {
                let alpha_last = Self::alpha_filter_values().len().saturating_sub(1);
                let entering_grid = matches!(direction, MenuNavDirection::Down);
                self.state.menu_nav.move_alpha_filter(direction, alpha_last);
                if entering_grid {
                    if let Some(source) = self.current_browse_grid_source() {
                        self.repair_grid_selection(source);
                    }
                }
            }
            MenuFocusRegion::ManageHeader
            | MenuFocusRegion::ManageScope
            | MenuFocusRegion::ManageActions
            | MenuFocusRegion::ManageSettings
            | MenuFocusRegion::ManageScrapeSystems
            | MenuFocusRegion::ManageScrapeActions
            | MenuFocusRegion::ManageList
            | MenuFocusRegion::SettingsAppConfigCoreTab
            | MenuFocusRegion::SettingsAppConfigCoreVariable
            | MenuFocusRegion::SettingsAppConfigSave
            | MenuFocusRegion::SettingsCoverSettings => {}
            MenuFocusRegion::Grid => {
                let Some(source) = self.current_browse_grid_source() else {
                    return;
                };
                let metrics = self.grid_nav_metrics(source);
                match direction {
                    MenuNavDirection::Left => {
                        self.move_grid_selection_by_delta(source, -1, metrics);
                    }
                    MenuNavDirection::Right => {
                        self.move_grid_selection_by_delta(source, 1, metrics);
                    }
                    MenuNavDirection::Down => {
                        self.move_grid_selection_by_delta(
                            source,
                            metrics.columns as isize,
                            metrics,
                        );
                    }
                    MenuNavDirection::Up => {
                        if self.grid_len(source) == 0
                            || self.active_grid_index(source) < metrics.columns
                        {
                            if matches!(
                                self.state.current_view,
                                crate::app::AppView::Home | crate::app::AppView::Library
                            ) {
                                if self.filters_panel_expanded() {
                                    self.state.menu_nav.focus_region =
                                        MenuFocusRegion::FiltersAlpha;
                                } else {
                                    self.state.menu_nav.focus_region =
                                        MenuFocusRegion::FiltersToggle;
                                }
                            } else {
                                self.state
                                    .menu_nav
                                    .focus_top_nav_for_view(self.state.current_view);
                            }
                        } else {
                            self.move_grid_selection_by_delta(
                                source,
                                -(metrics.columns as isize),
                                metrics,
                            );
                        }
                    }
                }
            }
        }
    }

    fn activate_menu_focus(&mut self) {
        if self.state.manage.open {
            self.activate_manage_focus();
            return;
        }

        if matches!(self.state.current_view, crate::app::AppView::Settings) {
            self.activate_settings_focus();
            return;
        }

        self.normalize_filters_panel_focus();

        match self.state.menu_nav.focus_region {
            MenuFocusRegion::TopNav => {
                let view = self.state.menu_nav.selected_top_nav_view();
                self.navigate_to_view(view);
            }
            MenuFocusRegion::LibraryManageButton => {
                self.state.manage.open = true;
                self.state.menu_nav.focus_region = MenuFocusRegion::ManageHeader;
                self.refresh_manage_rows();
                self.sync_manage_settings_from_services();
            }
            MenuFocusRegion::FiltersToggle => {
                self.state.library.filters_panel_collapsed =
                    !self.state.library.filters_panel_collapsed;
                self.normalize_filters_panel_focus();
            }
            MenuFocusRegion::FiltersSystem => {
                let changed =
                    self.apply_system_filter_by_index(self.state.menu_nav.system_filter_index);
                self.apply_current_view_filter_change(changed, false, false);
            }
            MenuFocusRegion::FiltersAlpha => {
                let changed =
                    self.apply_alpha_filter_by_index(self.state.menu_nav.alpha_filter_index);
                self.apply_current_view_filter_change(false, changed, false);
            }
            MenuFocusRegion::Grid => {
                self.launch_selected_rom();
            }
            MenuFocusRegion::ManageHeader
            | MenuFocusRegion::ManageScope
            | MenuFocusRegion::ManageActions
            | MenuFocusRegion::ManageSettings
            | MenuFocusRegion::ManageScrapeSystems
            | MenuFocusRegion::ManageScrapeActions
            | MenuFocusRegion::ManageList
            | MenuFocusRegion::SettingsAppConfigCoreTab
            | MenuFocusRegion::SettingsAppConfigCoreVariable
            | MenuFocusRegion::SettingsAppConfigSave
            | MenuFocusRegion::SettingsCoverSettings => {}
        }
    }

    fn step_back_menu_focus(&mut self) {
        if self.state.manage.open {
            self.step_back_manage_focus();
            return;
        }

        if matches!(self.state.current_view, crate::app::AppView::Settings) {
            self.step_back_settings_focus();
            return;
        }

        self.normalize_filters_panel_focus();
        self.state
            .menu_nav
            .step_back_focus(self.state.current_view, self.filters_panel_expanded());
    }

    fn handle_menu_west_action(&mut self) {
        if self.state.manage.open {
            self.handle_manage_west_action();
            return;
        }

        if matches!(self.state.current_view, crate::app::AppView::Settings) {
            return;
        }

        if self.state.menu_nav.focus_region != MenuFocusRegion::Grid {
            return;
        }

        if let Err(err) = self.toggle_selected_rom_favorite() {
            self.state.status = err.to_string();
        }
    }

    fn handle_grid_page(&mut self, direction: isize) {
        if self.state.manage.open {
            if self.state.menu_nav.focus_region == MenuFocusRegion::ManageList
                && !self.state.manage.rows.is_empty()
            {
                let len = self.state.manage.rows.len();
                let step = 8_usize;
                let current = self.state.menu_nav.manage_list_index;
                self.state.menu_nav.manage_list_index = if direction < 0 {
                    current.saturating_sub(step)
                } else {
                    (current + step).min(len.saturating_sub(1))
                };
            }
            return;
        }

        if self.state.menu_nav.focus_region != MenuFocusRegion::Grid {
            return;
        }

        let Some(source) = self.current_browse_grid_source() else {
            return;
        };
        let metrics = self.grid_nav_metrics(source);
        self.page_grid_selection(source, direction, metrics);
    }

    fn handle_manage_direction(&mut self, direction: MenuNavDirection) {
        self.sync_manage_nav_state();

        match self.state.menu_nav.focus_region {
            MenuFocusRegion::TopNav => match direction {
                MenuNavDirection::Down => {
                    self.state.menu_nav.focus_region = MenuFocusRegion::ManageHeader;
                }
                _ => self
                    .state
                    .menu_nav
                    .move_top_nav(direction, self.state.current_view),
            },
            MenuFocusRegion::LibraryManageButton => {
                self.state.menu_nav.focus_region = MenuFocusRegion::ManageHeader;
            }
            MenuFocusRegion::ManageHeader => match direction {
                MenuNavDirection::Up => {
                    self.state
                        .menu_nav
                        .focus_top_nav_for_view(self.state.current_view);
                }
                MenuNavDirection::Down => {
                    self.state.menu_nav.focus_region = MenuFocusRegion::ManageScope;
                }
                MenuNavDirection::Left | MenuNavDirection::Right => {}
            },
            MenuFocusRegion::ManageScope => match direction {
                MenuNavDirection::Up => {
                    self.state.menu_nav.focus_region = MenuFocusRegion::ManageHeader;
                }
                MenuNavDirection::Down => {
                    self.state.menu_nav.focus_region = MenuFocusRegion::ManageActions;
                }
                MenuNavDirection::Left => {
                    if !self.state.manage.job_running {
                        let current = self.state.menu_nav.manage_scope_index;
                        let next = current.saturating_sub(1);
                        self.apply_manage_scope_index(next);
                    }
                }
                MenuNavDirection::Right => {
                    if !self.state.manage.job_running {
                        let last = crate::theme::SYSTEM_FILTERS.len().saturating_sub(1);
                        let current = self.state.menu_nav.manage_scope_index;
                        let next = (current + 1).min(last);
                        self.apply_manage_scope_index(next);
                    }
                }
            },
            MenuFocusRegion::ManageActions => match direction {
                MenuNavDirection::Up => {
                    self.state.menu_nav.focus_region = MenuFocusRegion::ManageScope;
                }
                MenuNavDirection::Down => {
                    self.state.menu_nav.focus_region = MenuFocusRegion::ManageScrapeSystems;
                }
                MenuNavDirection::Left => {
                    self.state.menu_nav.manage_action_index =
                        self.state.menu_nav.manage_action_index.saturating_sub(1);
                }
                MenuNavDirection::Right => {
                    self.state.menu_nav.manage_action_index =
                        (self.state.menu_nav.manage_action_index + 1).min(2);
                }
            },
            MenuFocusRegion::ManageSettings => match direction {
                MenuNavDirection::Up => {
                    self.state.menu_nav.focus_region = MenuFocusRegion::ManageActions;
                }
                MenuNavDirection::Down => {
                    self.state.menu_nav.focus_region = MenuFocusRegion::ManageScrapeSystems;
                }
                MenuNavDirection::Left => {
                    self.state.menu_nav.manage_settings_index =
                        self.state.menu_nav.manage_settings_index.saturating_sub(1);
                }
                MenuNavDirection::Right => {
                    self.state.menu_nav.manage_settings_index =
                        (self.state.menu_nav.manage_settings_index + 1).min(1);
                }
            },
            MenuFocusRegion::ManageScrapeSystems => match direction {
                MenuNavDirection::Up => {
                    self.state.menu_nav.focus_region = MenuFocusRegion::ManageActions;
                }
                MenuNavDirection::Down => {
                    self.state.menu_nav.focus_region = MenuFocusRegion::ManageScrapeActions;
                }
                MenuNavDirection::Left => {
                    self.state.menu_nav.manage_scrape_system_index = self
                        .state
                        .menu_nav
                        .manage_scrape_system_index
                        .saturating_sub(1);
                }
                MenuNavDirection::Right => {
                    self.state.menu_nav.manage_scrape_system_index =
                        (self.state.menu_nav.manage_scrape_system_index + 1)
                            .min(crate::theme::SYSTEM_FILTERS.len().saturating_sub(1));
                }
            },
            MenuFocusRegion::ManageScrapeActions => match direction {
                MenuNavDirection::Up => {
                    self.state.menu_nav.focus_region = MenuFocusRegion::ManageScrapeSystems;
                }
                MenuNavDirection::Down => {
                    if self.state.manage.rows.is_empty() {
                        self.state.menu_nav.focus_region = MenuFocusRegion::ManageScrapeActions;
                    } else {
                        self.state.menu_nav.focus_region = MenuFocusRegion::ManageList;
                    }
                }
                MenuNavDirection::Left | MenuNavDirection::Right => {}
            },
            MenuFocusRegion::ManageList => match direction {
                MenuNavDirection::Up => {
                    if self.state.menu_nav.manage_list_index == 0 {
                        self.state.menu_nav.focus_region = MenuFocusRegion::ManageScrapeActions;
                    } else {
                        self.state.menu_nav.manage_list_index =
                            self.state.menu_nav.manage_list_index.saturating_sub(1);
                    }
                }
                MenuNavDirection::Down => {
                    if !self.state.manage.rows.is_empty() {
                        self.state.menu_nav.manage_list_index =
                            (self.state.menu_nav.manage_list_index + 1)
                                .min(self.state.manage.rows.len().saturating_sub(1));
                    }
                }
                MenuNavDirection::Left | MenuNavDirection::Right => {}
            },
            MenuFocusRegion::FiltersToggle
            | MenuFocusRegion::FiltersSystem
            | MenuFocusRegion::FiltersAlpha
            | MenuFocusRegion::Grid
            | MenuFocusRegion::SettingsAppConfigCoreTab
            | MenuFocusRegion::SettingsAppConfigCoreVariable
            | MenuFocusRegion::SettingsAppConfigSave
            | MenuFocusRegion::SettingsCoverSettings => {
                self.state.menu_nav.focus_region = MenuFocusRegion::ManageHeader;
            }
        }
    }

    fn activate_manage_focus(&mut self) {
        self.sync_manage_nav_state();

        match self.state.menu_nav.focus_region {
            MenuFocusRegion::TopNav => {
                let view = self.state.menu_nav.selected_top_nav_view();
                self.navigate_to_view(view);
            }
            MenuFocusRegion::LibraryManageButton => {
                self.state.menu_nav.focus_region = MenuFocusRegion::ManageHeader;
            }
            MenuFocusRegion::ManageHeader => self.close_manage_view(),
            MenuFocusRegion::ManageScope => {}
            MenuFocusRegion::ManageActions => match self.state.menu_nav.manage_action_index {
                0 => {
                    if !self.state.manage.job_running {
                        self.refresh_manage_rows();
                    }
                }
                1 => {
                    if !self.state.manage.job_running {
                        self.start_smart_scan_job();
                    }
                }
                _ => {
                    if !self.state.manage.job_running && self.state.manage.selected_count() > 0 {
                        self.start_remove_job();
                    }
                }
            },
            MenuFocusRegion::ManageSettings => {}
            MenuFocusRegion::ManageScrapeSystems => {
                let index = self.state.menu_nav.manage_scrape_system_index;
                self.toggle_manage_scrape_system_by_index(index);
            }
            MenuFocusRegion::ManageScrapeActions => {
                if !self.state.manage.job_running {
                    self.start_scrape_job();
                }
            }
            MenuFocusRegion::ManageList => {
                let index = self.state.menu_nav.manage_list_index;
                self.toggle_manage_row_selection(index);
            }
            MenuFocusRegion::FiltersToggle
            | MenuFocusRegion::FiltersSystem
            | MenuFocusRegion::FiltersAlpha
            | MenuFocusRegion::Grid
            | MenuFocusRegion::SettingsAppConfigCoreTab
            | MenuFocusRegion::SettingsAppConfigCoreVariable
            | MenuFocusRegion::SettingsAppConfigSave
            | MenuFocusRegion::SettingsCoverSettings => {
                self.state.menu_nav.focus_region = MenuFocusRegion::ManageHeader;
            }
        }
    }

    fn step_back_manage_focus(&mut self) {
        self.sync_manage_nav_state();
        self.state.menu_nav.focus_region = match self.state.menu_nav.focus_region {
            MenuFocusRegion::TopNav => MenuFocusRegion::TopNav,
            MenuFocusRegion::LibraryManageButton => MenuFocusRegion::TopNav,
            MenuFocusRegion::ManageHeader => MenuFocusRegion::TopNav,
            MenuFocusRegion::ManageScope => MenuFocusRegion::ManageHeader,
            MenuFocusRegion::ManageActions => MenuFocusRegion::ManageScope,
            MenuFocusRegion::ManageSettings => MenuFocusRegion::ManageActions,
            MenuFocusRegion::ManageScrapeSystems => MenuFocusRegion::ManageActions,
            MenuFocusRegion::ManageScrapeActions => MenuFocusRegion::ManageScrapeSystems,
            MenuFocusRegion::ManageList => MenuFocusRegion::ManageScrapeActions,
            MenuFocusRegion::FiltersToggle
            | MenuFocusRegion::FiltersSystem
            | MenuFocusRegion::FiltersAlpha
            | MenuFocusRegion::Grid
            | MenuFocusRegion::SettingsAppConfigCoreTab
            | MenuFocusRegion::SettingsAppConfigCoreVariable
            | MenuFocusRegion::SettingsAppConfigSave
            | MenuFocusRegion::SettingsCoverSettings => MenuFocusRegion::ManageHeader,
        };
    }

    fn handle_manage_west_action(&mut self) {
        match self.state.menu_nav.focus_region {
            MenuFocusRegion::ManageScrapeSystems => {
                let index = self.state.menu_nav.manage_scrape_system_index;
                self.toggle_manage_scrape_system_by_index(index);
            }
            MenuFocusRegion::ManageList => {
                let index = self.state.menu_nav.manage_list_index;
                self.toggle_manage_row_selection(index);
            }
            _ => {}
        }
    }

    fn handle_settings_direction(&mut self, direction: MenuNavDirection) {
        let profiles = arcade_domain::core_profiles();
        let num_tabs = profiles.len();
        let tab_idx = self
            .state
            .menu_nav
            .settings_core_tab_index
            .min(num_tabs.saturating_sub(1));
        let num_vars = profiles
            .get(tab_idx)
            .map(|p| p.variables.len())
            .unwrap_or(0);

        match self.state.menu_nav.focus_region {
            MenuFocusRegion::TopNav => match direction {
                MenuNavDirection::Down => {
                    self.state.menu_nav.focus_region = MenuFocusRegion::SettingsAppConfigCoreTab;
                    self.state.menu_nav.settings_core_tab_index = self
                        .state
                        .menu_nav
                        .settings_core_tab_index
                        .min(num_tabs.saturating_sub(1));
                }
                _ => self
                    .state
                    .menu_nav
                    .move_top_nav(direction, self.state.current_view),
            },
            MenuFocusRegion::SettingsAppConfigCoreTab => match direction {
                MenuNavDirection::Up => {
                    self.state
                        .menu_nav
                        .focus_top_nav_for_view(self.state.current_view);
                }
                MenuNavDirection::Down => {
                    if num_vars > 0 {
                        self.state.menu_nav.settings_core_variable_index = 0;
                        self.state.menu_nav.settings_core_option_index = 0;
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigCoreVariable;
                    }
                }
                MenuNavDirection::Left => {
                    self.state.menu_nav.settings_core_tab_index = self
                        .state
                        .menu_nav
                        .settings_core_tab_index
                        .saturating_sub(1);
                    self.state.menu_nav.settings_core_variable_index = 0;
                    self.state.menu_nav.settings_core_option_index = 0;
                }
                MenuNavDirection::Right => {
                    self.state.menu_nav.settings_core_tab_index =
                        (self.state.menu_nav.settings_core_tab_index + 1)
                            .min(num_tabs.saturating_sub(1));
                    self.state.menu_nav.settings_core_variable_index = 0;
                    self.state.menu_nav.settings_core_option_index = 0;
                }
            },
            MenuFocusRegion::SettingsAppConfigCoreVariable => {
                let var_idx = self
                    .state
                    .menu_nav
                    .settings_core_variable_index
                    .min(num_vars.saturating_sub(1));
                let num_options = profiles
                    .get(tab_idx)
                    .and_then(|p| p.variables.get(var_idx))
                    .map(|v| v.options.len())
                    .unwrap_or(0);

                match direction {
                    MenuNavDirection::Up => {
                        if var_idx == 0 {
                            self.state.menu_nav.focus_region =
                                MenuFocusRegion::SettingsAppConfigCoreTab;
                        } else {
                            self.state.menu_nav.settings_core_variable_index = var_idx - 1;
                            self.state.menu_nav.settings_core_option_index = 0;
                        }
                    }
                    MenuNavDirection::Down => {
                        if var_idx + 1 < num_vars {
                            self.state.menu_nav.settings_core_variable_index = var_idx + 1;
                            self.state.menu_nav.settings_core_option_index = 0;
                        } else {
                            self.state.menu_nav.focus_region =
                                MenuFocusRegion::SettingsAppConfigSave;
                        }
                    }
                    MenuNavDirection::Left => {
                        self.state.menu_nav.settings_core_option_index = self
                            .state
                            .menu_nav
                            .settings_core_option_index
                            .saturating_sub(1);
                    }
                    MenuNavDirection::Right => {
                        self.state.menu_nav.settings_core_option_index =
                            (self.state.menu_nav.settings_core_option_index + 1)
                                .min(num_options.saturating_sub(1));
                    }
                }
            }
            MenuFocusRegion::SettingsAppConfigSave => match direction {
                MenuNavDirection::Up => {
                    if num_vars > 0 {
                        self.state.menu_nav.settings_core_variable_index = num_vars - 1;
                        self.state.menu_nav.settings_core_option_index = 0;
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigCoreVariable;
                    } else {
                        self.state.menu_nav.focus_region =
                            MenuFocusRegion::SettingsAppConfigCoreTab;
                    }
                }
                MenuNavDirection::Down => {
                    self.state.menu_nav.focus_region = MenuFocusRegion::SettingsCoverSettings;
                }
                MenuNavDirection::Left | MenuNavDirection::Right => {}
            },
            MenuFocusRegion::SettingsCoverSettings => match direction {
                MenuNavDirection::Up => {
                    self.state.menu_nav.focus_region = MenuFocusRegion::SettingsAppConfigSave;
                }
                MenuNavDirection::Down => {}
                MenuNavDirection::Left => {
                    self.state.menu_nav.settings_cover_action_index = self
                        .state
                        .menu_nav
                        .settings_cover_action_index
                        .saturating_sub(1);
                }
                MenuNavDirection::Right => {
                    self.state.menu_nav.settings_cover_action_index =
                        (self.state.menu_nav.settings_cover_action_index + 1).min(1);
                }
            },
            _ => {
                self.state.menu_nav.focus_region = MenuFocusRegion::SettingsAppConfigCoreTab;
            }
        }
    }

    fn activate_settings_focus(&mut self) {
        match self.state.menu_nav.focus_region {
            MenuFocusRegion::TopNav => {
                let view = self.state.menu_nav.selected_top_nav_view();
                self.navigate_to_view(view);
            }
            MenuFocusRegion::SettingsAppConfigCoreTab => {
                // Selecting a tab moves focus into the first variable.
                let profiles = arcade_domain::core_profiles();
                let tab_idx = self
                    .state
                    .menu_nav
                    .settings_core_tab_index
                    .min(profiles.len().saturating_sub(1));
                if let Some(core_name) = profiles.get(tab_idx).map(|p| p.core_name) {
                    self.state.manage.settings_selected_core = core_name.to_string();
                }
                let has_vars = profiles
                    .get(tab_idx)
                    .map(|p| !p.variables.is_empty())
                    .unwrap_or(false);
                if has_vars {
                    self.state.menu_nav.settings_core_variable_index = 0;
                    self.state.menu_nav.settings_core_option_index = 0;
                    self.state.menu_nav.focus_region =
                        MenuFocusRegion::SettingsAppConfigCoreVariable;
                }
            }
            MenuFocusRegion::SettingsAppConfigCoreVariable => {
                // Apply the currently highlighted option to the in-memory settings map.
                let profiles = arcade_domain::core_profiles();
                let tab_idx = self
                    .state
                    .menu_nav
                    .settings_core_tab_index
                    .min(profiles.len().saturating_sub(1));
                if let Some(profile) = profiles.get(tab_idx) {
                    let var_idx = self
                        .state
                        .menu_nav
                        .settings_core_variable_index
                        .min(profile.variables.len().saturating_sub(1));
                    if let Some(var_def) = profile.variables.get(var_idx) {
                        let opt_idx = self
                            .state
                            .menu_nav
                            .settings_core_option_index
                            .min(var_def.options.len().saturating_sub(1));
                        if let Some(chosen) = var_def.options.get(opt_idx) {
                            self.state
                                .manage
                                .settings_core_values
                                .entry(profile.core_name.to_string())
                                .or_default()
                                .insert(var_def.key.to_string(), chosen.value.to_string());
                        }
                    }
                }
            }
            MenuFocusRegion::SettingsAppConfigSave => {
                if !self.state.manage.job_running {
                    self.save_manage_app_settings();
                }
            }
            MenuFocusRegion::SettingsCoverSettings => {
                match self.state.menu_nav.settings_cover_action_index {
                    0 => {
                        self.state.manage.settings_show_api_key =
                            !self.state.manage.settings_show_api_key;
                    }
                    _ => {
                        if !self.state.manage.job_running {
                            self.save_manage_settings();
                        }
                    }
                }
            }
            _ => {
                self.state.menu_nav.focus_region = MenuFocusRegion::SettingsAppConfigCoreTab;
            }
        }
    }

    fn step_back_settings_focus(&mut self) {
        self.state.menu_nav.focus_region = match self.state.menu_nav.focus_region {
            MenuFocusRegion::TopNav => MenuFocusRegion::TopNav,
            MenuFocusRegion::SettingsAppConfigCoreTab => MenuFocusRegion::TopNav,
            MenuFocusRegion::SettingsAppConfigCoreVariable => {
                if self.state.menu_nav.settings_core_variable_index == 0 {
                    MenuFocusRegion::SettingsAppConfigCoreTab
                } else {
                    self.state.menu_nav.settings_core_variable_index = self
                        .state
                        .menu_nav
                        .settings_core_variable_index
                        .saturating_sub(1);
                    self.state.menu_nav.settings_core_option_index = 0;
                    MenuFocusRegion::SettingsAppConfigCoreVariable
                }
            }
            MenuFocusRegion::SettingsAppConfigSave => {
                MenuFocusRegion::SettingsAppConfigCoreVariable
            }
            MenuFocusRegion::SettingsCoverSettings => MenuFocusRegion::SettingsAppConfigSave,
            _ => MenuFocusRegion::TopNav,
        };
    }
}

#[cfg(feature = "gamepad")]
impl FrontendShortcutState {
    fn merge(&mut self, other: Self) {
        self.reset |= other.reset;
        self.quick_save |= other.quick_save;
        self.quick_load |= other.quick_load;
        self.next_save_slot |= other.next_save_slot;
        self.return_pressed |= other.return_pressed;
    }
}

fn normalize_system_name(system: &str) -> String {
    let normalized = system.trim().to_ascii_uppercase();
    if normalized.is_empty() || normalized == "ALL" {
        String::from("NES")
    } else {
        normalized
    }
}

fn n64_primary_stick_axes_for_preference(
    state: &CanonicalPadState,
    preference: N64PrimaryStick,
) -> (f32, f32) {
    match preference {
        N64PrimaryStick::Left => (state.left_x, n64_native_analog_y(state.left_y)),
        N64PrimaryStick::Right => (state.right_x, n64_native_analog_y(state.right_y)),
    }
}

fn n64_native_analog_y(value: f32) -> f32 {
    -value
}

fn mapping_entry_is_active(
    state: &CanonicalPadState,
    entry: &MappingEntry,
    threshold: f32,
) -> bool {
    match entry {
        MappingEntry::Button { button } => canonical_button_active(state, *button),
        MappingEntry::Axis { axis, direction } => {
            let value = canonical_axis_value(state, *axis);
            if *direction < 0 {
                value <= -threshold
            } else {
                value >= threshold
            }
        }
    }
}

fn frontend_shortcuts_from_mapping(
    mapping: &StoredGamepadMapping,
    state: &CanonicalPadState,
) -> FrontendShortcutState {
    FrontendShortcutState {
        reset: mapped_action_is_active(mapping, RESET_ACTION, state),
        quick_save: mapped_action_is_active(mapping, QUICK_SAVE_ACTION, state),
        quick_load: mapped_action_is_active(mapping, QUICK_LOAD_ACTION, state),
        next_save_slot: mapped_action_is_active(mapping, NEXT_SAVE_SLOT_ACTION, state),
        return_pressed: mapped_exit_action_is_active(mapping, state),
    }
}

fn mapped_exit_action_is_active(mapping: &StoredGamepadMapping, state: &CanonicalPadState) -> bool {
    if mapping.actions.contains_key(EXIT_ACTION) {
        mapped_action_is_active(mapping, EXIT_ACTION, state)
    } else {
        state.guide
    }
}

fn mapped_action_is_active(
    mapping: &StoredGamepadMapping,
    action: &str,
    state: &CanonicalPadState,
) -> bool {
    let Some(Some(entry)) = mapping.actions.get(action) else {
        return false;
    };
    mapping_entry_is_active(state, entry, mapping.threshold)
}

fn consume_rising_edge(was_held: &mut bool, is_held: bool) -> bool {
    let triggered = is_held && !*was_held;
    *was_held = is_held;
    triggered
}

fn canonical_button_active(state: &CanonicalPadState, button: CanonicalButton) -> bool {
    match button {
        CanonicalButton::South => state.south,
        CanonicalButton::East => state.east,
        CanonicalButton::North => state.north,
        CanonicalButton::West => state.west,
        CanonicalButton::DPadUp => state.dpad_up,
        CanonicalButton::DPadDown => state.dpad_down,
        CanonicalButton::DPadLeft => state.dpad_left,
        CanonicalButton::DPadRight => state.dpad_right,
        CanonicalButton::Start => state.start,
        CanonicalButton::Select => state.select,
        CanonicalButton::LeftShoulder => state.left_shoulder,
        CanonicalButton::RightShoulder => state.right_shoulder,
        CanonicalButton::Guide => state.guide,
        CanonicalButton::LeftThumb => state.left_thumb,
        CanonicalButton::RightThumb => state.right_thumb,
    }
}

fn canonical_axis_value(state: &CanonicalPadState, axis: CanonicalAxis) -> f32 {
    match axis {
        CanonicalAxis::LeftStickX => state.left_x,
        CanonicalAxis::LeftStickY => state.left_y,
        CanonicalAxis::RightStickX => state.right_x,
        CanonicalAxis::RightStickY => state.right_y,
        CanonicalAxis::LeftTrigger => state.left_trigger,
        CanonicalAxis::RightTrigger => state.right_trigger,
    }
}

fn action_to_retro_binding(system: &str, action: &str) -> Option<RetroActionBinding> {
    if normalize_system_name(system) == "N64" {
        return match action {
            "Up" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_UP)),
            "Down" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_DOWN)),
            "Left" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_LEFT)),
            "Right" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_RIGHT)),
            "A" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_B)),
            "B" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_Y)),
            "L" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L)),
            "R" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_R)),
            "Start" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_START)),
            "Z" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L2)),
            "C-Up" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_X)),
            "C-Right" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_A)),
            "C-Left" => Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_RIGHT,
                axis_id: RETRO_DEVICE_ID_ANALOG_X,
                value: -1.0,
            }),
            "C-Down" => Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_RIGHT,
                axis_id: RETRO_DEVICE_ID_ANALOG_Y,
                value: 1.0,
            }),
            _ => None,
        };
    }

    match action {
        "Up" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_UP)),
        "Down" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_DOWN)),
        "Left" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_LEFT)),
        "Right" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_RIGHT)),
        "A" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_A)),
        "B" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_B)),
        "X" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_X)),
        "Y" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_Y)),
        "L" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L)),
        "R" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_R)),
        "Start" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_START)),
        "Select" | "Coin" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_SELECT)),
        "C" | "C-Up" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_X)),
        "D" | "C-Left" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_Y)),
        "C-Right" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_A)),
        "C-Down" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_B)),
        "Z" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L2)),
        _ => None,
    }
}

#[cfg(feature = "gamepad")]
fn build_detected_pad_identity(id: GamepadId, gamepad: &gilrs::Gamepad<'_>) -> DetectedPadIdentity {
    let vendor_id = gamepad.vendor_id().map(|value| format!("{value:04x}"));
    let product_id = gamepad.product_id().map(|value| format!("{value:04x}"));
    let name = gamepad.name().trim().to_string();
    let device_key =
        if let (Some(vendor_id), Some(product_id)) = (vendor_id.as_ref(), product_id.as_ref()) {
            format!("{vendor_id}:{product_id}:{name}")
        } else if !name.is_empty() {
            name.clone()
        } else {
            format!("gilrs-id-{id}")
        };

    DetectedPadIdentity {
        device_key,
        name,
        vendor_id,
        product_id,
        mapping_name: Some(format!("{:?}", gamepad.mapping_source())),
    }
}

#[cfg(feature = "gamepad")]
fn is_playable_gamepad(gamepad: &gilrs::Gamepad<'_>) -> bool {
    [
        gamepad.button_code(Button::South),
        gamepad.button_code(Button::East),
        gamepad.button_code(Button::Start),
        gamepad.button_code(Button::Select),
        gamepad.button_code(Button::DPadUp),
        gamepad.button_code(Button::DPadDown),
        gamepad.button_code(Button::DPadLeft),
        gamepad.button_code(Button::DPadRight),
    ]
    .into_iter()
    .flatten()
    .next()
    .is_some()
        || [
            gamepad.axis_code(Axis::LeftStickX),
            gamepad.axis_code(Axis::LeftStickY),
        ]
        .into_iter()
        .flatten()
        .next()
        .is_some()
}

#[cfg(feature = "gamepad")]
fn button_down(gamepad: &gilrs::Gamepad<'_>, button: Button) -> bool {
    gamepad
        .button_data(button)
        .map(|data| data.is_pressed() || data.value() >= BUTTON_ACTIVE_THRESHOLD)
        .unwrap_or_else(|| gamepad.is_pressed(button))
}

#[cfg(feature = "gamepad")]
fn gamepad_has_dpad_buttons(gamepad: &gilrs::Gamepad<'_>) -> bool {
    [
        Button::DPadUp,
        Button::DPadDown,
        Button::DPadLeft,
        Button::DPadRight,
    ]
    .into_iter()
    .any(|button| gamepad.button_code(button).is_some())
}

#[cfg(feature = "gamepad")]
fn update_raw_dpad_state_from_event(cache: &mut HashMap<usize, RawDpadState>, event: gilrs::Event) {
    use gilrs::EventType;

    let id = usize::from(event.id);
    match event.event {
        EventType::Disconnected => {
            cache.remove(&id);
        }
        EventType::ButtonPressed(button, code) => {
            let state = cache.entry(id).or_default();
            apply_dpad_button_event(state, button, code, true);
        }
        EventType::ButtonReleased(button, code) => {
            let state = cache.entry(id).or_default();
            apply_dpad_button_event(state, button, code, false);
        }
        EventType::ButtonChanged(button, value, code) => {
            let state = cache.entry(id).or_default();
            apply_dpad_button_event(state, button, code, value >= BUTTON_ACTIVE_THRESHOLD);
        }
        EventType::AxisChanged(axis, value, code) => {
            let state = cache.entry(id).or_default();
            apply_dpad_axis_event(state, axis, code, value);
        }
        _ => {}
    }
}

#[cfg(feature = "gamepad")]
fn apply_dpad_button_event(
    state: &mut RawDpadState,
    button: Button,
    code: Code,
    pressed: bool,
) {
    match button {
        Button::DPadUp => {
            state.up = pressed;
            if pressed {
                state.down = false;
            }
        }
        Button::DPadDown => {
            state.down = pressed;
            if pressed {
                state.up = false;
            }
        }
        Button::DPadLeft => {
            state.left = pressed;
            if pressed {
                state.right = false;
            }
        }
        Button::DPadRight => {
            state.right = pressed;
            if pressed {
                state.left = false;
            }
        }
        Button::Unknown => {
            let (page, usage) = decode_hid_page_usage(code);
            if !apply_dpad_button_usage(state, page, usage, pressed) {
                return;
            }
        }
        _ => return,
    }

    state.axis_x = if state.left && !state.right {
        -1.0
    } else if state.right && !state.left {
        1.0
    } else {
        0.0
    };
    state.axis_y = if state.up && !state.down {
        -1.0
    } else if state.down && !state.up {
        1.0
    } else {
        0.0
    };
}

#[cfg(feature = "gamepad")]
fn apply_dpad_axis_event(state: &mut RawDpadState, axis: Axis, code: Code, value: f32) {
    match axis {
        Axis::DPadX => state.axis_x = value,
        Axis::DPadY => state.axis_y = value,
        Axis::Unknown => {
            let (page, usage) = decode_hid_page_usage(code);
            if page == HID_PAGE_GENERIC_DESKTOP && usage == HID_USAGE_GD_HATSWITCH_X {
                state.axis_x = value;
            } else if page == HID_PAGE_GENERIC_DESKTOP && usage == HID_USAGE_GD_HATSWITCH_Y {
                state.axis_y = value;
            } else if apply_dpad_axis_usage(state, page, usage, value) {
                // handled by explicit D-pad usage decoding.
            } else {
                return;
            }
        }
        _ => return,
    }
    state.left = state.axis_x <= -DIGITAL_FALLBACK_THRESHOLD;
    state.right = state.axis_x >= DIGITAL_FALLBACK_THRESHOLD;
    state.up = state.axis_y <= -DIGITAL_FALLBACK_THRESHOLD;
    state.down = state.axis_y >= DIGITAL_FALLBACK_THRESHOLD;
}

#[cfg(feature = "gamepad")]
fn apply_dpad_axis_usage(state: &mut RawDpadState, page: u16, usage: u16, value: f32) -> bool {
    let active = value >= BUTTON_ACTIVE_THRESHOLD;
    match (page, usage) {
        (HID_PAGE_BUTTON, HID_USAGE_BTN_DPAD_UP) | (HID_PAGE_GENERIC_DESKTOP, HID_USAGE_GD_DPAD_UP) => {
            state.up = active;
            state.down = false;
            state.axis_y = if active { -1.0 } else { 0.0 };
            true
        }
        (HID_PAGE_BUTTON, HID_USAGE_BTN_DPAD_DOWN)
        | (HID_PAGE_GENERIC_DESKTOP, HID_USAGE_GD_DPAD_DOWN) => {
            state.down = active;
            state.up = false;
            state.axis_y = if active { 1.0 } else { 0.0 };
            true
        }
        (HID_PAGE_BUTTON, HID_USAGE_BTN_DPAD_LEFT)
        | (HID_PAGE_GENERIC_DESKTOP, HID_USAGE_GD_DPAD_LEFT) => {
            state.left = active;
            state.right = false;
            state.axis_x = if active { -1.0 } else { 0.0 };
            true
        }
        (HID_PAGE_BUTTON, HID_USAGE_BTN_DPAD_RIGHT)
        | (HID_PAGE_GENERIC_DESKTOP, HID_USAGE_GD_DPAD_RIGHT) => {
            state.right = active;
            state.left = false;
            state.axis_x = if active { 1.0 } else { 0.0 };
            true
        }
        _ => false,
    }
}

#[cfg(feature = "gamepad")]
fn decode_hid_page_usage(code: Code) -> (u16, u16) {
    let raw = code.into_u32();
    (((raw >> 16) & 0xFFFF) as u16, (raw & 0xFFFF) as u16)
}

#[cfg(feature = "gamepad")]
fn apply_dpad_button_usage(state: &mut RawDpadState, page: u16, usage: u16, pressed: bool) -> bool {
    match (page, usage) {
        (HID_PAGE_BUTTON, HID_USAGE_BTN_DPAD_UP) | (HID_PAGE_GENERIC_DESKTOP, HID_USAGE_GD_DPAD_UP) => {
            state.up = pressed;
            if pressed {
                state.down = false;
            }
            true
        }
        (HID_PAGE_BUTTON, HID_USAGE_BTN_DPAD_DOWN) | (HID_PAGE_GENERIC_DESKTOP, HID_USAGE_GD_DPAD_DOWN) => {
            state.down = pressed;
            if pressed {
                state.up = false;
            }
            true
        }
        (HID_PAGE_BUTTON, HID_USAGE_BTN_DPAD_LEFT) | (HID_PAGE_GENERIC_DESKTOP, HID_USAGE_GD_DPAD_LEFT) => {
            state.left = pressed;
            if pressed {
                state.right = false;
            }
            true
        }
        (HID_PAGE_BUTTON, HID_USAGE_BTN_DPAD_RIGHT)
        | (HID_PAGE_GENERIC_DESKTOP, HID_USAGE_GD_DPAD_RIGHT) => {
            state.right = pressed;
            if pressed {
                state.left = false;
            }
            true
        }
        _ => false,
    }
}

#[cfg(feature = "gamepad")]
fn merge_dpad_axis_value(mapped: f32, raw: f32) -> f32 {
    if raw.abs() > mapped.abs() {
        raw
    } else {
        mapped
    }
}

#[cfg(feature = "gamepad")]
fn direction_active(
    button_pressed: bool,
    dpad_axis_value: f32,
    fallback_axis_value: f32,
    positive: bool,
) -> bool {
    button_pressed
        || axis_direction_active(dpad_axis_value, positive, DIGITAL_FALLBACK_THRESHOLD)
        || axis_direction_active(fallback_axis_value, positive, DIGITAL_FALLBACK_THRESHOLD)
}

#[cfg(feature = "gamepad")]
fn axis_direction_active(value: f32, positive: bool, threshold: f32) -> bool {
    if positive {
        value >= threshold
    } else {
        value <= -threshold
    }
}

#[cfg(feature = "gamepad")]
fn normalized_axis(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

#[cfg(feature = "gamepad")]
fn merge_trigger_axis_and_button(axis_value: f32, trigger_button_pressed: bool) -> f32 {
    normalized_axis(axis_value).max(if trigger_button_pressed { 1.0 } else { 0.0 })
}

#[cfg(feature = "gamepad")]
fn trigger_axis_value(gamepad: &gilrs::Gamepad<'_>, axis: Axis, trigger_button: Button) -> f32 {
    merge_trigger_axis_and_button(gamepad.value(axis), button_down(gamepad, trigger_button))
}

#[cfg(feature = "gamepad")]
fn debug_gamepad_capture(
    gamepad: &gilrs::Gamepad<'_>,
    has_dpad_buttons: bool,
    raw_dpad_x: f32,
    raw_dpad_y: f32,
    raw_left_x: f32,
    raw_left_y: f32,
    state: &CanonicalPadState,
) {
    if std::env::var_os("ARCADE_INPUT_DEBUG").is_none() {
        return;
    }
    let dpad_button_activity = gamepad.is_pressed(Button::DPadUp)
        || gamepad.is_pressed(Button::DPadDown)
        || gamepad.is_pressed(Button::DPadLeft)
        || gamepad.is_pressed(Button::DPadRight);
    let dpad_axis_activity = raw_dpad_x.abs() >= DIGITAL_FALLBACK_THRESHOLD
        || raw_dpad_y.abs() >= DIGITAL_FALLBACK_THRESHOLD;
    let left_stick_activity = raw_left_x.abs() >= DIGITAL_FALLBACK_THRESHOLD
        || raw_left_y.abs() >= DIGITAL_FALLBACK_THRESHOLD;
    let mapped_direction_activity =
        state.dpad_up || state.dpad_down || state.dpad_left || state.dpad_right;

    if !dpad_button_activity
        && !dpad_axis_activity
        && !left_stick_activity
        && !mapped_direction_activity
    {
        return;
    }

    debug!(
        target: "arcade_input",
        name = %gamepad.name(),
        has_dpad_buttons,
        dpad_up_pressed = gamepad.is_pressed(Button::DPadUp),
        dpad_down_pressed = gamepad.is_pressed(Button::DPadDown),
        dpad_left_pressed = gamepad.is_pressed(Button::DPadLeft),
        dpad_right_pressed = gamepad.is_pressed(Button::DPadRight),
        raw_dpad_x,
        raw_dpad_y,
        raw_left_x,
        raw_left_y,
        mapped_up = state.dpad_up,
        mapped_down = state.dpad_down,
        mapped_left = state.dpad_left,
        mapped_right = state.dpad_right,
        "gamepad directional capture"
    );
}

#[cfg(feature = "gamepad")]
fn format_gamepad_capture_debug_line(
    gamepad: &gilrs::Gamepad<'_>,
    has_dpad_buttons: bool,
    raw_dpad_x: f32,
    raw_dpad_y: f32,
    raw_left_x: f32,
    raw_left_y: f32,
    state: &CanonicalPadState,
) -> String {
    format!(
        "INPUT {} | btns U{} D{} L{} R{} | has_btns {} | dpad ({:.2},{:.2}) | stick ({:.2},{:.2}) | mapped U{} D{} L{} R{}",
        gamepad.name(),
        if gamepad.is_pressed(Button::DPadUp) { 1 } else { 0 },
        if gamepad.is_pressed(Button::DPadDown) { 1 } else { 0 },
        if gamepad.is_pressed(Button::DPadLeft) { 1 } else { 0 },
        if gamepad.is_pressed(Button::DPadRight) { 1 } else { 0 },
        if has_dpad_buttons { 1 } else { 0 },
        raw_dpad_x,
        raw_dpad_y,
        raw_left_x,
        raw_left_y,
        if state.dpad_up { 1 } else { 0 },
        if state.dpad_down { 1 } else { 0 },
        if state.dpad_left { 1 } else { 0 },
        if state.dpad_right { 1 } else { 0 },
    )
}

#[cfg(feature = "gamepad")]
fn should_invert_vertical_axis(vendor_id: Option<u16>, name: &str) -> bool {
    if vendor_id == Some(SONY_VENDOR_ID) {
        return true;
    }

    let normalized_name = name.trim().to_ascii_lowercase();
    normalized_name.contains("dualsense")
        || normalized_name.contains("dualshock")
        || normalized_name.contains("playstation")
        || normalized_name == "wireless controller"
}

#[cfg(feature = "gamepad")]
fn normalize_vertical_axis(value: f32, invert: bool) -> f32 {
    if invert { -value } else { value }
}

#[cfg(feature = "gamepad")]
fn normalize_dpad_vertical_axis(value: f32) -> f32 {
    value
}

fn system_supports_native_analog(system: &str) -> bool {
    matches!(normalize_system_name(system).as_str(), "N64")
}

fn normalize_mapping_threshold(value: f32) -> f32 {
    let normalized = value.clamp(0.1, 0.95);
    (normalized * 100.0).round() / 100.0
}

fn set_button(app: &NativeArcadeUiApp, port: u32, joypad_id: u32, pressed: bool) {
    let value = if pressed { 1.0 } else { 0.0 };
    set_analog_button(app, port, joypad_id, value);
    app.host.set_input_state(
        port,
        RETRO_DEVICE_JOYPAD,
        0,
        joypad_id,
        if pressed {
            RETRO_BUTTON_PRESSED_VALUE
        } else {
            0
        },
    );
}

fn set_analog_button(app: &NativeArcadeUiApp, port: u32, joypad_id: u32, value: f32) {
    let scaled = (value.clamp(0.0, 1.0) * RETRO_BUTTON_PRESSED_VALUE as f32).round() as i16;
    app.host.set_input_state(
        port,
        RETRO_DEVICE_ANALOG,
        RETRO_DEVICE_INDEX_ANALOG_BUTTON,
        joypad_id,
        scaled,
    );
}

fn set_analog(app: &NativeArcadeUiApp, port: u32, index: u32, axis_id: u32, value: f32) {
    let scaled = (value.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
    app.host
        .set_input_state(port, RETRO_DEVICE_ANALOG, index, axis_id, scaled);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "gamepad")]
    #[test]
    fn direction_active_accepts_windows_dpad_axes_for_diagonals() {
        assert!(direction_active(false, 1.0, 0.0, true));
        assert!(direction_active(false, -1.0, 0.0, false));
        assert!(direction_active(false, 0.8, 0.8, true));
        assert!(!direction_active(false, 0.0, 0.0, true));
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn direction_active_keeps_left_stick_fallback_when_no_dpad_signal_exists() {
        assert!(direction_active(false, 0.0, 0.7, true));
        assert!(direction_active(false, 0.0, -0.7, false));
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn dpad_direction_active_uses_axis_when_buttons_are_not_available() {
        assert!(dpad_direction_active_from_parts(false, 1.0, 0.0, true));
        assert!(dpad_direction_active_from_parts(false, -1.0, 0.0, false));
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn trigger_axis_button_fallback_supports_button_only_pads() {
        assert_eq!(merge_trigger_axis_and_button(0.0, false), 0.0);
        assert_eq!(merge_trigger_axis_and_button(0.0, true), 1.0);
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn trigger_axis_button_fallback_keeps_analog_values() {
        assert_eq!(merge_trigger_axis_and_button(0.42, false), 0.42);
        assert_eq!(merge_trigger_axis_and_button(0.75, true), 1.0);
        assert_eq!(merge_trigger_axis_and_button(-1.0, false), 0.0);
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn normalize_vertical_axis_keeps_non_windows_convention() {
        assert_eq!(normalize_vertical_axis(0.75, false), 0.75);
        assert_eq!(normalize_vertical_axis(-0.75, false), -0.75);
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn normalize_vertical_axis_inverts_when_requested() {
        assert_eq!(normalize_vertical_axis(0.75, true), -0.75);
        assert_eq!(normalize_vertical_axis(-0.75, true), 0.75);
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn normalize_dpad_vertical_axis_keeps_gilrs_axis_direction() {
        assert_eq!(normalize_dpad_vertical_axis(1.0), 1.0);
        assert_eq!(normalize_dpad_vertical_axis(-1.0), -1.0);
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn apply_dpad_button_usage_maps_hid_codes() {
        let mut state = RawDpadState::default();
        assert!(apply_dpad_button_usage(
            &mut state,
            HID_PAGE_BUTTON,
            HID_USAGE_BTN_DPAD_LEFT,
            true
        ));
        assert!(state.left);
        assert!(apply_dpad_button_usage(
            &mut state,
            HID_PAGE_GENERIC_DESKTOP,
            HID_USAGE_GD_DPAD_UP,
            true
        ));
        assert!(state.up);
        assert!(!apply_dpad_button_usage(&mut state, HID_PAGE_BUTTON, 0x1234, true));
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn apply_dpad_button_usage_keeps_up_down_exclusive_when_pressed() {
        let mut state = RawDpadState::default();
        assert!(apply_dpad_button_usage(
            &mut state,
            HID_PAGE_BUTTON,
            HID_USAGE_BTN_DPAD_UP,
            true
        ));
        assert!(state.up);
        assert!(!state.down);
        assert!(apply_dpad_button_usage(
            &mut state,
            HID_PAGE_BUTTON,
            HID_USAGE_BTN_DPAD_DOWN,
            true
        ));
        assert!(state.down);
        assert!(!state.up);
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn merge_dpad_axis_value_prefers_nonzero_raw_signal() {
        assert_eq!(merge_dpad_axis_value(0.0, 1.0), 1.0);
        assert_eq!(merge_dpad_axis_value(-1.0, 0.0), -1.0);
        assert_eq!(merge_dpad_axis_value(0.5, -0.9), -0.9);
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn ps_controller_detection_requests_vertical_inversion() {
        assert!(should_invert_vertical_axis(Some(SONY_VENDOR_ID), "DualSense Wireless Controller"));
        assert!(should_invert_vertical_axis(None, "Wireless Controller"));
        assert!(!should_invert_vertical_axis(Some(0x045e), "Xbox Wireless Controller"));
    }

    #[test]
    fn only_n64_forwards_native_analog_axes_by_default() {
        assert!(!system_supports_native_analog("NES"));
        assert!(!system_supports_native_analog("SNES"));
        assert!(!system_supports_native_analog("ARCADE"));
        assert!(system_supports_native_analog("N64"));
        assert!(system_supports_native_analog("n64"));
    }

    #[test]
    fn n64_action_bindings_match_libretro_controller_layout() {
        assert_eq!(
            action_to_retro_binding("N64", "A"),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_B))
        );
        assert_eq!(
            action_to_retro_binding("N64", "B"),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_Y))
        );
        assert_eq!(
            action_to_retro_binding("N64", "C-Up"),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_X))
        );
        assert_eq!(
            action_to_retro_binding("N64", "C-Right"),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_A))
        );
        assert_eq!(
            action_to_retro_binding("N64", "C-Left"),
            Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_RIGHT,
                axis_id: RETRO_DEVICE_ID_ANALOG_X,
                value: -1.0,
            })
        );
        assert_eq!(
            action_to_retro_binding("N64", "C-Down"),
            Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_RIGHT,
                axis_id: RETRO_DEVICE_ID_ANALOG_Y,
                value: 1.0,
            })
        );
        assert_eq!(
            action_to_retro_binding("N64", "Z"),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L2))
        );
    }

    #[test]
    fn n64_primary_stick_axes_for_preference_uses_selected_stick() {
        let state = CanonicalPadState {
            left_x: 0.25,
            left_y: -0.5,
            right_x: -0.75,
            right_y: 0.9,
            ..Default::default()
        };

        assert_eq!(
            n64_primary_stick_axes_for_preference(&state, N64PrimaryStick::Left),
            (0.25, 0.5)
        );
        assert_eq!(
            n64_primary_stick_axes_for_preference(&state, N64PrimaryStick::Right),
            (-0.75, -0.9)
        );
    }

    #[cfg(feature = "gamepad")]
    fn dpad_direction_active_from_parts(
        has_dpad_buttons: bool,
        dpad_axis_value: f32,
        fallback_axis_value: f32,
        positive: bool,
    ) -> bool {
        let dpad_axis_active = if has_dpad_buttons {
            false
        } else {
            axis_direction_active(dpad_axis_value, positive, DIGITAL_FALLBACK_THRESHOLD)
        };

        dpad_axis_active
            || axis_direction_active(fallback_axis_value, positive, DIGITAL_FALLBACK_THRESHOLD)
    }
}
