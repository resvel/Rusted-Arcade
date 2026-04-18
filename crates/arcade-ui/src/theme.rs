use eframe::egui;
use egui::Color32;

use crate::app::NativeArcadeUiApp;

pub(crate) const SYSTEM_FILTERS: [&str; 12] = [
    "ALL",
    "NES",
    "SNES",
    "GENESIS",
    "GB",
    "GBA",
    "N64",
    "ARCADE",
    "PSX",
    "PS2",
    "DREAMCAST",
    "DOS",
];

#[derive(Debug, Clone, Copy)]
pub(crate) struct ThemePalette {
    pub(crate) bg_top: Color32,
    pub(crate) bg_bottom: Color32,
    pub(crate) accent: Color32,
    pub(crate) accent_soft: Color32,
    pub(crate) panel: Color32,
    pub(crate) panel_alt: Color32,
    pub(crate) border: Color32,
    pub(crate) text: Color32,
    pub(crate) text_muted: Color32,
}

impl NativeArcadeUiApp {
    pub(crate) fn active_system(&self) -> &str {
        if self.state.library.system_filter.is_empty() {
            "ALL"
        } else {
            self.state.library.system_filter.as_str()
        }
    }

    pub(crate) fn palette_for_system(system: &str) -> ThemePalette {
        match system {
            "ALL" => ThemePalette {
                bg_top: Color32::from_rgb(10, 38, 52),
                bg_bottom: Color32::from_rgb(4, 10, 22),
                accent: Color32::from_rgb(68, 214, 255),
                accent_soft: Color32::from_rgba_premultiplied(68, 214, 255, 66),
                panel: Color32::from_rgba_premultiplied(10, 18, 28, 228),
                panel_alt: Color32::from_rgba_premultiplied(20, 30, 44, 232),
                border: Color32::from_rgba_premultiplied(255, 255, 255, 34),
                text: Color32::from_rgb(242, 247, 251),
                text_muted: Color32::from_rgb(171, 186, 200),
            },
            "NES" => ThemePalette {
                bg_top: Color32::from_rgb(36, 9, 12),
                bg_bottom: Color32::from_rgb(14, 18, 27),
                accent: Color32::from_rgb(230, 0, 18),
                accent_soft: Color32::from_rgba_premultiplied(230, 0, 18, 60),
                panel: Color32::from_rgba_premultiplied(16, 20, 30, 230),
                panel_alt: Color32::from_rgba_premultiplied(29, 18, 24, 235),
                border: Color32::from_rgba_premultiplied(255, 255, 255, 34),
                text: Color32::from_rgb(244, 245, 248),
                text_muted: Color32::from_rgb(176, 180, 190),
            },
            "SNES" => ThemePalette {
                bg_top: Color32::from_rgb(28, 22, 53),
                bg_bottom: Color32::from_rgb(14, 18, 32),
                accent: Color32::from_rgb(123, 108, 255),
                accent_soft: Color32::from_rgba_premultiplied(123, 108, 255, 70),
                panel: Color32::from_rgba_premultiplied(16, 20, 36, 230),
                panel_alt: Color32::from_rgba_premultiplied(25, 23, 49, 235),
                border: Color32::from_rgba_premultiplied(255, 255, 255, 36),
                text: Color32::from_rgb(244, 244, 250),
                text_muted: Color32::from_rgb(183, 185, 205),
            },
            "GENESIS" => ThemePalette {
                bg_top: Color32::from_rgb(10, 26, 44),
                bg_bottom: Color32::from_rgb(8, 16, 31),
                accent: Color32::from_rgb(0, 166, 255),
                accent_soft: Color32::from_rgba_premultiplied(0, 166, 255, 58),
                panel: Color32::from_rgba_premultiplied(12, 20, 34, 230),
                panel_alt: Color32::from_rgba_premultiplied(14, 29, 46, 235),
                border: Color32::from_rgba_premultiplied(255, 255, 255, 34),
                text: Color32::from_rgb(243, 246, 249),
                text_muted: Color32::from_rgb(170, 182, 194),
            },
            "GB" => ThemePalette {
                bg_top: Color32::from_rgb(24, 35, 16),
                bg_bottom: Color32::from_rgb(13, 22, 18),
                accent: Color32::from_rgb(155, 182, 111),
                accent_soft: Color32::from_rgba_premultiplied(155, 182, 111, 65),
                panel: Color32::from_rgba_premultiplied(16, 26, 20, 230),
                panel_alt: Color32::from_rgba_premultiplied(30, 39, 24, 235),
                border: Color32::from_rgba_premultiplied(255, 255, 255, 32),
                text: Color32::from_rgb(236, 242, 226),
                text_muted: Color32::from_rgb(169, 179, 156),
            },
            "GBA" => ThemePalette {
                bg_top: Color32::from_rgb(18, 28, 55),
                bg_bottom: Color32::from_rgb(12, 18, 32),
                accent: Color32::from_rgb(90, 160, 255),
                accent_soft: Color32::from_rgba_premultiplied(90, 160, 255, 62),
                panel: Color32::from_rgba_premultiplied(14, 22, 38, 230),
                panel_alt: Color32::from_rgba_premultiplied(22, 33, 58, 235),
                border: Color32::from_rgba_premultiplied(255, 255, 255, 34),
                text: Color32::from_rgb(242, 246, 252),
                text_muted: Color32::from_rgb(174, 186, 204),
            },
            "N64" => ThemePalette {
                bg_top: Color32::from_rgb(46, 22, 18),
                bg_bottom: Color32::from_rgb(16, 18, 29),
                accent: Color32::from_rgb(255, 93, 61),
                accent_soft: Color32::from_rgba_premultiplied(255, 93, 61, 65),
                panel: Color32::from_rgba_premultiplied(22, 20, 34, 230),
                panel_alt: Color32::from_rgba_premultiplied(43, 25, 23, 235),
                border: Color32::from_rgba_premultiplied(255, 255, 255, 35),
                text: Color32::from_rgb(248, 244, 243),
                text_muted: Color32::from_rgb(190, 176, 176),
            },
            "ARCADE" => ThemePalette {
                bg_top: Color32::from_rgb(46, 14, 18),
                bg_bottom: Color32::from_rgb(10, 8, 20),
                accent: Color32::from_rgb(255, 171, 58),
                accent_soft: Color32::from_rgba_premultiplied(255, 171, 58, 68),
                panel: Color32::from_rgba_premultiplied(18, 14, 24, 230),
                panel_alt: Color32::from_rgba_premultiplied(34, 18, 28, 235),
                border: Color32::from_rgba_premultiplied(255, 255, 255, 34),
                text: Color32::from_rgb(248, 244, 238),
                text_muted: Color32::from_rgb(196, 182, 170),
            },
            "PSX" => ThemePalette {
                bg_top: Color32::from_rgb(28, 32, 48),
                bg_bottom: Color32::from_rgb(10, 12, 24),
                accent: Color32::from_rgb(150, 160, 190),
                accent_soft: Color32::from_rgba_premultiplied(150, 160, 190, 60),
                panel: Color32::from_rgba_premultiplied(16, 18, 32, 230),
                panel_alt: Color32::from_rgba_premultiplied(26, 30, 48, 235),
                border: Color32::from_rgba_premultiplied(255, 255, 255, 32),
                text: Color32::from_rgb(240, 242, 248),
                text_muted: Color32::from_rgb(170, 175, 192),
            },
            "PS2" => ThemePalette {
                bg_top: Color32::from_rgb(10, 14, 42),
                bg_bottom: Color32::from_rgb(4, 6, 18),
                accent: Color32::from_rgb(60, 120, 220),
                accent_soft: Color32::from_rgba_premultiplied(60, 120, 220, 62),
                panel: Color32::from_rgba_premultiplied(10, 14, 30, 230),
                panel_alt: Color32::from_rgba_premultiplied(16, 22, 48, 235),
                border: Color32::from_rgba_premultiplied(255, 255, 255, 30),
                text: Color32::from_rgb(230, 236, 250),
                text_muted: Color32::from_rgb(140, 155, 190),
            },
            "DREAMCAST" => ThemePalette {
                bg_top: Color32::from_rgb(44, 24, 12),
                bg_bottom: Color32::from_rgb(14, 14, 22),
                accent: Color32::from_rgb(240, 130, 40),
                accent_soft: Color32::from_rgba_premultiplied(240, 130, 40, 65),
                panel: Color32::from_rgba_premultiplied(20, 16, 24, 230),
                panel_alt: Color32::from_rgba_premultiplied(38, 26, 18, 235),
                border: Color32::from_rgba_premultiplied(255, 255, 255, 34),
                text: Color32::from_rgb(250, 244, 236),
                text_muted: Color32::from_rgb(194, 180, 166),
            },
            "DOS" => ThemePalette {
                bg_top: Color32::from_rgb(8, 22, 8),
                bg_bottom: Color32::from_rgb(4, 10, 6),
                accent: Color32::from_rgb(80, 220, 100),
                accent_soft: Color32::from_rgba_premultiplied(80, 220, 100, 58),
                panel: Color32::from_rgba_premultiplied(10, 20, 14, 230),
                panel_alt: Color32::from_rgba_premultiplied(16, 30, 18, 235),
                border: Color32::from_rgba_premultiplied(255, 255, 255, 30),
                text: Color32::from_rgb(220, 248, 224),
                text_muted: Color32::from_rgb(130, 180, 140),
            },
            _ => ThemePalette {
                bg_top: Color32::from_rgb(22, 27, 42),
                bg_bottom: Color32::from_rgb(10, 14, 22),
                accent: Color32::from_rgb(255, 200, 87),
                accent_soft: Color32::from_rgba_premultiplied(255, 200, 87, 62),
                panel: Color32::from_rgba_premultiplied(16, 20, 31, 230),
                panel_alt: Color32::from_rgba_premultiplied(24, 30, 46, 235),
                border: Color32::from_rgba_premultiplied(255, 255, 255, 34),
                text: Color32::from_rgb(243, 245, 248),
                text_muted: Color32::from_rgb(175, 184, 198),
            },
        }
    }

    pub(crate) fn palette(&self) -> ThemePalette {
        Self::palette_for_system(self.active_system())
    }

    pub(crate) fn panel_frame(&self) -> egui::Frame {
        let palette = self.palette();
        egui::Frame::new()
            .fill(palette.panel)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(egui::CornerRadius::same(14))
            .inner_margin(egui::Margin::same(12))
    }
}
