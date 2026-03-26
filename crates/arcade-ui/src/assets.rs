use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use arcade_domain::RomCard;
use eframe::egui;
use egui::{Color32, ColorImage, TextureHandle, Vec2};

use crate::app::NativeArcadeUiApp;
use crate::render::fit_size;

pub(crate) struct AssetCache {
    pub(crate) asset_roots: Vec<PathBuf>,
    pub(crate) cover_textures: HashMap<PathBuf, TextureHandle>,
    pub(crate) background_textures: HashMap<PathBuf, TextureHandle>,
    pub(crate) repeating_background_textures: HashMap<PathBuf, TextureHandle>,
    pub(crate) system_logo_textures: HashMap<PathBuf, TextureHandle>,
    pub(crate) system_controller_textures: HashMap<PathBuf, TextureHandle>,
    pub(crate) themed_art_textures: HashMap<PathBuf, TextureHandle>,
    pub(crate) image_load_failures: HashSet<PathBuf>,
    pub(crate) last_frame_texture: Option<TextureHandle>,
    pub(crate) play_frame_rgba: Vec<u8>,
    pub(crate) cover_load_budget: usize,
}

impl AssetCache {
    pub(crate) fn new(rom_root: &Path) -> Self {
        Self {
            asset_roots: NativeArcadeUiApp::discover_asset_roots(rom_root),
            cover_textures: HashMap::new(),
            background_textures: HashMap::new(),
            repeating_background_textures: HashMap::new(),
            system_logo_textures: HashMap::new(),
            system_controller_textures: HashMap::new(),
            themed_art_textures: HashMap::new(),
            image_load_failures: HashSet::new(),
            last_frame_texture: None,
            play_frame_rgba: Vec::new(),
            cover_load_budget: 0,
        }
    }
}

struct AllThemeAssets {
    devices: Vec<PathBuf>,
    controllers: Vec<PathBuf>,
}

impl NativeArcadeUiApp {
    pub(crate) fn begin_cover_load_frame(&mut self) {
        const COVER_LOADS_PER_FRAME: usize = 2;
        self.assets.cover_load_budget = COVER_LOADS_PER_FRAME;
    }

    pub(crate) fn draw_thematic_background(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.active_system() == "ALL" {
            self.draw_all_systems_background(ui, ctx);
        } else {
            self.draw_standard_system_background(ui, ctx);
        }
    }

    fn draw_standard_system_background(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let palette = self.palette();
        let rect = ui.max_rect();
        Self::paint_background_gradient(ui, rect, palette.bg_top, palette.bg_bottom);

        if let Some(texture) = self.system_background_texture(ctx) {
            let size = texture.size_vec2();
            if size.x > 0.0 && size.y > 0.0 {
                let width_scale = rect.width() / size.x;
                let height_scale = rect.height() / size.y;
                let scale = width_scale.max(height_scale);
                let draw_size = egui::vec2(size.x * scale, size.y * scale);
                let draw_rect = egui::Rect::from_center_size(rect.center(), draw_size);
                let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
                ui.painter().image(
                    texture.id(),
                    draw_rect,
                    uv,
                    Color32::from_rgba_premultiplied(255, 255, 255, 76),
                );
            }
        }

        if self.active_system() == "ARCADE" {
            self.draw_arcade_side_art(ui, ctx, rect);
        }
    }

