use std::collections::{BTreeMap, HashSet};
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

#[cfg(feature = "gamepad")]
use crate::state::{
    ControllerAssignmentSource, ControllerInputButtonDebug, ControllerInputDebugSnapshot,
};
use crate::{
    app::NativeArcadeUiApp,
    state::{ControllerMappingCacheKey, MenuFocusRegion, MenuNavDirection},
};
#[cfg(feature = "gamepad")]
use gilrs::{ev::Code, Axis, Button, GamepadId};
#[cfg(feature = "gamepad")]
use std::collections::HashMap;

const RETRO_DEVICE_JOYPAD: u32 = 1;
const RETRO_DEVICE_KEYBOARD: u32 = 3;
const RETRO_DEVICE_ANALOG: u32 = 5;
const RETRO_BUTTON_PRESSED_VALUE: i16 = i16::MAX;
const RETROKMOD_SHIFT: u16 = 0x01;
const RETROKMOD_CTRL: u16 = 0x02;
const RETROKMOD_ALT: u16 = 0x04;
const RETROKMOD_META: u16 = 0x08;
const RETROK_ESCAPE: u32 = 27;
const RETROK_BACKSPACE: u32 = 8;
const RETROK_TAB: u32 = 9;
const RETROK_RETURN: u32 = 13;
const RETROK_SPACE: u32 = 32;
const RETROK_DELETE: u32 = 127;
const RETROK_UP: u32 = 273;
const RETROK_DOWN: u32 = 274;
const RETROK_RIGHT: u32 = 275;
const RETROK_LEFT: u32 = 276;
const RETROK_INSERT: u32 = 277;
const RETROK_HOME: u32 = 278;
const RETROK_END: u32 = 279;
const RETROK_PAGEUP: u32 = 280;
const RETROK_PAGEDOWN: u32 = 281;
const RETROK_LSHIFT: u32 = 304;
const RETROK_LCTRL: u32 = 306;
const RETROK_LALT: u32 = 308;

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
const RETRO_DEVICE_ID_JOYPAD_R2: u32 = 13;
const RETRO_DEVICE_ID_JOYPAD_L3: u32 = 14;
const RETRO_DEVICE_ID_JOYPAD_R3: u32 = 15;

