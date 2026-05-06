use eframe::egui;

use super::NativeArcadeUiApp;

impl NativeArcadeUiApp {
    pub(super) fn sync_session_viewport(
        &mut self,
        ctx: &egui::Context,
        session_active: bool,
        external_present_active: bool,
        _external_present_expected: bool,
        external_window_available: bool,
        immersive_viewport_allowed: bool,
    ) {
        let external_window_session =
            session_active && (external_present_active || external_window_available);
        let window_level = if external_window_session {
            egui::viewport::WindowLevel::AlwaysOnBottom
        } else {
            egui::viewport::WindowLevel::Normal
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(window_level));
        let viewport = ctx.input(|i| i.viewport().clone());
        self.sync_external_present_window_position(
            ctx,
            &viewport,
            session_active && external_present_active,
        );

        if !session_active
            && (self.state.play.viewport_immersive_applied
                || self.state.play.viewport_restore_stage != 0
                || viewport.fullscreen.unwrap_or(false)
                || !Self::viewport_chrome_restored(&viewport))
        {
            // macOS can occasionally keep a borderless style mask after fullscreen sessions.
            // Re-assert normal framed window state while no game session is active.
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            self.send_viewport_restore_cmds(ctx);
        }

        // If an external Vulkan window exists for this session, never push the main app
        // into immersive fullscreen; fullscreen can trap the app above the external window.
        let immersive_session =
            session_active && !external_window_session && immersive_viewport_allowed;

        if immersive_session {
            self.state.play.viewport_restore_stage = 0;
            self.state.play.viewport_restore_frames = 0;
            if self.state.play.viewport_enter_stage == 0 {
                self.state.play.restore_maximized = viewport.maximized.unwrap_or(false);
                self.state.play.viewport_enter_stage = 1;
            }

            if !viewport.fullscreen.unwrap_or(false) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                self.state.play.viewport_immersive_applied = false;
                return;
            }

            self.state.play.viewport_enter_stage = 0;
            self.state.play.viewport_immersive_applied = true;
            return;
        }

        if self.state.play.viewport_enter_stage != 0 {
            self.state.play.viewport_enter_stage = 0;
        }

        if external_window_session && viewport.fullscreen.unwrap_or(false) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            self.send_viewport_restore_cmds(ctx);
            self.state.play.viewport_immersive_applied = false;
            self.state.play.viewport_restore_stage = Self::VIEWPORT_RESTORE_APPLY;
            self.state.play.viewport_restore_frames = Self::VIEWPORT_RESTORE_RETRY_FRAMES;
            return;
        }

        if self.state.play.viewport_immersive_applied {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            self.send_viewport_restore_cmds(ctx);
            self.state.play.viewport_immersive_applied = false;
            self.state.play.viewport_restore_stage = Self::VIEWPORT_RESTORE_APPLY;
            self.state.play.viewport_restore_frames = Self::VIEWPORT_RESTORE_RETRY_FRAMES;
            return;
        }

        if self.state.play.viewport_restore_stage == 0 {
            // Safety net for platforms where fullscreen exit can race with decoration restore.
            // If we are back in a non-session state and chrome is still missing, keep restoring.
            if !session_active
                && !viewport.fullscreen.unwrap_or(false)
                && !Self::viewport_chrome_restored(&viewport)
            {
                self.send_viewport_restore_cmds(ctx);
            }
            return;
        }

        if viewport.fullscreen.unwrap_or(false) {
            return;
        }

        match self.state.play.viewport_restore_stage {
            Self::VIEWPORT_RESTORE_APPLY => {
                if self.state.play.restore_maximized && viewport.maximized.unwrap_or(false) {
                    return;
                }

                self.send_viewport_restore_cmds(ctx);
                self.state.play.viewport_restore_stage = Self::VIEWPORT_RESTORE_VERIFY;
            }
            Self::VIEWPORT_RESTORE_VERIFY => {
                self.send_viewport_restore_cmds(ctx);

                if (Self::viewport_chrome_restored(&viewport)
                    && self.viewport_maximize_restored(&viewport))
                    || self.state.play.viewport_restore_frames == 0
                {
                    self.finish_viewport_restore();
                } else {
                    self.state.play.viewport_restore_frames -= 1;
                }
            }
            _ => {
                self.finish_viewport_restore();
            }
        }
    }

    fn send_viewport_restore_cmds(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::EnableButtons {
            close: true,
            minimized: true,
            maximize: true,
        });
        if self.state.play.restore_maximized {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        }
    }

    fn viewport_maximize_restored(&self, viewport: &egui::ViewportInfo) -> bool {
        !self.state.play.restore_maximized || viewport.maximized.unwrap_or(false)
    }

    fn viewport_chrome_restored(viewport: &egui::ViewportInfo) -> bool {
        match (viewport.outer_rect, viewport.inner_rect) {
            (Some(outer), Some(inner)) => outer != inner,
            _ => false,
        }
    }

    fn finish_viewport_restore(&mut self) {
        self.state.play.restore_maximized = false;
        self.state.play.viewport_restore_stage = 0;
        self.state.play.viewport_restore_frames = 0;
    }

    fn sync_external_present_window_position(
        &mut self,
        ctx: &egui::Context,
        viewport: &egui::ViewportInfo,
        hide_for_external_present: bool,
    ) {
        let _ = (ctx, viewport, hide_for_external_present);
    }
}
