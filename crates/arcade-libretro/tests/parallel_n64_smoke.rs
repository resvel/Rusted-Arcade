use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::sync::Arc;

use arcade_domain::EmulationConfig;
#[cfg(target_os = "macos")]
use arcade_libretro::FrontendCapabilities;
use arcade_libretro::LibretroHost;
#[cfg(target_os = "macos")]
use libloading::Library;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate dir should have workspace parent")
        .parent()
        .expect("workspace root should exist")
        .to_path_buf()
}

fn parallel_n64_host_and_rom() -> (LibretroHost, PathBuf) {
    let root = workspace_root();
    let core_root = root.join("target/debug/cores");
    let bios_root = root.join("target/debug/bios");
    let save_root = root.join("target/debug/data/save-states");
    let rom_path = root.join("target/debug/roms/n64/Diddy Kong Racing (U) (M2) (V1.0) [!].V64");

    assert!(
        core_root.join("parallel_n64_libretro.dylib").exists(),
        "parallel_n64 core is missing from {}",
        core_root.display()
    );
    assert!(
        rom_path.exists(),
        "test ROM is missing at {}",
        rom_path.display()
    );

    (
        LibretroHost::new(core_root, bios_root, save_root, EmulationConfig::default()),
        rom_path,
    )
}

#[cfg(target_os = "macos")]
type CGLContextObj = *mut std::ffi::c_void;
#[cfg(target_os = "macos")]
type CGLPixelFormatObj = *mut std::ffi::c_void;
#[cfg(target_os = "macos")]
type CGLPixelFormatAttribute = i32;
#[cfg(target_os = "macos")]
type CGLError = i32;

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn CGLChoosePixelFormat(
        attribs: *const CGLPixelFormatAttribute,
        pix: *mut CGLPixelFormatObj,
        npix: *mut i32,
    ) -> CGLError;
    fn CGLCreateContext(
        pix: CGLPixelFormatObj,
        share: CGLContextObj,
        ctx: *mut CGLContextObj,
    ) -> CGLError;
    fn CGLDestroyContext(ctx: CGLContextObj) -> CGLError;
    fn CGLDestroyPixelFormat(pix: CGLPixelFormatObj) -> CGLError;
    fn CGLSetCurrentContext(ctx: CGLContextObj) -> CGLError;
}

#[cfg(target_os = "macos")]
struct HeadlessMacOpenGlContext {
    _framework: Library,
    gl: Arc<glow::Context>,
    context: CGLContextObj,
    pixel_format: CGLPixelFormatObj,
}

#[cfg(target_os = "macos")]
impl HeadlessMacOpenGlContext {
    fn create() -> anyhow::Result<Self> {
        const K_CGL_PFA_OPENGL_PROFILE: CGLPixelFormatAttribute = 99;
        const K_CGL_PFA_ACCELERATED: CGLPixelFormatAttribute = 73;
        const K_CGL_PFA_ALLOW_OFFLINE_RENDERERS: CGLPixelFormatAttribute = 96;
        const K_CGL_OGLP_VERSION_LEGACY: CGLPixelFormatAttribute = 0x1000;

        let attributes = [
            K_CGL_PFA_OPENGL_PROFILE,
            K_CGL_OGLP_VERSION_LEGACY,
            K_CGL_PFA_ACCELERATED,
            K_CGL_PFA_ALLOW_OFFLINE_RENDERERS,
            0,
        ];
        let mut pixel_format = std::ptr::null_mut();
        let mut pixel_format_count = 0;
        let error = unsafe {
            CGLChoosePixelFormat(
                attributes.as_ptr(),
                &mut pixel_format,
                &mut pixel_format_count,
            )
        };
        anyhow::ensure!(error == 0, "CGLChoosePixelFormat failed with error {error}");
        anyhow::ensure!(
            !pixel_format.is_null() && pixel_format_count > 0,
            "CGLChoosePixelFormat did not return a usable pixel format"
        );

        let mut context = std::ptr::null_mut();
        let error = unsafe { CGLCreateContext(pixel_format, std::ptr::null_mut(), &mut context) };
        if error != 0 {
            unsafe {
                CGLDestroyPixelFormat(pixel_format);
            }
            anyhow::bail!("CGLCreateContext failed with error {error}");
        }

        let error = unsafe { CGLSetCurrentContext(context) };
        if error != 0 {
            unsafe {
                CGLDestroyContext(context);
                CGLDestroyPixelFormat(pixel_format);
            }
            anyhow::bail!("CGLSetCurrentContext failed with error {error}");
        }

        let framework =
            unsafe { Library::new("/System/Library/Frameworks/OpenGL.framework/OpenGL") }?;
        let gl = Arc::new(unsafe {
            glow::Context::from_loader_function(|symbol| {
                let symbol = format!("{symbol}\0");
                framework
                    .get::<*const std::ffi::c_void>(symbol.as_bytes())
                    .map(|proc| *proc)
                    .unwrap_or(std::ptr::null())
            })
        });

        Ok(Self {
            _framework: framework,
            gl,
            context,
            pixel_format,
        })
    }