const RETRO_DEVICE_INDEX_ANALOG_LEFT: u32 = 0;
const RETRO_DEVICE_INDEX_ANALOG_RIGHT: u32 = 1;
const RETRO_DEVICE_INDEX_ANALOG_BUTTON: u32 = 2;
const RETRO_DEVICE_ID_ANALOG_X: u32 = 0;
const RETRO_DEVICE_ID_ANALOG_Y: u32 = 1;
const DIRECTIONAL_SHORTCUT_GUARD_THRESHOLD: f32 = 0.45;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetroKeyboardRouting {
    Passthrough,
    SystemMapping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShortcutDirectionalGuardMode {
    Disabled,
    Enabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuntimeInputPolicy {
    keyboard_routing: RetroKeyboardRouting,
    uses_primary_stick_selector: bool,
    supports_native_analog: bool,
    shortcut_directional_guard: ShortcutDirectionalGuardMode,
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

#[derive(Clone)]
pub(crate) struct ControllerMappingTarget {
    pub(crate) identity: DetectedPadIdentity,
    pub(crate) player_slot: Option<u8>,
    pub(crate) connect_seq: u64,
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

#[cfg(feature = "gamepad")]
#[derive(Clone, Copy)]
enum RuntimeMappingSource {
    DeviceOverride,
    SystemDefault,
    BuiltInDefault,
}

#[cfg(feature = "gamepad")]
impl RuntimeMappingSource {
    fn label(self) -> &'static str {
        match self {
            RuntimeMappingSource::DeviceOverride => "Active Device Override",
            RuntimeMappingSource::SystemDefault => "System Default",
            RuntimeMappingSource::BuiltInDefault => "System Default (Built-in)",
        }
    }
}

#[cfg(feature = "gamepad")]
struct RuntimeEffectiveMappingProfile {
    system: String,
    mapping_key: String,
    source: RuntimeMappingSource,
}

impl NativeArcadeUiApp {
    pub(crate) fn apply_keyboard_and_gamepad_input(&mut self, ctx: &egui::Context) {
        self.host.clear_input_state();

        let system = self.active_input_system();
        let input_policy =
            runtime_input_policy_for_system(&system, self.host.has_keyboard_callback());
        let dos_keyboard_passthrough = matches!(
            input_policy.keyboard_routing,
            RetroKeyboardRouting::Passthrough
        );
        let primary_stick_preference = if input_policy.uses_primary_stick_selector {
            Some(self.services.n64_primary_stick())
        } else {
            None
        };
        let mut shortcuts = FrontendShortcutState::default();
        if !dos_keyboard_passthrough {
            let keyboard_state = self.capture_keyboard_state(ctx);
            shortcuts = self.apply_system_mapping_to_host(
                0,
                &keyboard_state,
                &system,
                &input_policy,
                None,
                primary_stick_preference,
            );
        }
        self.sync_retro_keyboard_state(ctx, dos_keyboard_passthrough);
        let escape_pressed = ctx.input(|i| i.key_down(egui::Key::Escape));
        shortcuts.return_pressed |= escape_pressed;

        #[cfg(feature = "gamepad")]
        for capture in self.capture_gamepad_state(&system) {
            let capture_shortcuts = self.apply_system_mapping_to_host(
                capture.port,
                &capture.state,
                &system,
                &input_policy,
                Some(&capture.identity),
                primary_stick_preference,
            );
            shortcuts.merge(capture_shortcuts);
        }

        self.apply_frontend_shortcuts(shortcuts);
    }

    fn sync_retro_keyboard_state(&mut self, ctx: &egui::Context, defer_tap_releases: bool) {
        if !self.host.has_keyboard_callback() {
            self.prev_keyboard_keys_down.clear();
            self.retro_keys_pressed_since_frame.clear();
            self.pending_retro_key_releases.clear();
            return;
        }

        let (events, keys_down, current_modifiers) =
            ctx.input(|i| (i.events.clone(), i.keys_down.clone(), i.modifiers));
        let retro_events = collect_retro_keyboard_events(
            &mut self.prev_keyboard_keys_down,
            &mut self.retro_keys_pressed_since_frame,
            &events,
            &keys_down,
            current_modifiers,
            defer_tap_releases,
            !defer_tap_releases,
        );
        for (down, keycode, character, key_modifiers) in retro_events.immediate_events {
            self.host
                .send_keyboard_event(down, keycode, character, key_modifiers);
        }
        self.pending_retro_key_releases
            .extend(retro_events.deferred_releases);

        if defer_tap_releases {
            for keycode in collect_retro_keyboard_polled_keys(&keys_down, current_modifiers) {
                self.host
                    .set_input_state(0, RETRO_DEVICE_KEYBOARD, 0, keycode, 1);
            }
        }
    }

    pub(crate) fn mark_retro_keyboard_frame_advanced(&mut self) {
        self.retro_keys_pressed_since_frame.clear();
    }

    pub(crate) fn flush_deferred_retro_keyboard_releases(&mut self) {
        if self.pending_retro_key_releases.is_empty() {
            return;
        }
        if !self.host.has_keyboard_callback() {
            self.pending_retro_key_releases.clear();
            return;
        }

        let deferred = std::mem::take(&mut self.pending_retro_key_releases);
        for (keycode, key_modifiers) in deferred {
            self.host
                .send_keyboard_event(false, keycode, 0, key_modifiers);
        }
    }

    pub(crate) fn tick_frontend_navigation(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.focused) {
            #[cfg(feature = "gamepad")]
            {
                let system = self.active_input_system();
                let _ = self.capture_gamepad_state(&system);
            }
            self.clear_menu_repeat_state();
            ctx.request_repaint_after(Duration::from_millis(120));
            return;
        }

        if matches!(self.state.current_view, crate::app::AppView::Settings)
            && self.state.controller_input_debug.open
        {
            #[cfg(feature = "gamepad")]
            {
                let system = self.active_input_system();
                let _ = self.capture_gamepad_state(&system);
            }
            self.clear_menu_repeat_state();
            ctx.request_repaint_after(Duration::from_millis(50));
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

    #[cfg(feature = "gamepad")]
    pub(crate) fn sync_controller_mapping_editor(&mut self, targets: &[ControllerMappingTarget]) {
        let system = self.state.controller_mapping.input_system.clone();
        if system == "ALL" || system.is_empty() {
            return;
        }

        self.reconcile_selected_controller_mapping_target(targets);
        let Some(selected_target) = self.selected_controller_mapping_target(targets) else {
            return;
        };
        let desired_key = selected_target.identity.device_key.clone();

        if !self
            .state
            .controller_mapping
            .needs_reload(&system, &desired_key)
        {
            return;
        }

        let mapping = self.resolved_mapping_for(&system, Some(&selected_target.identity));
        let threshold = normalize_mapping_threshold(mapping.threshold);

        self.state.controller_mapping.load_from_mapping(
            system,
            desired_key,
            mapping.as_ref().clone(),
            threshold,
        );
    }

    #[cfg(not(feature = "gamepad"))]
    pub(crate) fn sync_controller_mapping_editor(&mut self, _targets: &[ControllerMappingTarget]) {}

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

    pub(crate) fn clear_controller_mapping_editor_bindings(&mut self) {
        let system = self.state.controller_mapping.input_system.clone();
        if system == "ALL" || system.is_empty() {
            return;
        }

        clear_mapping_actions(
            &mut self.state.controller_mapping.actions,
            supported_gamepad_actions(&system),
        );
    }

    #[cfg(feature = "gamepad")]
    pub(crate) fn request_controller_mapping_target_switch(
        &mut self,
        target: &ControllerMappingTarget,
    ) {
        if self.state.controller_mapping.selected_device_key()
            == Some(target.identity.device_key.as_str())
        {
            return;
        }

        if self.controller_mapping_is_dirty() {
            self.state.controller_mapping.queue_pending_device_switch(
                target.identity.device_key.clone(),
                target.identity.name.clone(),
            );
        } else {
            self.state
                .controller_mapping
                .set_selected_device_key(Some(target.identity.device_key.clone()));
        }
    }

    #[cfg(feature = "gamepad")]
    pub(crate) fn cancel_pending_controller_mapping_target_switch(&mut self) {
        self.state.controller_mapping.clear_pending_device_switch();
    }

    #[cfg(feature = "gamepad")]
    pub(crate) fn confirm_pending_controller_mapping_target_switch(&mut self) {
        self.state.controller_mapping.apply_pending_device_switch();
    }

    #[cfg(not(feature = "gamepad"))]
    pub(crate) fn request_controller_mapping_target_switch(
        &mut self,
        _target: &ControllerMappingTarget,
    ) {
    }

    #[cfg(not(feature = "gamepad"))]
    pub(crate) fn cancel_pending_controller_mapping_target_switch(&mut self) {}

    #[cfg(not(feature = "gamepad"))]
    pub(crate) fn confirm_pending_controller_mapping_target_switch(&mut self) {}

    pub(crate) fn save_controller_mapping_from_editor(&mut self) {
        let system = self.state.controller_mapping.input_system.clone();
        if system == "ALL" || system.is_empty() {
            self.state.status =
                String::from("Choose a specific system before saving a controller mapping.");
            return;
        }

        let targets = self.connected_controller_mapping_targets();
        self.reconcile_selected_controller_mapping_target(&targets);
        let Some(target) = self.selected_controller_mapping_target(&targets) else {
            self.state.status =
                String::from("No controller connected for saving a device mapping.");
            return;
        };
        let device = &target.identity;
        let mapping_key = device.device_key.as_str();
        let vendor_id = device.vendor_id.as_deref();
        let product_id = device.product_id.as_deref();
        let device_meta = Some(StoredDeviceMeta {
            id: Some(device.device_key.clone()),
            mapping: device.mapping_name.clone(),
        });
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
                self.state.status =
                    format!("Saved {} controller mapping for {}.", system, mapping_key);
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

    fn reconcile_selected_controller_mapping_target(
        &mut self,
        targets: &[ControllerMappingTarget],
    ) {
        if targets.is_empty() {
            self.state.controller_mapping.set_selected_device_key(None);
            return;
        }

        let pending_key = self
            .state
            .controller_mapping
            .pending_device_switch()
            .map(|(key, _)| key.to_string());
        if let Some(key) = pending_key {
            if !targets
                .iter()
                .any(|target| target.identity.device_key == key)
            {
                self.state.controller_mapping.clear_pending_device_switch();
            }
        }

        let selected_valid = self.state.controller_mapping.selected_device_key();
        let next_selected = resolved_selected_mapping_device_key(selected_valid, targets);
        self.state
            .controller_mapping
            .set_selected_device_key(next_selected);
    }

    fn selected_controller_mapping_target<'a>(
        &self,
        targets: &'a [ControllerMappingTarget],
    ) -> Option<&'a ControllerMappingTarget> {
        let selected_key = self.state.controller_mapping.selected_device_key()?;
        targets
            .iter()
            .find(|target| target.identity.device_key == selected_key)
    }

    #[cfg(feature = "gamepad")]
    pub(crate) fn connected_controller_mapping_targets(&mut self) -> Vec<ControllerMappingTarget> {
        let connected = self.poll_connected_gamepads();
        let Some(gilrs) = self.gilrs.as_ref() else {
            return Vec::new();
        };
        let identity_cache = &mut self.gamepad_identity_cache;
        let mut targets = Vec::new();
        for (id_usize, id, is_playable) in connected {
            if !is_playable {
                continue;
            }
            let gamepad = gilrs.gamepad(id);
            let identity = if let Some(existing) = identity_cache.get(&id_usize) {
                existing.clone()
            } else {
                let built = build_detected_pad_identity(id, &gamepad);
                identity_cache.insert(id_usize, built.clone());
                built
            };
            targets.push(ControllerMappingTarget {
                identity,
                player_slot: self.gamepad_slot_assignments.get(&id_usize).copied(),
                connect_seq: self
                    .gamepad_connect_order
                    .get(&id_usize)
                    .copied()
                    .unwrap_or(u64::MAX),
            });
        }

        targets.sort_by(|left, right| {
            match (left.player_slot, right.player_slot) {
                (Some(a), Some(b)) => a.cmp(&b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => left.connect_seq.cmp(&right.connect_seq),
            }
            .then(left.identity.name.cmp(&right.identity.name))
        });
        targets
    }

    #[cfg(not(feature = "gamepad"))]
    pub(crate) fn connected_controller_mapping_targets(&mut self) -> Vec<ControllerMappingTarget> {
        Vec::new()
    }

    #[cfg(feature = "gamepad")]
    fn poll_connected_gamepads(&mut self) -> Vec<(usize, GamepadId, bool)> {
        let Some(gilrs) = self.gilrs.as_mut() else {
            return Vec::new();
        };

        while let Some(event) = gilrs.next_event() {
            update_raw_dpad_state_from_event(&mut self.raw_dpad_state_cache, event);
        }

        let mut connected = gilrs
            .gamepads()
            .filter(|(_, gamepad)| gamepad.is_connected())
            .map(|(id, gamepad)| (usize::from(id), id, is_playable_gamepad(&gamepad)))
            .collect::<Vec<_>>();
        connected.sort_by(|left, right| left.0.cmp(&right.0));

        let connected_ids = connected.iter().map(|(id, _, _)| *id).collect::<Vec<_>>();
        let connected_id_set = connected_ids.iter().copied().collect::<HashSet<_>>();
        let playable_ids = connected
            .iter()
            .filter_map(|(id, _, is_playable)| is_playable.then_some(*id))
            .collect::<Vec<_>>();

        self.raw_dpad_state_cache
            .retain(|id, _| connected_id_set.contains(id));
        self.gamepad_identity_cache
            .retain(|id, _| connected_id_set.contains(id));

        reconcile_player_slot_assignments(
            &mut self.gamepad_connect_order,
            &mut self.gamepad_slot_assignments,
            &mut self.next_gamepad_connect_seq,
            &connected_ids,
            &playable_ids,
            MAX_GAMEPAD_PLAYERS,
        );

        connected.sort_by(|left, right| {
            match (
                self.gamepad_slot_assignments.get(&left.0).copied(),
                self.gamepad_slot_assignments.get(&right.0).copied(),
            ) {
                (Some(a), Some(b)) => a.cmp(&b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => self
                    .gamepad_connect_order
                    .get(&left.0)
                    .copied()
                    .unwrap_or(u64::MAX)
                    .cmp(
                        &self
                            .gamepad_connect_order
                            .get(&right.0)
                            .copied()
                            .unwrap_or(u64::MAX),
                    ),
            }
            .then(left.0.cmp(&right.0))
        });
        connected
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
        let system = self.active_input_system();
        #[cfg(feature = "gamepad")]
        for capture in self.capture_gamepad_state(&system) {
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
    fn capture_gamepad_state(&mut self, runtime_system: &str) -> Vec<GamepadCapture> {
        if self.gilrs.is_none() {
            self.state.input_debug.clear();
            self.state.controller_input_debug.snapshots.clear();
            self.state.controller_input_debug.connected_total = 0;
            self.state.controller_input_debug.assigned_playable_total = 0;
            self.state.controller_input_debug.unassigned_total = 0;
            return Vec::new();
        }
        let normalized_runtime_system = normalize_system_name(runtime_system);
        let debug_open = self.state.controller_input_debug.open;
        let saved_mapping_keys = if debug_open {
            self.services
                .list_gamepad_mappings(&normalized_runtime_system)
                .map(|records| {
                    records
                        .into_iter()
                        .map(|record| record.name)
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut connected = {
            let raw_dpad_state_cache = &mut self.raw_dpad_state_cache;
            let Some(gilrs) = self.gilrs.as_mut() else {
                self.state.input_debug.clear();
                self.state.controller_input_debug.snapshots.clear();
                self.state.controller_input_debug.connected_total = 0;
                self.state.controller_input_debug.assigned_playable_total = 0;
                self.state.controller_input_debug.unassigned_total = 0;
                return Vec::new();
            };
            while let Some(event) = gilrs.next_event() {
                update_raw_dpad_state_from_event(raw_dpad_state_cache, event);
            }
            gilrs
                .gamepads()
                .filter(|(_, gamepad)| gamepad.is_connected())
                .map(|(id, gamepad)| (usize::from(id), id, is_playable_gamepad(&gamepad)))
                .collect::<Vec<_>>()
        };
        connected.sort_by(|left, right| left.0.cmp(&right.0));

        let connected_ids = connected.iter().map(|(id, _, _)| *id).collect::<Vec<_>>();
        let connected_id_set = connected_ids.iter().copied().collect::<HashSet<_>>();
        let playable_ids = connected
            .iter()
            .filter_map(|(id, _, playable)| playable.then_some(*id))
            .collect::<Vec<_>>();

        self.raw_dpad_state_cache
            .retain(|id, _| connected_id_set.contains(id));
        self.gamepad_identity_cache
            .retain(|id, _| connected_id_set.contains(id));

        reconcile_player_slot_assignments(
            &mut self.gamepad_connect_order,
            &mut self.gamepad_slot_assignments,
            &mut self.next_gamepad_connect_seq,
            &connected_ids,
            &playable_ids,
            MAX_GAMEPAD_PLAYERS,
        );

        connected.sort_by(|left, right| {
            let left_slot = self.gamepad_slot_assignments.get(&left.0).copied();
            let right_slot = self.gamepad_slot_assignments.get(&right.0).copied();
            match (left_slot, right_slot) {
                (Some(a), Some(b)) => a.cmp(&b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => {
                    let left_seq = self
                        .gamepad_connect_order
                        .get(&left.0)
                        .copied()
                        .unwrap_or(u64::MAX);
                    let right_seq = self
                        .gamepad_connect_order
                        .get(&right.0)
                        .copied()
                        .unwrap_or(u64::MAX);
                    left_seq.cmp(&right_seq).then(left.0.cmp(&right.0))
                }
            }
        });

        let Some(gilrs) = self.gilrs.as_ref() else {
            self.state.input_debug.clear();
            self.state.controller_input_debug.snapshots.clear();
            self.state.controller_input_debug.connected_total = 0;
            self.state.controller_input_debug.assigned_playable_total = 0;
            self.state.controller_input_debug.unassigned_total = 0;
            return Vec::new();
        };

        let assigned_playable_total = self.gamepad_slot_assignments.len();
        let mut captures = Vec::with_capacity(assigned_playable_total);
        let mut debug_snapshots = Vec::with_capacity(connected.len());
        let mut input_debug = String::new();
        let input_debug_enabled = std::env::var_os("ARCADE_INPUT_DEBUG").is_some();
        let identity_cache = &mut self.gamepad_identity_cache;
        for (id_usize, id, is_playable) in connected {
            let gamepad = gilrs.gamepad(id);
            let raw_dpad = self
                .raw_dpad_state_cache
                .get(&id_usize)
                .copied()
                .unwrap_or_default();
            let invert_vertical = should_invert_vertical_axis(gamepad.vendor_id(), gamepad.name());
            let has_dpad_buttons = gamepad_has_dpad_buttons(&gamepad);
            let has_mapped_dpad_axes = gamepad.axis_code(Axis::DPadX).is_some()
                || gamepad.axis_code(Axis::DPadY).is_some();
            let has_raw_dpad_state = raw_dpad.up
                || raw_dpad.down
                || raw_dpad.left
                || raw_dpad.right
                || raw_dpad.axis_x.abs() >= DIGITAL_FALLBACK_THRESHOLD
                || raw_dpad.axis_y.abs() >= DIGITAL_FALLBACK_THRESHOLD;
            let has_explicit_dpad_input =
                has_dpad_buttons || has_mapped_dpad_axes || has_raw_dpad_state;
            let raw_dpad_x = gamepad.value(Axis::DPadX);
            let raw_dpad_y = gamepad.value(Axis::DPadY);
            let effective_raw_dpad_x = merge_dpad_axis_value(raw_dpad_x, raw_dpad.axis_x);
            let effective_raw_dpad_y = merge_dpad_axis_value(raw_dpad_y, raw_dpad.axis_y);
            let raw_left_x = gamepad.value(Axis::LeftStickX);
            let raw_left_y = gamepad.value(Axis::LeftStickY);
            let raw_right_x = gamepad.value(Axis::RightStickX);
            let raw_right_y = gamepad.value(Axis::RightStickY);
            let dpad_up_debug = button_debug_data(&gamepad, Button::DPadUp);
            let dpad_down_debug = button_debug_data(&gamepad, Button::DPadDown);
            let dpad_left_debug = button_debug_data(&gamepad, Button::DPadLeft);
            let dpad_right_debug = button_debug_data(&gamepad, Button::DPadRight);
            let mut guide_debug = button_debug_data(&gamepad, Button::Mode);
            let mut right_thumb_debug = button_debug_data(&gamepad, Button::RightThumb);
            let dpad_up_pressed = (has_dpad_buttons && dpad_up_debug.is_pressed) || raw_dpad.up;
            let dpad_down_pressed =
                (has_dpad_buttons && dpad_down_debug.is_pressed) || raw_dpad.down;
            let dpad_left_pressed =
                (has_dpad_buttons && dpad_left_debug.is_pressed) || raw_dpad.left;
            let dpad_right_pressed =
                (has_dpad_buttons && dpad_right_debug.is_pressed) || raw_dpad.right;
            let dpad_debugs = [
                &dpad_up_debug,
                &dpad_down_debug,
                &dpad_left_debug,
                &dpad_right_debug,
            ];
            let dpad_x = effective_raw_dpad_x;
            // gilrs already normalizes DPadY with platform reversal rules; applying
            // our Sony stick inversion here would double-invert on macOS.
            let dpad_y = normalize_dpad_vertical_axis(effective_raw_dpad_y);
            let left_x = raw_left_x;
            let left_y = normalize_vertical_axis(raw_left_y, invert_vertical);
            let right_y = normalize_vertical_axis(raw_right_y, invert_vertical);
            let dpad_fallback_x = if has_explicit_dpad_input { 0.0 } else { left_x };
            let dpad_fallback_y = if has_explicit_dpad_input { 0.0 } else { left_y };
            let explicit_dpad_up_active =
                explicit_dpad_direction_active(dpad_up_pressed, dpad_y, false);
            let explicit_dpad_down_active =
                explicit_dpad_direction_active(dpad_down_pressed, dpad_y, true);
            let explicit_dpad_left_active =
                explicit_dpad_direction_active(dpad_left_pressed, dpad_x, false);
            let explicit_dpad_right_active =
                explicit_dpad_direction_active(dpad_right_pressed, dpad_x, true);
            let explicit_dpad_active = explicit_dpad_up_active
                || explicit_dpad_down_active
                || explicit_dpad_left_active
                || explicit_dpad_right_active;
            guide_debug.effective_is_pressed = strict_safety_effective_non_dpad_button(
                &guide_debug,
                &dpad_debugs,
                explicit_dpad_active,
            );
            right_thumb_debug.effective_is_pressed = strict_safety_effective_non_dpad_button(
                &right_thumb_debug,
                &dpad_debugs,
                explicit_dpad_active,
            );
            let dpad_up_active = direction_active(dpad_up_pressed, dpad_y, dpad_fallback_y, false);
            let dpad_down_active =
                direction_active(dpad_down_pressed, dpad_y, dpad_fallback_y, true);
            let dpad_left_active =
                direction_active(dpad_left_pressed, dpad_x, dpad_fallback_x, false);
            let dpad_right_active =
                direction_active(dpad_right_pressed, dpad_x, dpad_fallback_x, true);
            let state = CanonicalPadState {
                dpad_up: dpad_up_active,
                dpad_down: dpad_down_active,
                dpad_left: dpad_left_active,
                dpad_right: dpad_right_active,
                south: button_pressed_digital(&gamepad, Button::South),
                east: button_pressed_digital(&gamepad, Button::East),
                north: button_pressed_digital(&gamepad, Button::North),
                west: button_pressed_digital(&gamepad, Button::West),
                left_shoulder: button_pressed_digital(&gamepad, Button::LeftTrigger),
                right_shoulder: button_pressed_digital(&gamepad, Button::RightTrigger),
                guide: guide_debug.effective_is_pressed,
                left_trigger: trigger_axis_value(&gamepad, Axis::LeftZ, Button::LeftTrigger2),
                right_trigger: trigger_axis_value(&gamepad, Axis::RightZ, Button::RightTrigger2),
                select: button_pressed_digital(&gamepad, Button::Select),
                start: button_pressed_digital(&gamepad, Button::Start),
                left_thumb: button_pressed_digital(&gamepad, Button::LeftThumb),
                right_thumb: right_thumb_debug.effective_is_pressed,
                left_x,
                left_y,
                right_x: raw_right_x,
                right_y,
            };

            debug_gamepad_capture(
                &gamepad,
                has_dpad_buttons,
                effective_raw_dpad_x,
                effective_raw_dpad_y,
                raw_left_x,
                raw_left_y,
                raw_right_x,
                raw_right_y,
                &state,
            );
            let cache_key = id_usize;
            let identity = if let Some(existing) = identity_cache.get(&cache_key) {
                existing.clone()
            } else {
                let built = build_detected_pad_identity(id, &gamepad);
                identity_cache.insert(cache_key, built.clone());
                built
            };
            let runtime_profile = if debug_open {
                Some(resolve_runtime_mapping_profile_from_keys(
                    &normalized_runtime_system,
                    Some(identity.device_key.as_str()),
                    &saved_mapping_keys,
                ))
            } else {
                None
            };
            let assigned_slot = self.gamepad_slot_assignments.get(&cache_key).copied();
            let assignment_source = if !is_playable {
                ControllerAssignmentSource::Unsupported
            } else if assigned_slot.is_some() {
                ControllerAssignmentSource::Assigned
            } else {
                ControllerAssignmentSource::UnassignedOverLimit
            };
            let connect_seq = self
                .gamepad_connect_order
                .get(&cache_key)
                .copied()
                .unwrap_or(u64::MAX);
            if input_debug_enabled && input_debug.is_empty() && assigned_slot.is_some() {
                input_debug = format_gamepad_capture_debug_line(
                    &gamepad,
                    has_dpad_buttons,
                    effective_raw_dpad_x,
                    effective_raw_dpad_y,
                    raw_left_x,
                    raw_left_y,
                    raw_right_x,
                    raw_right_y,
                    &state,
                );
            }

            debug_snapshots.push(ControllerInputDebugSnapshot {
                player_slot: assigned_slot,
                is_playable,
                assignment_source,
                connect_seq,
                name: identity.name.clone(),
                vendor_id: identity.vendor_id.clone(),
                product_id: identity.product_id.clone(),
                mapping_name: identity.mapping_name.clone(),
                runtime_system: runtime_profile
                    .as_ref()
                    .map(|profile| profile.system.clone()),
                runtime_mapping_key: runtime_profile
                    .as_ref()
                    .map(|profile| profile.mapping_key.clone()),
                runtime_mapping_source: runtime_profile
                    .as_ref()
                    .map(|profile| profile.source.label().to_string()),
                dpad_up: state.dpad_up,
                dpad_down: state.dpad_down,
                dpad_left: state.dpad_left,
                dpad_right: state.dpad_right,
                south: state.south,
                east: state.east,
                north: state.north,
                west: state.west,
                left_shoulder: state.left_shoulder,
                right_shoulder: state.right_shoulder,
                left_trigger: state.left_trigger,
                right_trigger: state.right_trigger,
                select: state.select,
                start: state.start,
                left_thumb: state.left_thumb,
                raw_dpad_x: effective_raw_dpad_x,
                raw_dpad_y: effective_raw_dpad_y,
                raw_left_x,
                raw_left_y,
                raw_right_x,
                raw_right_y,
                mapped_left_x: state.left_x,
                mapped_left_y: state.left_y,
                mapped_right_x: state.right_x,
                mapped_right_y: state.right_y,
                dpad_up_debug,
                dpad_down_debug,
                guide_debug,
                right_thumb_debug,
            });

            if let Some(slot) = assigned_slot {
                captures.push(GamepadCapture {
                    port: slot as u32,
                    identity,
                    state,
                });
            }
        }
        captures.sort_by_key(|capture| capture.port);
        sort_debug_snapshots_for_display(&mut debug_snapshots);

        self.state.input_debug = input_debug;
        let connected_total = debug_snapshots.len();
        self.state.controller_input_debug.snapshots = debug_snapshots;
        self.state.controller_input_debug.connected_total = connected_total;
        self.state.controller_input_debug.assigned_playable_total = assigned_playable_total;
        self.state.controller_input_debug.unassigned_total =
            connected_total.saturating_sub(assigned_playable_total);
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
        input_policy: &RuntimeInputPolicy,
        device: Option<&DetectedPadIdentity>,
        primary_stick_preference: Option<N64PrimaryStick>,
    ) -> FrontendShortcutState {
        let mapping = self.resolved_mapping_for(system, device);
        let use_explicit_n64_stick_mapping =
            normalize_system_name(system) == "N64" && n64_control_stick_mapping_enabled(&mapping);

        for action in supported_gamepad_actions(system) {
            let Some(Some(entry)) = mapping.actions.get(*action) else {
                continue;
            };
            if !mapping_entry_is_active(state, entry, mapping.threshold) {
                continue;
            }

            if let Some(binding) = action_to_retro_binding(system, action, primary_stick_preference)
            {
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

        if input_policy.supports_native_analog && !use_explicit_n64_stick_mapping {
            let preference = primary_stick_preference.unwrap_or(N64PrimaryStick::Left);
            let (primary_x, primary_y) = primary_stick_axes_for_system(system, state, preference);
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

        frontend_shortcuts_from_mapping(&mapping, state, input_policy)
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
                MenuNavDirection::Left => {
                    self.state.menu_nav.manage_scrape_action_index = self
                        .state
                        .menu_nav
                        .manage_scrape_action_index
                        .saturating_sub(1);
                }
                MenuNavDirection::Right => {
                    self.state.menu_nav.manage_scrape_action_index =
                        (self.state.menu_nav.manage_scrape_action_index + 1).min(1);
                }
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
                    match self.state.menu_nav.manage_scrape_action_index {
                        0 => self.start_scrape_job(),
                        _ => self.start_relink_local_covers_job(),
                    }
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

fn resolved_selected_mapping_device_key(
    current: Option<&str>,
    targets: &[ControllerMappingTarget],
) -> Option<String> {
    if targets.is_empty() {
        return None;
    }
    if let Some(current) = current {
        if targets
            .iter()
            .any(|target| target.identity.device_key == current)
        {
            return Some(current.to_string());
        }
    }
    Some(targets[0].identity.device_key.clone())
}

fn clear_mapping_actions(
    actions: &mut BTreeMap<String, Option<MappingEntry>>,
    supported_actions: &[&str],
) {
    for action in supported_actions {
        actions.insert((*action).to_string(), None);
    }
}

fn n64_control_stick_mapping_enabled(mapping: &StoredGamepadMapping) -> bool {
    ["Stick Up", "Stick Down", "Stick Left", "Stick Right"]
        .into_iter()
        .any(|action| {
            mapping
                .actions
                .get(action)
                .is_some_and(|entry| entry.is_some())
        })
}

#[cfg(feature = "gamepad")]
fn reconcile_player_slot_assignments(
    connect_order: &mut HashMap<usize, u64>,
    assignments: &mut HashMap<usize, u8>,
    next_connect_seq: &mut u64,
    connected_ids: &[usize],
    playable_ids: &[usize],
    max_players: u8,
) {
    let connected_set = connected_ids.iter().copied().collect::<HashSet<_>>();
    for id in connected_ids {
        if connect_order.contains_key(id) {
            continue;
        }
        connect_order.insert(*id, *next_connect_seq);
        *next_connect_seq = next_connect_seq.saturating_add(1);
    }
    connect_order.retain(|id, _| connected_set.contains(id));

    let playable_set = playable_ids.iter().copied().collect::<HashSet<_>>();
    assignments.retain(|id, slot| {
        connected_set.contains(id) && playable_set.contains(id) && *slot < max_players
    });

    let mut used_slots = assignments.values().copied().collect::<HashSet<_>>();
    let mut unassigned_playable = playable_ids
        .iter()
        .filter(|id| connected_set.contains(id) && !assignments.contains_key(id))
        .copied()
        .collect::<Vec<_>>();
    unassigned_playable.sort_by(|left, right| {
        let left_seq = connect_order.get(left).copied().unwrap_or(u64::MAX);
        let right_seq = connect_order.get(right).copied().unwrap_or(u64::MAX);
        left_seq.cmp(&right_seq).then(left.cmp(right))
    });

    for id in unassigned_playable {
        if let Some(slot) = next_available_player_slot(max_players, &used_slots) {
            assignments.insert(id, slot);
            used_slots.insert(slot);
        } else {
            break;
        }
    }
}

#[cfg(feature = "gamepad")]
fn next_available_player_slot(max_players: u8, used_slots: &HashSet<u8>) -> Option<u8> {
    (0..max_players).find(|slot| !used_slots.contains(slot))
}

#[cfg(feature = "gamepad")]
fn sort_debug_snapshots_for_display(snapshots: &mut [ControllerInputDebugSnapshot]) {
    snapshots.sort_by(|left, right| {
        let left_rank = match left.assignment_source {
            ControllerAssignmentSource::Assigned => 0,
            ControllerAssignmentSource::UnassignedOverLimit => 1,
            ControllerAssignmentSource::Unsupported => 2,
        };
        let right_rank = match right.assignment_source {
            ControllerAssignmentSource::Assigned => 0,
            ControllerAssignmentSource::UnassignedOverLimit => 1,
            ControllerAssignmentSource::Unsupported => 2,
        };
        left_rank
            .cmp(&right_rank)
            .then(
                left.player_slot
                    .unwrap_or(u8::MAX)
                    .cmp(&right.player_slot.unwrap_or(u8::MAX)),
            )
            .then(left.connect_seq.cmp(&right.connect_seq))
            .then(left.name.cmp(&right.name))
    });
}

#[cfg(feature = "gamepad")]
fn resolve_runtime_mapping_profile_from_keys(
    system: &str,
    device_key: Option<&str>,
    saved_mapping_keys: &[String],
) -> RuntimeEffectiveMappingProfile {
    if let Some(device_key) = device_key {
        if saved_mapping_keys.iter().any(|key| key == device_key) {
            return RuntimeEffectiveMappingProfile {
                system: system.to_string(),
                mapping_key: device_key.to_string(),
                source: RuntimeMappingSource::DeviceOverride,
            };
        }
    }

    let source = if saved_mapping_keys
        .iter()
        .any(|key| key == SYSTEM_DEFAULT_MAPPING_KEY)
    {
        RuntimeMappingSource::SystemDefault
    } else {
        RuntimeMappingSource::BuiltInDefault
    };
    RuntimeEffectiveMappingProfile {
        system: system.to_string(),
        mapping_key: SYSTEM_DEFAULT_MAPPING_KEY.to_string(),
        source,
    }
}

fn primary_stick_axes_for_system(
    system: &str,
    state: &CanonicalPadState,
    preference: N64PrimaryStick,
) -> (f32, f32) {
    let (x, y) = match preference {
        N64PrimaryStick::Left => (state.left_x, state.left_y),
        N64PrimaryStick::Right => (state.right_x, state.right_y),
    };
    (x, native_analog_y_for_system(system, y))
}

fn native_analog_y_for_system(system: &str, value: f32) -> f32 {
    if system.eq_ignore_ascii_case("N64") {
        -value
    } else {
        value
    }
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
    input_policy: &RuntimeInputPolicy,
) -> FrontendShortcutState {
    FrontendShortcutState {
        reset: mapped_action_is_active(mapping, RESET_ACTION, state),
        quick_save: mapped_shortcut_action_is_active(
            mapping,
            QUICK_SAVE_ACTION,
            state,
            input_policy,
        ),
        quick_load: mapped_shortcut_action_is_active(
            mapping,
            QUICK_LOAD_ACTION,
            state,
            input_policy,
        ),
        next_save_slot: mapped_shortcut_action_is_active(
            mapping,
            NEXT_SAVE_SLOT_ACTION,
            state,
            input_policy,
        ),
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

fn mapped_shortcut_action_is_active(
    mapping: &StoredGamepadMapping,
    action: &str,
    state: &CanonicalPadState,
    input_policy: &RuntimeInputPolicy,
) -> bool {
    let Some(Some(entry)) = mapping.actions.get(action) else {
        return false;
    };
    if !mapping_entry_is_active(state, entry, mapping.threshold) {
        return false;
    }

    if matches!(
        input_policy.shortcut_directional_guard,
        ShortcutDirectionalGuardMode::Enabled
    ) && directional_input_active(state)
        && !mapping_entry_is_directional(entry)
    {
        return false;
    }

    true
}

fn mapping_entry_is_directional(entry: &MappingEntry) -> bool {
    match entry {
        MappingEntry::Button { button } => matches!(
            button,
            CanonicalButton::DPadUp
                | CanonicalButton::DPadDown
                | CanonicalButton::DPadLeft
                | CanonicalButton::DPadRight
        ),
        MappingEntry::Axis { axis, .. } => matches!(
            axis,
            CanonicalAxis::LeftStickX
                | CanonicalAxis::LeftStickY
                | CanonicalAxis::RightStickX
                | CanonicalAxis::RightStickY
        ),
    }
}

fn directional_input_active(state: &CanonicalPadState) -> bool {
    state.dpad_up
        || state.dpad_down
        || state.dpad_left
        || state.dpad_right
        || state.left_x.abs() >= DIRECTIONAL_SHORTCUT_GUARD_THRESHOLD
        || state.left_y.abs() >= DIRECTIONAL_SHORTCUT_GUARD_THRESHOLD
        || state.right_x.abs() >= DIRECTIONAL_SHORTCUT_GUARD_THRESHOLD
        || state.right_y.abs() >= DIRECTIONAL_SHORTCUT_GUARD_THRESHOLD
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

fn action_to_retro_binding(
    system: &str,
    action: &str,
    primary_stick_preference: Option<N64PrimaryStick>,
) -> Option<RetroActionBinding> {
    let normalized_system = normalize_system_name(system);

    if normalized_system == "N64" {
        let primary_stick = primary_stick_preference.unwrap_or(N64PrimaryStick::Left);
        let c_stick_index = match primary_stick {
            N64PrimaryStick::Left => RETRO_DEVICE_INDEX_ANALOG_RIGHT,
            N64PrimaryStick::Right => RETRO_DEVICE_INDEX_ANALOG_LEFT,
        };
        return match action {
            "Up" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_UP)),
            "Down" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_DOWN)),
            "Left" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_LEFT)),
            "Right" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_RIGHT)),
            "Stick Up" => Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_LEFT,
                axis_id: RETRO_DEVICE_ID_ANALOG_Y,
                value: 1.0,
            }),
            "Stick Down" => Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_LEFT,
                axis_id: RETRO_DEVICE_ID_ANALOG_Y,
                value: -1.0,
            }),
            "Stick Left" => Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_LEFT,
                axis_id: RETRO_DEVICE_ID_ANALOG_X,
                value: -1.0,
            }),
            "Stick Right" => Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_LEFT,
                axis_id: RETRO_DEVICE_ID_ANALOG_X,
                value: 1.0,
            }),
            "A" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_B)),
            "B" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_Y)),
            "L" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L)),
            "R" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_R)),
            "Start" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_START)),
            "Z" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L2)),
            "C-Up" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_X)),
            "C-Right" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_A)),
            "C-Left" => Some(RetroActionBinding::Analog {
                index: c_stick_index,
                axis_id: RETRO_DEVICE_ID_ANALOG_X,
                value: -1.0,
            }),
            "C-Down" => Some(RetroActionBinding::Analog {
                index: c_stick_index,
                axis_id: RETRO_DEVICE_ID_ANALOG_Y,
                value: 1.0,
            }),
            _ => None,
        };
    }

    if normalized_system == "SATURN" {
        return match action {
            "Up" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_UP)),
            "Down" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_DOWN)),
            "Left" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_LEFT)),
            "Right" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_RIGHT)),
            "A" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_B)),
            "B" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_A)),
            "X" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_Y)),
            "Y" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_X)),
            "C" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L2)),
            "Z" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_R2)),
            "L" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L)),
            "R" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_R)),
            "Start" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_START)),
            _ => None,
        };
    }

    match action {
        "Up" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_UP)),
        "Down" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_DOWN)),
        "Left" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_LEFT)),
        "Right" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_RIGHT)),
        "I" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_A)),
        "II" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_B)),
        "A" | "Circle" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_A)),
        "B" | "Cross" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_B)),
        "X" | "Triangle" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_X)),
        "Y" | "Square" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_Y)),
        "L" | "L1" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L)),
        "R" | "R1" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_R)),
        "L2" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L2)),
        "R2" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_R2)),
        "L3" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L3)),
        "R3" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_R3)),
        "Left Stick Up" => Some(RetroActionBinding::Analog {
            index: RETRO_DEVICE_INDEX_ANALOG_LEFT,
            axis_id: RETRO_DEVICE_ID_ANALOG_Y,
            value: -1.0,
        }),
        "Left Stick Down" => Some(RetroActionBinding::Analog {
            index: RETRO_DEVICE_INDEX_ANALOG_LEFT,
            axis_id: RETRO_DEVICE_ID_ANALOG_Y,
            value: 1.0,
        }),
        "Left Stick Left" => Some(RetroActionBinding::Analog {
            index: RETRO_DEVICE_INDEX_ANALOG_LEFT,
            axis_id: RETRO_DEVICE_ID_ANALOG_X,
            value: -1.0,
        }),
        "Left Stick Right" => Some(RetroActionBinding::Analog {
            index: RETRO_DEVICE_INDEX_ANALOG_LEFT,
            axis_id: RETRO_DEVICE_ID_ANALOG_X,
            value: 1.0,
        }),
        "Right Stick Up" => Some(RetroActionBinding::Analog {
            index: RETRO_DEVICE_INDEX_ANALOG_RIGHT,
            axis_id: RETRO_DEVICE_ID_ANALOG_Y,
            value: -1.0,
        }),
        "Right Stick Down" => Some(RetroActionBinding::Analog {
            index: RETRO_DEVICE_INDEX_ANALOG_RIGHT,
            axis_id: RETRO_DEVICE_ID_ANALOG_Y,
            value: 1.0,
        }),
        "Right Stick Left" => Some(RetroActionBinding::Analog {
            index: RETRO_DEVICE_INDEX_ANALOG_RIGHT,
            axis_id: RETRO_DEVICE_ID_ANALOG_X,
            value: -1.0,
        }),
        "Right Stick Right" => Some(RetroActionBinding::Analog {
            index: RETRO_DEVICE_INDEX_ANALOG_RIGHT,
            axis_id: RETRO_DEVICE_ID_ANALOG_X,
            value: 1.0,
        }),
        "Start" | "Run" => Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_START)),
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
fn button_debug_data(gamepad: &gilrs::Gamepad<'_>, button: Button) -> ControllerInputButtonDebug {
    let data = gamepad.button_data(button);
    let raw_pressed = digital_button_active(data.map(|entry| entry.is_pressed()));
    ControllerInputButtonDebug {
        code: gamepad.button_code(button).map(|code| code.into_u32()),
        gilrs_is_pressed: gamepad.is_pressed(button),
        is_pressed: raw_pressed,
        effective_is_pressed: raw_pressed,
        data_pressed: data.map(|entry| entry.is_pressed()),
        data_value: data.map(|entry| entry.value()),
    }
}