    fn draw_all_systems_background(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let palette = self.palette();
        let rect = ui.max_rect();
        let assets = self.resolve_all_theme_assets();
        let lane_width = Self::content_band_width_for(rect.width()).min(rect.width() * 0.94);
        let lane_rect = egui::Rect::from_center_size(
            rect.center(),
            egui::vec2((lane_width + 40.0).min(rect.width()), rect.height() * 0.98),
        );
        let gutter_gap = 20.0;
        let left_gutter =
            (lane_rect.left() - rect.left() > 72.0).then_some(egui::Rect::from_min_max(
                rect.left_top(),
                egui::pos2(lane_rect.left() - gutter_gap, rect.bottom()),
            ));
        let right_gutter =
            (rect.right() - lane_rect.right() > 72.0).then_some(egui::Rect::from_min_max(
                egui::pos2(lane_rect.right() + gutter_gap, rect.top()),
                rect.right_bottom(),
            ));

        Self::paint_background_gradient(ui, rect, palette.bg_top, palette.bg_bottom);
        if let Some(texture) = self.all_systems_background_texture(ctx) {
            Self::paint_tiled_background_texture(
                ui,
                rect,
                &texture,
                0.55,
                Color32::from_rgba_premultiplied(214, 228, 235, 44),
            );
        }
        let mut side_art: Vec<&PathBuf> =
            Vec::with_capacity(assets.devices.len() + assets.controllers.len());
        side_art.extend(assets.devices.iter());
        side_art.extend(assets.controllers.iter());

        let gutter_top = rect.top() + 10.0;
        let left_column = left_gutter.map(|left_rect| {
            egui::Rect::from_min_max(egui::pos2(left_rect.min.x, gutter_top), left_rect.max)
                .shrink2(egui::vec2(8.0, 10.0))
        });
        let right_column = right_gutter.map(|right_rect| {
            egui::Rect::from_min_max(egui::pos2(right_rect.min.x, gutter_top), right_rect.max)
                .shrink2(egui::vec2(8.0, 10.0))
        });

        match (left_column, right_column) {
            (Some(left_rect), Some(right_rect)) => {
                let split_at = side_art.len().div_ceil(2);
                self.paint_vertical_art_stack(ui, ctx, left_rect, &side_art[..split_at], 92, 14);
                self.paint_vertical_art_stack(ui, ctx, right_rect, &side_art[split_at..], 84, 12);
            }
            (Some(left_rect), None) => {
                self.paint_vertical_art_stack(ui, ctx, left_rect, &side_art, 90, 10);
            }
            (None, Some(right_rect)) => {
                self.paint_vertical_art_stack(ui, ctx, right_rect, &side_art, 90, 10);
            }
            (None, None) => {}
        }
    }

    fn paint_background_gradient(ui: &egui::Ui, rect: egui::Rect, top: Color32, bottom: Color32) {
        let mut mesh = egui::epaint::Mesh::default();
        mesh.colored_vertex(rect.left_top(), top);
        mesh.colored_vertex(rect.right_top(), top);
        mesh.colored_vertex(rect.right_bottom(), bottom);
        mesh.colored_vertex(rect.left_bottom(), bottom);
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        ui.painter().add(egui::Shape::mesh(mesh));
    }

    fn paint_tiled_background_texture(
        ui: &egui::Ui,
        rect: egui::Rect,
        texture: &TextureHandle,
        tile_scale: f32,
        tint: Color32,
    ) {
        let size = texture.size_vec2();
        if rect.width() <= 0.0 || rect.height() <= 0.0 || size.x <= 0.0 || size.y <= 0.0 {
            return;
        }

        let tile_scale = tile_scale.max(0.1);
        let uv = egui::Rect::from_min_max(
            egui::Pos2::ZERO,
            egui::pos2(
                rect.width() / (size.x * tile_scale),
                rect.height() / (size.y * tile_scale),
            ),
        );
        ui.painter().image(texture.id(), rect, uv, tint);
    }

    fn paint_vertical_art_stack(
        &mut self,
        ui: &egui::Ui,
        ctx: &egui::Context,
        column_rect: egui::Rect,
        paths: &[&PathBuf],
        start_alpha: u8,
        alpha_step: u8,
    ) -> bool {
        if paths.is_empty() || column_rect.width() <= 8.0 || column_rect.height() <= 8.0 {
            return false;
        }

        let slot_height = column_rect.height() / paths.len() as f32;
        let mut drew_any = false;
        for (index, path) in paths.iter().enumerate() {
            let slot = egui::Rect::from_min_max(
                egui::pos2(
                    column_rect.min.x,
                    column_rect.min.y + slot_height * index as f32,
                ),
                egui::pos2(
                    column_rect.max.x,
                    column_rect.min.y + slot_height * (index + 1) as f32,
                ),
            )
            .shrink2(egui::vec2(2.0, 6.0));
            let alpha = start_alpha.saturating_sub(alpha_step.saturating_mul(index as u8));
            self.paint_fitted_accent_texture(ui, ctx, slot, path, alpha);
            drew_any = true;
        }

        drew_any
    }

