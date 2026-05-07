use eframe::egui;
use egui::Color32;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::info;

use crate::app::NativeArcadeUiApp;
use crate::render::fit_size_to_aspect;
use crate::state::PlayLaunchPhase;

static PLAY_DRAW_DEBUG_COUNTER: AtomicU64 = AtomicU64::new(0);

impl NativeArcadeUiApp {
    pub(crate) fn draw_play(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if self.state.play.pending_launch_ready_to_load() {
            self.continue_pending_launch(ctx);
        }
        if self.host.is_loaded() {
            self.apply_keyboard_and_gamepad_input(ctx);
            if self.host.is_loaded() {
                self.pump_play_runner(ctx);
            } else {
                // Controller-triggered exits can unload mid-frame without any new egui input event.
                ctx.request_repaint();
            }
        }
        #[cfg(target_os = "macos")]
        if let Some(error) = Self::take_macos_iosurface_renderer_error() {
            self.stop_play_session();
            self.state
                .play
                .set_status(format!("Play GPU bridge failed: {error}"));
            ctx.request_repaint();
            return;
        }
        if !self.host.is_loaded() && !self.state.play.launch_shell_active() {
            return;
        }
        if self.host.is_loaded() {
            self.refresh_play_bar_visibility(ctx);
        }
        self.draw_fullscreen_play(ctx, ui);
        if self.host.is_loaded() {
            self.draw_play_reset_bar(ctx);
        }
    }

