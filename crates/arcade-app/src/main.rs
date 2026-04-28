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

#[cfg(target_os = "macos")]
fn should_auto_select_glow_renderer(effective_core_root: Option<&Path>) -> bool {
    effective_core_root
        .map(|root| root.join("play_libretro.dylib").is_file())
        .unwrap_or(false)
}

fn select_native_renderer(effective_core_root: Option<&Path>) -> eframe::Renderer {
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
                if requested.is_none() && should_auto_select_glow_renderer(effective_core_root) {
                    info!(
                        "Using macOS auto renderer: glow (OpenGL) because play_libretro.dylib was detected. Set ARCADE_MACOS_RENDERER=wgpu to force Metal."
                    );
                    return eframe::Renderer::Glow;
                }
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
            .with_title("Personal Arcade Native")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1024.0, 720.0])
            .with_icon(load_app_icon().context("failed to load window icon")?),
        renderer,
        depth_buffer: 24,
        stencil_buffer: 8,
        ..Default::default()
    };

    eframe::run_native(
        "Personal Arcade Native",
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
