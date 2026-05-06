use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use arcade_data::Database;
use arcade_domain::{resolve_arch_core_root, AppConfig};
use arcade_libretro::LibretroHost;
use arcade_services::NativeServices;
use arcade_ui::NativeArcadeUiApp;
use eframe::egui;
use tracing::{info, Level};
use tracing_subscriber::EnvFilter;

const APP_TITLE: &str = "Rusted Arcade";

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if let Some(log_path) = std::env::var_os("ARCADE_LOG_FILE").map(PathBuf::from) {
        if let Err(err) = init_file_tracing(filter, &log_path) {
            eprintln!(
                "failed to initialize file logging at {}: {err}",
                log_path.display()
            );
        }
        return;
    }

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_max_level(Level::INFO)
        .try_init();
}

fn init_file_tracing(filter: EnvFilter, log_path: &Path) -> Result<()> {
    if let Some(parent) = log_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create log directory {}", parent.display()))?;
    }
    let log_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(log_path)
        .with_context(|| format!("failed to open log file {}", log_path.display()))?;

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_max_level(Level::INFO)
        .with_ansi(false)
        .with_writer(move || {
            log_file
                .try_clone()
                .expect("failed to clone tracing log file handle")
        })
        .try_init();

    Ok(())
}

fn load_app_icon() -> Result<egui::IconData> {
    let image = image::load_from_memory(include_bytes!("../../../public/icon.png"))
        .context("failed to decode embedded app icon")?
        .into_rgba8();
    let (width, height) = image.dimensions();

    Ok(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

fn select_native_renderer(_effective_core_root: Option<&Path>) -> eframe::Renderer {
    #[cfg(target_os = "macos")]
    {
        let requested = std::env::var("ARCADE_MACOS_RENDERER")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase());
        match requested.as_deref() {
            Some("glow") | Some("opengl") => {
                info!("Using macOS renderer override: glow (OpenGL)");
                eframe::Renderer::Glow
            }
            None | Some("") | Some("wgpu") | Some("metal") => {
                if std::env::var_os("WGPU_BACKEND").is_none() {
                    std::env::set_var("WGPU_BACKEND", "metal");
                }
                info!("Using macOS default renderer: wgpu (Metal). Set ARCADE_MACOS_RENDERER=glow to use OpenGL.");
                eframe::Renderer::Wgpu
            }
            Some(other) => {
                if std::env::var_os("WGPU_BACKEND").is_none() {
                    std::env::set_var("WGPU_BACKEND", "metal");
                }
                info!(
                    "Unknown ARCADE_MACOS_RENDERER={other}; using macOS default wgpu (Metal). Valid values: wgpu, glow"
                );
                eframe::Renderer::Wgpu
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        eframe::Renderer::Glow
    }
}

fn main() -> Result<()> {
    init_tracing();

    // ARCADE_MACOS_GL_PROFILE defaults to the system Core 4.1 profile on macOS,
    // which is required by GLideN64 (needs Core 3.3+). Set ARCADE_MACOS_GL_PROFILE=legacy
    // to force the OpenGL 2.1 legacy profile if a specific core requires it.

    let config_path = std::env::args().nth(1).map(PathBuf::from);
    let (config, config_path) = AppConfig::load_or_create(config_path.as_deref())?;

    let effective_core_root = resolve_arch_core_root(&config.paths.core_root);
    let renderer = select_native_renderer(Some(&effective_core_root));

    info!("Using config: {}", config_path.display());
    info!("ROM root: {}", config.paths.rom_root.display());
    info!("DB path: {}", config.paths.db_path.display());
    info!("Core root: {}", config.paths.core_root.display());
    info!("Effective core root: {}", effective_core_root.display());
    info!("BIOS root: {}", config.paths.bios_root.display());
    info!("Rosetta: {}", arcade_domain::is_running_under_rosetta());

    let db = Database::open(&config)?;
    let services = NativeServices::bootstrap(config.clone(), config_path.clone(), db)?;
    let host = LibretroHost::new(
        effective_core_root,
        config.paths.bios_root.clone(),
        config.paths.save_state_root.clone(),
        config.emulation.clone(),
    );

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_TITLE)
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1024.0, 720.0])
            .with_icon(load_app_icon().context("failed to load window icon")?),
        renderer,
        depth_buffer: 24,
        stencil_buffer: 8,
        ..Default::default()
    };

    eframe::run_native(
        APP_TITLE,
        native_options,
        Box::new(move |cc| {
            NativeArcadeUiApp::configure_egui(&cc.egui_ctx);
            Ok(Box::new(NativeArcadeUiApp::new(
                services.clone(),
                host.clone(),
            )))
        }),
    )
    .map_err(|err| anyhow::anyhow!(err.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_default_renderer_stays_wgpu_even_when_play_core_exists() {
        let previous = std::env::var_os("ARCADE_MACOS_RENDERER");
        std::env::remove_var("ARCADE_MACOS_RENDERER");
        let core_root =
            std::env::temp_dir().join(format!("arcade-renderer-test-{}", std::process::id()));
        std::fs::create_dir_all(&core_root).expect("create core root");
        std::fs::write(core_root.join("play_libretro.dylib"), b"core").expect("write core marker");

        assert!(matches!(
            select_native_renderer(Some(&core_root)),
            eframe::Renderer::Wgpu
        ));

        let _ = std::fs::remove_dir_all(&core_root);
        match previous {
            Some(value) => std::env::set_var("ARCADE_MACOS_RENDERER", value),
            None => std::env::remove_var("ARCADE_MACOS_RENDERER"),
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_explicit_glow_renderer_override_still_works() {
        let previous = std::env::var_os("ARCADE_MACOS_RENDERER");
        std::env::set_var("ARCADE_MACOS_RENDERER", "glow");

        assert!(matches!(
            select_native_renderer(None),
            eframe::Renderer::Glow
        ));

        match previous {
            Some(value) => std::env::set_var("ARCADE_MACOS_RENDERER", value),
            None => std::env::remove_var("ARCADE_MACOS_RENDERER"),
        }
    }
}