    fn draw_arcade_side_art(&mut self, ui: &egui::Ui, ctx: &egui::Context, rect: egui::Rect) {
        let lane_width = Self::content_band_width_for(rect.width()).min(rect.width() * 0.94);
        let lane_rect = egui::Rect::from_center_size(
            rect.center(),
            egui::vec2((lane_width + 40.0).min(rect.width()), rect.height() * 0.98),
        );
        let gutter_gap = 20.0;
        let left_width = (lane_rect.left() - rect.left() - gutter_gap).max(0.0);
        let right_width = (rect.right() - lane_rect.right() - gutter_gap).max(0.0);

        let left_gutter = (left_width >= 48.0).then_some(egui::Rect::from_min_max(
            rect.left_top(),
            egui::pos2(lane_rect.left() - gutter_gap, rect.bottom()),
        ));
        let right_gutter = (right_width >= 48.0).then_some(egui::Rect::from_min_max(
            egui::pos2(lane_rect.right() + gutter_gap, rect.top()),
            rect.right_bottom(),
        ));

        let cabinet = self.resolve_db_asset_path("/system-logos/arcade-neogeo-snk.png");
        let handheld = self.resolve_db_asset_path("/system-logos/Neo-Geo_Pocket_Color.JPG");
        let console = self.resolve_db_asset_path("/system-logos/Neo-Geo-AES-Console-Set.jpg");

        if let Some(left_rect) = left_gutter {
            let top_slot = egui::Rect::from_min_max(
                left_rect.min,
                egui::pos2(left_rect.max.x, left_rect.min.y + left_rect.height() * 0.54),
            )
            .shrink2(egui::vec2(8.0, 10.0));
            let bottom_slot = egui::Rect::from_min_max(
                egui::pos2(left_rect.min.x, left_rect.min.y + left_rect.height() * 0.56),
                left_rect.max,
            )
            .shrink2(egui::vec2(8.0, 10.0));

            if let Some(path) = cabinet.as_ref() {
                self.paint_fitted_accent_texture(ui, ctx, top_slot, path, 104);
            }
            if let Some(path) = handheld.as_ref() {
                self.paint_fitted_accent_texture(ui, ctx, bottom_slot, path, 84);
            }
        }

        if let Some(right_rect) = right_gutter {
            let slot = right_rect.shrink2(egui::vec2(8.0, 14.0));
            if let Some(path) = console.as_ref() {
                self.paint_fitted_accent_texture(ui, ctx, slot, path, 88);
            }
        }
    }

    fn paint_fitted_accent_texture(
        &mut self,
        ui: &egui::Ui,
        ctx: &egui::Context,
        rect: egui::Rect,
        path: &Path,
        alpha: u8,
    ) {
        if rect.width() <= 8.0 || rect.height() <= 8.0 {
            return;
        }
        let Some(texture) = self.themed_art_texture(ctx, path.to_path_buf()) else {
            return;
        };
        let max_size = egui::vec2(rect.width(), rect.height());
        let draw_size = fit_size(texture.size_vec2(), max_size);
        let draw_rect = egui::Rect::from_center_size(rect.center(), draw_size);
        let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
        ui.painter().image(
            texture.id(),
            draw_rect,
            uv,
            Color32::from_rgba_premultiplied(255, 255, 255, alpha),
        );
    }