    fn frontend_capabilities(&self) -> FrontendCapabilities {
        FrontendCapabilities {
            renderer_name: Some(String::from("eframe_glow")),
            gl_context: Some(self.gl.clone()),
            window_handle_kind: None,
            display_handle_kind: None,
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for HeadlessMacOpenGlContext {
    fn drop(&mut self) {
        unsafe {
            CGLSetCurrentContext(std::ptr::null_mut());
            CGLDestroyContext(self.context);
            CGLDestroyPixelFormat(self.pixel_format);
        }
    }
}

#[test]
#[cfg(target_os = "macos")]
#[ignore = "requires a locally built parallel_n64 core and bundled N64 ROM"]
fn parallel_n64_macos_loads_and_unloads() {
    let (host, rom_path) = parallel_n64_host_and_rom();
    let loaded_core = host
        .load_for_rom("N64", Some("parallel_n64"), &rom_path)
        .expect("parallel_n64 should load on macOS");
    assert_eq!(loaded_core, "parallel_n64");
    assert!(host.is_loaded(), "host should report the core as loaded");

    host.unload().expect("core should unload cleanly");
    assert!(!host.is_loaded(), "host should report the core as unloaded");
}

#[test]
#[cfg(target_os = "macos")]
#[ignore = "requires a locally built parallel_n64 core and bundled N64 ROM"]
fn parallel_n64_macos_smoke_runs_frames() {
    let (host, rom_path) = parallel_n64_host_and_rom();
    let gl_context =
        HeadlessMacOpenGlContext::create().expect("macOS smoke test should create a GL context");
    host.set_frontend_capabilities(gl_context.frontend_capabilities());
    host.load_for_rom("N64", Some("parallel_n64"), &rom_path)
        .expect("parallel_n64 should load on macOS");

    let mut saw_frame = false;
    for _ in 0..60 {
        if let Some(frame) = host.run_frame().expect("core should render a frame") {
            assert!(frame.width > 0, "frame width should be non-zero");
            assert!(frame.height > 0, "frame height should be non-zero");
            saw_frame = true;
            break;
        }
    }

    host.unload().expect("core should unload cleanly");
    assert!(saw_frame, "parallel_n64 did not produce a CPU frame");
}

#[test]
#[cfg(target_os = "macos")]
#[ignore = "requires a locally built parallel_n64 core and bundled N64 ROM"]
fn parallel_n64_macos_smoke_runs_many_frames() {
    let (host, rom_path) = parallel_n64_host_and_rom();
    let gl_context =
        HeadlessMacOpenGlContext::create().expect("macOS smoke test should create a GL context");
    host.set_frontend_capabilities(gl_context.frontend_capabilities());
    host.load_for_rom("N64", Some("parallel_n64"), &rom_path)
        .expect("parallel_n64 should load on macOS");

    let mut cpu_frames = 0_u32;
    let mut empty_frames = 0_u32;
    for _ in 0..600 {
        match host
            .run_frame()
            .expect("core should continue running frames")
        {
            Some(frame) => {
                assert!(frame.width > 0, "frame width should be non-zero");
                assert!(frame.height > 0, "frame height should be non-zero");
                cpu_frames += 1;
            }
            None => empty_frames += 1,
        }
    }

    host.unload().expect("core should unload cleanly");
    assert!(
        cpu_frames > 0,
        "parallel_n64 did not produce any CPU frames"
    );
    eprintln!("parallel_n64 many-frame smoke cpu_frames={cpu_frames} empty_frames={empty_frames}");
}
