use eframe::egui;
use egui::Color32;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::info;

use crate::app::NativeArcadeUiApp;
use crate::render::fit_size_to_aspect;

static PLAY_DRAW_DEBUG_COUNTER: AtomicU64 = AtomicU64::new(0);

impl NativeArcadeUiApp {
    pub(crate) fn draw_play(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.tick_play_session(ctx);
        if !self.host.is_loaded() {
            return;
        }
        self.refresh_play_bar_visibility(ctx);
        self.draw_fullscreen_play(ui);
        self.draw_play_reset_bar(ctx);
    }

    fn refresh_play_bar_visibility(&mut self, ctx: &egui::Context) {
        let revealed =
            ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_click());
        if revealed {
            self.show_play_bar();
        }
    }

    fn draw_fullscreen_play(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let palette = self.palette();
        if let Some(texture) = &self.assets.last_frame_texture {
            let source_size = texture.size_vec2();
            let source_aspect = if source_size.y > 0.0 {
                source_size.x / source_size.y
            } else {
                4.0 / 3.0
            };
            let target_aspect = self
                .default_system_play_aspect()
                .or_else(|| self.host.video_aspect_ratio())
                .unwrap_or(source_aspect);
            let fitted = fit_size_to_aspect(rect.size(), target_aspect);
            let min = egui::pos2(
                (rect.center().x - fitted.x * 0.5).round(),
                (rect.center().y - fitted.y * 0.5).round(),
            );
            let image_rect = egui::Rect::from_min_size(min, fitted);
            let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
            if std::env::var_os("ARCADE_VULKAN_DEBUG").is_some() {
                let draw_index = PLAY_DRAW_DEBUG_COUNTER.fetch_add(1, Ordering::Relaxed);
                if draw_index < 32 {
                    info!(
                        target: "arcade_ui::video_debug",
                        "play draw frame={} texture_id={:?} texture_size={:.0}x{:.0} panel_rect=({:.1},{:.1})-({:.1},{:.1}) image_rect=({:.1},{:.1})-({:.1},{:.1}) target_aspect={:.3} source_aspect={:.3}",
                        draw_index,
                        texture.id(),
                        source_size.x,
                        source_size.y,
                        rect.min.x,
                        rect.min.y,
                        rect.max.x,
                        rect.max.y,
                        image_rect.min.x,
                        image_rect.min.y,
                        image_rect.max.x,
                        image_rect.max.y,
                        target_aspect,
                        source_aspect,
                    );
                }
            }
            ui.painter()
                .image(texture.id(), image_rect, uv, Color32::WHITE);
        } else {
            if std::env::var_os("ARCADE_VULKAN_DEBUG").is_some() {
                let draw_index = PLAY_DRAW_DEBUG_COUNTER.fetch_add(1, Ordering::Relaxed);
                if draw_index < 32 {
                    info!(
                        target: "arcade_ui::video_debug",
                        "play draw frame={} missing_texture panel_rect=({:.1},{:.1})-({:.1},{:.1}) external_vulkan={}",
                        draw_index,
                        rect.min.x,
                        rect.min.y,
                        rect.max.x,
                        rect.max.y,
                        self.host.using_external_vulkan_present_window(),
                    );
                }
            }
            ui.allocate_ui_with_layout(
                rect.size(),
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    if self.host.using_external_vulkan_present_window() {
                        ui.label("Rendering in external game window...");
                    } else {
                        ui.label("Loading game frame...");
                    }
                },
            );
        }

        let overlay_message = self.compose_play_overlay_message();
        self.host
            .set_external_overlay_message(overlay_message.as_deref());

        if let Some(message) = overlay_message {
            egui::Area::new(egui::Id::new("immersive-play-feedback"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -28.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_rgba_premultiplied(8, 12, 20, 228))
                        .stroke(egui::Stroke::new(1.0, palette.accent))
                        .corner_radius(egui::CornerRadius::same(14))
                        .inner_margin(egui::Margin::symmetric(14, 9))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(message).color(palette.text).strong());
                        });
                });
        }
    }

    fn draw_play_reset_bar(&mut self, ctx: &egui::Context) {
        if !self.play_bar_visible() {
            return;
        }

        let palette = self.palette();
        let response = egui::Area::new(egui::Id::new("play-reset-bar"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 18.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_premultiplied(8, 12, 20, 220))
                    .stroke(egui::Stroke::new(1.0, palette.border))
                    .corner_radius(egui::CornerRadius::same(255))
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        let button = egui::Button::new(
                            egui::RichText::new("Reset").color(palette.text).strong(),
                        )
                        .fill(palette.accent_soft)
                        .stroke(egui::Stroke::new(1.0, palette.accent))
                        .corner_radius(egui::CornerRadius::same(255));
                        if ui.add(button).clicked() {
                            self.reset_play_session();
                            self.keep_play_bar_visible();
                        }
                    });
            })
            .response;

        if response.hovered() {
            self.keep_play_bar_visible();
        }
    }

    fn default_system_play_aspect(&self) -> Option<f32> {
        let system = self.state.play.active_system.as_deref().or_else(|| {
            self.current_selected_rom()
                .map(|rom| rom.rom.system.as_str())
        })?;
        match system {
            "NES" | "SNES" | "GENESIS" | "SATURN" | "PCECD" => Some(4.0 / 3.0),
            "N64" => {
                let config = self.services.config();
                match config.emulation.n64.aspect_ratio {
                    arcade_domain::N64AspectRatio::Ratio169
                    | arcade_domain::N64AspectRatio::Ratio169Adjusted => Some(16.0 / 9.0),
                    _ => Some(4.0 / 3.0),
                }
            }
            "GB" => Some(10.0 / 9.0),
            "GBA" => Some(3.0 / 2.0),
            _ => None,
        }
    }

    fn compose_play_overlay_message(&mut self) -> Option<String> {
        let feedback = self.active_play_overlay();
        let input_debug_enabled = std::env::var_os("ARCADE_INPUT_DEBUG").is_some();
        let input_debug = if input_debug_enabled && !self.state.input_debug.is_empty() {
            Some(self.state.input_debug.clone())
        } else {
            None
        };

        match (feedback, input_debug) {
            (Some(feedback), Some(input_debug)) => Some(format!("{feedback}\n{input_debug}")),
            (Some(feedback), None) => Some(feedback),
            (None, Some(input_debug)) => Some(input_debug),
            (None, None) => None,
        }
    }
}