    pub(crate) fn discover_asset_roots(rom_root: &Path) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        if let Ok(cwd) = std::env::current_dir() {
            roots.push(cwd.join("public"));
            if let Some(parent) = cwd.parent() {
                roots.push(parent.join("public"));
            }
        }
        if let Some(parent) = rom_root.parent() {
            roots.push(parent.join("public"));
        }
        roots.retain(|path| path.exists());
        roots.sort();
        roots.dedup();
        roots
    }

    pub(crate) fn resolve_db_asset_path(&self, raw_path: &str) -> Option<PathBuf> {
        let trimmed = raw_path.trim();
        if trimmed.is_empty() {
            return None;
        }

        let direct = PathBuf::from(trimmed);
        if direct.is_absolute() && direct.exists() {
            return Some(direct);
        }

        let without_lead = trimmed.trim_start_matches('/');
        for root in &self.assets.asset_roots {
            let candidate = root.join(without_lead);
            if candidate.exists() {
                return Some(candidate);
            }
        }

        None
    }

    pub(crate) fn resolve_cover_path(&self, rom: &RomCard) -> Option<PathBuf> {
        if let Some(cover_path) = rom.rom.cover_path.as_deref() {
            if let Some(optimized_path) = to_optimized_cover_path(cover_path)
                .as_deref()
                .and_then(|path| self.resolve_db_asset_path(path))
            {
                return Some(optimized_path);
            }
            if let Some(primary) = self.resolve_db_asset_path(cover_path) {
                return Some(primary);
            }
        }

        rom.rom
            .preview_poster_path
            .as_deref()
            .and_then(|path| self.resolve_db_asset_path(path))
    }

    fn resolve_existing_assets(&self, candidates: &[&str]) -> Vec<PathBuf> {
        candidates
            .iter()
            .filter_map(|path| self.resolve_db_asset_path(path))
            .collect()
    }

    fn resolve_all_theme_assets(&self) -> AllThemeAssets {
        AllThemeAssets {
            devices: self.resolve_existing_assets(&[
                "/system-logos/Game-Boy-system.png",
                "/system-logos/Game-Boy-Advance-SP-Mk1-Blue.png",
                "/system-logos/N64-Console-Set.png",
                "/system-logos/arcadesystem.png",
                "/system-logos/221-2216289_image-gba-sp-and-light-blue-gameboy-sp.png",
            ]),
            controllers: self.resolve_existing_assets(&[
                "/system-logos/nescontroller.png",
                "/system-logos/snescontroller.png",
                "/system-logos/genesiscontroller.png",
            ]),
        }
    }

    fn resolve_all_background_path(&self) -> Option<PathBuf> {
        self.resolve_db_asset_path("/system-logos/All-background.png")
    }

    fn resolve_header_background_path(&self) -> Option<PathBuf> {
        self.resolve_db_asset_path("/system-logos/headerbackground.png")
    }

    fn resolve_header_title_path(&self) -> Option<PathBuf> {
        self.resolve_db_asset_path("headerTitle.png")
    }

    pub(crate) fn header_title_texture(
        &mut self,
        ctx: &egui::Context,
    ) -> Option<TextureHandle> {
        let path = self.resolve_header_title_path()?;
        Self::load_texture_from_path(
            &mut self.assets.background_textures,
            &mut self.assets.image_load_failures,
            ctx,
            path,
            "header-title",
            egui::TextureOptions::LINEAR,
        )
    }

    pub(crate) fn resolve_system_background_path(&self) -> Option<PathBuf> {
        let candidate = match self.active_system() {
            "NES" => "system-logos-web/nesbackground.w960.webp",
            "SNES" => "system-logos-web/snesbackground.w960.webp",
            "GENESIS" => "system-logos-web/genesisbackground.w960.webp",
            "GB" => "system-logos-web/gameboybackground.w960.webp",
            "GBA" => "system-logos-web/GBAsectionbackground.w960.webp",
            "N64" => "system-logos-web/n64sectionbackground.w960.webp",
            "ARCADE" => "system-logos-web/arcadebackground.w1200.webp",
            _ => "system-logos-web/arcadebackground.w1200.webp",
        };
        self.resolve_db_asset_path(candidate)
    }

    pub(crate) fn load_texture_from_path(
        cache: &mut HashMap<PathBuf, TextureHandle>,
        failures: &mut HashSet<PathBuf>,
        ctx: &egui::Context,
        path: PathBuf,
        key_prefix: &str,
        options: egui::TextureOptions,
    ) -> Option<TextureHandle> {
        if failures.contains(&path) {
            return None;
        }
        if let Some(texture) = cache.get(&path) {
            return Some(texture.clone());
        }

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase());

        let (size, pixels): ([usize; 2], Vec<u8>) = if extension.as_deref() == Some("svg") {
            let data = match fs::read(&path) {
                Ok(data) => data,
                Err(_) => {
                    failures.insert(path);
                    return None;
                }
            };
            if looks_like_svg_markup(&data) {
                match rasterize_svg_data(&data) {
                    Some((size, pixels)) => (size, pixels),
                    None => {
                        failures.insert(path);
                        return None;
                    }
                }
            } else {
                match image::load_from_memory(&data) {
                    Ok(image) => {
                        let rgba = image.to_rgba8();
                        (
                            [rgba.width() as usize, rgba.height() as usize],
                            rgba.into_raw(),
                        )
                    }
                    Err(_) => {
                        failures.insert(path);
                        return None;
                    }
                }
            }
        } else {
            let image = match image::open(&path) {
                Ok(image) => image,
                Err(_) => {
                    failures.insert(path);
                    return None;
                }
            };
            let rgba = image.to_rgba8();
            (
                [rgba.width() as usize, rgba.height() as usize],
                rgba.into_raw(),
            )
        };

        if size[0] == 0 || size[1] == 0 {
            failures.insert(path);
            return None;
        }

        let texture = ctx.load_texture(
            format!("{key_prefix}:{}", path.display()),
            ColorImage::from_rgba_unmultiplied(size, &pixels),
            options,
        );
        cache.insert(path.clone(), texture.clone());
        Some(texture)
    }

    pub(crate) fn rom_cover_texture(
        &mut self,
        ctx: &egui::Context,
        rom: &RomCard,
    ) -> Option<TextureHandle> {
        let path = self.resolve_cover_path(rom)?;
        if self.assets.image_load_failures.contains(&path) {
            return None;
        }
        if !self.assets.cover_textures.contains_key(&path) {
            if self.assets.cover_load_budget == 0 {
                ctx.request_repaint();
                return None;
            }
            self.assets.cover_load_budget = self.assets.cover_load_budget.saturating_sub(1);
        }
        Self::load_texture_from_path(
            &mut self.assets.cover_textures,
            &mut self.assets.image_load_failures,
            ctx,
            path,
            "cover",
            egui::TextureOptions::LINEAR,
        )
    }

    pub(crate) fn system_background_texture(
        &mut self,
        ctx: &egui::Context,
    ) -> Option<TextureHandle> {
        let path = self.resolve_system_background_path()?;
        Self::load_texture_from_path(
            &mut self.assets.background_textures,
            &mut self.assets.image_load_failures,
            ctx,
            path,
            "system-bg",
            egui::TextureOptions::LINEAR,
        )
    }

    pub(crate) fn all_systems_background_texture(
        &mut self,
        ctx: &egui::Context,
    ) -> Option<TextureHandle> {
        let path = self.resolve_all_background_path()?;
        Self::load_texture_from_path(
            &mut self.assets.repeating_background_textures,
            &mut self.assets.image_load_failures,
            ctx,
            path,
            "all-system-bg",
            egui::TextureOptions::LINEAR_REPEAT,
        )
    }

    pub(crate) fn header_background_texture(
        &mut self,
        ctx: &egui::Context,
    ) -> Option<TextureHandle> {
        let path = self.resolve_header_background_path()?;
        Self::load_texture_from_path(
            &mut self.assets.background_textures,
            &mut self.assets.image_load_failures,
            ctx,
            path,
            "header-bg",
            egui::TextureOptions::LINEAR,
        )
    }

    pub(crate) fn themed_art_texture(
        &mut self,
        ctx: &egui::Context,
        path: PathBuf,
    ) -> Option<TextureHandle> {
        Self::load_texture_from_path(
            &mut self.assets.themed_art_textures,
            &mut self.assets.image_load_failures,
            ctx,
            path,
            "theme-art",
            egui::TextureOptions::LINEAR,
        )
    }

    pub(crate) fn resolve_system_logo_path(&self, system: &str) -> Option<PathBuf> {
        if system == "ALL" {
            return None;
        }

        let candidate = match system {
            "NES" => "/system-logos/nintendo.svg",
            "SNES" => "/system-logos/snes.svg",
            "GENESIS" => "/system-logos/genesis.svg",
            "GB" => "/system-logos/Game_Boy_logo.png",
            "GBA" => "/system-logos/Game_Boy_Advance_logo.png",
            "N64" => "/system-logos/n64logo.png",
            "ARCADE" => "/system-logos-web/SNK_logo.svg",
            _ => "/system-logos/arcadesystem.png",
        };
        self.resolve_db_asset_path(candidate)
    }

    pub(crate) fn system_logo_texture(
        &mut self,
        ctx: &egui::Context,
        system: &str,
    ) -> Option<TextureHandle> {
        let path = self.resolve_system_logo_path(system)?;
        Self::load_texture_from_path(
            &mut self.assets.system_logo_textures,
            &mut self.assets.image_load_failures,
            ctx,
            path,
            "system-logo",
            egui::TextureOptions::LINEAR,
        )
    }

    pub(crate) fn system_logo_size(system: &str) -> Vec2 {
        match system {
            "NES" => egui::vec2(102.0, 32.0),
            "SNES" => egui::vec2(112.0, 32.0),
            "GENESIS" => egui::vec2(112.0, 32.0),
            "GB" => egui::vec2(96.0, 32.0),
            "GBA" => egui::vec2(132.0, 32.0),
            "N64" => egui::vec2(88.0, 32.0),
            "ARCADE" => egui::vec2(104.0, 28.0),
            _ => egui::vec2(76.0, 28.0),
        }
    }

    pub(crate) fn resolve_system_controller_path(&self, system: &str) -> Option<PathBuf> {
        let candidate = match system {
            "NES" => "/system-logos/nescontroller.png",
            "SNES" => "/system-logos/snescontroller.png",
            "GENESIS" => "/system-logos/genesiscontroller.png",
            "GB" => "/system-logos/Game-Boy-system.png",
            "GBA" => "/system-logos/Game-Boy-Advance-SP-Mk1-Blue.png",
            "N64" => "/system-logos/N64-Console-Set.png",
            "ARCADE" => "/system-logos/arcadesystem.png",
            _ => "/system-logos/arcadesystem.png",
        };
        self.resolve_db_asset_path(candidate)
    }

    pub(crate) fn system_controller_texture(
        &mut self,
        ctx: &egui::Context,
        system: &str,
    ) -> Option<TextureHandle> {
        let path = self.resolve_system_controller_path(system)?;
        Self::load_texture_from_path(
            &mut self.assets.system_controller_textures,
            &mut self.assets.image_load_failures,
            ctx,
            path,
            "system-controller",
            egui::TextureOptions::LINEAR,
        )
    }
}

fn rasterize_svg_data(data: &[u8]) -> Option<([usize; 2], Vec<u8>)> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(data, &options).ok()?;
    let svg_size = tree.size().to_int_size();
    let width = svg_size.width();
    let height = svg_size.height();
    if width == 0 || height == 0 {
        return None;
    }

    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    let mut pixmap_mut = pixmap.as_mut();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap_mut,
    );

    Some(([width as usize, height as usize], pixmap.data().to_vec()))
}

fn looks_like_svg_markup(data: &[u8]) -> bool {
    let data = data.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(data);
    let mut trimmed = data.iter().skip_while(|byte| byte.is_ascii_whitespace());
    matches!(trimmed.next(), Some(b'<'))
}

fn to_optimized_cover_path(raw_path: &str) -> Option<String> {
    let normalized = raw_path.split('?').next()?.split('#').next()?.trim();
    let relative = normalized.strip_prefix("/covers/")?;
    let (base, _) = relative.rsplit_once('.')?;
    Some(format!("/covers-web/{base}.w320.webp"))
}
