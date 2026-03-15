use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use arcade_data::Database;
use arcade_domain::AppConfig;
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
    let image = image::load_from_memory(include_bytes!("../../../public/icons.png"))
        .context("failed to decode embedded app icon")?
        .into_rgba8();
    let (width, height) = image.dimensions();

    Ok(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

fn main() -> Result<()> {
    init_tracing();

    #[cfg(target_os = "macos")]
    if std::env::var_os("ARCADE_MACOS_GL_PROFILE").is_none() {
        std::env::set_var("ARCADE_MACOS_GL_PROFILE", "legacy");
    }

    let config_path = std::env::args().nth(1).map(PathBuf::from);
    let (config, config_path) = AppConfig::load_or_create(config_path.as_deref())?;

    info!("Using config: {}", config_path.display());
    info!("ROM root: {}", config.paths.rom_root.display());
    info!("DB path: {}", config.paths.db_path.display());
    info!("Core root: {}", config.paths.core_root.display());
    info!("BIOS root: {}", config.paths.bios_root.display());

    let db = Database::open(&config)?;
    let services = NativeServices::bootstrap(config.clone(), config_path.clone(), db)?;
    let host = LibretroHost::new(
        config.paths.core_root.clone(),
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
        renderer: eframe::Renderer::Glow,
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