fn digital_button_active(data_pressed: Option<bool>) -> bool {
    data_pressed.unwrap_or(false)
}

fn valued_button_active(
    data_pressed: Option<bool>,
    data_value: Option<f32>,
    threshold: f32,
) -> bool {
    digital_button_active(data_pressed) || data_value.unwrap_or(0.0) >= threshold
}

#[cfg(feature = "gamepad")]
fn button_pressed_digital(gamepad: &gilrs::Gamepad<'_>, button: Button) -> bool {
    let data = gamepad.button_data(button);
    digital_button_active(data.map(|d| d.is_pressed()))
}

#[cfg(feature = "gamepad")]
fn button_pressed_with_value_fallback(gamepad: &gilrs::Gamepad<'_>, button: Button) -> bool {
    let data = gamepad.button_data(button);
    valued_button_active(
        data.map(|d| d.is_pressed()),
        data.map(|d| d.value()),
        BUTTON_ACTIVE_THRESHOLD,
    )
}

#[cfg(feature = "gamepad")]
fn deghost_non_dpad_button_against_active_dpad(
    button: &ControllerInputButtonDebug,
    dpad_buttons: &[&ControllerInputButtonDebug],
) -> bool {
    if !button.is_pressed {
        return false;
    }

    let Some(button_code) = button.code else {
        return true;
    };

    let collides_with_active_dpad = dpad_buttons
        .iter()
        .any(|dpad| dpad.is_pressed && dpad.code == Some(button_code));
    !collides_with_active_dpad
}