    fn refresh_play_bar_visibility(&mut self, ctx: &egui::Context) {
        let revealed =
            ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_click());
        if revealed {
            self.show_play_bar();
        }
    }

    fn draw_fullscreen_play(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let palette = self.palette();
        let mut drew_frame = false;
        #[cfg(target_os = "macos")]
        if let Some(surface_frame) = self.assets.last_macos_iosurface_frame.clone() {
            let source_size = egui::vec2(surface_frame.width as f32, surface_frame.height as f32);
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
            if std::env::var_os("ARCADE_VULKAN_DEBUG").is_some() {
                let draw_index = PLAY_DRAW_DEBUG_COUNTER.fetch_add(1, Ordering::Relaxed);
                if draw_index < 32 {
                    info!(
                        target: "arcade_ui::video_debug",
                        "play draw frame={} macos_iosurface={:?} generation={} texture_size={:.0}x{:.0} panel_rect=({:.1},{:.1})-({:.1},{:.1}) image_rect=({:.1},{:.1})-({:.1},{:.1}) target_aspect={:.3} source_aspect={:.3}",
                        draw_index,
                        surface_frame.surface.as_ptr(),
                        surface_frame.generation,
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
            self.draw_macos_iosurface_frame(ui, image_rect, surface_frame);
            drew_frame = true;
        }
        if !drew_frame {
            if let Some(gl_frame) = self.assets.last_gl_texture_frame {
                let source_size = egui::vec2(gl_frame.width as f32, gl_frame.height as f32);
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
                if std::env::var_os("ARCADE_VULKAN_DEBUG").is_some() {
                    let draw_index = PLAY_DRAW_DEBUG_COUNTER.fetch_add(1, Ordering::Relaxed);
                    if draw_index < 32 {
                        info!(
                            target: "arcade_ui::video_debug",
                            "play draw frame={} gl_texture={:?} generation={} texture_size={:.0}x{:.0} panel_rect=({:.1},{:.1})-({:.1},{:.1}) image_rect=({:.1},{:.1})-({:.1},{:.1}) target_aspect={:.3} source_aspect={:.3}",
                            draw_index,
                            gl_frame.texture,
                            gl_frame.generation,
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
                self.draw_gl_texture_frame(ui, image_rect, gl_frame);
                drew_frame = true;
            } else if let Some(texture) = &self.assets.last_frame_texture {
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
                drew_frame = true;
            }
        }
        if !drew_frame {
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
            if self.state.play.launch_shell_active() {
                self.draw_launch_shell(ctx, ui, rect);
            } else {
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

    fn draw_launch_shell(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, rect: egui::Rect) {
        let title = self
            .state
            .play
            .launch_title
            .clone()
            .unwrap_or_else(|| String::from("Game"));
        let system = self.state.play.launch_system.clone().unwrap_or_default();
        let cover_path = self.state.play.launch_cover_path.clone();
        let poster_path = self.state.play.launch_preview_poster_path.clone();
        let phase = self.state.play.launch_phase;
        let message = self
            .state
            .play
            .launch_friendly_message
            .clone()
            .unwrap_or_else(|| launch_phase_message(phase).to_owned());
        let note = self.state.play.launch_note_message.clone();
        let detail = self.state.play.launch_detail_message.clone();

        ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
        if let Some(bg) = self.launch_system_background_texture(ctx, Some(&system)) {
            let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
            ui.painter()
                .image(bg.id(), rect, uv, Color32::from_white_alpha(72));
        }
        ui.painter()
            .rect_filled(rect, 0.0, Color32::from_rgba_premultiplied(0, 0, 0, 150));

        let max_width = (rect.width() * 0.82).clamp(360.0, 900.0);
        let content_height = (rect.height() * 0.52).clamp(280.0, 430.0);
        let content_rect =
            egui::Rect::from_center_size(rect.center(), egui::vec2(max_width, content_height));

        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content_rect), |ui| {
            let horizontal = ui.available_width() >= 620.0;
            if horizontal {
                ui.horizontal_centered(|ui| {
                    self.draw_launch_art(ctx, ui, cover_path.as_deref(), poster_path.as_deref());
                    ui.add_space(28.0);
                    self.draw_launch_text(
                        ui,
                        &title,
                        &system,
                        &message,
                        note.as_deref(),
                        detail.as_deref(),
                    );
                });
            } else {
                ui.vertical_centered(|ui| {
                    self.draw_launch_art(ctx, ui, cover_path.as_deref(), poster_path.as_deref());
                    ui.add_space(18.0);
                    self.draw_launch_text(
                        ui,
                        &title,
                        &system,
                        &message,
                        note.as_deref(),
                        detail.as_deref(),
                    );
                });
            }
        });

        if phase == PlayLaunchPhase::Queued {
            self.state.play.mark_launch_shell_painted();
            ctx.request_repaint();
        }
    }

    fn draw_launch_art(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        cover_path: Option<&str>,
        poster_path: Option<&str>,
    ) {
        let size = egui::vec2(190.0, 268.0);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let palette = self.palette();
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(8), palette.panel);
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(8),
            egui::Stroke::new(1.0, palette.border),
            egui::StrokeKind::Outside,
        );
        if let Some(texture) = self.launch_art_texture(ctx, cover_path, poster_path) {
            let source_size = texture.size_vec2();
            let source_aspect = if source_size.y > 0.0 {
                source_size.x / source_size.y
            } else {
                2.0 / 3.0
            };
            let fitted = fit_size_to_aspect(rect.size(), source_aspect);
            let image_rect = egui::Rect::from_center_size(rect.center(), fitted);
            let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
            ui.painter()
                .image(texture.id(), image_rect, uv, Color32::WHITE);
        }
    }

    fn draw_launch_text(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        system: &str,
        message: &str,
        note: Option<&str>,
        detail: Option<&str>,
    ) {
        let palette = self.palette();
        ui.vertical(|ui| {
            ui.set_max_width(520.0);
            ui.label(
                egui::RichText::new(system.to_ascii_uppercase())
                    .color(palette.accent)
                    .strong()
                    .size(13.0),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(title)
                    .color(palette.text)
                    .strong()
                    .size(34.0),
            );
            ui.add_space(14.0);
            ui.label(
                egui::RichText::new(message)
                    .color(palette.text_muted)
                    .size(18.0),
            );
            if let Some(note) = note.filter(|note| !note.trim().is_empty()) {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(note).color(palette.text_muted));
            }
            if let Some(detail) = detail.filter(|detail| !detail.trim().is_empty()) {
                ui.add_space(16.0);
                egui::CollapsingHeader::new("Details")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(detail)
                                .color(palette.text_muted)
                                .monospace(),
                        );
                    });
            }
            if self.state.play.launch_phase == PlayLaunchPhase::Failed {
                ui.add_space(18.0);
                if ui.button("Back").clicked() {
                    self.state.play.dismiss_launch_shell();
                }
            }
        });
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
            "NES" | "SNES" | "GENESIS" | "GAMECUBE" | "SATURN" | "PCECD" => Some(4.0 / 3.0),
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

fn launch_phase_message(phase: PlayLaunchPhase) -> &'static str {
    match phase {
        PlayLaunchPhase::Idle => "",
        PlayLaunchPhase::Queued | PlayLaunchPhase::LoadingCore => "Starting...",
        PlayLaunchPhase::WaitingForPresentation => "Waiting for video...",
        PlayLaunchPhase::Playing => "Playing",
        PlayLaunchPhase::Warning => "Still starting...",
        PlayLaunchPhase::Failed => "Couldn’t start this game.",
    }
}
