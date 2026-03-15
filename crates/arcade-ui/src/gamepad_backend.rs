#[cfg(all(feature = "gamepad", target_os = "windows"))]
use std::collections::BTreeMap;

#[cfg(all(feature = "gamepad", target_os = "windows"))]
use anyhow::{anyhow, Result};
#[cfg(all(feature = "gamepad", target_os = "windows"))]
use arcade_domain::DetectedPadIdentity;
#[cfg(all(feature = "gamepad", target_os = "windows"))]
use sdl2::{
    controller::{Axis, Button, GameController},
    event::Event,
    GameControllerSubsystem, Sdl,
};

#[cfg(all(feature = "gamepad", target_os = "windows"))]
#[derive(Clone, Debug)]
pub(crate) struct WindowsGamepadSnapshot {
    pub(crate) identity: DetectedPadIdentity,
    pub(crate) has_dpad_buttons: bool,
    pub(crate) raw_dpad_x: f32,
    pub(crate) raw_dpad_y: f32,
    pub(crate) raw_left_x: f32,
    pub(crate) raw_left_y: f32,
    pub(crate) south: bool,
    pub(crate) east: bool,
    pub(crate) north: bool,
    pub(crate) west: bool,
    pub(crate) dpad_up: bool,
    pub(crate) dpad_down: bool,
    pub(crate) dpad_left: bool,
    pub(crate) dpad_right: bool,
    pub(crate) start: bool,
    pub(crate) select: bool,
    pub(crate) guide: bool,
    pub(crate) left_shoulder: bool,
    pub(crate) right_shoulder: bool,
    pub(crate) left_thumb: bool,
    pub(crate) right_thumb: bool,
    pub(crate) left_trigger: f32,
    pub(crate) right_trigger: f32,
    pub(crate) left_x: f32,
    pub(crate) left_y: f32,
    pub(crate) right_x: f32,
    pub(crate) right_y: f32,
}

#[cfg(all(feature = "gamepad", target_os = "windows"))]
pub(crate) struct WindowsGamepadRuntime {
    _sdl: Sdl,
    controller_subsystem: GameControllerSubsystem,
    event_pump: sdl2::EventPump,
    controllers: BTreeMap<u32, GameController>,
}

#[cfg(all(feature = "gamepad", target_os = "windows"))]
impl WindowsGamepadRuntime {
    pub(crate) fn new() -> Result<Self> {
        let sdl = sdl2::init().map_err(|err| anyhow!("SDL initialization failed: {err}"))?;
        let controller_subsystem = sdl
            .game_controller()
            .map_err(|err| anyhow!("SDL game controller subsystem failed: {err}"))?;
        let event_pump = sdl
            .event_pump()
            .map_err(|err| anyhow!("SDL event pump failed: {err}"))?;

        let mut runtime = Self {
            _sdl: sdl,
            controller_subsystem,
            event_pump,
            controllers: BTreeMap::new(),
        };
        runtime.sync_connected_controllers();
        Ok(runtime)
    }

    pub(crate) fn captures(&mut self) -> Vec<WindowsGamepadSnapshot> {
        self.poll_events();

        let mut controllers = self
            .controllers
            .iter()
            .filter(|(_, controller)| controller.attached())
            .collect::<Vec<_>>();
        controllers.sort_by_key(|(instance_id, _)| **instance_id);

        controllers
            .into_iter()
            .map(|(_, controller)| snapshot_from_controller(controller))
            .collect()
    }

    fn poll_events(&mut self) {
        let mut added = Vec::new();
        let mut removed = Vec::new();
        for event in self.event_pump.poll_iter() {
            match event {
                Event::ControllerDeviceAdded { which, .. } => {
                    added.push(which);
                }
                Event::ControllerDeviceRemoved { which, .. } => {
                    removed.push(which as u32);
                }
                _ => {}
            }
        }

        for instance_id in removed {
            self.controllers.remove(&instance_id);
        }
        for joystick_index in added {
            let _ = self.open_controller(joystick_index);
        }

        self.sync_connected_controllers();
    }