#[cfg(feature = "gamepad")]
fn strict_safety_effective_non_dpad_button(
    button: &ControllerInputButtonDebug,
    dpad_buttons: &[&ControllerInputButtonDebug],
    dpad_active: bool,
) -> bool {
    if !button.is_pressed {
        return false;
    }
    if !dpad_active {
        return true;
    }

    let Some(button_code) = button.code else {
        return false;
    };

    let mut dpad_codes = HashSet::with_capacity(dpad_buttons.len());
    for dpad in dpad_buttons {
        let Some(dpad_code) = dpad.code else {
            return false;
        };
        if !dpad_codes.insert(dpad_code) {
            return false;
        }
    }

    !dpad_codes.contains(&button_code)
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
fn apply_dpad_button_event(state: &mut RawDpadState, button: Button, code: Code, pressed: bool) {
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
        (HID_PAGE_BUTTON, HID_USAGE_BTN_DPAD_UP)
        | (HID_PAGE_GENERIC_DESKTOP, HID_USAGE_GD_DPAD_UP) => {
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
        (HID_PAGE_BUTTON, HID_USAGE_BTN_DPAD_UP)
        | (HID_PAGE_GENERIC_DESKTOP, HID_USAGE_GD_DPAD_UP) => {
            state.up = pressed;
            if pressed {
                state.down = false;
            }
            true
        }
        (HID_PAGE_BUTTON, HID_USAGE_BTN_DPAD_DOWN)
        | (HID_PAGE_GENERIC_DESKTOP, HID_USAGE_GD_DPAD_DOWN) => {
            state.down = pressed;
            if pressed {
                state.up = false;
            }
            true
        }
        (HID_PAGE_BUTTON, HID_USAGE_BTN_DPAD_LEFT)
        | (HID_PAGE_GENERIC_DESKTOP, HID_USAGE_GD_DPAD_LEFT) => {
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
fn explicit_dpad_direction_active(
    button_pressed: bool,
    dpad_axis_value: f32,
    positive: bool,
) -> bool {
    direction_active(button_pressed, dpad_axis_value, 0.0, positive)
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
    merge_trigger_axis_and_button(
        gamepad.value(axis),
        button_pressed_with_value_fallback(gamepad, trigger_button),
    )
}

#[cfg(feature = "gamepad")]
fn debug_gamepad_capture(
    gamepad: &gilrs::Gamepad<'_>,
    has_dpad_buttons: bool,
    raw_dpad_x: f32,
    raw_dpad_y: f32,
    raw_left_x: f32,
    raw_left_y: f32,
    raw_right_x: f32,
    raw_right_y: f32,
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
    let right_stick_activity = raw_right_x.abs() >= DIGITAL_FALLBACK_THRESHOLD
        || raw_right_y.abs() >= DIGITAL_FALLBACK_THRESHOLD;
    let mapped_direction_activity =
        state.dpad_up || state.dpad_down || state.dpad_left || state.dpad_right;

    if !dpad_button_activity
        && !dpad_axis_activity
        && !left_stick_activity
        && !right_stick_activity
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
        raw_right_x,
        raw_right_y,
        mapped_left_x = state.left_x,
        mapped_left_y = state.left_y,
        mapped_right_x = state.right_x,
        mapped_right_y = state.right_y,
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
    raw_right_x: f32,
    raw_right_y: f32,
    state: &CanonicalPadState,
) -> String {
    format!(
        "INPUT {} | btns U{} D{} L{} R{} | has_btns {} | dpad ({:.2},{:.2}) | Lstick raw({:.2},{:.2}) mapped({:.2},{:.2}) | Rstick raw({:.2},{:.2}) mapped({:.2},{:.2}) | mapped U{} D{} L{} R{}",
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
        state.left_x,
        state.left_y,
        raw_right_x,
        raw_right_y,
        state.right_x,
        state.right_y,
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
    if invert {
        -value
    } else {
        value
    }
}

#[cfg(feature = "gamepad")]
fn normalize_dpad_vertical_axis(value: f32) -> f32 {
    value
}

fn runtime_input_policy_for_system(
    system: &str,
    has_keyboard_callback: bool,
) -> RuntimeInputPolicy {
    let normalized_system = normalize_system_name(system);
    let uses_primary_stick_selector = matches!(normalized_system.as_str(), "N64" | "DREAMCAST");
    RuntimeInputPolicy {
        keyboard_routing: if has_keyboard_callback && normalized_system == "DOS" {
            RetroKeyboardRouting::Passthrough
        } else {
            RetroKeyboardRouting::SystemMapping
        },
        uses_primary_stick_selector,
        supports_native_analog: uses_primary_stick_selector,
        // Keep hook available for targeted re-enable without restoring a global guard.
        shortcut_directional_guard: ShortcutDirectionalGuardMode::Disabled,
    }
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

type RetroKeyboardEvent = (bool, u32, u32, u16);

#[derive(Default)]
struct RetroKeyboardEventBatch {
    immediate_events: Vec<RetroKeyboardEvent>,
    deferred_releases: Vec<(u32, u16)>,
}

fn egui_key_to_retro_keycode(key: egui::Key) -> Option<u32> {
    Some(match key {
        egui::Key::ArrowDown => RETROK_DOWN,
        egui::Key::ArrowLeft => RETROK_LEFT,
        egui::Key::ArrowRight => RETROK_RIGHT,
        egui::Key::ArrowUp => RETROK_UP,
        egui::Key::Escape => RETROK_ESCAPE,
        egui::Key::Tab => RETROK_TAB,
        egui::Key::Backspace => RETROK_BACKSPACE,
        egui::Key::Enter => RETROK_RETURN,
        egui::Key::Space => RETROK_SPACE,
        egui::Key::Insert => RETROK_INSERT,
        egui::Key::Delete => RETROK_DELETE,
        egui::Key::Home => RETROK_HOME,
        egui::Key::End => RETROK_END,
        egui::Key::PageUp => RETROK_PAGEUP,
        egui::Key::PageDown => RETROK_PAGEDOWN,
        egui::Key::Colon => b':' as u32,
        egui::Key::Comma => b',' as u32,
        egui::Key::Backslash => b'\\' as u32,
        egui::Key::Slash => b'/' as u32,
        egui::Key::Pipe => b'|' as u32,
        egui::Key::Questionmark => b'?' as u32,
        egui::Key::Exclamationmark => b'!' as u32,
        egui::Key::OpenBracket => b'[' as u32,
        egui::Key::CloseBracket => b']' as u32,
        egui::Key::OpenCurlyBracket => b'{' as u32,
        egui::Key::CloseCurlyBracket => b'}' as u32,
        egui::Key::Backtick => b'`' as u32,
        egui::Key::Minus => b'-' as u32,
        egui::Key::Period => b'.' as u32,
        egui::Key::Plus => b'+' as u32,
        egui::Key::Equals => b'=' as u32,
        egui::Key::Semicolon => b';' as u32,
        egui::Key::Quote => b'\'' as u32,
        egui::Key::Num0 => b'0' as u32,
        egui::Key::Num1 => b'1' as u32,
        egui::Key::Num2 => b'2' as u32,
        egui::Key::Num3 => b'3' as u32,
        egui::Key::Num4 => b'4' as u32,
        egui::Key::Num5 => b'5' as u32,
        egui::Key::Num6 => b'6' as u32,
        egui::Key::Num7 => b'7' as u32,
        egui::Key::Num8 => b'8' as u32,
        egui::Key::Num9 => b'9' as u32,
        egui::Key::A => b'a' as u32,
        egui::Key::B => b'b' as u32,
        egui::Key::C => b'c' as u32,
        egui::Key::D => b'd' as u32,
        egui::Key::E => b'e' as u32,
        egui::Key::F => b'f' as u32,
        egui::Key::G => b'g' as u32,
        egui::Key::H => b'h' as u32,
        egui::Key::I => b'i' as u32,
        egui::Key::J => b'j' as u32,
        egui::Key::K => b'k' as u32,
        egui::Key::L => b'l' as u32,
        egui::Key::M => b'm' as u32,
        egui::Key::N => b'n' as u32,
        egui::Key::O => b'o' as u32,
        egui::Key::P => b'p' as u32,
        egui::Key::Q => b'q' as u32,
        egui::Key::R => b'r' as u32,
        egui::Key::S => b's' as u32,
        egui::Key::T => b't' as u32,
        egui::Key::U => b'u' as u32,
        egui::Key::V => b'v' as u32,
        egui::Key::W => b'w' as u32,
        egui::Key::X => b'x' as u32,
        egui::Key::Y => b'y' as u32,
        egui::Key::Z => b'z' as u32,
        egui::Key::F1 => 282,
        egui::Key::F2 => 283,
        egui::Key::F3 => 284,
        egui::Key::F4 => 285,
        egui::Key::F5 => 286,
        egui::Key::F6 => 287,
        egui::Key::F7 => 288,
        egui::Key::F8 => 289,
        egui::Key::F9 => 290,
        egui::Key::F10 => 291,
        egui::Key::F11 => 292,
        egui::Key::F12 => 293,
        egui::Key::F13 => 294,
        egui::Key::F14 => 295,
        egui::Key::F15 => 296,
        egui::Key::Copy
        | egui::Key::Cut
        | egui::Key::Paste
        | egui::Key::F16
        | egui::Key::F17
        | egui::Key::F18
        | egui::Key::F19
        | egui::Key::F20
        | egui::Key::F21
        | egui::Key::F22
        | egui::Key::F23
        | egui::Key::F24
        | egui::Key::F25
        | egui::Key::F26
        | egui::Key::F27
        | egui::Key::F28
        | egui::Key::F29
        | egui::Key::F30
        | egui::Key::F31
        | egui::Key::F32
        | egui::Key::F33
        | egui::Key::F34
        | egui::Key::F35 => return None,
    })
}

fn retro_character_for_keycode(keycode: u32) -> u32 {
    if (32..=126).contains(&keycode) {
        keycode
    } else {
        0
    }
}

#[cfg(test)]
fn update_keyboard_key_state(
    keys_down: &mut HashSet<u32>,
    keycode: u32,
    is_pressed: bool,
) -> Option<bool> {
    let was_pressed = keys_down.contains(&keycode);
    if was_pressed == is_pressed {
        return None;
    }

    if is_pressed {
        keys_down.insert(keycode);
    } else {
        keys_down.remove(&keycode);
    }

    Some(is_pressed)
}

fn key_event_to_retro_keycode(key: egui::Key, physical_key: Option<egui::Key>) -> Option<u32> {
    egui_key_to_retro_keycode(key).or_else(|| physical_key.and_then(egui_key_to_retro_keycode))
}

fn retro_key_modifiers_from_egui(modifiers: egui::Modifiers) -> u16 {
    let mut bits = 0u16;
    if modifiers.shift {
        bits |= RETROKMOD_SHIFT;
    }
    if modifiers.ctrl {
        bits |= RETROKMOD_CTRL;
    }
    if modifiers.alt {
        bits |= RETROKMOD_ALT;
    }
    if modifiers.mac_cmd {
        bits |= RETROKMOD_META;
    }
    bits
}

fn collect_retro_keyboard_polled_keys(
    keys_down: &HashSet<egui::Key>,
    modifiers: egui::Modifiers,
) -> HashSet<u32> {
    let mut keys = keys_down
        .iter()
        .copied()
        .filter_map(egui_key_to_retro_keycode)
        .collect::<HashSet<_>>();
    if modifiers.shift {
        keys.insert(RETROK_LSHIFT);
    }
    if modifiers.ctrl {
        keys.insert(RETROK_LCTRL);
    }
    if modifiers.alt {
        keys.insert(RETROK_LALT);
    }
    keys
}

fn collect_retro_keyboard_events(
    prev_keys_down: &mut HashSet<u32>,
    pressed_since_frame: &mut HashSet<u32>,
    events: &[egui::Event],
    keys_down: &HashSet<egui::Key>,
    current_modifiers: egui::Modifiers,
    defer_same_frame_releases: bool,
    reconcile_releases_from_state: bool,
) -> RetroKeyboardEventBatch {
    let mut retro_events = RetroKeyboardEventBatch::default();

    // Consume per-frame key events in order so quick taps (press+release within one frame)
    // and short cross-frame taps still reach cores that depend on keyboard polling.
    for event in events {
        let egui::Event::Key {
            key,
            physical_key,
            pressed,
            modifiers,
            ..
        } = event
        else {
            continue;
        };

        let Some(keycode) = key_event_to_retro_keycode(*key, *physical_key) else {
            continue;
        };
        let key_modifiers = retro_key_modifiers_from_egui(*modifiers);
        if *pressed {
            pressed_since_frame.insert(keycode);
            prev_keys_down.insert(keycode);
        } else {
            prev_keys_down.remove(&keycode);
        }
        let character = if *pressed {
            retro_character_for_keycode(keycode)
        } else {
            0
        };
        if !*pressed && defer_same_frame_releases && pressed_since_frame.contains(&keycode) {
            retro_events
                .deferred_releases
                .push((keycode, key_modifiers));
        } else {
            retro_events
                .immediate_events
                .push((*pressed, keycode, character, key_modifiers));
        }
    }

    // Safety net: reconcile with aggregate key state in case the backend misses a key event.
    let key_modifiers = retro_key_modifiers_from_egui(current_modifiers);
    let current_keys_down = keys_down
        .iter()
        .copied()
        .filter_map(egui_key_to_retro_keycode)
        .collect::<HashSet<_>>();

    let missing_presses = current_keys_down
        .iter()
        .copied()
        .filter(|keycode| !prev_keys_down.contains(keycode))
        .collect::<Vec<_>>();
    for keycode in missing_presses {
        prev_keys_down.insert(keycode);
        retro_events.immediate_events.push((
            true,
            keycode,
            retro_character_for_keycode(keycode),
            key_modifiers,
        ));
    }

    if reconcile_releases_from_state {
        let missing_releases = prev_keys_down
            .iter()
            .copied()
            .filter(|keycode| !current_keys_down.contains(keycode))
            .collect::<Vec<_>>();
        for keycode in missing_releases {
            prev_keys_down.remove(&keycode);
            retro_events
                .immediate_events
                .push((false, keycode, 0, key_modifiers));
        }
    }

    retro_events
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
    fn explicit_dpad_direction_active_ignores_left_stick_fallback() {
        assert!(explicit_dpad_direction_active(false, 1.0, true));
        assert!(explicit_dpad_direction_active(false, -1.0, false));
        assert!(!explicit_dpad_direction_active(false, 0.0, true));
        assert!(direction_active(false, 0.0, 1.0, true));
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

    #[test]
    fn digital_button_active_rejects_value_only_noise() {
        assert!(!digital_button_active(Some(false)));
        assert!(!digital_button_active(None));
    }

    #[test]
    fn digital_button_active_honors_pressed_sources() {
        assert!(digital_button_active(Some(true)));
    }

    #[test]
    fn valued_button_active_keeps_trigger_value_fallback_behavior() {
        assert!(valued_button_active(Some(false), Some(0.20), 0.10));
        assert!(!valued_button_active(Some(false), Some(0.05), 0.10));
    }

    #[test]
    fn keyboard_key_state_emits_transitions_on_press_and_release() {
        let mut keys = HashSet::new();

        assert_eq!(
            update_keyboard_key_state(&mut keys, RETROK_ESCAPE, true),
            Some(true)
        );
        assert!(keys.contains(&RETROK_ESCAPE));
        assert_eq!(
            update_keyboard_key_state(&mut keys, RETROK_ESCAPE, true),
            None
        );
        assert_eq!(
            update_keyboard_key_state(&mut keys, RETROK_ESCAPE, false),
            Some(false)
        );
        assert!(!keys.contains(&RETROK_ESCAPE));
        assert_eq!(
            update_keyboard_key_state(&mut keys, RETROK_ESCAPE, false),
            None
        );
    }

    #[test]
    fn key_events_capture_quick_tap_within_single_frame_with_deferred_release() {
        let mut previous = HashSet::new();
        let mut pressed_since_frame = HashSet::new();
        let events = vec![
            egui::Event::Key {
                key: egui::Key::ArrowUp,
                physical_key: Some(egui::Key::ArrowUp),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Key {
                key: egui::Key::ArrowUp,
                physical_key: Some(egui::Key::ArrowUp),
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        let keys_down = HashSet::new();

        let retro = collect_retro_keyboard_events(
            &mut previous,
            &mut pressed_since_frame,
            &events,
            &keys_down,
            egui::Modifiers::NONE,
            true,
            false,
        );

        assert_eq!(retro.immediate_events.len(), 1);
        assert_eq!(retro.immediate_events[0].0, true);
        assert_eq!(retro.immediate_events[0].1, RETROK_UP);
        assert_eq!(retro.deferred_releases, vec![(RETROK_UP, 0)]);
        assert!(previous.is_empty());
    }

    #[test]
    fn key_state_reconciliation_recovers_missing_press_event() {
        let mut previous = HashSet::new();
        let mut pressed_since_frame = HashSet::new();
        let events = vec![];
        let keys_down = [egui::Key::ArrowLeft].into_iter().collect::<HashSet<_>>();

        let retro = collect_retro_keyboard_events(
            &mut previous,
            &mut pressed_since_frame,
            &events,
            &keys_down,
            egui::Modifiers::NONE,
            true,
            false,
        );

        assert_eq!(retro.immediate_events.len(), 1);
        assert_eq!(retro.immediate_events[0].0, true);
        assert_eq!(retro.immediate_events[0].1, RETROK_LEFT);
        assert!(previous.contains(&RETROK_LEFT));
    }

    #[test]
    fn quick_tap_without_deferral_emits_release_immediately() {
        let mut previous = HashSet::new();
        let mut pressed_since_frame = HashSet::new();
        let events = vec![
            egui::Event::Key {
                key: egui::Key::ArrowUp,
                physical_key: Some(egui::Key::ArrowUp),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Key {
                key: egui::Key::ArrowUp,
                physical_key: Some(egui::Key::ArrowUp),
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        let keys_down = HashSet::new();

        let retro = collect_retro_keyboard_events(
            &mut previous,
            &mut pressed_since_frame,
            &events,
            &keys_down,
            egui::Modifiers::NONE,
            false,
            true,
        );

        assert_eq!(retro.immediate_events.len(), 2);
        assert!(retro.deferred_releases.is_empty());
        assert_eq!(retro.immediate_events[0].0, true);
        assert_eq!(retro.immediate_events[1].0, false);
    }

    #[test]
    fn retro_key_modifiers_maps_shift_ctrl_alt_and_meta() {
        let modifiers = egui::Modifiers {
            alt: true,
            ctrl: true,
            shift: true,
            mac_cmd: true,
            command: true,
        };

        let bits = retro_key_modifiers_from_egui(modifiers);
        assert_eq!(
            bits,
            RETROKMOD_SHIFT | RETROKMOD_CTRL | RETROKMOD_ALT | RETROKMOD_META
        );
    }

    #[test]
    fn reconciliation_uses_current_modifier_state() {
        let mut previous = HashSet::new();
        let mut pressed_since_frame = HashSet::new();
        let events = vec![];
        let keys_down = [egui::Key::A].into_iter().collect::<HashSet<_>>();
        let modifiers = egui::Modifiers {
            alt: false,
            ctrl: true,
            shift: true,
            mac_cmd: false,
            command: true,
        };

        let retro = collect_retro_keyboard_events(
            &mut previous,
            &mut pressed_since_frame,
            &events,
            &keys_down,
            modifiers,
            true,
            true,
        );

        assert_eq!(retro.immediate_events.len(), 1);
        assert_eq!(
            retro.immediate_events[0],
            (
                true,
                b'a' as u32,
                b'a' as u32,
                RETROKMOD_SHIFT | RETROKMOD_CTRL
            )
        );
    }

    #[test]
    fn cross_tick_tap_without_frame_advancement_defers_release() {
        let mut previous = HashSet::new();
        let mut pressed_since_frame = HashSet::new();

        let press_events = vec![egui::Event::Key {
            key: egui::Key::ArrowUp,
            physical_key: Some(egui::Key::ArrowUp),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }];
        let pressed_keys_down = [egui::Key::ArrowUp].into_iter().collect::<HashSet<_>>();
        let press_batch = collect_retro_keyboard_events(
            &mut previous,
            &mut pressed_since_frame,
            &press_events,
            &pressed_keys_down,
            egui::Modifiers::NONE,
            true,
            false,
        );
        assert_eq!(press_batch.immediate_events.len(), 1);
        assert!(press_batch.deferred_releases.is_empty());

        let release_events = vec![egui::Event::Key {
            key: egui::Key::ArrowUp,
            physical_key: Some(egui::Key::ArrowUp),
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }];
        let released_keys_down = HashSet::new();
        let release_batch = collect_retro_keyboard_events(
            &mut previous,
            &mut pressed_since_frame,
            &release_events,
            &released_keys_down,
            egui::Modifiers::NONE,
            true,
            false,
        );
        assert!(release_batch.immediate_events.is_empty());
        assert_eq!(release_batch.deferred_releases, vec![(RETROK_UP, 0)]);
    }

    #[test]
    fn keyboard_polled_state_keeps_held_key_across_ticks() {
        let keys_down = [egui::Key::ArrowDown].into_iter().collect::<HashSet<_>>();
        let modifiers = egui::Modifiers::NONE;

        let first_tick = collect_retro_keyboard_polled_keys(&keys_down, modifiers);
        let second_tick = collect_retro_keyboard_polled_keys(&keys_down, modifiers);

        assert!(first_tick.contains(&RETROK_DOWN));
        assert!(second_tick.contains(&RETROK_DOWN));
    }

    #[test]
    fn keyboard_polled_state_includes_synthetic_modifier_keys() {
        let keys_down = [egui::Key::ArrowLeft].into_iter().collect::<HashSet<_>>();
        let modifiers = egui::Modifiers {
            alt: true,
            ctrl: true,
            shift: true,
            mac_cmd: false,
            command: false,
        };

        let polled = collect_retro_keyboard_polled_keys(&keys_down, modifiers);

        assert!(polled.contains(&RETROK_LEFT));
        assert!(polled.contains(&RETROK_LSHIFT));
        assert!(polled.contains(&RETROK_LCTRL));
        assert!(polled.contains(&RETROK_LALT));
    }

    #[test]
    fn dos_path_does_not_synthesize_release_from_keys_down_snapshot() {
        let mut previous = [RETROK_DOWN].into_iter().collect::<HashSet<_>>();
        let mut pressed_since_frame = HashSet::new();
        let events = vec![];
        let keys_down = HashSet::new();

        let retro = collect_retro_keyboard_events(
            &mut previous,
            &mut pressed_since_frame,
            &events,
            &keys_down,
            egui::Modifiers::NONE,
            true,
            false,
        );

        assert!(retro.immediate_events.is_empty());
        assert!(retro.deferred_releases.is_empty());
        assert!(previous.contains(&RETROK_DOWN));
    }

    #[test]
    fn egui_arrow_keys_map_to_libretro_arrows() {
        assert_eq!(
            egui_key_to_retro_keycode(egui::Key::ArrowUp),
            Some(RETROK_UP)
        );
        assert_eq!(
            egui_key_to_retro_keycode(egui::Key::ArrowDown),
            Some(RETROK_DOWN)
        );
        assert_eq!(
            egui_key_to_retro_keycode(egui::Key::ArrowLeft),
            Some(RETROK_LEFT)
        );
        assert_eq!(
            egui_key_to_retro_keycode(egui::Key::ArrowRight),
            Some(RETROK_RIGHT)
        );
    }

    #[test]
    fn runtime_policy_routes_keyboard_passthrough_for_dos_only() {
        assert_eq!(
            runtime_input_policy_for_system("DOS", true).keyboard_routing,
            RetroKeyboardRouting::Passthrough
        );
        assert_eq!(
            runtime_input_policy_for_system("dos", true).keyboard_routing,
            RetroKeyboardRouting::Passthrough
        );
        assert_eq!(
            runtime_input_policy_for_system("DOS", false).keyboard_routing,
            RetroKeyboardRouting::SystemMapping
        );
        assert_eq!(
            runtime_input_policy_for_system("SNES", true).keyboard_routing,
            RetroKeyboardRouting::SystemMapping
        );
    }

    #[test]
    fn resolved_selected_mapping_device_key_keeps_current_when_present() {
        let targets = vec![
            ControllerMappingTarget {
                identity: DetectedPadIdentity {
                    device_key: String::from("054c:09cc:PS4"),
                    name: String::from("PS4"),
                    vendor_id: Some(String::from("054c")),
                    product_id: Some(String::from("09cc")),
                    mapping_name: Some(String::from("SdlMappings")),
                },
                player_slot: Some(0),
                connect_seq: 0,
            },
            ControllerMappingTarget {
                identity: DetectedPadIdentity {
                    device_key: String::from("045e:02fd:Xbox"),
                    name: String::from("Xbox"),
                    vendor_id: Some(String::from("045e")),
                    product_id: Some(String::from("02fd")),
                    mapping_name: Some(String::from("SdlMappings")),
                },
                player_slot: Some(1),
                connect_seq: 1,
            },
        ];

        let selected = resolved_selected_mapping_device_key(Some("045e:02fd:Xbox"), &targets);
        assert_eq!(selected.as_deref(), Some("045e:02fd:Xbox"));
    }

    #[test]
    fn resolved_selected_mapping_device_key_falls_back_to_first_target() {
        let targets = vec![
            ControllerMappingTarget {
                identity: DetectedPadIdentity {
                    device_key: String::from("054c:09cc:PS4"),
                    name: String::from("PS4"),
                    vendor_id: Some(String::from("054c")),
                    product_id: Some(String::from("09cc")),
                    mapping_name: Some(String::from("SdlMappings")),
                },
                player_slot: Some(0),
                connect_seq: 0,
            },
            ControllerMappingTarget {
                identity: DetectedPadIdentity {
                    device_key: String::from("054c:0ce6:PS5"),
                    name: String::from("PS5"),
                    vendor_id: Some(String::from("054c")),
                    product_id: Some(String::from("0ce6")),
                    mapping_name: Some(String::from("SdlMappings")),
                },
                player_slot: Some(1),
                connect_seq: 1,
            },
        ];

        let selected = resolved_selected_mapping_device_key(Some("missing"), &targets);
        assert_eq!(selected.as_deref(), Some("054c:09cc:PS4"));
    }

    #[test]
    fn clear_mapping_actions_unassigns_supported_actions_only() {
        let mut actions = BTreeMap::from([
            (
                String::from("A"),
                Some(MappingEntry::Button {
                    button: CanonicalButton::South,
                }),
            ),
            (
                String::from("B"),
                Some(MappingEntry::Button {
                    button: CanonicalButton::East,
                }),
            ),
            (
                String::from("Custom"),
                Some(MappingEntry::Button {
                    button: CanonicalButton::Guide,
                }),
            ),
        ]);

        clear_mapping_actions(&mut actions, &["A", "B"]);

        assert_eq!(actions.get("A"), Some(&None));
        assert_eq!(actions.get("B"), Some(&None));
        assert_eq!(
            actions.get("Custom"),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::Guide,
            }))
        );
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn deghost_non_dpad_button_suppresses_same_code_as_active_dpad() {
        let suspect = ControllerInputButtonDebug {
            code: Some(42),
            is_pressed: true,
            ..Default::default()
        };
        let dpad_active = ControllerInputButtonDebug {
            code: Some(42),
            is_pressed: true,
            ..Default::default()
        };
        assert!(!deghost_non_dpad_button_against_active_dpad(
            &suspect,
            &[&dpad_active]
        ));
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn deghost_non_dpad_button_keeps_unique_or_inactive_codes() {
        let suspect = ControllerInputButtonDebug {
            code: Some(42),
            is_pressed: true,
            ..Default::default()
        };
        let dpad_different = ControllerInputButtonDebug {
            code: Some(99),
            is_pressed: true,
            ..Default::default()
        };
        let dpad_same_but_idle = ControllerInputButtonDebug {
            code: Some(42),
            is_pressed: false,
            ..Default::default()
        };
        assert!(deghost_non_dpad_button_against_active_dpad(
            &suspect,
            &[&dpad_different]
        ));
        assert!(deghost_non_dpad_button_against_active_dpad(
            &suspect,
            &[&dpad_same_but_idle]
        ));
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn strict_safety_effective_non_dpad_button_suppresses_missing_or_ambiguous_codes() {
        let suspect_missing = ControllerInputButtonDebug {
            code: None,
            is_pressed: true,
            ..Default::default()
        };
        let dpad_up = ControllerInputButtonDebug {
            code: Some(1),
            is_pressed: true,
            ..Default::default()
        };
        let dpad_down = ControllerInputButtonDebug {
            code: Some(2),
            is_pressed: false,
            ..Default::default()
        };
        let dpad_left = ControllerInputButtonDebug {
            code: Some(3),
            is_pressed: false,
            ..Default::default()
        };
        let dpad_right = ControllerInputButtonDebug {
            code: Some(4),
            is_pressed: false,
            ..Default::default()
        };
        assert!(!strict_safety_effective_non_dpad_button(
            &suspect_missing,
            &[&dpad_up, &dpad_down, &dpad_left, &dpad_right],
            true
        ));

        let suspect = ControllerInputButtonDebug {
            code: Some(9),
            is_pressed: true,
            ..Default::default()
        };
        let ambiguous_up = ControllerInputButtonDebug {
            code: Some(7),
            is_pressed: true,
            ..Default::default()
        };
        let ambiguous_down = ControllerInputButtonDebug {
            code: Some(7),
            is_pressed: false,
            ..Default::default()
        };
        assert!(!strict_safety_effective_non_dpad_button(
            &suspect,
            &[&ambiguous_up, &ambiguous_down, &dpad_left, &dpad_right],
            true
        ));
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn strict_safety_effective_non_dpad_button_allows_distinct_codes() {
        let suspect = ControllerInputButtonDebug {
            code: Some(9),
            is_pressed: true,
            ..Default::default()
        };
        let dpad_up = ControllerInputButtonDebug {
            code: Some(1),
            is_pressed: true,
            ..Default::default()
        };
        let dpad_down = ControllerInputButtonDebug {
            code: Some(2),
            is_pressed: false,
            ..Default::default()
        };
        let dpad_left = ControllerInputButtonDebug {
            code: Some(3),
            is_pressed: false,
            ..Default::default()
        };
        let dpad_right = ControllerInputButtonDebug {
            code: Some(4),
            is_pressed: false,
            ..Default::default()
        };
        assert!(strict_safety_effective_non_dpad_button(
            &suspect,
            &[&dpad_up, &dpad_down, &dpad_left, &dpad_right],
            true
        ));
        assert!(strict_safety_effective_non_dpad_button(
            &suspect,
            &[&dpad_up, &dpad_down, &dpad_left, &dpad_right],
            false
        ));
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn strict_safety_effective_non_dpad_button_suppresses_axis_only_dpad_activity() {
        let suspect = ControllerInputButtonDebug {
            code: Some(9),
            is_pressed: true,
            ..Default::default()
        };
        let dpad_up = ControllerInputButtonDebug {
            code: None,
            is_pressed: false,
            ..Default::default()
        };
        let dpad_down = ControllerInputButtonDebug {
            code: None,
            is_pressed: false,
            ..Default::default()
        };
        let dpad_left = ControllerInputButtonDebug {
            code: None,
            is_pressed: false,
            ..Default::default()
        };
        let dpad_right = ControllerInputButtonDebug {
            code: None,
            is_pressed: false,
            ..Default::default()
        };
        let dpad_active = explicit_dpad_direction_active(false, 1.0, true);
        assert!(!strict_safety_effective_non_dpad_button(
            &suspect,
            &[&dpad_up, &dpad_down, &dpad_left, &dpad_right],
            dpad_active
        ));
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn runtime_mapping_profile_prefers_device_override_then_system_default() {
        let saved = vec![
            String::from(SYSTEM_DEFAULT_MAPPING_KEY),
            String::from("054c:0ce6:DualSense"),
        ];
        let device_profile =
            resolve_runtime_mapping_profile_from_keys("NES", Some("054c:0ce6:DualSense"), &saved);
        assert_eq!(
            device_profile.source.label(),
            RuntimeMappingSource::DeviceOverride.label()
        );
        assert_eq!(device_profile.mapping_key, "054c:0ce6:DualSense");

        let system_profile =
            resolve_runtime_mapping_profile_from_keys("NES", Some("missing"), &saved);
        assert_eq!(
            system_profile.source.label(),
            RuntimeMappingSource::SystemDefault.label()
        );
        assert_eq!(system_profile.mapping_key, SYSTEM_DEFAULT_MAPPING_KEY);
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn runtime_mapping_profile_uses_builtin_default_without_saved_records() {
        let profile = resolve_runtime_mapping_profile_from_keys("NES", Some("missing"), &[]);
        assert_eq!(
            profile.source.label(),
            RuntimeMappingSource::BuiltInDefault.label()
        );
        assert_eq!(profile.mapping_key, SYSTEM_DEFAULT_MAPPING_KEY);
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn reconcile_assignments_uses_first_come_with_four_player_cap() {
        let mut connect_order = HashMap::new();
        let mut assignments = HashMap::new();
        let mut next_seq = 0;
        reconcile_player_slot_assignments(
            &mut connect_order,
            &mut assignments,
            &mut next_seq,
            &[20, 10, 30, 40, 50],
            &[20, 10, 30, 40, 50],
            MAX_GAMEPAD_PLAYERS,
        );

        assert_eq!(assignments.get(&20), Some(&0));
        assert_eq!(assignments.get(&10), Some(&1));
        assert_eq!(assignments.get(&30), Some(&2));
        assert_eq!(assignments.get(&40), Some(&3));
        assert!(!assignments.contains_key(&50));
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn reconcile_assignments_refills_lowest_free_slot_after_disconnect() {
        let mut connect_order = HashMap::new();
        let mut assignments = HashMap::new();
        let mut next_seq = 0;
        reconcile_player_slot_assignments(
            &mut connect_order,
            &mut assignments,
            &mut next_seq,
            &[1, 2, 3, 4],
            &[1, 2, 3, 4],
            MAX_GAMEPAD_PLAYERS,
        );
        reconcile_player_slot_assignments(
            &mut connect_order,
            &mut assignments,
            &mut next_seq,
            &[1, 3, 4, 5],
            &[1, 3, 4, 5],
            MAX_GAMEPAD_PLAYERS,
        );

        assert_eq!(assignments.get(&1), Some(&0));
        assert_eq!(assignments.get(&5), Some(&1));
        assert_eq!(assignments.get(&3), Some(&2));
        assert_eq!(assignments.get(&4), Some(&3));
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn reconcile_assignments_skips_non_playable_pads() {
        let mut connect_order = HashMap::new();
        let mut assignments = HashMap::new();
        let mut next_seq = 0;
        reconcile_player_slot_assignments(
            &mut connect_order,
            &mut assignments,
            &mut next_seq,
            &[1, 2, 3],
            &[1, 3],
            MAX_GAMEPAD_PLAYERS,
        );

        assert_eq!(assignments.len(), 2);
        assert!(assignments.contains_key(&1));
        assert!(!assignments.contains_key(&2));
        assert!(assignments.contains_key(&3));
    }

    #[cfg(feature = "gamepad")]
    #[test]
    fn sort_debug_snapshots_prioritizes_assigned_then_unassigned_then_unsupported() {
        let mut snapshots = vec![
            ControllerInputDebugSnapshot {
                name: String::from("unassigned"),
                assignment_source: ControllerAssignmentSource::UnassignedOverLimit,
                player_slot: None,
                connect_seq: 2,
                ..Default::default()
            },
            ControllerInputDebugSnapshot {
                name: String::from("player2"),
                assignment_source: ControllerAssignmentSource::Assigned,
                player_slot: Some(1),
                connect_seq: 5,
                ..Default::default()
            },
            ControllerInputDebugSnapshot {
                name: String::from("unsupported"),
                assignment_source: ControllerAssignmentSource::Unsupported,
                player_slot: None,
                connect_seq: 1,
                ..Default::default()
            },
            ControllerInputDebugSnapshot {
                name: String::from("player1"),
                assignment_source: ControllerAssignmentSource::Assigned,
                player_slot: Some(0),
                connect_seq: 7,
                ..Default::default()
            },
        ];

        sort_debug_snapshots_for_display(&mut snapshots);

        assert_eq!(snapshots[0].name, "player1");
        assert_eq!(snapshots[1].name, "player2");
        assert_eq!(snapshots[2].name, "unassigned");
        assert_eq!(snapshots[3].name, "unsupported");
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
        assert!(!apply_dpad_button_usage(
            &mut state,
            HID_PAGE_BUTTON,
            0x1234,
            true
        ));
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
        assert!(should_invert_vertical_axis(
            Some(SONY_VENDOR_ID),
            "DualSense Wireless Controller"
        ));
        assert!(should_invert_vertical_axis(None, "Wireless Controller"));
        assert!(!should_invert_vertical_axis(
            Some(0x045e),
            "Xbox Wireless Controller"
        ));
    }

    #[test]
    fn runtime_policy_resolves_primary_stick_and_analog_capabilities() {
        assert!(!runtime_input_policy_for_system("NES", false).supports_native_analog);
        assert!(!runtime_input_policy_for_system("NES", false).uses_primary_stick_selector);
        assert!(!runtime_input_policy_for_system("SNES", false).supports_native_analog);
        assert!(!runtime_input_policy_for_system("ARCADE", false).supports_native_analog);
        assert!(runtime_input_policy_for_system("N64", false).supports_native_analog);
        assert!(runtime_input_policy_for_system("N64", false).uses_primary_stick_selector);
        assert!(runtime_input_policy_for_system("n64", false).supports_native_analog);
        assert!(runtime_input_policy_for_system("DREAMCAST", false).supports_native_analog);
        assert!(runtime_input_policy_for_system("DREAMCAST", false).uses_primary_stick_selector);
        assert!(runtime_input_policy_for_system("dreamcast", false).supports_native_analog);
        assert!(!runtime_input_policy_for_system("SATURN", false).supports_native_analog);
        assert!(!runtime_input_policy_for_system("SATURN", false).uses_primary_stick_selector);
        assert!(!runtime_input_policy_for_system("PCECD", false).supports_native_analog);
        assert_eq!(
            runtime_input_policy_for_system("ALL", false),
            runtime_input_policy_for_system("NES", false)
        );
    }

    #[test]
    fn runtime_policy_defaults_shortcut_directional_guard_to_disabled() {
        assert_eq!(
            runtime_input_policy_for_system("DOS", false).shortcut_directional_guard,
            ShortcutDirectionalGuardMode::Disabled
        );
        assert_eq!(
            runtime_input_policy_for_system("N64", false).shortcut_directional_guard,
            ShortcutDirectionalGuardMode::Disabled
        );
        assert_eq!(
            runtime_input_policy_for_system("DREAMCAST", false).shortcut_directional_guard,
            ShortcutDirectionalGuardMode::Disabled
        );
        assert_eq!(
            runtime_input_policy_for_system("SATURN", false).shortcut_directional_guard,
            ShortcutDirectionalGuardMode::Disabled
        );
        assert_eq!(
            runtime_input_policy_for_system("PCECD", false).shortcut_directional_guard,
            ShortcutDirectionalGuardMode::Disabled
        );
    }

    #[test]
    fn n64_action_bindings_match_libretro_controller_layout() {
        assert_eq!(
            action_to_retro_binding("N64", "A", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_B))
        );
        assert_eq!(
            action_to_retro_binding("N64", "B", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_Y))
        );
        assert_eq!(
            action_to_retro_binding("N64", "Stick Left", None),
            Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_LEFT,
                axis_id: RETRO_DEVICE_ID_ANALOG_X,
                value: -1.0,
            })
        );
        assert_eq!(
            action_to_retro_binding("N64", "Stick Right", None),
            Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_LEFT,
                axis_id: RETRO_DEVICE_ID_ANALOG_X,
                value: 1.0,
            })
        );
        assert_eq!(
            action_to_retro_binding("N64", "Stick Up", None),
            Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_LEFT,
                axis_id: RETRO_DEVICE_ID_ANALOG_Y,
                value: 1.0,
            })
        );
        assert_eq!(
            action_to_retro_binding("N64", "Stick Down", None),
            Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_LEFT,
                axis_id: RETRO_DEVICE_ID_ANALOG_Y,
                value: -1.0,
            })
        );
        assert_eq!(
            action_to_retro_binding("N64", "C-Up", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_X))
        );
        assert_eq!(
            action_to_retro_binding("N64", "C-Right", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_A))
        );
        assert_eq!(
            action_to_retro_binding("N64", "C-Left", None),
            Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_RIGHT,
                axis_id: RETRO_DEVICE_ID_ANALOG_X,
                value: -1.0,
            })
        );
        assert_eq!(
            action_to_retro_binding("N64", "C-Down", None),
            Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_RIGHT,
                axis_id: RETRO_DEVICE_ID_ANALOG_Y,
                value: 1.0,
            })
        );
        assert_eq!(
            action_to_retro_binding("N64", "Z", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L2))
        );
    }

    #[test]
    fn n64_c_button_analog_binding_uses_opposite_selected_primary_stick() {
        assert_eq!(
            action_to_retro_binding("N64", "C-Left", Some(N64PrimaryStick::Right)),
            Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_LEFT,
                axis_id: RETRO_DEVICE_ID_ANALOG_X,
                value: -1.0,
            })
        );
        assert_eq!(
            action_to_retro_binding("N64", "C-Down", Some(N64PrimaryStick::Right)),
            Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_LEFT,
                axis_id: RETRO_DEVICE_ID_ANALOG_Y,
                value: 1.0,
            })
        );
    }

    #[test]
    fn ps2_action_bindings_map_playstation_labels_to_retropad() {
        assert_eq!(
            action_to_retro_binding("PS2", "Cross", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_B))
        );
        assert_eq!(
            action_to_retro_binding("PS2", "Circle", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_A))
        );
        assert_eq!(
            action_to_retro_binding("PS2", "Square", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_Y))
        );
        assert_eq!(
            action_to_retro_binding("PS2", "Triangle", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_X))
        );
        assert_eq!(
            action_to_retro_binding("PS2", "L1", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L))
        );
        assert_eq!(
            action_to_retro_binding("PS2", "R1", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_R))
        );
        assert_eq!(
            action_to_retro_binding("PS2", "L2", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L2))
        );
        assert_eq!(
            action_to_retro_binding("PS2", "R2", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_R2))
        );
        assert_eq!(
            action_to_retro_binding("PS2", "L3", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L3))
        );
        assert_eq!(
            action_to_retro_binding("PS2", "R3", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_R3))
        );
        assert_eq!(
            action_to_retro_binding("PS2", "Left Stick Up", None),
            Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_LEFT,
                axis_id: RETRO_DEVICE_ID_ANALOG_Y,
                value: -1.0,
            })
        );
        assert_eq!(
            action_to_retro_binding("PS2", "Left Stick Right", None),
            Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_LEFT,
                axis_id: RETRO_DEVICE_ID_ANALOG_X,
                value: 1.0,
            })
        );
        assert_eq!(
            action_to_retro_binding("PS2", "Right Stick Up", None),
            Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_RIGHT,
                axis_id: RETRO_DEVICE_ID_ANALOG_Y,
                value: -1.0,
            })
        );
        assert_eq!(
            action_to_retro_binding("PS2", "Right Stick Left", None),
            Some(RetroActionBinding::Analog {
                index: RETRO_DEVICE_INDEX_ANALOG_RIGHT,
                axis_id: RETRO_DEVICE_ID_ANALOG_X,
                value: -1.0,
            })
        );
    }

    #[test]
    fn pcecd_action_bindings_map_to_expected_retropad_ids() {
        assert_eq!(
            action_to_retro_binding("PCECD", "I", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_A))
        );
        assert_eq!(
            action_to_retro_binding("PCECD", "II", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_B))
        );
        assert_eq!(
            action_to_retro_binding("PCECD", "Run", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_START))
        );
        assert_eq!(
            action_to_retro_binding("PCECD", "Select", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_SELECT))
        );
    }

    #[test]
    fn saturn_action_bindings_map_to_expected_retropad_ids() {
        assert_eq!(
            action_to_retro_binding("SATURN", "A", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_B))
        );
        assert_eq!(
            action_to_retro_binding("SATURN", "B", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_A))
        );
        assert_eq!(
            action_to_retro_binding("SATURN", "X", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_Y))
        );
        assert_eq!(
            action_to_retro_binding("SATURN", "Y", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_X))
        );
        assert_eq!(
            action_to_retro_binding("SATURN", "C", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L2))
        );
        assert_eq!(
            action_to_retro_binding("SATURN", "Z", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_R2))
        );
        assert_eq!(
            action_to_retro_binding("SATURN", "L", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_L))
        );
        assert_eq!(
            action_to_retro_binding("SATURN", "R", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_R))
        );
        assert_eq!(
            action_to_retro_binding("SATURN", "Start", None),
            Some(RetroActionBinding::Joypad(RETRO_DEVICE_ID_JOYPAD_START))
        );
    }

    #[test]
    fn n64_control_stick_mapping_enabled_only_when_assigned() {
        let mapping = default_gamepad_mapping_for_system("N64");
        assert!(!n64_control_stick_mapping_enabled(&mapping));

        let mut mapped = mapping.clone();
        mapped.actions.insert(
            String::from("Stick Up"),
            Some(MappingEntry::Axis {
                axis: CanonicalAxis::RightStickY,
                direction: -1,
            }),
        );
        assert!(n64_control_stick_mapping_enabled(&mapped));
    }

    #[test]
    fn primary_stick_axes_follow_selected_stick_and_system_y_convention() {
        let state = CanonicalPadState {
            left_x: 0.25,
            left_y: -0.5,
            right_x: -0.75,
            right_y: 0.9,
            ..Default::default()
        };

        assert_eq!(
            primary_stick_axes_for_system("N64", &state, N64PrimaryStick::Left),
            (0.25, 0.5)
        );
        assert_eq!(
            primary_stick_axes_for_system("N64", &state, N64PrimaryStick::Right),
            (-0.75, -0.9)
        );
        assert_eq!(
            primary_stick_axes_for_system("DREAMCAST", &state, N64PrimaryStick::Left),
            (0.25, -0.5)
        );
        assert_eq!(
            primary_stick_axes_for_system("DREAMCAST", &state, N64PrimaryStick::Right),
            (-0.75, 0.9)
        );
    }

    #[test]
    fn mapped_shortcut_remains_active_during_directional_input_when_guard_disabled() {
        let mut mapping = default_gamepad_mapping_for_system("NES");
        mapping.actions.insert(
            QUICK_LOAD_ACTION.to_string(),
            Some(MappingEntry::Button {
                button: CanonicalButton::Guide,
            }),
        );
        let state = CanonicalPadState {
            dpad_down: true,
            guide: true,
            ..Default::default()
        };
        let policy = runtime_input_policy_for_system("NES", false);
        assert!(mapped_shortcut_action_is_active(
            &mapping,
            QUICK_LOAD_ACTION,
            &state,
            &policy
        ));
    }

    #[test]
    fn mapped_shortcut_allows_non_directional_when_directional_input_is_idle() {
        let mut mapping = default_gamepad_mapping_for_system("NES");
        mapping.actions.insert(
            QUICK_LOAD_ACTION.to_string(),
            Some(MappingEntry::Button {
                button: CanonicalButton::Guide,
            }),
        );
        let state = CanonicalPadState {
            guide: true,
            ..Default::default()
        };
        let policy = runtime_input_policy_for_system("NES", false);
        assert!(mapped_shortcut_action_is_active(
            &mapping,
            QUICK_LOAD_ACTION,
            &state,
            &policy
        ));
    }

    #[test]
    fn mapped_shortcut_allows_directional_mapping_during_directional_input() {
        let mut mapping = default_gamepad_mapping_for_system("NES");
        mapping.actions.insert(
            QUICK_LOAD_ACTION.to_string(),
            Some(MappingEntry::Button {
                button: CanonicalButton::DPadUp,
            }),
        );
        let state = CanonicalPadState {
            dpad_up: true,
            ..Default::default()
        };
        let policy = runtime_input_policy_for_system("NES", false);
        assert!(mapped_shortcut_action_is_active(
            &mapping,
            QUICK_LOAD_ACTION,
            &state,
            &policy
        ));
    }

    #[test]
    fn mapped_shortcut_is_blocked_when_directional_guard_is_enabled() {
        let mut mapping = default_gamepad_mapping_for_system("NES");
        mapping.actions.insert(
            QUICK_LOAD_ACTION.to_string(),
            Some(MappingEntry::Button {
                button: CanonicalButton::Guide,
            }),
        );
        let state = CanonicalPadState {
            dpad_down: true,
            guide: true,
            ..Default::default()
        };
        let mut policy = runtime_input_policy_for_system("NES", false);
        policy.shortcut_directional_guard = ShortcutDirectionalGuardMode::Enabled;
        assert!(!mapped_shortcut_action_is_active(
            &mapping,
            QUICK_LOAD_ACTION,
            &state,
            &policy
        ));
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
