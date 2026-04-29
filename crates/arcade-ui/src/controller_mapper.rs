use std::collections::{BTreeMap, HashSet};

use arcade_domain::{CanonicalAxis, CanonicalButton, DetectedPadIdentity, MappingEntry};
use egui::{pos2, vec2, Pos2, Rect};
use roxmltree::Document;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControllerMapperArt {
    Ps3,
    Ps4,
    Ps5,
    Xbox,
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SystemControllerLayout {
    Nes,
    Snes,
    Genesis,
    N64,
    Psx,
    Ps2,
    Dreamcast,
    Saturn,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SystemActionHotspot {
    pub(crate) action: &'static str,
    pub(crate) label: &'static str,
    pub(crate) shape: HotspotShape,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum OverlayHotspotShape {
    Circle {
        center: [f32; 2],
        radius: f32,
    },
    Rect {
        center: [f32; 2],
        size: [f32; 2],
    },
    Polygon {
        points: Vec<[f32; 2]>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SystemOverlayHotspot {
    pub(crate) id: String,
    pub(crate) action: String,
    pub(crate) label: String,
    pub(crate) shape: OverlayHotspotShape,
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

const NES_SYSTEM_HOTSPOTS: [SystemActionHotspot; 8] = [
    system_circle("Up", "Up", 0.235, 0.435, 0.040),
    system_circle("Down", "Down", 0.235, 0.625, 0.040),
    system_circle("Left", "Left", 0.145, 0.530, 0.040),
    system_circle("Right", "Right", 0.325, 0.530, 0.040),
    system_circle("A", "A", 0.775, 0.470, 0.040),
    system_circle("B", "B", 0.690, 0.555, 0.040),
    system_rect("Select", "Select", 0.445, 0.705, 0.090, 0.048),
    system_rect("Start", "Start", 0.565, 0.705, 0.090, 0.048),
];

const SNES_SYSTEM_HOTSPOTS: [SystemActionHotspot; 12] = [
    system_circle("Up", "Up", 0.238, 0.452, 0.034),
    system_circle("Down", "Down", 0.238, 0.620, 0.034),
    system_circle("Left", "Left", 0.158, 0.536, 0.034),
    system_circle("Right", "Right", 0.318, 0.536, 0.034),
    system_circle("X", "X", 0.760, 0.430, 0.035),
    system_circle("Y", "Y", 0.685, 0.505, 0.035),
    system_circle("A", "A", 0.835, 0.505, 0.035),
    system_circle("B", "B", 0.760, 0.580, 0.035),
    system_rect("L", "L", 0.170, 0.190, 0.120, 0.052),
    system_rect("R", "R", 0.830, 0.190, 0.120, 0.052),
    system_rect("Select", "Select", 0.455, 0.705, 0.080, 0.042),
    system_rect("Start", "Start", 0.550, 0.705, 0.080, 0.042),
];

const GENESIS_SYSTEM_HOTSPOTS: [SystemActionHotspot; 8] = [
    system_circle("Up", "Up", 0.245, 0.450, 0.036),
    system_circle("Down", "Down", 0.245, 0.625, 0.036),
    system_circle("Left", "Left", 0.162, 0.538, 0.036),
    system_circle("Right", "Right", 0.328, 0.538, 0.036),
    system_circle("A", "A", 0.665, 0.575, 0.038),
    system_circle("B", "B", 0.760, 0.520, 0.038),
    system_circle("C", "C", 0.855, 0.470, 0.038),
    system_rect("Start", "Start", 0.520, 0.735, 0.120, 0.048),
];

const N64_SYSTEM_HOTSPOTS: [SystemActionHotspot; 14] = [
    system_circle("Up", "Up", 0.190, 0.430, 0.030),
    system_circle("Down", "Down", 0.190, 0.575, 0.030),
    system_circle("Left", "Left", 0.120, 0.502, 0.030),
    system_circle("Right", "Right", 0.260, 0.502, 0.030),
    system_rect("Stick Up", "Stick Up", 0.500, 0.355, 0.055, 0.035),
    system_rect("Stick Down", "Stick Down", 0.500, 0.535, 0.055, 0.035),
    system_rect("Stick Left", "Stick Left", 0.435, 0.445, 0.035, 0.055),
    system_rect("Stick Right", "Stick Right", 0.565, 0.445, 0.035, 0.055),
    system_circle("A", "A", 0.770, 0.510, 0.038),
    system_circle("B", "B", 0.700, 0.585, 0.034),
    system_rect("L", "L", 0.255, 0.145, 0.115, 0.048),
    system_rect("R", "R", 0.745, 0.145, 0.115, 0.048),
    system_circle("Z", "Z", 0.500, 0.800, 0.040),
    system_circle("Start", "Start", 0.500, 0.645, 0.030),
];

const N64_SYSTEM_HOTSPOTS_C: [SystemActionHotspot; 4] = [
    system_circle("C-Up", "C-Up", 0.865, 0.360, 0.026),
    system_circle("C-Down", "C-Down", 0.865, 0.510, 0.026),
    system_circle("C-Left", "C-Left", 0.795, 0.435, 0.026),
    system_circle("C-Right", "C-Right", 0.935, 0.435, 0.026),
];

const PSX_SYSTEM_HOTSPOTS: [SystemActionHotspot; 14] = [
    system_circle("Up", "Up", 0.200, 0.345, 0.030),
    system_circle("Down", "Down", 0.200, 0.490, 0.030),
    system_circle("Left", "Left", 0.132, 0.418, 0.030),
    system_circle("Right", "Right", 0.268, 0.418, 0.030),
    system_circle("Triangle", "Triangle", 0.800, 0.330, 0.030),
    system_circle("Square", "Square", 0.730, 0.415, 0.030),
    system_circle("Circle", "Circle", 0.870, 0.415, 0.030),
    system_circle("Cross", "Cross", 0.800, 0.500, 0.030),
    system_rect("L1", "L1", 0.215, 0.165, 0.100, 0.040),
    system_rect("R1", "R1", 0.785, 0.165, 0.100, 0.040),
    system_rect("L2", "L2", 0.215, 0.105, 0.090, 0.034),
    system_rect("R2", "R2", 0.785, 0.105, 0.090, 0.034),
    system_rect("Select", "Select", 0.430, 0.575, 0.080, 0.040),
    system_rect("Start", "Start", 0.570, 0.575, 0.080, 0.040),
];

const PS2_SYSTEM_HOTSPOTS: [SystemActionHotspot; 22] = [
    system_circle("Up", "Up", 0.200, 0.345, 0.028),
    system_circle("Down", "Down", 0.200, 0.490, 0.028),
    system_circle("Left", "Left", 0.132, 0.418, 0.028),
    system_circle("Right", "Right", 0.268, 0.418, 0.028),
    system_circle("Triangle", "Triangle", 0.800, 0.330, 0.028),
    system_circle("Square", "Square", 0.730, 0.415, 0.028),
    system_circle("Circle", "Circle", 0.870, 0.415, 0.028),
    system_circle("Cross", "Cross", 0.800, 0.500, 0.028),
    system_rect("L1", "L1", 0.215, 0.165, 0.100, 0.040),
    system_rect("R1", "R1", 0.785, 0.165, 0.100, 0.040),
    system_rect("L2", "L2", 0.215, 0.105, 0.090, 0.034),
    system_rect("R2", "R2", 0.785, 0.105, 0.090, 0.034),
    system_circle("L3", "L3", 0.365, 0.585, 0.048),
    system_circle("R3", "R3", 0.635, 0.585, 0.048),
    system_rect("Left Stick Up", "Left Stick Up", 0.365, 0.530, 0.042, 0.020),
    system_rect("Left Stick Down", "Left Stick Down", 0.365, 0.640, 0.042, 0.020),
    system_rect("Left Stick Left", "Left Stick Left", 0.310, 0.585, 0.020, 0.042),
    system_rect("Left Stick Right", "Left Stick Right", 0.420, 0.585, 0.020, 0.042),
    system_rect("Right Stick Up", "Right Stick Up", 0.635, 0.530, 0.042, 0.020),
    system_rect("Right Stick Down", "Right Stick Down", 0.635, 0.640, 0.042, 0.020),
    system_rect("Right Stick Left", "Right Stick Left", 0.580, 0.585, 0.020, 0.042),
    system_rect("Right Stick Right", "Right Stick Right", 0.690, 0.585, 0.020, 0.042),
];

const PS2_SYSTEM_HOTSPOTS_CENTER: [SystemActionHotspot; 2] = [
    system_rect("Select", "Select", 0.430, 0.475, 0.080, 0.040),
    system_rect("Start", "Start", 0.570, 0.475, 0.080, 0.040),
];

const DREAMCAST_SYSTEM_HOTSPOTS: [SystemActionHotspot; 9] = [
    system_circle("Up", "Up", 0.285, 0.395, 0.032),
    system_circle("Down", "Down", 0.285, 0.560, 0.032),
    system_circle("Left", "Left", 0.205, 0.478, 0.032),
    system_circle("Right", "Right", 0.365, 0.478, 0.032),
    system_circle("Y", "Y", 0.760, 0.355, 0.034),
    system_circle("X", "X", 0.690, 0.440, 0.034),
    system_circle("B", "B", 0.830, 0.440, 0.034),
    system_circle("A", "A", 0.760, 0.525, 0.034),
    system_rect("Start", "Start", 0.525, 0.630, 0.110, 0.046),
];

const SATURN_SYSTEM_HOTSPOTS: [SystemActionHotspot; 13] = [
    system_circle("Up", "Up", 0.205, 0.420, 0.032),
    system_circle("Down", "Down", 0.205, 0.575, 0.032),
    system_circle("Left", "Left", 0.130, 0.498, 0.032),
    system_circle("Right", "Right", 0.280, 0.498, 0.032),
    system_circle("X", "X", 0.670, 0.360, 0.030),
    system_circle("Y", "Y", 0.760, 0.360, 0.030),
    system_circle("Z", "Z", 0.850, 0.360, 0.030),
    system_circle("A", "A", 0.670, 0.525, 0.030),
    system_circle("B", "B", 0.760, 0.525, 0.030),
    system_circle("C", "C", 0.850, 0.525, 0.030),
    system_rect("L", "L", 0.165, 0.170, 0.110, 0.046),
    system_rect("R", "R", 0.835, 0.170, 0.110, 0.046),
    system_rect("Start", "Start", 0.515, 0.690, 0.120, 0.046),
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

pub(crate) fn system_controller_layout_for_system(system: &str) -> Option<SystemControllerLayout> {
    match system.trim().to_ascii_uppercase().as_str() {
        "NES" => Some(SystemControllerLayout::Nes),
        "SNES" => Some(SystemControllerLayout::Snes),
        "GENESIS" => Some(SystemControllerLayout::Genesis),
        "N64" => Some(SystemControllerLayout::N64),
        "PSX" => Some(SystemControllerLayout::Psx),
        "PS2" => Some(SystemControllerLayout::Ps2),
        "DREAMCAST" => Some(SystemControllerLayout::Dreamcast),
        "SATURN" => Some(SystemControllerLayout::Saturn),
        _ => None,
    }
}

pub(crate) fn system_controller_mapper_art_asset(layout: SystemControllerLayout) -> &'static str {
    match layout {
        SystemControllerLayout::Nes => "/gamepads/controllercons.2.1/svg/outline/nes.svg",
        SystemControllerLayout::Snes => "/gamepads/controllercons.2.1/svg/outline/snes.svg",
        SystemControllerLayout::Genesis => "/gamepads/controllercons.2.1/svg/outline/mega-drive.svg",
        SystemControllerLayout::N64 => "/gamepads/controllercons.2.1/svg/outline/n64.svg",
        SystemControllerLayout::Psx => "/gamepads/controllercons.2.1/svg/outline/ps1.svg",
        SystemControllerLayout::Ps2 => "/gamepads/controllercons.2.1/svg/outline/ps2.svg",
        SystemControllerLayout::Dreamcast => "/gamepads/controllercons.2.1/svg/outline/dreamcast.svg",
        SystemControllerLayout::Saturn => "/gamepads/controllercons.2.1/svg/outline/sega-saturn.svg",
    }
}

pub(crate) fn system_controller_hotspot_overlay_asset(
    layout: SystemControllerLayout,
) -> &'static str {
    match layout {
        SystemControllerLayout::Nes => "/gamepads/hotspots/outline/nes.hotspots.svg",
        SystemControllerLayout::Snes => "/gamepads/hotspots/outline/snes.hotspots.svg",
        SystemControllerLayout::Genesis => "/gamepads/hotspots/outline/genesis.hotspots.svg",
        SystemControllerLayout::N64 => "/gamepads/hotspots/outline/n64.hotspots.svg",
        SystemControllerLayout::Psx => "/gamepads/hotspots/outline/psx.hotspots.svg",
        SystemControllerLayout::Ps2 => "/gamepads/hotspots/outline/ps2.hotspots.svg",
        SystemControllerLayout::Dreamcast => "/gamepads/hotspots/outline/dreamcast.hotspots.svg",
        SystemControllerLayout::Saturn => "/gamepads/hotspots/outline/saturn.hotspots.svg",
    }
}

pub(crate) fn system_controller_hotspots(
    layout: SystemControllerLayout,
) -> &'static [SystemActionHotspot] {
    match layout {
        SystemControllerLayout::Nes => &NES_SYSTEM_HOTSPOTS,
        SystemControllerLayout::Snes => &SNES_SYSTEM_HOTSPOTS,
        SystemControllerLayout::Genesis => &GENESIS_SYSTEM_HOTSPOTS,
        SystemControllerLayout::N64 => &N64_SYSTEM_HOTSPOTS,
        SystemControllerLayout::Psx => &PSX_SYSTEM_HOTSPOTS,
        SystemControllerLayout::Ps2 => &PS2_SYSTEM_HOTSPOTS,
        SystemControllerLayout::Dreamcast => &DREAMCAST_SYSTEM_HOTSPOTS,
        SystemControllerLayout::Saturn => &SATURN_SYSTEM_HOTSPOTS,
    }
}

pub(crate) fn system_controller_additional_hotspots(
    layout: SystemControllerLayout,
) -> &'static [SystemActionHotspot] {
    match layout {
        SystemControllerLayout::N64 => &N64_SYSTEM_HOTSPOTS_C,
        SystemControllerLayout::Ps2 => &PS2_SYSTEM_HOTSPOTS_CENTER,
        _ => &[],
    }
}

pub(crate) fn system_action_label(
    layout: SystemControllerLayout,
    action: &str,
) -> Option<&'static str> {
    system_controller_hotspots(layout)
        .iter()
        .chain(system_controller_additional_hotspots(layout).iter())
        .find_map(|hotspot| (hotspot.action == action).then_some(hotspot.label))
}

pub(crate) fn system_action_is_native(system: &str, action: &str) -> bool {
    system_controller_layout_for_system(system)
        .and_then(|layout| system_action_label(layout, action))
        .is_some()
}

pub(crate) fn parse_system_hotspot_overlay(
    layout: SystemControllerLayout,
    svg_data: &str,
) -> Result<Vec<SystemOverlayHotspot>, String> {
    let document = Document::parse(svg_data).map_err(|err| err.to_string())?;
    let root = document.root_element();
    let view_box = parse_view_box(root.attribute("viewBox"))
        .ok_or_else(|| String::from("hotspot overlay is missing a valid viewBox"))?;
    if view_box[2] <= 0.0 || view_box[3] <= 0.0 {
        return Err(String::from("hotspot overlay viewBox must have positive width and height"));
    }

    let mut hotspots = Vec::new();
    let mut ids = HashSet::new();
    let mut actions = HashSet::new();

    for node in root.descendants().filter(|node| node.is_element()) {
        let tag = node.tag_name().name();
        if !matches!(tag, "circle" | "rect" | "polygon") {
            continue;
        }

        let id = node
            .attribute("id")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("{tag} hotspot is missing an id"))?;
        if !ids.insert(id.to_string()) {
            return Err(format!("duplicate hotspot id: {id}"));
        }

        let action = node
            .attribute("data-action")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("hotspot {id} is missing data-action"))?;
        if system_action_label(layout, action).is_none() {
            return Err(format!("hotspot {id} uses unknown action {action}"));
        }
        if !actions.insert(action.to_string()) {
            return Err(format!("duplicate hotspot action: {action}"));
        }

        let label = node
            .attribute("data-label")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                system_action_label(layout, action)
                    .unwrap_or(action)
                    .to_string()
            });

        let shape = parse_overlay_hotspot_shape(tag, &node, view_box)
            .map_err(|err| format!("hotspot {id}: {err}"))?;

        hotspots.push(SystemOverlayHotspot {
            id: id.to_string(),
            action: action.to_string(),
            label,
            shape,
        });
    }

    if hotspots.is_empty() {
        return Err(String::from("hotspot overlay does not define any supported shapes"));
    }

    validate_system_overlay_actions(layout, &hotspots)?;
    Ok(hotspots)
}

