use std::collections::BTreeMap;

use arcade_domain::{CanonicalAxis, CanonicalButton, DetectedPadIdentity, MappingEntry};
use egui::{pos2, vec2, Pos2, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControllerMapperArt {
    Ps3,
    Ps4,
    Ps5,
    Xbox,
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControllerMapperView {
    Front,
    Top,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VisualControlId {
    Button(CanonicalButton),
    Axis {
        axis: CanonicalAxis,
        direction: i8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum HotspotShape {
    Circle {
        center: [f32; 2],
        radius: f32,
    },
    Rect {
        center: [f32; 2],
        size: [f32; 2],
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ControllerHotspot {
    pub(crate) control: VisualControlId,
    pub(crate) label: &'static str,
    pub(crate) shape: HotspotShape,
}

const PS3_FRONT_HOTSPOTS: [ControllerHotspot; 21] = [
    circle_button(CanonicalButton::Guide, "PS", 0.50, 0.53, 0.030),
    rect_button(CanonicalButton::Select, "Select", 0.40, 0.44, 0.060, 0.042),
    rect_button(CanonicalButton::Start, "Start", 0.60, 0.44, 0.070, 0.044),
    circle_button(CanonicalButton::DPadUp, "D-Pad Up", 0.20, 0.33, 0.028),
    circle_button(CanonicalButton::DPadDown, "D-Pad Down", 0.20, 0.49, 0.028),
    circle_button(CanonicalButton::DPadLeft, "D-Pad Left", 0.13, 0.41, 0.028),
    circle_button(CanonicalButton::DPadRight, "D-Pad Right", 0.27, 0.41, 0.028),
    circle_button(CanonicalButton::West, "Square / West", 0.75, 0.42, 0.030),
    circle_button(CanonicalButton::North, "Triangle / North", 0.81, 0.33, 0.030),
    circle_button(CanonicalButton::East, "Circle / East", 0.88, 0.42, 0.030),
    circle_button(CanonicalButton::South, "Cross / South", 0.81, 0.51, 0.030),
    circle_button(CanonicalButton::LeftThumb, "L3 / Left Stick Click", 0.36, 0.55, 0.052),
    circle_button(CanonicalButton::RightThumb, "R3 / Right Stick Click", 0.64, 0.55, 0.052),
    rect_axis(CanonicalAxis::LeftStickY, -1, "Left Stick Up", 0.36, 0.47, 0.070, 0.040),
    rect_axis(CanonicalAxis::LeftStickY, 1, "Left Stick Down", 0.36, 0.63, 0.070, 0.040),
    rect_axis(CanonicalAxis::LeftStickX, -1, "Left Stick Left", 0.28, 0.55, 0.040, 0.070),
    rect_axis(CanonicalAxis::LeftStickX, 1, "Left Stick Right", 0.44, 0.55, 0.040, 0.070),
    rect_axis(CanonicalAxis::RightStickY, -1, "Right Stick Up", 0.64, 0.47, 0.070, 0.040),
    rect_axis(CanonicalAxis::RightStickY, 1, "Right Stick Down", 0.64, 0.63, 0.070, 0.040),
    rect_axis(CanonicalAxis::RightStickX, -1, "Right Stick Left", 0.56, 0.55, 0.040, 0.070),
    rect_axis(CanonicalAxis::RightStickX, 1, "Right Stick Right", 0.72, 0.55, 0.040, 0.070),
];

const PS3_TOP_HOTSPOTS: [ControllerHotspot; 4] = [
    rect_button(
        CanonicalButton::LeftShoulder,
        "L1 / Left Shoulder",
        0.20,
        0.31,
        0.18,
        0.14,
    ),
    rect_axis(
        CanonicalAxis::LeftTrigger,
        1,
        "L2 / Left Trigger",
        0.18,
        0.58,
        0.18,
        0.34,
    ),
    rect_button(
        CanonicalButton::RightShoulder,
        "R1 / Right Shoulder",
        0.80,
        0.31,
        0.18,
        0.14,
    ),
    rect_axis(
        CanonicalAxis::RightTrigger,
        1,
        "R2 / Right Trigger",
        0.82,
        0.58,
        0.18,
        0.34,
    ),
];

const PS4_FRONT_HOTSPOTS: [ControllerHotspot; 21] = [
    circle_button(CanonicalButton::Guide, "PS", 0.50, 0.62, 0.030),
    rect_button(CanonicalButton::Select, "Share", 0.28, 0.20, 0.060, 0.050),
    rect_button(CanonicalButton::Start, "Options", 0.72, 0.20, 0.060, 0.050),
    circle_button(CanonicalButton::DPadUp, "D-Pad Up", 0.20, 0.30, 0.028),
    circle_button(CanonicalButton::DPadDown, "D-Pad Down", 0.20, 0.46, 0.028),
    circle_button(CanonicalButton::DPadLeft, "D-Pad Left", 0.13, 0.38, 0.028),
    circle_button(CanonicalButton::DPadRight, "D-Pad Right", 0.27, 0.38, 0.028),
    circle_button(CanonicalButton::West, "Square / West", 0.75, 0.37, 0.030),
    circle_button(CanonicalButton::North, "Triangle / North", 0.82, 0.29, 0.030),
    circle_button(CanonicalButton::East, "Circle / East", 0.89, 0.37, 0.030),
    circle_button(CanonicalButton::South, "Cross / South", 0.82, 0.47, 0.030),
    circle_button(CanonicalButton::LeftThumb, "L3 / Left Stick Click", 0.36, 0.57, 0.052),
    circle_button(CanonicalButton::RightThumb, "R3 / Right Stick Click", 0.64, 0.57, 0.052),
    rect_axis(CanonicalAxis::LeftStickY, -1, "Left Stick Up", 0.36, 0.49, 0.070, 0.040),
    rect_axis(CanonicalAxis::LeftStickY, 1, "Left Stick Down", 0.36, 0.65, 0.070, 0.040),
    rect_axis(CanonicalAxis::LeftStickX, -1, "Left Stick Left", 0.28, 0.57, 0.040, 0.070),
    rect_axis(CanonicalAxis::LeftStickX, 1, "Left Stick Right", 0.44, 0.57, 0.040, 0.070),
    rect_axis(CanonicalAxis::RightStickY, -1, "Right Stick Up", 0.64, 0.49, 0.070, 0.040),
    rect_axis(CanonicalAxis::RightStickY, 1, "Right Stick Down", 0.64, 0.65, 0.070, 0.040),
    rect_axis(CanonicalAxis::RightStickX, -1, "Right Stick Left", 0.56, 0.57, 0.040, 0.070),
    rect_axis(CanonicalAxis::RightStickX, 1, "Right Stick Right", 0.72, 0.57, 0.040, 0.070),
];

const PS4_TOP_HOTSPOTS: [ControllerHotspot; 4] = [
    rect_button(
        CanonicalButton::LeftShoulder,
        "L1 / Left Shoulder",
        0.20,
        0.29,
        0.19,
        0.12,
    ),
    rect_axis(
        CanonicalAxis::LeftTrigger,
        1,
        "L2 / Left Trigger",
        0.18,
        0.58,
        0.18,
        0.32,
    ),
    rect_button(
        CanonicalButton::RightShoulder,
        "R1 / Right Shoulder",
        0.80,
        0.29,
        0.19,
        0.12,
    ),
    rect_axis(
        CanonicalAxis::RightTrigger,
        1,
        "R2 / Right Trigger",
        0.82,
        0.58,
        0.18,
        0.32,
    ),
];

const PS5_FRONT_HOTSPOTS: [ControllerHotspot; 21] = [
    circle_button(CanonicalButton::Guide, "PS", 0.503, 0.478, 0.015),
    rect_button(CanonicalButton::Select, "Create", 0.267, 0.218, 0.020, 0.034),
    rect_button(CanonicalButton::Start, "Menu", 0.733, 0.218, 0.020, 0.034),
    circle_button(CanonicalButton::DPadUp, "D-Pad Up", 0.199, 0.290, 0.016),
    circle_button(CanonicalButton::DPadDown, "D-Pad Down", 0.199, 0.404, 0.016),
    circle_button(CanonicalButton::DPadLeft, "D-Pad Left", 0.133, 0.347, 0.016),
    circle_button(CanonicalButton::DPadRight, "D-Pad Right", 0.264, 0.347, 0.016),
    circle_button(CanonicalButton::West, "Square / West", 0.731, 0.347, 0.018),
    circle_button(CanonicalButton::North, "Triangle / North", 0.807, 0.278, 0.018),
    circle_button(CanonicalButton::East, "Circle / East", 0.857, 0.347, 0.018),
    circle_button(CanonicalButton::South, "Cross / South", 0.781, 0.417, 0.018),
    circle_button(CanonicalButton::LeftThumb, "L3 / Left Stick Click", 0.342, 0.476, 0.028),
    circle_button(CanonicalButton::RightThumb, "R3 / Right Stick Click", 0.658, 0.476, 0.028),
    rect_axis(CanonicalAxis::LeftStickY, -1, "Left Stick Up", 0.342, 0.443, 0.024, 0.012),
    rect_axis(CanonicalAxis::LeftStickY, 1, "Left Stick Down", 0.342, 0.509, 0.024, 0.012),
    rect_axis(CanonicalAxis::LeftStickX, -1, "Left Stick Left", 0.309, 0.476, 0.012, 0.024),
    rect_axis(CanonicalAxis::LeftStickX, 1, "Left Stick Right", 0.375, 0.476, 0.012, 0.024),
    rect_axis(CanonicalAxis::RightStickY, -1, "Right Stick Up", 0.658, 0.443, 0.024, 0.012),
    rect_axis(CanonicalAxis::RightStickY, 1, "Right Stick Down", 0.658, 0.509, 0.024, 0.012),
    rect_axis(CanonicalAxis::RightStickX, -1, "Right Stick Left", 0.625, 0.476, 0.012, 0.024),
    rect_axis(CanonicalAxis::RightStickX, 1, "Right Stick Right", 0.691, 0.476, 0.012, 0.024),
];

const PS5_TOP_HOTSPOTS: [ControllerHotspot; 4] = [
    rect_button(
        CanonicalButton::LeftShoulder,
        "L1 / Left Shoulder",
        0.170,
        0.472,
        0.046,
        0.032,
    ),
    rect_axis(
        CanonicalAxis::LeftTrigger,
        1,
        "L2 / Left Trigger",
        0.178,
        0.625,
        0.030,
        0.094,
    ),
    rect_button(
        CanonicalButton::RightShoulder,
        "R1 / Right Shoulder",
        0.830,
        0.472,
        0.046,
        0.032,
    ),
    rect_axis(
        CanonicalAxis::RightTrigger,
        1,
        "R2 / Right Trigger",
        0.822,
        0.625,
        0.030,
        0.094,
    ),
];

const XBOX_FRONT_HOTSPOTS: [ControllerHotspot; 21] = [
    circle_button(CanonicalButton::Guide, "Guide", 0.50, 0.22, 0.038),
    circle_button(CanonicalButton::Select, "View", 0.43, 0.35, 0.026),
    circle_button(CanonicalButton::Start, "Menu", 0.57, 0.35, 0.026),
    circle_button(CanonicalButton::DPadUp, "D-Pad Up", 0.31, 0.56, 0.030),
    circle_button(CanonicalButton::DPadDown, "D-Pad Down", 0.31, 0.72, 0.030),
    circle_button(CanonicalButton::DPadLeft, "D-Pad Left", 0.23, 0.64, 0.030),
    circle_button(CanonicalButton::DPadRight, "D-Pad Right", 0.39, 0.64, 0.030),
    circle_button(CanonicalButton::West, "X / West", 0.73, 0.39, 0.030),
    circle_button(CanonicalButton::North, "Y / North", 0.83, 0.28, 0.030),
    circle_button(CanonicalButton::East, "B / East", 0.91, 0.39, 0.030),
    circle_button(CanonicalButton::South, "A / South", 0.83, 0.50, 0.030),
    circle_button(CanonicalButton::LeftThumb, "L3 / Left Stick Click", 0.14, 0.31, 0.054),
    circle_button(CanonicalButton::RightThumb, "R3 / Right Stick Click", 0.64, 0.56, 0.054),
    rect_axis(CanonicalAxis::LeftStickY, -1, "Left Stick Up", 0.14, 0.23, 0.074, 0.042),
    rect_axis(CanonicalAxis::LeftStickY, 1, "Left Stick Down", 0.14, 0.39, 0.074, 0.042),
    rect_axis(CanonicalAxis::LeftStickX, -1, "Left Stick Left", 0.06, 0.31, 0.042, 0.074),
    rect_axis(CanonicalAxis::LeftStickX, 1, "Left Stick Right", 0.22, 0.31, 0.042, 0.074),
    rect_axis(CanonicalAxis::RightStickY, -1, "Right Stick Up", 0.64, 0.48, 0.074, 0.042),
    rect_axis(CanonicalAxis::RightStickY, 1, "Right Stick Down", 0.64, 0.64, 0.074, 0.042),
    rect_axis(CanonicalAxis::RightStickX, -1, "Right Stick Left", 0.56, 0.56, 0.042, 0.074),
    rect_axis(CanonicalAxis::RightStickX, 1, "Right Stick Right", 0.72, 0.56, 0.042, 0.074),
];

const XBOX_TOP_HOTSPOTS: [ControllerHotspot; 4] = [
    rect_button(
        CanonicalButton::LeftShoulder,
        "LB / Left Shoulder",
        0.20,
        0.25,
        0.18,
        0.12,
    ),
    rect_axis(
        CanonicalAxis::LeftTrigger,
        1,
        "LT / Left Trigger",
        0.18,
        0.58,
        0.18,
        0.32,
    ),
    rect_button(
        CanonicalButton::RightShoulder,
        "RB / Right Shoulder",
        0.80,
        0.25,
        0.18,
        0.12,
    ),
    rect_axis(
        CanonicalAxis::RightTrigger,
        1,
        "RT / Right Trigger",
        0.82,
        0.58,
        0.18,
        0.32,
    ),
];

const GENERIC_FRONT_HOTSPOTS: [ControllerHotspot; 21] = [
    circle_button(CanonicalButton::Guide, "Guide", 0.50, 0.33, 0.030),
    circle_button(CanonicalButton::Select, "Select", 0.42, 0.40, 0.024),
    circle_button(CanonicalButton::Start, "Start", 0.58, 0.40, 0.024),
    circle_button(CanonicalButton::DPadUp, "D-Pad Up", 0.28, 0.48, 0.030),
    circle_button(CanonicalButton::DPadDown, "D-Pad Down", 0.28, 0.64, 0.030),
    circle_button(CanonicalButton::DPadLeft, "D-Pad Left", 0.20, 0.56, 0.030),
    circle_button(CanonicalButton::DPadRight, "D-Pad Right", 0.36, 0.56, 0.030),
    circle_button(CanonicalButton::West, "West Button", 0.74, 0.42, 0.030),
    circle_button(CanonicalButton::North, "North Button", 0.84, 0.34, 0.030),
    circle_button(CanonicalButton::East, "East Button", 0.92, 0.42, 0.030),
    circle_button(CanonicalButton::South, "South Button", 0.84, 0.50, 0.030),
    circle_button(CanonicalButton::LeftThumb, "L3 / Left Stick Click", 0.14, 0.33, 0.054),
    circle_button(CanonicalButton::RightThumb, "R3 / Right Stick Click", 0.63, 0.53, 0.054),
    rect_axis(CanonicalAxis::LeftStickY, -1, "Left Stick Up", 0.14, 0.25, 0.074, 0.042),
    rect_axis(CanonicalAxis::LeftStickY, 1, "Left Stick Down", 0.14, 0.41, 0.074, 0.042),
    rect_axis(CanonicalAxis::LeftStickX, -1, "Left Stick Left", 0.06, 0.33, 0.042, 0.074),
    rect_axis(CanonicalAxis::LeftStickX, 1, "Left Stick Right", 0.22, 0.33, 0.042, 0.074),
    rect_axis(CanonicalAxis::RightStickY, -1, "Right Stick Up", 0.63, 0.45, 0.074, 0.042),
    rect_axis(CanonicalAxis::RightStickY, 1, "Right Stick Down", 0.63, 0.61, 0.074, 0.042),
    rect_axis(CanonicalAxis::RightStickX, -1, "Right Stick Left", 0.55, 0.53, 0.042, 0.074),
    rect_axis(CanonicalAxis::RightStickX, 1, "Right Stick Right", 0.71, 0.53, 0.042, 0.074),
];

const GENERIC_TOP_HOTSPOTS: [ControllerHotspot; 4] = [
    rect_button(
        CanonicalButton::LeftShoulder,
        "Left Shoulder",
        0.20,
        0.25,
        0.18,
        0.12,
    ),
    rect_axis(
        CanonicalAxis::LeftTrigger,
        1,
        "Left Trigger",
        0.18,
        0.58,
        0.18,
        0.32,
    ),
    rect_button(
        CanonicalButton::RightShoulder,
        "Right Shoulder",
        0.80,
        0.25,
        0.18,
        0.12,
    ),
    rect_axis(
        CanonicalAxis::RightTrigger,
        1,
        "Right Trigger",
        0.82,
        0.58,
        0.18,
        0.32,
    ),
];

pub(crate) fn controller_mapper_art_for_device(
    device: Option<&DetectedPadIdentity>,
) -> ControllerMapperArt {
    let Some(device) = device else {
        return ControllerMapperArt::Generic;
    };

    match (device.vendor_id.as_deref(), device.product_id.as_deref()) {
        (Some("054c"), Some("0268")) => return ControllerMapperArt::Ps3,
        (Some("054c"), Some("09cc")) => return ControllerMapperArt::Ps4,
        (Some("054c"), Some("0ce6")) => return ControllerMapperArt::Ps5,
        (Some("045e"), _) => return ControllerMapperArt::Xbox,
        (Some("054c"), _) => return ControllerMapperArt::Ps3,
        _ => {}
    }

    let name = device.name.trim().to_ascii_lowercase();
    if name.contains("dualsense") || name.contains("ps5") {
        return ControllerMapperArt::Ps5;
    }
    if name.contains("dualshock 4") || name.contains("ps4") {
        return ControllerMapperArt::Ps4;
    }
    if name.contains("dualshock")
        || name.contains("playstation")
        || name.contains("ps3")
        || name == "wireless controller"
    {
        return ControllerMapperArt::Ps3;
    }
    if name.contains("xbox") || name.contains("x-input") || name.contains("xinput") {
        return ControllerMapperArt::Xbox;
    }

    ControllerMapperArt::Generic
}

pub(crate) fn controller_mapper_art_asset(
    art: ControllerMapperArt,
    view: ControllerMapperView,
) -> &'static str {
    match (art, view) {
        (ControllerMapperArt::Ps3, ControllerMapperView::Front) => {
            "/controller-mapper/ps3-controller.png"
        }
        (ControllerMapperArt::Ps3, ControllerMapperView::Top) => {
            "/controller-mapper/ps3-controller-top.png"
        }
        (ControllerMapperArt::Ps4, ControllerMapperView::Front) => {
            "/controller-mapper/ps4-controller.png"
        }
        (ControllerMapperArt::Ps4, ControllerMapperView::Top) => {
            "/controller-mapper/ps4-controller-top.png"
        }
        (ControllerMapperArt::Ps5, ControllerMapperView::Front) => {
            "/controller-mapper/ps5-controller.png"
        }
        (ControllerMapperArt::Ps5, ControllerMapperView::Top) => {
            "/controller-mapper/ps5-controller-top.png"
        }
        (ControllerMapperArt::Xbox, ControllerMapperView::Front) => {
            "/controller-mapper/xbox-controller.png"
        }
        (ControllerMapperArt::Xbox, ControllerMapperView::Top) => {
            "/controller-mapper/xbox-controller-top.png"
        }
        (ControllerMapperArt::Generic, ControllerMapperView::Front) => {
            "/controller-mapper/generic-controller.png"
        }
        (ControllerMapperArt::Generic, ControllerMapperView::Top) => {
            "/controller-mapper/generic-controller-top.png"
        }
    }
}

pub(crate) fn controller_mapper_hotspots(
    art: ControllerMapperArt,
    view: ControllerMapperView,
) -> &'static [ControllerHotspot] {
    match (art, view) {
        (ControllerMapperArt::Ps3, ControllerMapperView::Front) => &PS3_FRONT_HOTSPOTS,
        (ControllerMapperArt::Ps3, ControllerMapperView::Top) => &PS3_TOP_HOTSPOTS,
        (ControllerMapperArt::Ps4, ControllerMapperView::Front) => &PS4_FRONT_HOTSPOTS,
        (ControllerMapperArt::Ps4, ControllerMapperView::Top) => &PS4_TOP_HOTSPOTS,
        (ControllerMapperArt::Ps5, ControllerMapperView::Front) => &PS5_FRONT_HOTSPOTS,
        (ControllerMapperArt::Ps5, ControllerMapperView::Top) => &PS5_TOP_HOTSPOTS,
        (ControllerMapperArt::Xbox, ControllerMapperView::Front) => &XBOX_FRONT_HOTSPOTS,
        (ControllerMapperArt::Xbox, ControllerMapperView::Top) => &XBOX_TOP_HOTSPOTS,
        (ControllerMapperArt::Generic, ControllerMapperView::Front) => &GENERIC_FRONT_HOTSPOTS,
        (ControllerMapperArt::Generic, ControllerMapperView::Top) => &GENERIC_TOP_HOTSPOTS,
    }
}

pub(crate) fn controller_mapper_control_label(
    art: ControllerMapperArt,
    control: VisualControlId,
) -> &'static str {
    controller_mapper_hotspots(art, ControllerMapperView::Front)
        .iter()
        .chain(controller_mapper_hotspots(art, ControllerMapperView::Top).iter())
        .find_map(|hotspot| (hotspot.control == control).then_some(hotspot.label))
        .unwrap_or_else(|| visual_control_fallback_label(control))
}

pub(crate) fn visual_control_to_mapping_entry(control: VisualControlId) -> MappingEntry {
    match control {
        VisualControlId::Button(button) => MappingEntry::Button { button },
        VisualControlId::Axis { axis, direction } => MappingEntry::Axis { axis, direction },
    }
}

pub(crate) fn visual_control_from_mapping_entry(entry: &MappingEntry) -> Option<VisualControlId> {
    match entry {
        MappingEntry::Button { button } => Some(VisualControlId::Button(*button)),
        MappingEntry::Axis { axis, direction } => Some(VisualControlId::Axis {
            axis: *axis,
            direction: *direction,
        }),
    }
}

pub(crate) fn action_for_visual_control<'a>(
    actions: &'a BTreeMap<String, Option<MappingEntry>>,
    control: VisualControlId,
) -> Option<&'a str> {
    actions.iter().find_map(|(action, entry)| {
        let matches = entry
            .as_ref()
            .and_then(visual_control_from_mapping_entry)
            .is_some_and(|existing| existing == control);
        matches.then_some(action.as_str())
    })
}

pub(crate) fn assign_action_to_visual_control(
    actions: &mut BTreeMap<String, Option<MappingEntry>>,
    action: &str,
    control: VisualControlId,
) {
    let mapped_entry = visual_control_to_mapping_entry(control);
    for (existing_action, entry) in actions.iter_mut() {
        if entry.as_ref().is_some_and(|existing| *existing == mapped_entry) {
            *entry = None;
        }
        if existing_action == action {
            *entry = None;
        }
    }
    actions.insert(action.to_string(), Some(mapped_entry));
}

pub(crate) fn unassign_visual_control(
    actions: &mut BTreeMap<String, Option<MappingEntry>>,
    control: VisualControlId,
) {
    let mapped_entry = visual_control_to_mapping_entry(control);
    for entry in actions.values_mut() {
        if entry.as_ref().is_some_and(|existing| *existing == mapped_entry) {
            *entry = None;
        }
    }
}

impl ControllerHotspot {
    pub(crate) fn contains(self, image_rect: Rect, uv_rect: Rect, pointer_pos: Pos2) -> bool {
        match self.shape {
            HotspotShape::Circle { center, radius } => {
                let center = hotspot_pos(image_rect, uv_rect, center);
                pointer_pos.distance(center)
                    <= radius * image_rect.width().min(image_rect.height()) / uv_rect.width()
            }
            HotspotShape::Rect { center, size } => {
                let center = hotspot_pos(image_rect, uv_rect, center);
                let size = vec2(
                    size[0] * image_rect.width() / uv_rect.width(),
                    size[1] * image_rect.height() / uv_rect.height(),
                );
                Rect::from_center_size(center, size).contains(pointer_pos)
            }
        }
    }

    pub(crate) fn paint_rect(self, image_rect: Rect, uv_rect: Rect) -> Rect {
        match self.shape {
            HotspotShape::Circle { center, radius } => {
                let center = hotspot_pos(image_rect, uv_rect, center);
                let radius = radius * image_rect.width().min(image_rect.height()) / uv_rect.width();
                Rect::from_center_size(center, vec2(radius * 2.0, radius * 2.0))
            }
            HotspotShape::Rect { center, size } => {
                let center = hotspot_pos(image_rect, uv_rect, center);
                Rect::from_center_size(
                    center,
                    vec2(
                        size[0] * image_rect.width() / uv_rect.width(),
                        size[1] * image_rect.height() / uv_rect.height(),
                    ),
                )
            }
        }
    }
}

fn visual_control_fallback_label(control: VisualControlId) -> &'static str {
    match control {
        VisualControlId::Button(button) => match button {
            CanonicalButton::South => "South Button",
            CanonicalButton::East => "East Button",
            CanonicalButton::North => "North Button",
            CanonicalButton::West => "West Button",
            CanonicalButton::DPadUp => "D-Pad Up",
            CanonicalButton::DPadDown => "D-Pad Down",
            CanonicalButton::DPadLeft => "D-Pad Left",
            CanonicalButton::DPadRight => "D-Pad Right",
            CanonicalButton::Start => "Start",
            CanonicalButton::Select => "Select",
            CanonicalButton::LeftShoulder => "Left Shoulder",
            CanonicalButton::RightShoulder => "Right Shoulder",
            CanonicalButton::Guide => "Guide",
            CanonicalButton::LeftThumb => "L3 / Left Stick Click",
            CanonicalButton::RightThumb => "R3 / Right Stick Click",
        },
        VisualControlId::Axis { axis, direction } => match (axis, direction.signum()) {
            (CanonicalAxis::LeftStickX, -1) => "Left Stick Left",
            (CanonicalAxis::LeftStickX, 1) => "Left Stick Right",
            (CanonicalAxis::LeftStickY, -1) => "Left Stick Up",
            (CanonicalAxis::LeftStickY, 1) => "Left Stick Down",
            (CanonicalAxis::RightStickX, -1) => "Right Stick Left",
            (CanonicalAxis::RightStickX, 1) => "Right Stick Right",
            (CanonicalAxis::RightStickY, -1) => "Right Stick Up",
            (CanonicalAxis::RightStickY, 1) => "Right Stick Down",
            (CanonicalAxis::LeftTrigger, _) => "Left Trigger",
            (CanonicalAxis::RightTrigger, _) => "Right Trigger",
            _ => "Axis Binding",
        },
    }
}

fn hotspot_pos(image_rect: Rect, uv_rect: Rect, normalized_center: [f32; 2]) -> Pos2 {
    pos2(
        image_rect.left() + ((normalized_center[0] - uv_rect.left()) / uv_rect.width()) * image_rect.width(),
        image_rect.top() + ((normalized_center[1] - uv_rect.top()) / uv_rect.height()) * image_rect.height(),
    )
}

const fn circle_button(
    button: CanonicalButton,
    label: &'static str,
    x: f32,
    y: f32,
    radius: f32,
) -> ControllerHotspot {
    ControllerHotspot {
        control: VisualControlId::Button(button),
        label,
        shape: HotspotShape::Circle {
            center: [x, y],
            radius,
        },
    }
}

const fn rect_button(
    button: CanonicalButton,
    label: &'static str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> ControllerHotspot {
    ControllerHotspot {
        control: VisualControlId::Button(button),
        label,
        shape: HotspotShape::Rect {
            center: [x, y],
            size: [width, height],
        },
    }
}

const fn rect_axis(
    axis: CanonicalAxis,
    direction: i8,
    label: &'static str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> ControllerHotspot {
    ControllerHotspot {
        control: VisualControlId::Axis { axis, direction },
        label,
        shape: HotspotShape::Rect {
            center: [x, y],
            size: [width, height],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_exact_sony_models() {
        let ps3 = DetectedPadIdentity {
            device_key: String::from("054c:0268:PS3"),
            name: String::from("PLAYSTATION(R)3 Controller"),
            vendor_id: Some(String::from("054c")),
            product_id: Some(String::from("0268")),
            mapping_name: None,
        };
        let ps4 = DetectedPadIdentity {
            device_key: String::from("054c:09cc:PS4"),
            name: String::from("Wireless Controller"),
            vendor_id: Some(String::from("054c")),
            product_id: Some(String::from("09cc")),
            mapping_name: None,
        };
        let ps5 = DetectedPadIdentity {
            device_key: String::from("054c:0ce6:DualSense"),
            name: String::from("DualSense Wireless Controller"),
            vendor_id: Some(String::from("054c")),
            product_id: Some(String::from("0ce6")),
            mapping_name: None,
        };

        assert_eq!(
            controller_mapper_art_for_device(Some(&ps3)),
            ControllerMapperArt::Ps3
        );
        assert_eq!(
            controller_mapper_art_for_device(Some(&ps4)),
            ControllerMapperArt::Ps4
        );
        assert_eq!(
            controller_mapper_art_for_device(Some(&ps5)),
            ControllerMapperArt::Ps5
        );
    }

    #[test]
    fn resolves_xbox_and_unknown_fallbacks() {
        let xbox = DetectedPadIdentity {
            device_key: String::from("045e:02fd:Xbox"),
            name: String::from("Xbox Wireless Controller"),
            vendor_id: Some(String::from("045e")),
            product_id: Some(String::from("02fd")),
            mapping_name: None,
        };
        let sony_unknown = DetectedPadIdentity {
            device_key: String::from("054c:1234:Sony"),
            name: String::from("Sony Pad"),
            vendor_id: Some(String::from("054c")),
            product_id: Some(String::from("1234")),
            mapping_name: None,
        };
        let generic = DetectedPadIdentity {
            device_key: String::from("unknown"),
            name: String::from("8BitPad"),
            vendor_id: Some(String::from("9999")),
            product_id: Some(String::from("0001")),
            mapping_name: None,
        };

        assert_eq!(
            controller_mapper_art_for_device(Some(&xbox)),
            ControllerMapperArt::Xbox
        );
        assert_eq!(
            controller_mapper_art_for_device(Some(&sony_unknown)),
            ControllerMapperArt::Ps3
        );
        assert_eq!(
            controller_mapper_art_for_device(Some(&generic)),
            ControllerMapperArt::Generic
        );
    }

    #[test]
    fn name_heuristics_resolve_without_vendor_ids() {
        let ps5 = DetectedPadIdentity {
            device_key: String::from("dualsense"),
            name: String::from("DualSense Controller"),
            vendor_id: None,
            product_id: None,
            mapping_name: None,
        };
        let ps4 = DetectedPadIdentity {
            device_key: String::from("ps4"),
            name: String::from("PS4 Controller"),
            vendor_id: None,
            product_id: None,
            mapping_name: None,
        };
        let xbox = DetectedPadIdentity {
            device_key: String::from("xinput"),
            name: String::from("XInput Controller"),
            vendor_id: None,
            product_id: None,
            mapping_name: None,
        };

        assert_eq!(
            controller_mapper_art_for_device(Some(&ps5)),
            ControllerMapperArt::Ps5
        );
        assert_eq!(
            controller_mapper_art_for_device(Some(&ps4)),
            ControllerMapperArt::Ps4
        );
        assert_eq!(
            controller_mapper_art_for_device(Some(&xbox)),
            ControllerMapperArt::Xbox
        );
    }

    #[test]
    fn top_hotspots_only_expose_shoulders_and_triggers() {
        let expected = [
            VisualControlId::Button(CanonicalButton::LeftShoulder),
            VisualControlId::Axis {
                axis: CanonicalAxis::LeftTrigger,
                direction: 1,
            },
            VisualControlId::Button(CanonicalButton::RightShoulder),
            VisualControlId::Axis {
                axis: CanonicalAxis::RightTrigger,
                direction: 1,
            },
        ];

        for art in [
            ControllerMapperArt::Ps3,
            ControllerMapperArt::Ps4,
            ControllerMapperArt::Ps5,
            ControllerMapperArt::Xbox,
            ControllerMapperArt::Generic,
        ] {
            let hotspots = controller_mapper_hotspots(art, ControllerMapperView::Top);
            assert_eq!(hotspots.len(), expected.len());
            for control in expected {
                assert!(hotspots.iter().any(|hotspot| hotspot.control == control));
            }
        }
    }

    #[test]
    fn front_hotspots_include_required_non_shoulder_controls() {
        let required = [
            VisualControlId::Button(CanonicalButton::Guide),
            VisualControlId::Button(CanonicalButton::Select),
            VisualControlId::Button(CanonicalButton::Start),
            VisualControlId::Button(CanonicalButton::DPadUp),
            VisualControlId::Button(CanonicalButton::DPadDown),
            VisualControlId::Button(CanonicalButton::DPadLeft),
            VisualControlId::Button(CanonicalButton::DPadRight),
            VisualControlId::Button(CanonicalButton::West),
            VisualControlId::Button(CanonicalButton::North),
            VisualControlId::Button(CanonicalButton::East),
            VisualControlId::Button(CanonicalButton::South),
            VisualControlId::Button(CanonicalButton::LeftThumb),
            VisualControlId::Button(CanonicalButton::RightThumb),
            VisualControlId::Axis {
                axis: CanonicalAxis::LeftStickX,
                direction: -1,
            },
            VisualControlId::Axis {
                axis: CanonicalAxis::LeftStickX,
                direction: 1,
            },
            VisualControlId::Axis {
                axis: CanonicalAxis::LeftStickY,
                direction: -1,
            },
            VisualControlId::Axis {
                axis: CanonicalAxis::LeftStickY,
                direction: 1,
            },
            VisualControlId::Axis {
                axis: CanonicalAxis::RightStickX,
                direction: -1,
            },
            VisualControlId::Axis {
                axis: CanonicalAxis::RightStickX,
                direction: 1,
            },
            VisualControlId::Axis {
                axis: CanonicalAxis::RightStickY,
                direction: -1,
            },
            VisualControlId::Axis {
                axis: CanonicalAxis::RightStickY,
                direction: 1,
            },
        ];

        for art in [
            ControllerMapperArt::Ps3,
            ControllerMapperArt::Ps4,
            ControllerMapperArt::Ps5,
            ControllerMapperArt::Xbox,
            ControllerMapperArt::Generic,
        ] {
            let hotspots = controller_mapper_hotspots(art, ControllerMapperView::Front);
            assert_eq!(hotspots.len(), required.len());
            for control in required {
                assert!(hotspots.iter().any(|hotspot| hotspot.control == control));
            }
        }
    }

    #[test]
    fn ps5_top_hotspots_are_tight_in_cropped_strip() {
        let image_rect = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
        let uv_rect = Rect::from_min_max(pos2(0.04, 0.18), pos2(0.96, 0.72));
        let hotspots = controller_mapper_hotspots(ControllerMapperArt::Ps5, ControllerMapperView::Top);

        for hotspot in hotspots {
            let rect = hotspot.paint_rect(image_rect, uv_rect);
            match hotspot.control {
                VisualControlId::Button(CanonicalButton::LeftShoulder)
                | VisualControlId::Button(CanonicalButton::RightShoulder) => {
                    assert!(rect.width() < 0.16);
                    assert!(rect.height() < 0.16);
                }
                VisualControlId::Axis {
                    axis: CanonicalAxis::LeftTrigger,
                    ..
                }
                | VisualControlId::Axis {
                    axis: CanonicalAxis::RightTrigger,
                    ..
                } => {
                    assert!(rect.width() < 0.12);
                    assert!(rect.height() < 0.26);
                }
                _ => {}
            }
        }
    }

    #[test]
    fn ps5_front_stick_direction_zones_are_tight_and_non_overlapping() {
        let image_rect = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
        let uv_rect = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
        let hotspots =
            controller_mapper_hotspots(ControllerMapperArt::Ps5, ControllerMapperView::Front);
        let left_zones: Vec<_> = hotspots
            .iter()
            .filter(|hotspot| {
                matches!(
                    hotspot.control,
                    VisualControlId::Axis {
                        axis: CanonicalAxis::LeftStickX | CanonicalAxis::LeftStickY,
                        ..
                    }
                )
            })
            .map(|hotspot| hotspot.paint_rect(image_rect, uv_rect))
            .collect();
        let right_zones: Vec<_> = hotspots
            .iter()
            .filter(|hotspot| {
                matches!(
                    hotspot.control,
                    VisualControlId::Axis {
                        axis: CanonicalAxis::RightStickX | CanonicalAxis::RightStickY,
                        ..
                    }
                )
            })
            .map(|hotspot| hotspot.paint_rect(image_rect, uv_rect))
            .collect();

        for rect in left_zones.iter().chain(right_zones.iter()) {
            assert!(rect.width() <= 0.06);
            assert!(rect.height() <= 0.06);
        }

        for rects in [&left_zones, &right_zones] {
            for (index, first) in rects.iter().enumerate() {
                for second in rects.iter().skip(index + 1) {
                    assert!(!first.intersects(*second));
                }
            }
        }
    }

    #[test]
    fn model_specific_labels_are_exposed() {
        assert_eq!(
            controller_mapper_control_label(
                ControllerMapperArt::Ps3,
                VisualControlId::Button(CanonicalButton::Select)
            ),
            "Select"
        );
        assert_eq!(
            controller_mapper_control_label(
                ControllerMapperArt::Ps4,
                VisualControlId::Button(CanonicalButton::Select)
            ),
            "Share"
        );
        assert_eq!(
            controller_mapper_control_label(
                ControllerMapperArt::Ps5,
                VisualControlId::Button(CanonicalButton::Select)
            ),
            "Create"
        );
        assert_eq!(
            controller_mapper_control_label(
                ControllerMapperArt::Xbox,
                VisualControlId::Button(CanonicalButton::Guide)
            ),
            "Guide"
        );
    }

    #[test]
    fn assign_action_moves_existing_action_and_control() {
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
        ]);

        assign_action_to_visual_control(
            &mut actions,
            "A",
            VisualControlId::Button(CanonicalButton::East),
        );

        assert_eq!(
            actions.get("A"),
            Some(&Some(MappingEntry::Button {
                button: CanonicalButton::East,
            }))
        );
        assert_eq!(actions.get("B"), Some(&None));
    }

    #[test]
    fn unassign_visual_control_clears_matching_action() {
        let mut actions = BTreeMap::from([(
            String::from("L2"),
            Some(MappingEntry::Axis {
                axis: CanonicalAxis::LeftTrigger,
                direction: 1,
            }),
        )]);

        unassign_visual_control(
            &mut actions,
            VisualControlId::Axis {
                axis: CanonicalAxis::LeftTrigger,
                direction: 1,
            },
        );

        assert_eq!(actions.get("L2"), Some(&None));
    }

    #[test]
    fn mapping_entry_round_trips_through_visual_control() {
        let entry = MappingEntry::Axis {
            axis: CanonicalAxis::RightStickX,
            direction: -1,
        };
        let control = visual_control_from_mapping_entry(&entry).expect("visual control");
        assert_eq!(visual_control_to_mapping_entry(control), entry);
    }
}