    fn sync_connected_controllers(&mut self) {
        self.controllers
            .retain(|_, controller| controller.attached());

        let Ok(total) = self.controller_subsystem.num_joysticks() else {
            return;
        };

        for joystick_index in 0..total {
            let _ = self.open_controller(joystick_index);
        }
    }

    fn open_controller(&mut self, joystick_index: u32) -> Result<()> {
        if !self.controller_subsystem.is_game_controller(joystick_index) {
            return Ok(());
        }

        let controller = self
            .controller_subsystem
            .open(joystick_index)
            .map_err(|err| anyhow!("SDL failed to open controller {joystick_index}: {err}"))?;
        let instance_id = controller.instance_id();
        self.controllers.entry(instance_id).or_insert(controller);
        Ok(())
    }
}

#[cfg(all(feature = "gamepad", target_os = "windows"))]
fn snapshot_from_controller(controller: &GameController) -> WindowsGamepadSnapshot {
    let identity = build_detected_pad_identity(controller);
    let dpad_up = controller.button(Button::DPadUp);
    let dpad_down = controller.button(Button::DPadDown);
    let dpad_left = controller.button(Button::DPadLeft);
    let dpad_right = controller.button(Button::DPadRight);

    WindowsGamepadSnapshot {
        identity,
        has_dpad_buttons: true,
        raw_dpad_x: button_pair_axis(dpad_left, dpad_right),
        raw_dpad_y: button_pair_axis(dpad_up, dpad_down),
        raw_left_x: axis_value(controller.axis(Axis::LeftX)),
        raw_left_y: axis_value(controller.axis(Axis::LeftY)),
        south: controller.button(Button::A),
        east: controller.button(Button::B),
        north: controller.button(Button::Y),
        west: controller.button(Button::X),
        dpad_up,
        dpad_down,
        dpad_left,
        dpad_right,
        start: controller.button(Button::Start),
        select: controller.button(Button::Back),
        guide: controller.button(Button::Guide),
        left_shoulder: controller.button(Button::LeftShoulder),
        right_shoulder: controller.button(Button::RightShoulder),
        left_thumb: controller.button(Button::LeftStick),
        right_thumb: controller.button(Button::RightStick),
        left_trigger: trigger_value(controller.axis(Axis::TriggerLeft)),
        right_trigger: trigger_value(controller.axis(Axis::TriggerRight)),
        left_x: axis_value(controller.axis(Axis::LeftX)),
        left_y: axis_value(controller.axis(Axis::LeftY)),
        right_x: axis_value(controller.axis(Axis::RightX)),
        right_y: axis_value(controller.axis(Axis::RightY)),
    }
}

#[cfg(all(feature = "gamepad", target_os = "windows"))]
fn build_detected_pad_identity(controller: &GameController) -> DetectedPadIdentity {
    let name = controller.name().trim().to_string();
    let vendor_id = controller.vendor_id().map(|value| format!("{value:04x}"));
    let product_id = controller.product_id().map(|value| format!("{value:04x}"));
    let device_key =
        if let (Some(vendor_id), Some(product_id)) = (vendor_id.as_ref(), product_id.as_ref()) {
            format!("{vendor_id}:{product_id}:{name}")
        } else if !name.is_empty() {
            name.clone()
        } else {
            format!("sdl-instance-{}", controller.instance_id())
        };

    DetectedPadIdentity {
        device_key,
        name,
        vendor_id,
        product_id,
        mapping_name: Some(String::from("SDL2 GameController")),
    }
}

#[cfg(all(feature = "gamepad", target_os = "windows"))]
fn axis_value(value: i16) -> f32 {
    if value == i16::MIN {
        -1.0
    } else {
        (value as f32 / i16::MAX as f32).clamp(-1.0, 1.0)
    }
}

#[cfg(all(feature = "gamepad", target_os = "windows"))]
fn trigger_value(value: i16) -> f32 {
    axis_value(value).max(0.0)
}

#[cfg(all(feature = "gamepad", target_os = "windows"))]
fn button_pair_axis(negative: bool, positive: bool) -> f32 {
    match (negative, positive) {
        (true, false) => -1.0,
        (false, true) => 1.0,
        _ => 0.0,
    }
}
