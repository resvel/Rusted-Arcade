#[cfg(target_os = "macos")]
use std::ffi::OsString;
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::sync::Arc;

use arcade_domain::EmulationConfig;
#[cfg(target_os = "macos")]
use arcade_libretro::FrontendCapabilities;
use arcade_libretro::{LibretroHost, VideoBackendKind, VulkanPresentTestMetrics};
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

fn n64_host_and_rom(core_library_filename: &str) -> (LibretroHost, PathBuf) {
    let root = workspace_root();
    let core_root = root.join("target/debug/cores");
    let bios_root = root.join("target/debug/bios");
    let save_root = root.join("target/debug/data/save-states");
    let rom_path = root.join("target/debug/roms/n64/Diddy Kong Racing (U) (M2) (V1.0) [!].V64");

    assert!(
        core_root.join(core_library_filename).exists(),
        "required N64 core ({core_library_filename}) is missing from {}",
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

fn parallel_n64_host_and_rom() -> (LibretroHost, PathBuf) {
    n64_host_and_rom("parallel_n64_libretro.dylib")
}

fn mupen64plus_next_host_and_rom() -> (LibretroHost, PathBuf) {
    n64_host_and_rom("mupen64plus_next_libretro.dylib")
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

#[cfg(target_os = "macos")]
struct ScopedEnvVar {
    key: &'static str,
    previous_value: Option<OsString>,
}

#[cfg(target_os = "macos")]
impl ScopedEnvVar {
    fn set(key: &'static str, value: &str) -> Self {
        let previous_value = std::env::var_os(key);
        std::env::set_var(key, value);
        Self {
            key,
            previous_value,
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        match &self.previous_value {
            Some(previous) => std::env::set_var(self.key, previous),
            None => std::env::remove_var(self.key),
        }
    }
}

#[cfg(target_os = "macos")]
fn metrics_satisfy_vulkan_acceptance(metrics: &VulkanPresentTestMetrics) -> bool {
    metrics.backend_kind == VideoBackendKind::Vulkan
        && metrics.external_window_created
        && metrics.queue_present_successes > 0
        && metrics.external_present_deliveries > 0
        && metrics.source_non_black_seen
        && metrics.swapchain_non_black_seen
        && metrics.non_tiny_source_frame_seen
}

#[cfg(target_os = "macos")]
fn metrics_satisfy_vulkan_cpu_readback_acceptance(metrics: &VulkanPresentTestMetrics) -> bool {
    metrics.backend_kind == VideoBackendKind::Vulkan
        && !metrics.external_window_created
        && metrics.cpu_frame_deliveries > 0
        && metrics.source_non_black_seen
        && metrics.non_tiny_source_frame_seen
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
#[ignore = "requires a locally built/available mupen64plus_next core and bundled N64 ROM"]
fn mupen64plus_next_macos_loads_and_unloads() {
    let (host, rom_path) = mupen64plus_next_host_and_rom();
    let loaded_core = host
        .load_for_rom("N64", Some("mupen64plus_next"), &rom_path)
        .expect("mupen64plus_next should load on macOS");
    assert_eq!(loaded_core, "mupen64plus_next");
    assert!(host.is_loaded(), "host should report the core as loaded");

    host.unload().expect("core should unload cleanly");
    assert!(!host.is_loaded(), "host should report the core as unloaded");
}

#[test]
#[cfg(target_os = "macos")]
#[ignore = "requires a locally built parallel_n64 core and bundled N64 ROM"]
fn parallel_n64_macos_smoke_runs_frames() {
    let _enable_vulkan = ScopedEnvVar::set("ARCADE_MACOS_EXPERIMENTAL_VULKAN", "1");
    let _force_dynarec = ScopedEnvVar::set("ARCADE_PARALLEL_N64_CPUCORE", "dynamic_recompiler");
    let _enable_test_metrics = ScopedEnvVar::set("ARCADE_VULKAN_TEST_METRICS", "1");
    let _enable_safe_diag = ScopedEnvVar::set("ARCADE_PARALLEL_RDP_SAFE_DIAG", "1");
    let _disable_fail_fast = ScopedEnvVar::set("ARCADE_VULKAN_DISABLE_FAIL_FAST", "1");
    let _fail_fast_threshold = ScopedEnvVar::set("ARCADE_VULKAN_BLACK_FAIL_FAST_FRAMES", "900");
    let (host, rom_path) = parallel_n64_host_and_rom();
    let gl_context =
        HeadlessMacOpenGlContext::create().expect("macOS smoke test should create a GL context");
    host.set_frontend_capabilities(gl_context.frontend_capabilities());
    host.load_for_rom("N64", Some("parallel_n64"), &rom_path)
        .expect("parallel_n64 should load on macOS");

    let frame_budget = 900_u32;
    let mut metrics = host.vulkan_present_test_metrics();
    for frame_index in 0..frame_budget {
        host.run_frame().unwrap_or_else(|error| {
            let message = error.to_string();
            assert!(
                !message.contains("source frame size remained tiny"),
                "parallel_n64 Vulkan smoke unexpectedly triggered tiny-frame fail-fast at frame {frame_index}; metrics={metrics:?}; error={message}"
            );
            panic!(
                "parallel_n64 Vulkan smoke failed at frame {frame_index} with error={error}; metrics={metrics:?}"
            )
        });
        metrics = host.vulkan_present_test_metrics();
        assert!(
            metrics.queue_present_successes <= metrics.queue_present_attempts,
            "queue_present accounting is inconsistent at frame {frame_index}; metrics={metrics:?}"
        );
        if metrics_satisfy_vulkan_acceptance(&metrics) {
            break;
        }
    }

    host.unload().expect("core should unload cleanly");
    assert!(
        metrics_satisfy_vulkan_acceptance(&metrics),
        "parallel_n64 Vulkan smoke timed out after {frame_budget} frames; metrics={metrics:?}"
    );
    assert_eq!(
        metrics.backend_kind,
        VideoBackendKind::Vulkan,
        "parallel_n64 Vulkan smoke used wrong backend; metrics={metrics:?}"
    );
    assert!(
        metrics.external_present_deliveries > 0,
        "parallel_n64 Vulkan smoke never reported external-present deliveries; metrics={metrics:?}"
    );
    assert!(
        metrics.queue_present_successes == 0 || metrics.external_present_deliveries > 0,
        "external-present deliveries should be nonzero when queue_present succeeds; metrics={metrics:?}"
    );
    assert!(
        metrics.non_tiny_source_frame_seen,
        "parallel_n64 Vulkan smoke never observed a non-tiny source frame; metrics={metrics:?}"
    );
}

#[test]
#[cfg(target_os = "macos")]
#[ignore = "requires a locally built/available mupen64plus_next core and bundled N64 ROM"]
fn mupen64plus_next_macos_vulkan_parallel_rdp_smoke_runs_frames() {
    let _enable_parallel_rdp_mode = ScopedEnvVar::set("ARCADE_MUPEN64PLUS_NEXT_PARALLEL_RDP", "1");
    let _force_rdp_parallel = ScopedEnvVar::set("ARCADE_MUPEN64PLUS_NEXT_RDP_PLUGIN", "parallel");
    let _force_rsp_parallel = ScopedEnvVar::set("ARCADE_MUPEN64PLUS_NEXT_RSP_PLUGIN", "parallel");
    let _force_cpucore_dynarec =
        ScopedEnvVar::set("ARCADE_MUPEN64PLUS_NEXT_CPUCORE", "dynamic_recompiler");
    let _enable_test_metrics = ScopedEnvVar::set("ARCADE_VULKAN_TEST_METRICS", "1");
    let _black_fail_fast_threshold =
        ScopedEnvVar::set("ARCADE_VULKAN_BLACK_FAIL_FAST_FRAMES", "900");
    let _tiny_fail_fast_threshold =
        ScopedEnvVar::set("ARCADE_VULKAN_TINY_FRAME_FAIL_FAST_FRAMES", "900");
    let (host, rom_path) = mupen64plus_next_host_and_rom();
    let gl_context =
        HeadlessMacOpenGlContext::create().expect("macOS smoke test should create a GL context");
    host.set_frontend_capabilities(gl_context.frontend_capabilities());
    host.load_for_rom("N64", Some("mupen64plus_next"), &rom_path)
        .expect("mupen64plus_next should load on macOS");

    let frame_budget = 900_u32;
    let mut metrics = host.vulkan_present_test_metrics();
    for frame_index in 0..frame_budget {
        host.run_frame().unwrap_or_else(|error| {
            panic!(
                "mupen64plus_next Vulkan ParaLLEl smoke failed at frame {frame_index} with error={error}; metrics={metrics:?}"
            )
        });
        metrics = host.vulkan_present_test_metrics();
        assert!(
            metrics.queue_present_successes <= metrics.queue_present_attempts,
            "queue_present accounting is inconsistent at frame {frame_index}; metrics={metrics:?}"
        );
        if metrics_satisfy_vulkan_cpu_readback_acceptance(&metrics) {
            break;
        }
    }

    host.unload().expect("core should unload cleanly");
    assert!(
        metrics_satisfy_vulkan_cpu_readback_acceptance(&metrics),
        "mupen64plus_next Vulkan ParaLLEl smoke timed out waiting for Vulkan CPU-frame generation; metrics={metrics:?}"
    );
    assert_eq!(
        metrics.backend_kind,
        VideoBackendKind::Vulkan,
        "mupen64plus_next Vulkan ParaLLEl smoke used wrong backend; metrics={metrics:?}"
    );
    assert!(
        !metrics.external_window_created,
        "mupen64plus_next Vulkan ParaLLEl smoke unexpectedly created external present window; metrics={metrics:?}"
    );
    assert!(
        metrics.cpu_frame_deliveries > 0,
        "mupen64plus_next Vulkan ParaLLEl smoke never delivered CPU frames; metrics={metrics:?}"
    );
    assert!(
        metrics.non_tiny_source_frame_seen,
        "mupen64plus_next Vulkan ParaLLEl smoke never observed a non-tiny source frame; metrics={metrics:?}"
    );
}

#[test]
#[cfg(target_os = "macos")]
#[ignore = "requires a locally built parallel_n64 core and bundled N64 ROM"]
fn parallel_n64_macos_smoke_runs_many_frames() {
    let _enable_vulkan = ScopedEnvVar::set("ARCADE_MACOS_EXPERIMENTAL_VULKAN", "1");
    let _force_dynarec = ScopedEnvVar::set("ARCADE_PARALLEL_N64_CPUCORE", "dynamic_recompiler");
    let _enable_test_metrics = ScopedEnvVar::set("ARCADE_VULKAN_TEST_METRICS", "1");
    let _enable_safe_diag = ScopedEnvVar::set("ARCADE_PARALLEL_RDP_SAFE_DIAG", "1");
    let _disable_fail_fast = ScopedEnvVar::set("ARCADE_VULKAN_DISABLE_FAIL_FAST", "1");
    let _fail_fast_threshold = ScopedEnvVar::set("ARCADE_VULKAN_BLACK_FAIL_FAST_FRAMES", "900");
    let (host, rom_path) = parallel_n64_host_and_rom();
    let gl_context =
        HeadlessMacOpenGlContext::create().expect("macOS smoke test should create a GL context");
    host.set_frontend_capabilities(gl_context.frontend_capabilities());
    host.load_for_rom("N64", Some("parallel_n64"), &rom_path)
        .expect("parallel_n64 should load on macOS");

    let frame_budget = 900_u32;
    let mut metrics = host.vulkan_present_test_metrics();
    for frame_index in 0..frame_budget {
        host.run_frame().unwrap_or_else(|error| {
            let message = error.to_string();
            assert!(
                !message.contains("source frame size remained tiny"),
                "parallel_n64 Vulkan many-frame smoke unexpectedly triggered tiny-frame fail-fast at frame {frame_index}; metrics={metrics:?}; error={message}"
            );
            panic!(
                "parallel_n64 Vulkan many-frame smoke failed at frame {frame_index} with error={error}; metrics={metrics:?}"
            )
        });
        metrics = host.vulkan_present_test_metrics();
        assert!(
            metrics.backend_kind == VideoBackendKind::Vulkan,
            "parallel_n64 Vulkan many-frame smoke switched backend unexpectedly at frame {frame_index}; metrics={metrics:?}"
        );
        assert!(
            metrics.queue_present_successes <= metrics.queue_present_attempts,
            "queue_present accounting is inconsistent at frame {frame_index}; metrics={metrics:?}"
        );
    }

    host.unload().expect("core should unload cleanly");
    assert!(
        metrics.queue_present_successes > 0,
        "parallel_n64 Vulkan many-frame smoke observed zero queue_present successes; metrics={metrics:?}"
    );
    assert!(
        metrics.external_present_deliveries > 0,
        "parallel_n64 Vulkan many-frame smoke observed zero external-present deliveries; metrics={metrics:?}"
    );
    assert!(
        metrics.source_non_black_seen && metrics.swapchain_non_black_seen,
        "parallel_n64 Vulkan many-frame smoke did not observe eventual non-black content on source+swapchain; metrics={metrics:?}"
    );
    assert!(
        metrics.non_tiny_source_frame_seen,
        "parallel_n64 Vulkan many-frame smoke never observed a non-tiny source frame; metrics={metrics:?}"
    );
    eprintln!("parallel_n64 Vulkan many-frame smoke metrics={metrics:?}");
}

#[test]
#[cfg(target_os = "macos")]
#[ignore = "requires a locally built parallel_n64 core and bundled N64 ROM"]
fn parallel_n64_macos_vulkan_fail_fast_on_forced_black_frames() {
    let _enable_vulkan = ScopedEnvVar::set("ARCADE_MACOS_EXPERIMENTAL_VULKAN", "1");
    let _force_dynarec = ScopedEnvVar::set("ARCADE_PARALLEL_N64_CPUCORE", "dynamic_recompiler");
    let _enable_test_metrics = ScopedEnvVar::set("ARCADE_VULKAN_TEST_METRICS", "1");
    let _force_black = ScopedEnvVar::set("ARCADE_VULKAN_TEST_FORCE_BLACK", "1");
    let _fail_fast_threshold = ScopedEnvVar::set("ARCADE_VULKAN_BLACK_FAIL_FAST_FRAMES", "45");
    let (host, rom_path) = parallel_n64_host_and_rom();
    let gl_context =
        HeadlessMacOpenGlContext::create().expect("macOS smoke test should create a GL context");
    host.set_frontend_capabilities(gl_context.frontend_capabilities());
    host.load_for_rom("N64", Some("parallel_n64"), &rom_path)
        .expect("parallel_n64 should load on macOS");

    let mut observed_fail_fast = false;
    let mut metrics = host.vulkan_present_test_metrics();
    for frame_index in 0..240_u32 {
        match host.run_frame() {
            Ok(_) => {
                metrics = host.vulkan_present_test_metrics();
                if frame_index == 239 {
                    panic!(
                        "forced-black fail-fast smoke did not fail within frame budget; metrics={metrics:?}"
                    );
                }
            }
            Err(error) => {
                let message = error.to_string();
                assert!(
                    message.contains("fail-fast"),
                    "forced-black smoke produced a non-fail-fast error: {message}; metrics={:?}",
                    host.vulkan_present_test_metrics()
                );
                observed_fail_fast = true;
                break;
            }
        }
    }

    host.unload().expect("core should unload cleanly");
    assert!(
        observed_fail_fast,
        "forced-black fail-fast smoke did not observe the expected fail-fast error; metrics={metrics:?}"
    );
}

#[test]
#[cfg(target_os = "macos")]
#[ignore = "requires a locally built parallel_n64 core and bundled N64 ROM"]
fn parallel_n64_macos_vulkan_fail_fast_on_forced_tiny_source_frames() {
    let _enable_vulkan = ScopedEnvVar::set("ARCADE_MACOS_EXPERIMENTAL_VULKAN", "1");
    let _force_dynarec = ScopedEnvVar::set("ARCADE_PARALLEL_N64_CPUCORE", "dynamic_recompiler");
    let _enable_test_metrics = ScopedEnvVar::set("ARCADE_VULKAN_TEST_METRICS", "1");
    let _force_tiny_source = ScopedEnvVar::set("ARCADE_VULKAN_TEST_FORCE_1X1", "1");
    let _fail_fast_threshold = ScopedEnvVar::set("ARCADE_VULKAN_TINY_FRAME_FAIL_FAST_FRAMES", "45");
    let (host, rom_path) = parallel_n64_host_and_rom();
    let gl_context =
        HeadlessMacOpenGlContext::create().expect("macOS smoke test should create a GL context");
    host.set_frontend_capabilities(gl_context.frontend_capabilities());
    host.load_for_rom("N64", Some("parallel_n64"), &rom_path)
        .expect("parallel_n64 should load on macOS");

    let mut observed_fail_fast = false;
    let mut metrics = host.vulkan_present_test_metrics();
    for frame_index in 0..240_u32 {
        match host.run_frame() {
            Ok(_) => {
                metrics = host.vulkan_present_test_metrics();
                if frame_index == 239 {
                    panic!(
                        "forced-tiny-source fail-fast smoke did not fail within frame budget; metrics={metrics:?}"
                    );
                }
            }
            Err(error) => {
                let message = error.to_string();
                assert!(
                    message.contains("source frame size remained tiny"),
                    "forced-tiny-source smoke produced a non tiny-frame fail-fast error: {message}; metrics={:?}",
                    host.vulkan_present_test_metrics()
                );
                observed_fail_fast = true;
                break;
            }
        }
    }

    host.unload().expect("core should unload cleanly");
    assert!(
        observed_fail_fast,
        "forced-tiny-source fail-fast smoke did not observe the expected fail-fast error; metrics={metrics:?}"
    );
}
