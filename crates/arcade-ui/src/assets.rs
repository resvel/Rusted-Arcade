use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arcade_domain::RomCard;
use arcade_libretro::GlTextureFrame;
#[cfg(target_os = "macos")]
use arcade_libretro::MacosIosurfaceFrame;
use eframe::egui;
use egui::{Color32, ColorImage, TextureHandle, Vec2};

use crate::app::NativeArcadeUiApp;
use crate::controller_mapper::{
    parse_system_hotspot_overlay, system_controller_hotspot_overlay_asset,
    system_controller_mapper_art_asset, SystemControllerLayout, SystemOverlayHotspot,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SizedTextureKey {
    pub(crate) path: PathBuf,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

pub(crate) struct AssetCache {
    pub(crate) asset_roots: Vec<PathBuf>,
    pub(crate) cover_textures: HashMap<PathBuf, TextureHandle>,
    pub(crate) background_textures: HashMap<PathBuf, TextureHandle>,
    pub(crate) repeating_background_textures: HashMap<PathBuf, TextureHandle>,
    pub(crate) system_logo_textures: HashMap<PathBuf, TextureHandle>,
    pub(crate) controller_mapper_textures: HashMap<SizedTextureKey, TextureHandle>,
    pub(crate) controller_mapper_hotspot_overlays: HashMap<PathBuf, Arc<Vec<SystemOverlayHotspot>>>,
    pub(crate) image_load_failures: HashSet<PathBuf>,
    pub(crate) controller_mapper_overlay_failures: HashSet<PathBuf>,
    pub(crate) last_frame_texture: Option<TextureHandle>,
    pub(crate) last_gl_texture_frame: Option<GlTextureFrame>,
    #[cfg(target_os = "macos")]
    pub(crate) last_macos_iosurface_frame: Option<MacosIosurfaceFrame>,
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
            controller_mapper_textures: HashMap::new(),
            controller_mapper_hotspot_overlays: HashMap::new(),
            image_load_failures: HashSet::new(),
            controller_mapper_overlay_failures: HashSet::new(),
            last_frame_texture: None,
            last_gl_texture_frame: None,
            #[cfg(target_os = "macos")]
            last_macos_iosurface_frame: None,
            play_frame_rgba: Vec::new(),
            cover_load_budget: 0,
        }
    }

    pub(crate) fn clear_for_cpu_frame(&mut self) {
        self.last_gl_texture_frame = None;
        #[cfg(target_os = "macos")]
        {
            self.last_macos_iosurface_frame = None;
        }
    }

    pub(crate) fn clear_for_gl_texture_frame(&mut self) {
        self.last_frame_texture = None;
        #[cfg(target_os = "macos")]
        {
            self.last_macos_iosurface_frame = None;
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn clear_for_macos_iosurface_frame(&mut self) {
        self.last_frame_texture = None;
        self.last_gl_texture_frame = None;
    }
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
    }

    fn draw_all_systems_background(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let palette = self.palette();
        let rect = ui.max_rect();
        Self::paint_background_gradient(ui, rect, palette.bg_top, palette.bg_bottom);
        if let Some(texture) = self.all_systems_background_texture(ctx) {
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
                    Color32::from_rgba_premultiplied(214, 228, 235, 64),
                );
            }
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

    fn resolve_all_background_path(&self) -> Option<PathBuf> {
        self.resolve_db_asset_path("/system-logos/All-background.jpg")
    }

    fn resolve_header_background_path(&self) -> Option<PathBuf> {
        let system = self.active_system().trim().to_ascii_lowercase();
        if !system.is_empty() {
            let candidates = [
                format!("/system-logos/{system}_header.png"),
                format!("/system-logos/{system}_header.webp"),
                format!("/system-logos/{system}_header.jpg"),
                format!("/system-logos/{system}_header.jpeg"),
            ];
            for candidate in candidates {
                if let Some(path) = self.resolve_db_asset_path(&candidate) {
                    return Some(path);
                }
            }
        }

        self.resolve_db_asset_path("/system-logos/all_header.jpg")
    }

    fn resolve_header_title_path(&self) -> Option<PathBuf> {
        self.resolve_db_asset_path("/system-logos/headerTitle.png")
    }

    pub(crate) fn header_title_texture(&mut self, ctx: &egui::Context) -> Option<TextureHandle> {
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
        let system = self.active_system().trim().to_ascii_lowercase();
        if !system.is_empty() {
            let custom_candidates = [
                format!("/system-logos/{system}_background.png"),
                format!("/system-logos/{system}_background.webp"),
                format!("/system-logos/{system}_background.jpg"),
                format!("/system-logos/{system}_background.jpeg"),
            ];
            for candidate in custom_candidates {
                if let Some(path) = self.resolve_db_asset_path(&candidate) {
                    return Some(path);
                }
            }
        }

        self.resolve_all_background_path()
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
            "ARCADE" => "/system-logos/SNK_logo.png",
            "PSX" => "/system-logos/Playstation_logo_colour.png",
            "PS2" => "/system-logos/PlayStation_2_logo.png",
            "DREAMCAST" => "/system-logos/Dreamcast_logo_Japan.png",
            "GAMECUBE" => "/system-logos/gamecube.svg",
            "SATURN" => "/system-logos/SegaSaturn_logo.png",
            "PCECD" => "/system-logos/pcecd_logo.png",
            "DOS" => "/system-logos/Msdos.png",
            _ => return None,
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
            "NES" => egui::vec2(56.0, 18.0),
            "SNES" => egui::vec2(62.0, 18.0),
            "GENESIS" => egui::vec2(62.0, 18.0),
            "GB" => egui::vec2(53.0, 18.0),
            "GBA" => egui::vec2(73.0, 18.0),
            "N64" => egui::vec2(48.0, 18.0),
            "ARCADE" => egui::vec2(57.0, 15.0),
            "PSX" => egui::vec2(62.0, 18.0),
            "PS2" => egui::vec2(56.0, 18.0),
            "DREAMCAST" => egui::vec2(64.0, 18.0),
            "GAMECUBE" => egui::vec2(72.0, 18.0),
            "SATURN" => egui::vec2(66.0, 18.0),
            "PCECD" => egui::vec2(60.0, 18.0),
            "DOS" => egui::vec2(42.0, 18.0),
            _ => egui::vec2(42.0, 15.0),
        }
    }

    fn resolve_system_mapper_diagram_path(
        &self,
        layout: SystemControllerLayout,
    ) -> Option<PathBuf> {
        self.resolve_db_asset_path(system_controller_mapper_art_asset(layout))
    }

    fn resolve_system_mapper_hotspot_path(
        &self,
        layout: SystemControllerLayout,
    ) -> Option<PathBuf> {
        self.resolve_db_asset_path(system_controller_hotspot_overlay_asset(layout))
    }

    pub(crate) fn system_mapper_art_available(&self, layout: SystemControllerLayout) -> bool {
        self.resolve_system_mapper_diagram_path(layout).is_some()
    }

    pub(crate) fn system_mapper_hotspots(
        &mut self,
        layout: SystemControllerLayout,
    ) -> Option<Arc<Vec<SystemOverlayHotspot>>> {
        let path = self.resolve_system_mapper_hotspot_path(layout)?;
        if self
            .assets
            .controller_mapper_overlay_failures
            .contains(&path)
        {
            return None;
        }
        if let Some(hotspots) = self.assets.controller_mapper_hotspot_overlays.get(&path) {
            return Some(hotspots.clone());
        }

        let svg_data = match fs::read_to_string(&path) {
            Ok(data) => data,
            Err(_) => {
                self.assets.controller_mapper_overlay_failures.insert(path);
                return None;
            }
        };
        let hotspots = match parse_system_hotspot_overlay(layout, &svg_data) {
            Ok(hotspots) => Arc::new(hotspots),
            Err(_) => {
                self.assets.controller_mapper_overlay_failures.insert(path);
                return None;
            }
        };
        self.assets
            .controller_mapper_hotspot_overlays
            .insert(path, hotspots.clone());
        Some(hotspots)
    }

    pub(crate) fn system_mapper_texture(
        &mut self,
        ctx: &egui::Context,
        layout: SystemControllerLayout,
        target_size: Vec2,
    ) -> Option<TextureHandle> {
        let path = self.resolve_system_mapper_diagram_path(layout)?;
        let scale_factor = (ctx.pixels_per_point() * 1.5).max(1.0);
        let raster_width = (target_size.x.max(1.0) * scale_factor).ceil() as u32;
        let raster_height = (target_size.y.max(1.0) * scale_factor).ceil() as u32;
        let cache_key = SizedTextureKey {
            path: path.clone(),
            width: raster_width.max(1),
            height: raster_height.max(1),
        };
        Self::load_sized_texture_from_path(
            &mut self.assets.controller_mapper_textures,
            &mut self.assets.image_load_failures,
            ctx,
            cache_key,
            "controller-mapper",
            egui::TextureOptions::LINEAR,
        )
    }

    fn load_sized_texture_from_path(
        cache: &mut HashMap<SizedTextureKey, TextureHandle>,
        failures: &mut HashSet<PathBuf>,
        ctx: &egui::Context,
        cache_key: SizedTextureKey,
        key_prefix: &str,
        options: egui::TextureOptions,
    ) -> Option<TextureHandle> {
        if failures.contains(&cache_key.path) {
            return None;
        }
        if let Some(texture) = cache.get(&cache_key) {
            return Some(texture.clone());
        }

        let extension = cache_key
            .path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase());

        let (size, pixels): ([usize; 2], Vec<u8>) = if extension.as_deref() == Some("svg") {
            let data = match fs::read(&cache_key.path) {
                Ok(data) => data,
                Err(_) => {
                    failures.insert(cache_key.path.clone());
                    return None;
                }
            };
            if looks_like_svg_markup(&data) {
                match rasterize_svg_data_to_size(&data, [cache_key.width, cache_key.height]) {
                    Some((size, pixels)) => (size, pixels),
                    None => {
                        failures.insert(cache_key.path.clone());
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
                        failures.insert(cache_key.path.clone());
                        return None;
                    }
                }
            }
        } else {
            let image = match image::open(&cache_key.path) {
                Ok(image) => image,
                Err(_) => {
                    failures.insert(cache_key.path.clone());
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
            failures.insert(cache_key.path.clone());
            return None;
        }

        let texture = ctx.load_texture(
            format!(
                "{key_prefix}:{}:{}x{}",
                cache_key.path.display(),
                cache_key.width,
                cache_key.height
            ),
            ColorImage::from_rgba_unmultiplied(size, &pixels),
            options,
        );
        cache.insert(cache_key, texture.clone());
        Some(texture)
    }
}

#[cfg(test)]
mod delivery_tests {
    use super::*;
    use eframe::glow;
    use std::num::NonZeroU32;

    fn cache() -> AssetCache {
        AssetCache::new(std::env::temp_dir().as_path())
    }

    #[test]
    fn cpu_frame_delivery_clears_stale_gl_frame() {
        let mut cache = cache();
        cache.last_gl_texture_frame = Some(GlTextureFrame {
            texture: glow::NativeTexture(NonZeroU32::new(1).unwrap()),
            width: 640,
            height: 448,
            bottom_left_origin: true,
            generation: 7,
        });

        cache.clear_for_cpu_frame();

        assert!(cache.last_gl_texture_frame.is_none());
    }

    #[test]
    fn gl_frame_delivery_clears_stale_cpu_texture() {
        let mut cache = cache();
        let ctx = egui::Context::default();
        let image = egui::ColorImage::from_rgba_unmultiplied([1, 1], &[255, 255, 255, 255]);
        cache.last_frame_texture =
            Some(ctx.load_texture("stale-cpu-frame", image, egui::TextureOptions::NEAREST));

        cache.clear_for_gl_texture_frame();

        assert!(cache.last_frame_texture.is_none());
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

fn rasterize_svg_data_to_size(data: &[u8], target_size: [u32; 2]) -> Option<([usize; 2], Vec<u8>)> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(data, &options).ok()?;
    let svg_size = tree.size().to_int_size();
    let raster_size = scaled_svg_raster_size([svg_size.width(), svg_size.height()], target_size)?;
    let width = raster_size[0] as u32;
    let height = raster_size[1] as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    let scale_x = width as f32 / tree.size().width();
    let scale_y = height as f32 / tree.size().height();
    let mut pixmap_mut = pixmap.as_mut();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale_x, scale_y),
        &mut pixmap_mut,
    );

    Some((raster_size, pixmap.data().to_vec()))
}