pub(crate) fn physical_input_controls(art: ControllerMapperArt) -> Vec<ControllerHotspot> {
    let mut controls = Vec::new();
    for hotspot in controller_mapper_hotspots(art, ControllerMapperView::Front)
        .iter()
        .chain(controller_mapper_hotspots(art, ControllerMapperView::Top).iter())
    {
        if controls
            .iter()
            .any(|existing: &ControllerHotspot| existing.control == hotspot.control)
        {
            continue;
        }
        controls.push(*hotspot);
    }
    controls
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

pub(crate) fn physical_input_for_action(
    actions: &BTreeMap<String, Option<MappingEntry>>,
    action: &str,
) -> Option<VisualControlId> {
    actions
        .get(action)
        .and_then(|entry| entry.as_ref())
        .and_then(visual_control_from_mapping_entry)
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

pub(crate) fn assign_physical_input_to_action(
    actions: &mut BTreeMap<String, Option<MappingEntry>>,
    action: &str,
    control: VisualControlId,
) {
    assign_action_to_visual_control(actions, action, control);
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

pub(crate) fn unassign_action(actions: &mut BTreeMap<String, Option<MappingEntry>>, action: &str) {
    if let Some(entry) = actions.get_mut(action) {
        *entry = None;
    }
}

impl ControllerHotspot {
    pub(crate) fn paint_rect(self, image_rect: Rect, uv_rect: Rect) -> Rect {
        hotspot_shape_rect_with_uv(self.shape, image_rect, uv_rect)
    }
}

impl SystemOverlayHotspot {
    pub(crate) fn contains(&self, image_rect: Rect, pointer_pos: Pos2) -> bool {
        overlay_hotspot_shape_contains(&self.shape, image_rect, pointer_pos)
    }

    pub(crate) fn paint_rect(&self, image_rect: Rect) -> Rect {
        overlay_hotspot_shape_rect(&self.shape, image_rect)
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

fn hotspot_shape_rect_with_uv(shape: HotspotShape, image_rect: Rect, uv_rect: Rect) -> Rect {
    match shape {
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

fn overlay_hotspot_shape_contains(
    shape: &OverlayHotspotShape,
    image_rect: Rect,
    pointer_pos: Pos2,
) -> bool {
    if !image_rect.contains(pointer_pos) {
        return false;
    }
    let normalized_point = [
        ((pointer_pos.x - image_rect.left()) / image_rect.width()).clamp(0.0, 1.0),
        ((pointer_pos.y - image_rect.top()) / image_rect.height()).clamp(0.0, 1.0),
    ];
    match shape {
        OverlayHotspotShape::Circle { center, radius } => {
            let dx = normalized_point[0] - center[0];
            let dy = normalized_point[1] - center[1];
            dx * dx + dy * dy <= radius * radius
        }
        OverlayHotspotShape::Rect { center, size } => {
            let half_width = size[0] * 0.5;
            let half_height = size[1] * 0.5;
            normalized_point[0] >= center[0] - half_width
                && normalized_point[0] <= center[0] + half_width
                && normalized_point[1] >= center[1] - half_height
                && normalized_point[1] <= center[1] + half_height
        }
        OverlayHotspotShape::Polygon { points } => point_in_polygon(normalized_point, points),
    }
}

fn overlay_hotspot_shape_rect(shape: &OverlayHotspotShape, image_rect: Rect) -> Rect {
    match shape {
        OverlayHotspotShape::Circle { center, radius } => Rect::from_center_size(
            hotspot_pos(
                image_rect,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                *center,
            ),
            vec2(
                radius * image_rect.width() * 2.0,
                radius * image_rect.height() * 2.0,
            ),
        ),
        OverlayHotspotShape::Rect { center, size } => Rect::from_center_size(
            hotspot_pos(
                image_rect,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                *center,
            ),
            vec2(size[0] * image_rect.width(), size[1] * image_rect.height()),
        ),
        OverlayHotspotShape::Polygon { points } => polygon_bounding_rect(points, image_rect),
    }
}

fn polygon_bounding_rect(points: &[[f32; 2]], image_rect: Rect) -> Rect {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for point in points {
        min_x = min_x.min(point[0]);
        min_y = min_y.min(point[1]);
        max_x = max_x.max(point[0]);
        max_y = max_y.max(point[1]);
    }
    Rect::from_min_max(
        pos2(
            image_rect.left() + min_x * image_rect.width(),
            image_rect.top() + min_y * image_rect.height(),
        ),
        pos2(
            image_rect.left() + max_x * image_rect.width(),
            image_rect.top() + max_y * image_rect.height(),
        ),
    )
}

fn point_in_polygon(point: [f32; 2], polygon: &[[f32; 2]]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut previous = polygon.last().copied().unwrap_or([0.0, 0.0]);
    for current in polygon {
        let intersects = ((current[1] > point[1]) != (previous[1] > point[1]))
            && (point[0]
                < (previous[0] - current[0]) * (point[1] - current[1])
                    / (previous[1] - current[1])
                    + current[0]);
        if intersects {
            inside = !inside;
        }
        previous = *current;
    }
    inside
}

fn required_system_actions(layout: SystemControllerLayout) -> Vec<&'static str> {
    system_controller_hotspots(layout)
        .iter()
        .chain(system_controller_additional_hotspots(layout).iter())
        .map(|hotspot| hotspot.action)
        .collect()
}

fn validate_system_overlay_actions(
    layout: SystemControllerLayout,
    hotspots: &[SystemOverlayHotspot],
) -> Result<(), String> {
    let required = required_system_actions(layout);
    for action in &required {
        if !hotspots.iter().any(|hotspot| hotspot.action == *action) {
            return Err(format!("hotspot overlay is missing required action {action}"));
        }
    }
    Ok(())
}

fn parse_overlay_hotspot_shape(
    tag: &str,
    node: &roxmltree::Node<'_, '_>,
    view_box: [f32; 4],
) -> Result<OverlayHotspotShape, String> {
    match tag {
        "circle" => {
            let cx = parse_required_attr(node, "cx")?;
            let cy = parse_required_attr(node, "cy")?;
            let radius = parse_required_attr(node, "r")?;
            Ok(OverlayHotspotShape::Circle {
                center: normalize_point([cx, cy], view_box),
                radius: radius / view_box[2],
            })
        }
        "rect" => {
            let x = parse_required_attr(node, "x")?;
            let y = parse_required_attr(node, "y")?;
            let width = parse_required_attr(node, "width")?;
            let height = parse_required_attr(node, "height")?;
            Ok(OverlayHotspotShape::Rect {
                center: normalize_point([x + width * 0.5, y + height * 0.5], view_box),
                size: [width / view_box[2], height / view_box[3]],
            })
        }
        "polygon" => {
            let points_attr = node
                .attribute("points")
                .ok_or_else(|| String::from("polygon is missing points"))?;
            let raw_points = parse_polygon_points(points_attr)?;
            Ok(OverlayHotspotShape::Polygon {
                points: raw_points
                    .into_iter()
                    .map(|point| normalize_point(point, view_box))
                    .collect(),
            })
        }
        _ => Err(format!("unsupported hotspot shape {tag}")),
    }
}

fn parse_required_attr(node: &roxmltree::Node<'_, '_>, name: &str) -> Result<f32, String> {
    node.attribute(name)
        .ok_or_else(|| format!("missing {name}"))
        .and_then(|value| {
            value
                .trim()
                .parse::<f32>()
                .map_err(|_| format!("invalid {name}: {value}"))
        })
}

fn parse_view_box(raw: Option<&str>) -> Option<[f32; 4]> {
    let values: Vec<f32> = raw?
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<f32>().ok())
        .collect::<Option<Vec<_>>>()?;
    if values.len() != 4 {
        return None;
    }
    Some([values[0], values[1], values[2], values[3]])
}

fn parse_polygon_points(raw: &str) -> Result<Vec<[f32; 2]>, String> {
    let values: Vec<f32> = raw
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<f32>()
                .map_err(|_| format!("invalid polygon point value: {part}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values.len() < 6 || values.len() % 2 != 0 {
        return Err(String::from("polygon must contain at least three points"));
    }
    Ok(values
        .chunks_exact(2)
        .map(|chunk| [chunk[0], chunk[1]])
        .collect())
}

fn normalize_point(point: [f32; 2], view_box: [f32; 4]) -> [f32; 2] {
    [
        (point[0] - view_box[0]) / view_box[2],
        (point[1] - view_box[1]) / view_box[3],
    ]
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

const fn system_circle(
    action: &'static str,
    label: &'static str,
    x: f32,
    y: f32,
    radius: f32,
) -> SystemActionHotspot {
    SystemActionHotspot {
        action,
        label,
        shape: HotspotShape::Circle {
            center: [x, y],
            radius,
        },
    }
}

const fn system_rect(
    action: &'static str,
    label: &'static str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> SystemActionHotspot {
    SystemActionHotspot {
        action,
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
    fn supported_systems_resolve_to_system_controller_layouts() {
        assert_eq!(
            system_controller_layout_for_system("NES"),
            Some(SystemControllerLayout::Nes)
        );
        assert_eq!(
            system_controller_layout_for_system("GENESIS"),
            Some(SystemControllerLayout::Genesis)
        );
        assert_eq!(
            system_controller_layout_for_system("PSX"),
            Some(SystemControllerLayout::Psx)
        );
        assert_eq!(
            system_controller_layout_for_system("SATURN"),
            Some(SystemControllerLayout::Saturn)
        );
        assert_eq!(system_controller_layout_for_system("GBA"), None);
        assert_eq!(system_controller_layout_for_system("DOS"), None);
    }

    #[test]
    fn system_native_action_detection_matches_supported_visual_layouts() {
        assert!(system_action_is_native("NES", "A"));
        assert!(system_action_is_native("SNES", "L"));
        assert!(system_action_is_native("PS2", "Right Stick Left"));
        assert!(system_action_is_native("N64", "C-Up"));
        assert!(!system_action_is_native("NES", "Quick Save"));
        assert!(!system_action_is_native("DOS", "A"));
    }

    #[test]
    fn parser_reads_circle_rect_and_polygon_hotspots() {
        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
              <circle id="up" data-action="Up" cx="12" cy="24" r="2" />
              <rect id="select" data-action="Select" x="24" y="40" width="6" height="3" />
              <polygon id="a" data-action="A" points="48,28 52,32 48,36 44,32" />
              <circle id="down" data-action="Down" cx="12" cy="40" r="2" />
              <circle id="left" data-action="Left" cx="8" cy="32" r="2" />
              <circle id="right" data-action="Right" cx="16" cy="32" r="2" />
              <circle id="b" data-action="B" cx="44" cy="36" r="2" />
              <rect id="start" data-action="Start" x="34" y="40" width="6" height="3" />
            </svg>
        "#;
        let hotspots = parse_system_hotspot_overlay(SystemControllerLayout::Nes, svg).unwrap();

        assert_eq!(hotspots.len(), 8);
        assert!(matches!(
            hotspots.iter().find(|hotspot| hotspot.id == "up").unwrap().shape,
            OverlayHotspotShape::Circle { .. }
        ));
        assert!(matches!(
            hotspots
                .iter()
                .find(|hotspot| hotspot.id == "select")
                .unwrap()
                .shape,
            OverlayHotspotShape::Rect { .. }
        ));
        assert!(matches!(
            hotspots.iter().find(|hotspot| hotspot.id == "a").unwrap().shape,
            OverlayHotspotShape::Polygon { .. }
        ));
    }

    #[test]
    fn parser_rejects_hotspots_without_action() {
        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
              <circle id="up" cx="12" cy="24" r="2" />
            </svg>
        "#;
        let error = parse_system_hotspot_overlay(SystemControllerLayout::Nes, svg).unwrap_err();
        assert!(error.contains("data-action"));
    }

    #[test]
    fn parser_rejects_duplicate_hotspot_ids() {
        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
              <circle id="dup" data-action="Up" cx="12" cy="24" r="2" />
              <circle id="dup" data-action="Down" cx="12" cy="40" r="2" />
              <circle id="left" data-action="Left" cx="8" cy="32" r="2" />
              <circle id="right" data-action="Right" cx="16" cy="32" r="2" />
              <circle id="a" data-action="A" cx="48" cy="28" r="2" />
              <circle id="b" data-action="B" cx="44" cy="36" r="2" />
              <rect id="select" data-action="Select" x="24" y="40" width="6" height="3" />
              <rect id="start" data-action="Start" x="34" y="40" width="6" height="3" />
            </svg>
        "#;
        let error = parse_system_hotspot_overlay(SystemControllerLayout::Nes, svg).unwrap_err();
        assert!(error.contains("duplicate hotspot id"));
    }

    #[test]
    fn overlay_files_cover_required_actions_for_supported_systems() {
        let cases = [
            (
                SystemControllerLayout::Nes,
                include_str!("../../../public/gamepads/hotspots/outline/nes.hotspots.svg"),
            ),
            (
                SystemControllerLayout::Snes,
                include_str!("../../../public/gamepads/hotspots/outline/snes.hotspots.svg"),
            ),
            (
                SystemControllerLayout::Genesis,
                include_str!("../../../public/gamepads/hotspots/outline/genesis.hotspots.svg"),
            ),
            (
                SystemControllerLayout::N64,
                include_str!("../../../public/gamepads/hotspots/outline/n64.hotspots.svg"),
            ),
            (
                SystemControllerLayout::Psx,
                include_str!("../../../public/gamepads/hotspots/outline/psx.hotspots.svg"),
            ),
            (
                SystemControllerLayout::Ps2,
                include_str!("../../../public/gamepads/hotspots/outline/ps2.hotspots.svg"),
            ),
            (
                SystemControllerLayout::Dreamcast,
                include_str!("../../../public/gamepads/hotspots/outline/dreamcast.hotspots.svg"),
            ),
            (
                SystemControllerLayout::Saturn,
                include_str!("../../../public/gamepads/hotspots/outline/saturn.hotspots.svg"),
            ),
        ];

        for (layout, svg) in cases {
            let hotspots = parse_system_hotspot_overlay(layout, svg).unwrap();
            assert!(validate_system_overlay_actions(layout, &hotspots).is_ok());
        }
    }

    #[test]
    fn polygon_hit_testing_is_stable_across_sizes() {
        let hotspot = SystemOverlayHotspot {
            id: String::from("poly"),
            action: String::from("Up"),
            label: String::from("Up"),
            shape: OverlayHotspotShape::Polygon {
                points: vec![[0.25, 0.20], [0.45, 0.20], [0.35, 0.40]],
            },
        };
        let small_rect = Rect::from_min_max(Pos2::ZERO, pos2(200.0, 200.0));
        let large_rect = Rect::from_min_max(Pos2::ZERO, pos2(600.0, 600.0));

        assert!(hotspot.contains(small_rect, pos2(70.0, 55.0)));
        assert!(hotspot.contains(large_rect, pos2(210.0, 165.0)));
        assert!(!hotspot.contains(small_rect, pos2(20.0, 20.0)));
        assert!(!hotspot.contains(large_rect, pos2(60.0, 60.0)));
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
    fn assign_physical_input_rebinds_actions_one_to_one() {
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

        assign_physical_input_to_action(
            &mut actions,
            "B",
            VisualControlId::Button(CanonicalButton::South),
        );

        assert_eq!(physical_input_for_action(&actions, "A"), None);
        assert_eq!(
            physical_input_for_action(&actions, "B"),
            Some(VisualControlId::Button(CanonicalButton::South))
        );
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
    fn unassign_action_clears_selected_binding() {
        let mut actions = BTreeMap::from([(
            String::from("Start"),
            Some(MappingEntry::Button {
                button: CanonicalButton::Start,
            }),
        )]);

        unassign_action(&mut actions, "Start");

        assert_eq!(physical_input_for_action(&actions, "Start"), None);
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
