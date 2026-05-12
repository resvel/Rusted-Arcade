mod status_bar;
mod top_nav;
mod viewport;

use std::collections::HashSet;
use std::fs;
use std::sync::mpsc::Receiver;

#[cfg(feature = "gamepad")]
use arcade_domain::DetectedPadIdentity;
use arcade_domain::{ManageOperationSummary, ManageProgressEvent};
use arcade_libretro::{FrontendCapabilities, LibretroHost};
use arcade_services::NativeServices;
use eframe::egui;
#[cfg(feature = "gamepad")]
use gilrs::Gilrs;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
#[cfg(feature = "gamepad")]
use std::collections::HashMap;

use crate::actions::INITIAL_LIBRARY_PRELOAD_SIZE;
use crate::assets::AssetCache;
use crate::play_runner::PlayRunner;
use crate::state::ArcadeUiState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppView {
    Home,
    Library,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GridSource {
    Favorites,
    Library,
}

pub struct NativeArcadeUiApp {
    pub(crate) services: NativeServices,
    pub(crate) host: LibretroHost,
    pub(crate) state: ArcadeUiState,
    pub(crate) assets: AssetCache,
    #[cfg(target_os = "macos")]
    pub(crate) wgpu_target_format: Option<eframe::wgpu::TextureFormat>,
    pub(crate) manage_job_rx: Option<Receiver<ManageUiMessage>>,
    #[cfg(feature = "gamepad")]
    pub(crate) gilrs: Option<Gilrs>,
    #[cfg(feature = "gamepad")]
    pub(crate) gamepad_identity_cache: HashMap<usize, DetectedPadIdentity>,
    #[cfg(feature = "gamepad")]
    pub(crate) raw_dpad_state_cache: HashMap<usize, crate::input::RawDpadState>,
    #[cfg(feature = "gamepad")]
    pub(crate) gamepad_connect_order: HashMap<usize, u64>,
    #[cfg(feature = "gamepad")]
    pub(crate) gamepad_slot_assignments: HashMap<usize, u8>,
    #[cfg(feature = "gamepad")]
    pub(crate) next_gamepad_connect_seq: u64,
    /// RETROK keycodes that were down last frame (for keyboard callback event generation).
    pub(crate) prev_keyboard_keys_down: HashSet<u32>,
    /// DOS passthrough: keys pressed since last emulated frame boundary.
    pub(crate) retro_keys_pressed_since_frame: HashSet<u32>,
    /// Deferred key releases for DOS keyboard passthrough; flushed after at least one core frame.
    pub(crate) pending_retro_key_releases: Vec<(u32, u16)>,
    pub(crate) play_runner: Option<PlayRunner>,
    close_after_play_session_stop: bool,
}

pub(crate) enum ManageUiMessage {
    Progress(ManageProgressEvent),
    Finished(anyhow::Result<ManageOperationSummary>),
}

impl NativeArcadeUiApp {
    const VIEWPORT_RESTORE_APPLY: u8 = 1;
    const VIEWPORT_RESTORE_VERIFY: u8 = 2;
    const VIEWPORT_RESTORE_RETRY_FRAMES: u8 = 120;

    pub fn configure_egui(ctx: &egui::Context) {
        const BODY_FONT_CANDIDATES: &[&str] = &[
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "/System/Library/Fonts/Supplemental/Helvetica.ttf",
        ];
        const DISPLAY_FONT_CANDIDATES: &[&str] = &[
            "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
            "/System/Library/Fonts/Supplemental/Helvetica Bold.ttf",
        ];
        const MONO_FONT_CANDIDATES: &[&str] = &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/System/Library/Fonts/Supplemental/Courier New.ttf",
            "/System/Library/Fonts/Supplemental/Menlo.ttc",
        ];

        let mut fonts = egui::FontDefinitions::default();
        let mut body_loaded = false;
        let mut display_loaded = false;

        if let Some(font_data) = load_font_data(BODY_FONT_CANDIDATES) {
            fonts
                .font_data
                .insert("arcade-ui-body".to_string(), font_data.into());
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "arcade-ui-body".to_string());
            body_loaded = true;
        }

        if let Some(font_data) = load_font_data(DISPLAY_FONT_CANDIDATES) {
            fonts
                .font_data
                .insert("arcade-ui-display".to_string(), font_data.into());
            let mut display_family_fonts = vec!["arcade-ui-display".to_string()];
            if body_loaded {
                display_family_fonts.push("arcade-ui-body".to_string());
            }
            fonts.families.insert(
                egui::FontFamily::Name("arcade-ui-display".into()),
                display_family_fonts,
            );
            display_loaded = true;
        }

        if let Some(font_data) = load_font_data(MONO_FONT_CANDIDATES) {
            fonts
                .font_data
                .insert("arcade-ui-mono".to_string(), font_data.into());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "arcade-ui-mono".to_string());
        }

        ctx.set_fonts(fonts);

        let mut style = (*ctx.style()).clone();
        let display_family = if display_loaded {
            egui::FontFamily::Name("arcade-ui-display".into())
        } else {
            egui::FontFamily::Proportional
        };
        style.text_styles = [
            (
                egui::TextStyle::Heading,
                egui::FontId::new(28.0, display_family.clone()),
            ),
            (
                egui::TextStyle::Name("title".into()),
                egui::FontId::new(21.0, display_family.clone()),
            ),
            (
                egui::TextStyle::Body,
                egui::FontId::new(15.5, egui::FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Button,
                egui::FontId::new(14.0, display_family),
            ),
            (
                egui::TextStyle::Small,
                egui::FontId::new(11.5, egui::FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Monospace,
                egui::FontId::new(13.5, egui::FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
        style.spacing.menu_margin = egui::Margin::symmetric(10, 8);
        style.visuals.button_frame = true;
        ctx.set_style(style);
    }

    fn viewport_width(ctx: &egui::Context) -> f32 {
        ctx.input(|i| i.screen_rect().width())
    }

    fn central_margin_for_width(width: f32) -> i8 {
        if width >= 1900.0 {
            14
        } else if width >= 1500.0 {
            10
        } else if width >= 1200.0 {
            8
        } else {
            6
        }
    }

    fn chrome_margin_for_width(width: f32) -> i8 {
        if width >= 1500.0 {
            8
        } else {
            6
        }
    }

    pub(crate) fn content_band_width_for(width: f32) -> f32 {
        let max_width = if width >= 1800.0 {
            1500.0
        } else if width >= 1450.0 {
            1380.0
        } else {
            1240.0
        };
        width.min(max_width)
    }

    pub fn new(services: NativeServices, host: LibretroHost) -> Self {
        #[cfg(feature = "gamepad")]
        let (gilrs, gamepad_warning) = match Gilrs::new() {
            Ok(runtime) => (Some(runtime), None),
            Err(err) => (None, Some(format!("Controller runtime unavailable: {err}"))),
        };

        let assets = AssetCache::new(services.config().as_ref());

        let mut app = Self {
            services,
            host,
            state: ArcadeUiState::new(),
            assets,
            #[cfg(target_os = "macos")]
            wgpu_target_format: None,
            manage_job_rx: None,
            #[cfg(feature = "gamepad")]
            gilrs,
            #[cfg(feature = "gamepad")]
            gamepad_identity_cache: HashMap::new(),
            #[cfg(feature = "gamepad")]
            raw_dpad_state_cache: HashMap::new(),
            #[cfg(feature = "gamepad")]
            gamepad_connect_order: HashMap::new(),
            #[cfg(feature = "gamepad")]
            gamepad_slot_assignments: HashMap::new(),
            #[cfg(feature = "gamepad")]
            next_gamepad_connect_seq: 0,
            prev_keyboard_keys_down: HashSet::new(),
            retro_keys_pressed_since_frame: HashSet::new(),
            pending_retro_key_releases: Vec::new(),
            play_runner: None,
            close_after_play_session_stop: false,
        };

        #[cfg(feature = "gamepad")]
        if let Some(message) = gamepad_warning {
            app.state.status = message;
        }

        if let Err(err) = app.refresh_favorites() {
            app.state.status = format!("Startup warning: {err}");
        }
        app.state.library.page_size = INITIAL_LIBRARY_PRELOAD_SIZE;
        if let Err(err) = app.refresh_library() {
            app.state.status = format!("Startup warning: {err}");
        }
        app.sync_manage_settings_from_services();
        app.refresh_manage_rows();
        let welcome_needed = !app.services.runtime_setup_welcome_completed();
        let missing_required_dependencies = app
            .state
            .manage
            .dependency_report
            .as_ref()
            .is_some_and(|report| report.has_missing_required());
        if welcome_needed || missing_required_dependencies {
            app.navigate_to_view(AppView::Settings);
            app.state.settings_scroll_target =
                Some(crate::state::SettingsScrollTarget::Dependencies);
            app.state.menu_nav.settings_section_index =
                crate::state::SettingsScrollTarget::Dependencies.nav_index();
            app.state.menu_nav.focus_region = crate::state::MenuFocusRegion::SettingsSystemToolbar;
            app.state.manage.settings_selected_system = String::from("ALL");
            app.state.manage.settings_system_index = 0;
            app.state.manage.runtime_setup_focus_index = 0;
            app.state.manage.runtime_setup_advanced_open = false;
            app.state.status = if welcome_needed {
                String::from("Welcome to Rusted Arcade. Opened Runtime Setup.")
            } else {
                String::from("Runtime dependencies are missing. Opened Runtime Setup.")
            };
        }

        app
    }

    pub(crate) fn paint_selection_glow(
        ui: &egui::Ui,
        rect: egui::Rect,
        corner_radius: u8,
        color: egui::Color32,
        strength: f32,
    ) {
        let glow_t = strength.clamp(0.0, 1.0);
        if glow_t <= 0.0 {
            return;
        }

        let soft_glow = egui::Color32::from_rgba_premultiplied(
            color.r(),
            color.g(),
            color.b(),
            (12.0 + glow_t * 24.0).round() as u8,
        );
        let core_glow = egui::Color32::from_rgba_premultiplied(
            color.r(),
            color.g(),
            color.b(),
            (18.0 + glow_t * 40.0).round() as u8,
        );

        ui.painter().rect_stroke(
            rect.expand(4.0),
            egui::CornerRadius::same(corner_radius.saturating_add(4)),
            egui::Stroke::new(2.0 + glow_t * 1.4, soft_glow),
            egui::StrokeKind::Outside,
        );
        ui.painter().rect_stroke(
            rect.expand(1.5),
            egui::CornerRadius::same(corner_radius.saturating_add(2)),
            egui::Stroke::new(1.0 + glow_t * 0.9, core_glow),
            egui::StrokeKind::Outside,
        );
    }
}

fn load_font_data(candidates: &[&str]) -> Option<egui::FontData> {
    for path in candidates {
        if let Ok(bytes) = fs::read(path) {
            return Some(egui::FontData::from_owned(bytes));
        }
    }

    None
}

fn describe_window_handle_kind(handle: RawWindowHandle) -> &'static str {
    match handle {
        RawWindowHandle::AppKit(_) => "AppKit",
        RawWindowHandle::UiKit(_) => "UiKit",
        RawWindowHandle::AndroidNdk(_) => "AndroidNdk",
        RawWindowHandle::Web(_) => "Web",
        RawWindowHandle::Orbital(_) => "Orbital",
        RawWindowHandle::Drm(_) => "Drm",
        RawWindowHandle::Gbm(_) => "Gbm",
        _ => "Other",
    }
}

fn describe_display_handle_kind(handle: RawDisplayHandle) -> &'static str {
    match handle {
        RawDisplayHandle::AppKit(_) => "AppKit",
        RawDisplayHandle::UiKit(_) => "UiKit",
        RawDisplayHandle::Android(_) => "Android",
        RawDisplayHandle::Web(_) => "Web",
        RawDisplayHandle::Orbital(_) => "Orbital",
        RawDisplayHandle::Drm(_) => "Drm",
        RawDisplayHandle::Gbm(_) => "Gbm",
        _ => "Other",
    }
}

fn frontend_capabilities(frame: &eframe::Frame) -> FrontendCapabilities {
    let glow_context = frame.gl().cloned();
    let wgpu_render_state = frame.wgpu_render_state();
    let window_handle_kind = frame
        .window_handle()
        .ok()
        .map(|handle| describe_window_handle_kind(handle.as_raw()).to_owned());
    let display_handle_kind = frame
        .display_handle()
        .ok()
        .map(|handle| describe_display_handle_kind(handle.as_raw()).to_owned());

    FrontendCapabilities {
        renderer_name: Some(
            if glow_context.is_some() {
                "eframe_glow"
            } else if wgpu_render_state.is_some() {
                "eframe_wgpu"
            } else {
                "eframe_non_gl"
            }
            .to_owned(),
        ),
        gl_context: glow_context,
        window_handle_kind,
        display_handle_kind,
    }
}

impl eframe::App for NativeArcadeUiApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.host.is_loaded() || self.play_runner.is_some() {
            tracing::info!("app exiting with active play session; unloading core");
            self.stop_play_session();
        }
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if self.close_after_play_session_stop {
            self.close_after_play_session_stop = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let close_requested = ctx.input(|input| input.viewport().close_requested());
        if close_requested && (self.host.is_loaded() || self.play_runner.is_some()) {
            tracing::info!("close requested with active play session; unloading core before close");
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.stop_play_session();
            self.close_after_play_session_stop = true;
            ctx.request_repaint();
            return;
        }

        #[cfg(target_os = "macos")]
        {
            self.wgpu_target_format = frame.wgpu_render_state().map(|state| state.target_format);
        }
        self.host
            .set_frontend_capabilities(frontend_capabilities(frame));
        self.host
            .sync_external_vulkan_window_visibility_on_main_thread();
        let session_active = self.host.is_loaded();
        let play_shell_active = session_active || self.state.play.launch_shell_active();
        if !play_shell_active {
            self.poll_manage_jobs();
            self.tick_frontend_navigation(ctx);
        }
        let external_present_active =
            session_active && self.host.using_external_vulkan_present_window();
        let external_present_expected =
            session_active && self.host.expects_external_vulkan_present_window();
        let external_window_available =
            session_active && self.host.has_external_vulkan_present_window();
        let immersive_viewport_allowed = self.host.should_use_immersive_play_viewport();
        let viewport_width = NativeArcadeUiApp::viewport_width(ctx);
        let central_margin = NativeArcadeUiApp::central_margin_for_width(viewport_width);
        self.sync_session_viewport(
            ctx,
            session_active,
            external_present_active,
            external_present_expected,
            external_window_available,
            immersive_viewport_allowed,
        );

        if !play_shell_active {
            self.draw_top_nav(ctx);
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(if play_shell_active {
                egui::Margin::same(0)
            } else {
                egui::Margin::same(central_margin)
            }))
            .show(ctx, |ui| {
                if !play_shell_active {
                    self.draw_thematic_background(ui, ctx);
                } else {
                    ui.painter()
                        .rect_filled(ui.max_rect(), 0.0, egui::Color32::BLACK);
                }
                if play_shell_active {
                    self.draw_play(ctx, ui);
                } else {
                    match self.state.current_view {
                        AppView::Home => self.draw_home(ctx, ui),
                        AppView::Library => self.draw_library(ctx, ui),
                        AppView::Settings => self.draw_settings(ctx, ui),
                    }
                }
            });

        if !play_shell_active {
            self.draw_status_bar(ctx);
        }

        if session_active != self.host.is_loaded() {
            ctx.request_repaint();
        }
    }
}