fn scaled_svg_raster_size(source_size: [u32; 2], target_size: [u32; 2]) -> Option<[usize; 2]> {
    let [source_width, source_height] = source_size;
    let [target_width, target_height] = target_size;
    if source_width == 0 || source_height == 0 || target_width == 0 || target_height == 0 {
        return None;
    }

    let width_scale = target_width as f32 / source_width as f32;
    let height_scale = target_height as f32 / source_height as f32;
    let scale = width_scale.min(height_scale);
    let width = (source_width as f32 * scale).round().max(1.0) as usize;
    let height = (source_height as f32 * scale).round().max(1.0) as usize;
    Some([width, height])
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

#[cfg(test)]
mod tests {
    use super::{scaled_svg_raster_size, SizedTextureKey};
    use std::path::PathBuf;

    #[test]
    fn scaled_svg_raster_size_preserves_aspect_ratio() {
        let size = scaled_svg_raster_size([64, 64], [900, 540]).unwrap();
        assert_eq!(size, [540, 540]);
    }

    #[test]
    fn scaled_svg_raster_size_scales_to_target_bounds() {
        let size = scaled_svg_raster_size([64, 32], [900, 540]).unwrap();
        assert_eq!(size, [900, 450]);
    }

    #[test]
    fn sized_texture_key_distinguishes_raster_sizes() {
        let path = PathBuf::from("controller.svg");
        let small = SizedTextureKey {
            path: path.clone(),
            width: 540,
            height: 540,
        };
        let large = SizedTextureKey {
            path,
            width: 1080,
            height: 1080,
        };
        assert_ne!(small, large);
    }
}
