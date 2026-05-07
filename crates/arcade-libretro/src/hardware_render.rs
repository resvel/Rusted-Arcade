use super::*;

/// Returns an opaque integer that uniquely identifies the GL context current on this thread.
/// Used to detect when a libretro core has switched to a different (core-owned) GL context.
/// Returns 0 when not implemented on the current platform.
#[cfg(target_os = "macos")]
pub(super) fn current_gl_ctx_id() -> usize {
    // CGLGetCurrentContext returns the CGL context current on the calling thread.
    // The pointer value is a stable identity for the lifetime of the context.
    extern "C" {
        fn CGLGetCurrentContext() -> *const std::ffi::c_void;
    }
    unsafe { CGLGetCurrentContext() as usize }
}

#[cfg(target_os = "macos")]
pub(super) fn make_gl_ctx_current(ctx_id: usize) -> bool {
    if ctx_id == 0 {
        return false;
    }
    extern "C" {
        fn CGLSetCurrentContext(ctx: *const std::ffi::c_void) -> i32;
    }
    unsafe { CGLSetCurrentContext(ctx_id as *const std::ffi::c_void) == 0 }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn CGLChoosePixelFormat(
        attribs: *const i32,
        pix: *mut *mut std::ffi::c_void,
        npix: *mut i32,
    ) -> i32;
    fn CGLCreateContext(
        pix: *mut std::ffi::c_void,
        share: *mut std::ffi::c_void,
        ctx: *mut *mut std::ffi::c_void,
    ) -> i32;
    fn CGLSetCurrentContext(ctx: *const std::ffi::c_void) -> i32;
    fn CGLGetCurrentContext() -> *const std::ffi::c_void;
    fn CGLTexImageIOSurface2D(
        ctx: *const std::ffi::c_void,
        target: u32,
        internal_format: u32,
        width: usize,
        height: usize,
        format: u32,
        ty: u32,
        io_surface: *mut std::ffi::c_void,
        plane: u32,
    ) -> i32;
    fn IOSurfaceCreate(properties: *const std::ffi::c_void) -> *mut std::ffi::c_void;

    static kIOSurfaceWidth: *mut Object;
    static kIOSurfaceHeight: *mut Object;
    static kIOSurfacePixelFormat: *mut Object;
    static kIOSurfaceBytesPerElement: *mut Object;
}

#[cfg(target_os = "macos")]
const MACOS_GL_TEXTURE_RECTANGLE: u32 = 0x84F5;

#[cfg(target_os = "macos")]
const MACOS_GL_TEXTURE_BINDING_RECTANGLE: u32 = 0x84F6;

#[cfg(target_os = "macos")]
const MACOS_IOSURFACE_RING_SIZE: usize = 2;

#[cfg(target_os = "macos")]
pub(super) fn ensure_private_play_gl_context_for_wgpu(runtime: &HostRuntime) -> Result<()> {
    let capabilities = runtime
        .video_coordinator
        .lock()
        .frontend_capabilities()
        .clone();
    if capabilities.gl_context.is_some() || !capabilities.supports_private_macos_play_gl_bridge() {
        return Ok(());
    }
    if runtime
        .hw_render_state
        .lock()
        .private_play_gl_context
        .is_some()
    {
        return Ok(());
    }

    let context = create_private_play_gl_context()
        .context("failed to set up private macOS Play OpenGL bridge context")?;
    let raw_context = context.raw_context;
    let gl = context.gl.clone();
    {
        let mut state = runtime.hw_render_state.lock();
        state.frontend_gl_context = Some(gl);
        state.eframe_gl_ctx_id = raw_context;
        state.private_play_gl_context = Some(context);
        state.using_private_play_gl_context = true;
    }
    info!(
        target: "arcade_libretro::core_loader",
        "Play macOS private GL bridge initialized CGL context=0x{raw_context:x}"
    );
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub(super) fn ensure_private_play_gl_context_for_wgpu(_runtime: &HostRuntime) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn create_private_play_gl_context() -> Result<MacosPrivateGlContext> {
    const KCGL_PFA_ACCELERATED: i32 = 73;
    const KCGL_PFA_OPENGL_PROFILE: i32 = 99;
    const KCGL_PFA_COLOR_SIZE: i32 = 8;
    const KCGL_PFA_DEPTH_SIZE: i32 = 12;
    const KCGL_PFA_STENCIL_SIZE: i32 = 13;
    const KCGLOGLP_VERSION_3_2_CORE: i32 = 0x3200;

    let attrs = [
        KCGL_PFA_OPENGL_PROFILE,
        KCGLOGLP_VERSION_3_2_CORE,
        KCGL_PFA_ACCELERATED,
        KCGL_PFA_COLOR_SIZE,
        24,
        KCGL_PFA_DEPTH_SIZE,
        24,
        KCGL_PFA_STENCIL_SIZE,
        8,
        0,
    ];
    let mut pixel_format = std::ptr::null_mut();
    let mut pixel_format_count = 0;
    let choose_status =
        unsafe { CGLChoosePixelFormat(attrs.as_ptr(), &mut pixel_format, &mut pixel_format_count) };
    if choose_status != 0 || pixel_format.is_null() || pixel_format_count <= 0 {
        return Err(anyhow!(
            "CGLChoosePixelFormat failed for private Play bridge (status={choose_status}, count={pixel_format_count})"
        ));
    }

    let mut raw_context = std::ptr::null_mut();
    let create_status =
        unsafe { CGLCreateContext(pixel_format, std::ptr::null_mut(), &mut raw_context) };
    if create_status != 0 || raw_context.is_null() {
        unsafe {
            CGLReleasePixelFormat(pixel_format);
        }
        return Err(anyhow!(
            "CGLCreateContext failed for private Play bridge (status={create_status})"
        ));
    }
    let current_status = unsafe { CGLSetCurrentContext(raw_context.cast_const()) };
    if current_status != 0 {
        unsafe {
            let _ = CGLDestroyContext(raw_context);
            CGLReleasePixelFormat(pixel_format);
        }
        return Err(anyhow!(
            "CGLSetCurrentContext failed for private Play bridge (status={current_status})"
        ));
    }

    let opengl_library =
        unsafe { Library::new("/System/Library/Frameworks/OpenGL.framework/OpenGL") }
            .context("failed to open OpenGL.framework for private Play bridge")?;
    let gl = unsafe {
        glow::Context::from_loader_function(|name| {
            let Ok(symbol_name) = CString::new(name) else {
                return std::ptr::null();
            };
            opengl_library
                .get::<*const std::ffi::c_void>(symbol_name.as_bytes_with_nul())
                .map(|symbol| *symbol)
                .unwrap_or(std::ptr::null())
        })
    };
    let gl = Arc::new(gl);
    log_frontend_gl_context(&gl);

    Ok(MacosPrivateGlContext {
        raw_context: raw_context as usize,
        raw_pixel_format: pixel_format as usize,
        gl,
        _opengl_library: opengl_library,
    })
}

#[cfg(not(target_os = "macos"))]
pub(super) fn current_gl_ctx_id() -> usize {
    0
}

#[cfg(not(target_os = "macos"))]
pub(super) fn make_gl_ctx_current(_ctx_id: usize) -> bool {
    false
}

pub(super) fn restore_frontend_gl_context(runtime: &HostRuntime, stage: &str) {
    let expected_ctx = runtime.hw_render_state.lock().eframe_gl_ctx_id;
    if expected_ctx == 0 {
        return;
    }

    let current_ctx = current_gl_ctx_id();
    if current_ctx == expected_ctx {
        return;
    }

    let restored = make_gl_ctx_current(expected_ctx);
    if std::env::var_os("LIBRETRO_TRACE_GL_CONTEXT").is_some()
        || std::env::var_os("LIBRETRO_TRACE_GL_FRAMEBUFFER").is_some()
    {
        let after_ctx = current_gl_ctx_id();
        eprintln!(
            "frontend GL context restore stage={stage} before=0x{current_ctx:x} expected=0x{expected_ctx:x} after=0x{after_ctx:x} restored={restored}"
        );
    }
}

pub(super) fn log_frontend_gl_context(gl: &glow::Context) {
    if std::env::var_os("LIBRETRO_TRACE_GL_CONTEXT").is_none() {
        return;
    }
    if GL_CONTEXT_DEBUG_LOGGED.fetch_add(1, Ordering::Relaxed) != 0 {
        return;
    }

    unsafe {
        let version = gl.get_parameter_string(glow::VERSION);
        let shading_language_version = gl.get_parameter_string(glow::SHADING_LANGUAGE_VERSION);
        let vendor = gl.get_parameter_string(glow::VENDOR);
        let renderer = gl.get_parameter_string(glow::RENDERER);
        let major_version = gl.get_parameter_i32(glow::MAJOR_VERSION);
        let minor_version = gl.get_parameter_i32(glow::MINOR_VERSION);
        let context_profile_mask = gl.get_parameter_i32(glow::CONTEXT_PROFILE_MASK);
        let depth_bits = gl.get_parameter_i32(glow::DEPTH_BITS);
        let stencil_bits = gl.get_parameter_i32(glow::STENCIL_BITS);
        let samples = gl.get_parameter_i32(glow::SAMPLES);
        let draw_framebuffer_binding = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
        let read_framebuffer_binding = gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
        let framebuffer_binding = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);

        eprintln!(
            "frontend GL context version={version:?} glsl={shading_language_version:?} vendor={vendor:?} renderer={renderer:?} major={major_version} minor={minor_version} profile_mask=0x{context_profile_mask:x} depth_bits={depth_bits} stencil_bits={stencil_bits} samples={samples} framebuffer={framebuffer_binding} draw_framebuffer={draw_framebuffer_binding} read_framebuffer={read_framebuffer_binding}"
        );
    }
}

pub(super) fn trace_frontend_gl_framebuffer(gl: &glow::Context, stage: &str) {
    if std::env::var_os("LIBRETRO_TRACE_GL_FRAMEBUFFER").is_none() {
        return;
    }

    unsafe {
        let current_ctx = current_gl_ctx_id();
        let framebuffer = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
        let draw_framebuffer = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
        let read_framebuffer = gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
        let draw_buffer = gl.get_parameter_i32(glow::DRAW_BUFFER);
        let read_buffer = gl.get_parameter_i32(glow::READ_BUFFER);
        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
        eprintln!(
            "frontend GL framebuffer stage={stage} ctx=0x{current_ctx:x} framebuffer={framebuffer} draw_framebuffer={draw_framebuffer} read_framebuffer={read_framebuffer} draw_buffer=0x{draw_buffer:x} read_buffer=0x{read_buffer:x} status=0x{status:x}"
        );
    }
}

pub(super) fn reset_callback_video_state(runtime: &HostRuntime) {
    let mut callback_state = runtime.callback_state.lock();
    callback_state.latest_frame = None;
    callback_state.latest_hw_frame = None;
    callback_state.pixel_format = PixelFormat::Argb1555;
    let mut audio_state = runtime.audio_state.lock();
    audio_state.samples.clear();
    audio_state.source_sample_rate = 0.0;
    audio_state.output_sample_rate = 0.0;
    audio_state.resample_phase = 0.0;
    audio_state.current_frame = None;
    audio_state.next_frame = None;
    audio_state.produced_samples_total = 0;
    audio_state.consumed_samples_total = 0;
    audio_state.trimmed_samples_total = 0;
    audio_state.produced_samples_window = 0;
    audio_state.consumed_samples_window = 0;
    audio_state.trimmed_samples_window = 0;
    audio_state.flow_window_started_at = None;
}

pub(super) fn hardware_render_frontend_available() -> bool {
    with_active_runtime(hardware_render_frontend_available_for).unwrap_or(false)
}

pub(super) fn hardware_render_frontend_available_for(runtime: &HostRuntime) -> bool {
    runtime.hw_render_state.lock().frontend_gl_context.is_some()
}

pub(super) fn hardware_render_preflight_available_for_core(
    runtime: &HostRuntime,
    core_name: &str,
) -> bool {
    let selection = runtime
        .video_coordinator
        .lock()
        .current_selection()
        .cloned();
    if let Some(selection) = selection {
        return selection.chosen != VideoBackendKind::Software
            || core_has_embedded_software_video_fallback(core_name);
    }

    hardware_render_frontend_available_for(runtime)
}

pub(super) fn hardware_render_requested() -> bool {
    with_active_runtime(|runtime| runtime.hw_render_state.lock().callbacks.is_some())
        .unwrap_or(false)
}

pub(super) fn hardware_render_context_ready() -> bool {
    with_active_runtime(|runtime| runtime.hw_render_state.lock().context_ready).unwrap_or(false)
}

pub(super) fn hw_render_target_size(max_size: (u32, u32), base_size: (u32, u32)) -> (u32, u32) {
    let width = max_size.0.max(base_size.0).max(1);
    let height = max_size.1.max(base_size.1).max(1);
    (width, height)
}

pub(super) fn apply_runtime_geometry_update(runtime: &HostRuntime, geometry: RetroGameGeometry) {
    let base_size = (geometry.base_width.max(1), geometry.base_height.max(1));
    let max_size = (
        geometry.max_width.max(base_size.0),
        geometry.max_height.max(base_size.1),
    );
    let target_size = hw_render_target_size(max_size, base_size);
    runtime.hw_render_state.lock().vulkan_fallback_frame_size = Some(target_size);

    if vulkan_debug_enabled() {
        info!(
            target: "arcade_libretro::vulkan_debug",
            "updated runtime geometry base={}x{} max={}x{} fallback={}x{} aspect={:.4}",
            base_size.0,
            base_size.1,
            max_size.0,
            max_size.1,
            target_size.0,
            target_size.1,
            geometry.aspect_ratio
        );
    }
}

pub(super) fn resolve_vulkan_frame_size(
    runtime: &HostRuntime,
    pending: PendingHardwareFrame,
) -> (u32, u32) {
    if pending.width > 1 && pending.height > 1 {
        return (pending.width, pending.height);
    }

    runtime
        .hw_render_state
        .lock()
        .vulkan_fallback_frame_size
        .filter(|(width, height)| *width > 1 && *height > 1)
        .unwrap_or((pending.width.max(1), pending.height.max(1)))
}

pub(super) fn hw_context_type_supported(context_type: u32) -> bool {
    const RETRO_HW_CONTEXT_OPENGL: u32 = 1;
    const RETRO_HW_CONTEXT_OPENGL_CORE: u32 = 3;

    matches!(
        context_type,
        RETRO_HW_CONTEXT_OPENGL | RETRO_HW_CONTEXT_OPENGL_CORE
    )
}

pub(super) fn hw_context_type_name(context_type: u32) -> &'static str {
    match context_type {
        1 => "OpenGL",
        2 => "OpenGLES2",
        3 => "OpenGLCore",
        4 => "OpenGLES3",
        5 => "OpenGLESVersion",
        6 => "Vulkan",
        _ => "Unknown",
    }
}

pub(super) fn hw_render_interface_type_name(interface_type: u32) -> &'static str {
    match interface_type {
        0 => "Vulkan",
        _ => "Unknown",
    }
}

#[cfg(test)]
pub(super) fn frontend_windowing_summary_for(runtime: &HostRuntime) -> Option<String> {
    let coordinator = runtime.video_coordinator.lock();
    let capabilities = coordinator.frontend_capabilities();
    let renderer = capabilities.renderer_name.as_deref();
    let window = capabilities.window_handle_kind.as_deref();
    let display = capabilities.display_handle_kind.as_deref();

    if renderer.is_none() && window.is_none() && display.is_none() {
        return None;
    }

    let mut parts = Vec::new();
    if let Some(renderer) = renderer {
        parts.push(format!("renderer={renderer}"));
    }
    if let Some(window) = window {
        parts.push(format!("window_handle={window}"));
    }
    if let Some(display) = display {
        parts.push(format!("display_handle={display}"));
    }

    Some(parts.join(", "))
}

pub(super) fn release_hw_render_target(state: &mut HardwareRenderState) {
    let Some(target) = state.target.take() else {
        return;
    };
    #[cfg(target_os = "macos")]
    if state.using_private_play_gl_context {
        if let Some(context) = state.private_play_gl_context.as_ref() {
            let _ = make_gl_ctx_current(context.raw_context);
        }
    }
    let Some(gl) = state.frontend_gl_context.clone() else {
        return;
    };

    unsafe {
        gl.delete_renderbuffer(target.depth_stencil);
        gl.delete_texture(target.color_texture);
        gl.delete_framebuffer(target.framebuffer);
        gl.delete_texture(target.play_present_texture);
        gl.delete_framebuffer(target.play_present_framebuffer);
        #[cfg(target_os = "macos")]
        for iosurface_target in target.play_iosurface_targets {
            gl.delete_texture(iosurface_target.texture);
            gl.delete_framebuffer(iosurface_target.framebuffer);
        }
    }
}

pub(super) fn destroy_hw_render_session_for(runtime: &HostRuntime) {
    destroy_hw_render_session_for_with_options(runtime, true, true);
}

pub(super) fn destroy_hw_render_session_after_core_deinit(runtime: &HostRuntime) {
    destroy_hw_render_session_for_with_options(runtime, false, false);
}

fn destroy_hw_render_session_for_with_options(
    runtime: &HostRuntime,
    call_core_context_destroy: bool,
    call_vulkan_core_destroy_device: bool,
) {
    runtime.video_coordinator.lock().end_session(runtime);
    let (callbacks, vulkan, external_vulkan_window) = {
        let mut state = runtime.hw_render_state.lock();
        release_hw_render_target(&mut state);
        let vulkan = state.vulkan.take();
        let external_vulkan_window = state.external_vulkan_window.take();
        state.external_vulkan_present_active = false;
        state.vulkan_fallback_frame_size = None;
        state.vulkan_negotiation = None;
        state.context_ready = false;
        state.context_type = None;
        (state.callbacks.take(), vulkan, external_vulkan_window)
    };

    if call_core_context_destroy {
        if let Some(callbacks) = callbacks {
            if let Some(context_destroy) = callbacks.context_destroy {
                unsafe {
                    context_destroy();
                }
            }
        }
    }

    if let Some(vulkan) = vulkan {
        if call_vulkan_core_destroy_device {
            destroy_vulkan_interface_state(vulkan);
        } else {
            destroy_vulkan_interface_state_without_core_callback(vulkan);
        }
    }
    if let Some(external_vulkan_window) = external_vulkan_window {
        destroy_external_vulkan_window(external_vulkan_window);
    }
}

pub(super) fn abandon_hw_render_core_callbacks_after_deinit(runtime: &HostRuntime) {
    let mut state = runtime.hw_render_state.lock();
    state.callbacks = None;
    if let Some(vulkan) = state.vulkan.as_mut() {
        vulkan.destroy_device_callback = None;
    }
}

pub(super) fn destroy_hw_render_session() {
    let Some(runtime) = active_runtime() else {
        return;
    };
    destroy_hw_render_session_for(&runtime);
}

pub(super) fn invoke_hw_context_reset() -> Result<()> {
    let (callbacks, context_type) = {
        let Some(runtime) = active_runtime() else {
            return Ok(());
        };
        let state = runtime.hw_render_state.lock();
        if state.context_ready {
            return Ok(());
        }
        (state.callbacks, state.context_type)
    };
    let Some(callbacks) = callbacks else {
        return Ok(());
    };
    if context_type == Some(RETRO_HW_CONTEXT_VULKAN) {
        let Some(runtime) = active_runtime() else {
            return Err(anyhow!("hardware-render core requires an active runtime"));
        };
        ensure_vulkan_interface_state_for(&runtime).map_err(|err| anyhow!(err))?;
    } else if !hardware_render_frontend_available() {
        return Err(anyhow!(
            "hardware-render core requested a GL context before the frontend GL context was ready"
        ));
    }
    if let Some(context_reset) = callbacks.context_reset {
        unsafe {
            context_reset();
        }
    }
    if let Some(runtime) = active_runtime() {
        runtime.hw_render_state.lock().context_ready = true;
    }
    Ok(())
}

pub(super) fn initialize_hw_render_context(target_size: (u32, u32)) -> Result<()> {
    let is_vulkan = with_active_runtime(|runtime| {
        runtime.hw_render_state.lock().context_type == Some(RETRO_HW_CONTEXT_VULKAN)
    })
    .unwrap_or(false);

    if !is_vulkan {
        ensure_hw_render_target(target_size)?;
    }

    invoke_hw_context_reset()
}

pub(super) fn ensure_hw_render_target(target_size: (u32, u32)) -> Result<()> {
    let Some(runtime) = active_runtime() else {
        return Err(anyhow!("hardware-render core requires an active runtime"));
    };
    #[cfg(target_os = "macos")]
    {
        let state = runtime.hw_render_state.lock();
        if state.using_private_play_gl_context {
            let raw_context = state
                .private_play_gl_context
                .as_ref()
                .map(|context| context.raw_context)
                .ok_or_else(|| {
                    anyhow!("private Play GL bridge was selected but no CGL context is available")
                })?;
            if !make_gl_ctx_current(raw_context) {
                return Err(anyhow!(
                    "failed to make private Play CGL context current before hardware target setup"
                ));
            }
        }
    }
    let Some(gl) = runtime.hw_render_state.lock().frontend_gl_context.clone() else {
        return Err(anyhow!(
            "hardware-render core requires an active GL context"
        ));
    };
    let mut state = runtime.hw_render_state.lock();

    if state
        .target
        .as_ref()
        .map(|target| target.width == target_size.0 && target.height == target_size.1)
        .unwrap_or(false)
    {
        return Ok(());
    }

    let (width, height) = target_size;
    let (framebuffer, color_texture, depth_stencil, play_present_framebuffer, play_present_texture) =
        if let Some(existing) = state.target.as_ref() {
            (
                existing.framebuffer,
                existing.color_texture,
                existing.depth_stencil,
                existing.play_present_framebuffer,
                existing.play_present_texture,
            )
        } else {
            let color_texture = unsafe { gl.create_texture() }
                .map_err(|err| anyhow!("failed to create hardware-render texture: {err}"))?;
            let framebuffer = unsafe { gl.create_framebuffer() }
                .map_err(|err| anyhow!("failed to create hardware-render framebuffer: {err}"))?;
            let depth_stencil = unsafe { gl.create_renderbuffer() }
                .map_err(|err| anyhow!("failed to create hardware-render depth buffer: {err}"))?;
            let play_present_texture = unsafe { gl.create_texture() }
                .map_err(|err| anyhow!("failed to create Play presentation texture: {err}"))?;
            let play_present_framebuffer = unsafe { gl.create_framebuffer() }
                .map_err(|err| anyhow!("failed to create Play presentation framebuffer: {err}"))?;
            (
                framebuffer,
                color_texture,
                depth_stencil,
                play_present_framebuffer,
                play_present_texture,
            )
        };

    unsafe {
        let previous_texture = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
        let previous_framebuffer = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
        let previous_read_framebuffer = gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
        let previous_draw_framebuffer = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
        let previous_read_buffer = gl.get_parameter_i32(glow::READ_BUFFER);
        let previous_draw_buffer = gl.get_parameter_i32(glow::DRAW_BUFFER);
        let previous_renderbuffer = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);

        let previous_texture = NonZeroU32::new(previous_texture as u32).map(glow::NativeTexture);
        let previous_framebuffer =
            NonZeroU32::new(previous_framebuffer as u32).map(glow::NativeFramebuffer);
        let previous_read_framebuffer =
            NonZeroU32::new(previous_read_framebuffer as u32).map(glow::NativeFramebuffer);
        let previous_draw_framebuffer =
            NonZeroU32::new(previous_draw_framebuffer as u32).map(glow::NativeFramebuffer);
        let previous_renderbuffer =
            NonZeroU32::new(previous_renderbuffer as u32).map(glow::NativeRenderbuffer);

        gl.bind_texture(glow::TEXTURE_2D, Some(color_texture));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            width as i32,
            height as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );

        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(color_texture),
            0,
        );
        gl.draw_buffer(glow::COLOR_ATTACHMENT0);
        gl.read_buffer(glow::COLOR_ATTACHMENT0);

        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth_stencil));
        gl.renderbuffer_storage(
            glow::RENDERBUFFER,
            glow::DEPTH24_STENCIL8,
            width as i32,
            height as i32,
        );
        gl.framebuffer_renderbuffer(
            glow::FRAMEBUFFER,
            glow::DEPTH_STENCIL_ATTACHMENT,
            glow::RENDERBUFFER,
            Some(depth_stencil),
        );

        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);

        gl.bind_texture(glow::TEXTURE_2D, Some(play_present_texture));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            width as i32,
            height as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(play_present_framebuffer));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(play_present_texture),
            0,
        );
        gl.draw_buffer(glow::COLOR_ATTACHMENT0);
        gl.read_buffer(glow::COLOR_ATTACHMENT0);
        let play_present_status = gl.check_framebuffer_status(glow::FRAMEBUFFER);

        gl.bind_renderbuffer(glow::RENDERBUFFER, previous_renderbuffer);
        gl.bind_framebuffer(glow::FRAMEBUFFER, previous_framebuffer);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, previous_read_framebuffer);
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, previous_draw_framebuffer);
        gl.read_buffer(previous_read_buffer as u32);
        gl.draw_buffer(previous_draw_buffer as u32);
        gl.bind_texture(glow::TEXTURE_2D, previous_texture);

        if status != glow::FRAMEBUFFER_COMPLETE || play_present_status != glow::FRAMEBUFFER_COMPLETE
        {
            if state.target.is_none() {
                gl.delete_renderbuffer(depth_stencil);
                gl.delete_texture(color_texture);
                gl.delete_framebuffer(framebuffer);
                gl.delete_texture(play_present_texture);
                gl.delete_framebuffer(play_present_framebuffer);
            }
            return Err(anyhow!(
                "failed to create a complete hardware-render framebuffer (status=0x{status:04x}, play_present_status=0x{play_present_status:04x})"
            ));
        }
    }

    if let Some(target) = state.target.as_mut() {
        target.width = width;
        target.height = height;
        target.play_texture_cache_score = f32::INFINITY;
        target.play_blank_frame_streak = 0;
        target.play_texture_frame_counter = 0;
        target.play_texture_last_signature = None;
        target.play_texture_stale_signature_streak = 0;
        target.play_texture_last_scan_frame = 0;
        target.play_texture_last_switch_frame = 0;
        target.play_present_width = width;
        target.play_present_height = height;
        target.play_present_generation = 0;
        #[cfg(target_os = "macos")]
        {
            for iosurface_target in target.play_iosurface_targets.drain(..) {
                unsafe {
                    gl.delete_texture(iosurface_target.texture);
                    gl.delete_framebuffer(iosurface_target.framebuffer);
                }
            }
            target.play_iosurface_index = 0;
            target.play_iosurface_generation = 0;
        }
        // Invalidate the emu-side FBO: it has a depth-stencil renderbuffer at the old size
        // and must be recreated in the emu thread's context at the new size.
        target.emu_ctx_framebuffer = None;
        target.emu_game_texture = None;
    } else {
        state.target = Some(HardwareRenderTarget {
            framebuffer,
            color_texture,
            depth_stencil,
            width,
            height,
            emu_ctx_framebuffer: None,
            play_present_framebuffer,
            play_present_texture,
            play_present_width: width,
            play_present_height: height,
            play_present_generation: 0,
            #[cfg(target_os = "macos")]
            play_iosurface_targets: Vec::new(),
            #[cfg(target_os = "macos")]
            play_iosurface_index: 0,
            #[cfg(target_os = "macos")]
            play_iosurface_generation: 0,
            emu_game_texture: None,
            play_texture_cache_score: f32::INFINITY,
            play_blank_frame_streak: 0,
            play_texture_frame_counter: 0,
            play_texture_last_signature: None,
            play_texture_stale_signature_streak: 0,
            play_texture_last_scan_frame: 0,
            play_texture_last_switch_frame: 0,
        });
    }
    Ok(())
}

pub(super) fn take_vulkan_render_frame(
    runtime: &HostRuntime,
    pending: PendingHardwareFrame,
) -> Result<VulkanRenderFrameResult> {
    let frame_size = resolve_vulkan_frame_size(runtime, pending);
    let source_probe_size = (pending.width.max(1), pending.height.max(1));
    if vulkan_debug_enabled() && (frame_size.0 != pending.width || frame_size.1 != pending.height) {
        let log_index = VULKAN_FALLBACK_SIZE_DEBUG_COUNTER.fetch_add(1, Ordering::Relaxed);
        if log_index < 32 || log_index % 120 == 0 {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "readback using fallback size idx={} pending={}x{} resolved={}x{}",
                log_index,
                pending.width,
                pending.height,
                frame_size.0,
                frame_size.1,
            );
        }
    }
    if vulkan_handoff_trace_enabled() && (source_probe_size.0 <= 1 || source_probe_size.1 <= 1) {
        let trace_index = VULKAN_HANDOFF_TRACE_COUNTER.fetch_add(1, Ordering::Relaxed);
        if trace_index < 32 || trace_index % 60 == 0 {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "handoff trace: tiny callback frame pending={}x{} source_probe={}x{} resolved_readback={}x{}",
                pending.width,
                pending.height,
                source_probe_size.0,
                source_probe_size.1,
                frame_size.0,
                frame_size.1
            );
        }
    }
    wait_for_unsignaled_vulkan_image(runtime)?;
    let required_size = frame_size.0 as usize * frame_size.1 as usize * 4;

    let mut state = runtime.hw_render_state.lock();
    let external_window = state
        .external_vulkan_window
        .as_ref()
        .map(ExternalVulkanWindow::descriptor);
    let Some(vulkan) = state.vulkan.as_mut() else {
        return Err(anyhow!(
            "Vulkan hardware-render frame requested before Vulkan interface setup"
        ));
    };
    debug_readback_vulkan_source_image(runtime, vulkan, source_probe_size)?;
    if present_vulkan_image(runtime, vulkan, frame_size, external_window)? {
        return Ok(VulkanRenderFrameResult::ExternalPresent);
    }
    if vulkan_force_fallback_idle() {
        wait_for_vulkan_device_idle(vulkan)?;
    }

    let Some(mut image) = take_current_pending_vulkan_image(vulkan) else {
        return Ok(VulkanRenderFrameResult::NoFrame);
    };

    #[derive(Clone, Copy)]
    enum ReadbackFormat {
        Rgba8,
        Bgra8,
        A1R5G5B5,
        R5G6B5,
    }
    let readback_format = match image.format {
        vk::Format::R8G8B8A8_UNORM | vk::Format::R8G8B8A8_SRGB => ReadbackFormat::Rgba8,
        vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB => ReadbackFormat::Bgra8,
        vk::Format::A1R5G5B5_UNORM_PACK16 => ReadbackFormat::A1R5G5B5,
        vk::Format::R5G6B5_UNORM_PACK16 => ReadbackFormat::R5G6B5,
        _ => {
            return Err(anyhow!(
                "unsupported Vulkan image format for readback: {:?}",
                image.format
            ));
        }
    };
    let src_bytes_per_pixel: usize = match readback_format {
        ReadbackFormat::Rgba8 | ReadbackFormat::Bgra8 => 4,
        ReadbackFormat::A1R5G5B5 | ReadbackFormat::R5G6B5 => 2,
    };
    let staging_size = frame_size.0 as usize * frame_size.1 as usize * src_bytes_per_pixel;
    ensure_vulkan_readback_resources(vulkan, staging_size)?;

    let readback = vulkan
        .readback
        .as_ref()
        .ok_or_else(|| anyhow!("Vulkan readback resources were not initialized"))?;
    let command_buffer = readback.command_buffer;
    let fence = readback.fence;
    let staging_buffer = readback.staging_buffer;
    let staging_memory = readback.staging_memory;
    let queue = vulkan.interface.queue;
    let queue_family_index = vulkan.queue_family_index;
    let image_handle = image.image;
    let image_layout = image.image_layout;
    let image_subresource_range = image.subresource_range;
    let image_subresource_layers = image.subresource_layers;
    let wait_semaphores = std::mem::take(&mut image.semaphores);
    let signal_semaphore = image.signal_semaphore.take();
    let src_queue_family_index = if image.src_queue_family == u32::MAX {
        vk::QUEUE_FAMILY_IGNORED
    } else {
        image.src_queue_family
    };

    unsafe {
        vulkan
            .device
            .reset_fences(&[fence])
            .map_err(|err| anyhow!("failed to reset Vulkan fence: {err:?}"))?;
        vulkan
            .device
            .reset_command_pool(readback.command_pool, vk::CommandPoolResetFlags::empty())
            .map_err(|err| anyhow!("failed to reset Vulkan command pool: {err:?}"))?;

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        vulkan
            .device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| anyhow!("failed to begin Vulkan command buffer: {err:?}"))?;

        let to_transfer = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .old_layout(image_layout)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(src_queue_family_index)
            .dst_queue_family_index(queue_family_index)
            .image(image_handle)
            .subresource_range(image_subresource_range);
        vulkan.device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_transfer],
        );

        let copy = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(image_subresource_layers)
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width: frame_size.0,
                height: frame_size.1,
                depth: 1,
            });
        vulkan.device.cmd_copy_image_to_buffer(
            command_buffer,
            image_handle,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            staging_buffer,
            &[copy],
        );

        let restore = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
            .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(image_layout)
            .src_queue_family_index(queue_family_index)
            .dst_queue_family_index(src_queue_family_index)
            .image(image_handle)
            .subresource_range(image_subresource_range);
        vulkan.device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[restore],
        );

        vulkan
            .device
            .end_command_buffer(command_buffer)
            .map_err(|err| anyhow!("failed to end Vulkan command buffer: {err:?}"))?;

        let wait_stage_masks = vec![vk::PipelineStageFlags::TRANSFER; wait_semaphores.len()];
        let signal_semaphores = signal_semaphore
            .map(|value| vec![value])
            .unwrap_or_default();
        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stage_masks)
            .command_buffers(std::slice::from_ref(&command_buffer))
            .signal_semaphores(&signal_semaphores);
        vulkan
            .device
            .queue_submit(queue, &[submit_info], fence)
            .map_err(|err| anyhow!("failed to submit Vulkan readback work: {err:?}"))?;
        vulkan
            .device
            .wait_for_fences(&[fence], true, 5_000_000_000)
            .map_err(|err| anyhow!("failed waiting for Vulkan readback fence: {err:?}"))?;
    }

    // Map the staging buffer and convert to RGBA8888.
    let mut pixels = vec![0u8; required_size];
    unsafe {
        let mapped = vulkan
            .device
            .map_memory(
                staging_memory,
                0,
                staging_size as u64,
                vk::MemoryMapFlags::empty(),
            )
            .map_err(|err| anyhow!("failed to map Vulkan staging memory: {err:?}"))?;
        let src = std::slice::from_raw_parts(mapped as *const u8, staging_size);
        match readback_format {
            ReadbackFormat::Rgba8 => {
                pixels.copy_from_slice(src);
            }
            ReadbackFormat::Bgra8 => {
                pixels.copy_from_slice(src);
            }
            ReadbackFormat::A1R5G5B5 => {
                // 16-bit packed: bit15=A, bits14-10=R, bits9-5=G, bits4-0=B
                let src16 = std::slice::from_raw_parts(mapped as *const u16, staging_size / 2);
                for (i, &packed) in src16.iter().enumerate() {
                    let r5 = ((packed >> 10) & 0x1F) as u8;
                    let g5 = ((packed >> 5) & 0x1F) as u8;
                    let b5 = (packed & 0x1F) as u8;
                    let off = i * 4;
                    pixels[off] = (r5 << 3) | (r5 >> 2);
                    pixels[off + 1] = (g5 << 3) | (g5 >> 2);
                    pixels[off + 2] = (b5 << 3) | (b5 >> 2);
                    pixels[off + 3] = 255;
                }
            }
            ReadbackFormat::R5G6B5 => {
                // 16-bit packed: bits15-11=R, bits10-5=G, bits4-0=B
                let src16 = std::slice::from_raw_parts(mapped as *const u16, staging_size / 2);
                for (i, &packed) in src16.iter().enumerate() {
                    let r5 = ((packed >> 11) & 0x1F) as u8;
                    let g6 = ((packed >> 5) & 0x3F) as u8;
                    let b5 = (packed & 0x1F) as u8;
                    let off = i * 4;
                    pixels[off] = (r5 << 3) | (r5 >> 2);
                    pixels[off + 1] = (g6 << 2) | (g6 >> 4);
                    pixels[off + 2] = (b5 << 3) | (b5 >> 2);
                    pixels[off + 3] = 255;
                }
            }
        }
        vulkan.device.unmap_memory(staging_memory);
    }
    drop(state);

    // For 32-bit formats, fix alpha and swizzle.  16-bit formats are already RGBA with
    // alpha=255 from the conversion above.
    match readback_format {
        ReadbackFormat::Bgra8 => {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
                pixel[3] = 255;
            }
        }
        ReadbackFormat::Rgba8 => {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
        }
        ReadbackFormat::A1R5G5B5 | ReadbackFormat::R5G6B5 => {
            // Already converted to RGBA with alpha=255
        }
    }

    if pending.bottom_left_origin {
        let row_len = frame_size.0 as usize * 4;
        let half_height = frame_size.1 as usize / 2;
        for row in 0..half_height {
            let top = row * row_len;
            let bottom = (frame_size.1 as usize - 1 - row) * row_len;
            let (head, tail) = pixels.split_at_mut(bottom);
            let top_row = &mut head[top..top + row_len];
            let bottom_row = &mut tail[..row_len];
            top_row.swap_with_slice(bottom_row);
        }
    }

    if vulkan_debug_enabled() {
        let frame_index = VULKAN_READBACK_DEBUG_COUNTER.fetch_add(1, Ordering::Relaxed);
        if frame_index < 16 {
            let (checksum, non_black_pixels, first_rgba) = summarize_rgba_debug_pixels(&pixels);
            info!(
                target: "arcade_libretro::vulkan_debug",
                "readback frame={} size={}x{} pitch={} checksum=0x{checksum:016x} non_black_samples={}/64 first_rgba={:02x},{:02x},{:02x},{:02x} bottom_left_origin={}",
                frame_index,
                frame_size.0,
                frame_size.1,
                frame_size.0 as usize * 4,
                non_black_pixels,
                first_rgba[0],
                first_rgba[1],
                first_rgba[2],
                first_rgba[3],
                pending.bottom_left_origin,
            );
        }
    }

    Ok(VulkanRenderFrameResult::CpuFrame(FrameBuffer {
        width: frame_size.0,
        height: frame_size.1,
        pitch: frame_size.0 as usize * 4,
        data: pixels,
        pixel_format: PixelFormat::Rgba8888,
    }))
}

pub(super) enum VulkanRenderFrameResult {
    CpuFrame(FrameBuffer),
    ExternalPresent,
    NoFrame,
}

pub(super) fn drain_frontend_gl_errors(runtime: &HostRuntime, stage: &str) {
    let (gl, context_type) = {
        let state = runtime.hw_render_state.lock();
        (state.frontend_gl_context.clone(), state.context_type)
    };
    if context_type == Some(RETRO_HW_CONTEXT_VULKAN) {
        return;
    }
    let Some(gl) = gl else {
        return;
    };

    trace_frontend_gl_framebuffer(&gl, stage);

    let mut errors = Vec::new();
    unsafe {
        loop {
            let error = gl.get_error();
            if error == glow::NO_ERROR {
                break;
            }
            errors.push(error);
            if errors.len() >= 16 {
                break;
            }
        }
    }

    if !errors.is_empty() && std::env::var_os("LIBRETRO_TRACE_GL_ERRORS").is_some() {
        eprintln!("drained frontend GL errors after {stage}: {errors:#x?}");
    }
}

pub(super) fn capture_frontend_gl_state(runtime: &HostRuntime) -> Option<FrontendGlStateSnapshot> {
    let Some(gl) = runtime.hw_render_state.lock().frontend_gl_context.clone() else {
        return None;
    };

    unsafe {
        let active_texture = gl.get_parameter_i32(glow::ACTIVE_TEXTURE);
        let max_texture_units = gl
            .get_parameter_i32(glow::MAX_COMBINED_TEXTURE_IMAGE_UNITS)
            .clamp(1, 8);
        let mut texture_units = Vec::with_capacity(max_texture_units as usize);
        for unit in 0..max_texture_units {
            gl.active_texture(glow::TEXTURE0 + unit as u32);
            texture_units.push(FrontendGlTextureUnitState {
                texture_2d: NonZeroU32::new(gl.get_parameter_i32(glow::TEXTURE_BINDING_2D) as u32)
                    .map(glow::NativeTexture),
                sampler: NonZeroU32::new(gl.get_parameter_i32(glow::SAMPLER_BINDING) as u32)
                    .map(glow::NativeSampler),
            });
        }

        gl.active_texture(active_texture as u32);

        Some(FrontendGlStateSnapshot {
            active_texture,
            texture_units,
            current_program: NonZeroU32::new(gl.get_parameter_i32(glow::CURRENT_PROGRAM) as u32)
                .map(glow::NativeProgram),
            vertex_array: NonZeroU32::new(gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING) as u32)
                .map(glow::NativeVertexArray),
            array_buffer: NonZeroU32::new(gl.get_parameter_i32(glow::ARRAY_BUFFER_BINDING) as u32)
                .map(glow::NativeBuffer),
            element_array_buffer: NonZeroU32::new(
                gl.get_parameter_i32(glow::ELEMENT_ARRAY_BUFFER_BINDING) as u32,
            )
            .map(glow::NativeBuffer),
            pixel_pack_buffer: NonZeroU32::new(
                gl.get_parameter_i32(glow::PIXEL_PACK_BUFFER_BINDING) as u32,
            )
            .map(glow::NativeBuffer),
            pixel_unpack_buffer: NonZeroU32::new(
                gl.get_parameter_i32(glow::PIXEL_UNPACK_BUFFER_BINDING) as u32,
            )
            .map(glow::NativeBuffer),
            renderbuffer: NonZeroU32::new(gl.get_parameter_i32(glow::RENDERBUFFER_BINDING) as u32)
                .map(glow::NativeRenderbuffer),
            framebuffer: NonZeroU32::new(gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING) as u32)
                .map(glow::NativeFramebuffer),
            read_framebuffer: NonZeroU32::new(
                gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING) as u32
            )
            .map(glow::NativeFramebuffer),
            draw_framebuffer: NonZeroU32::new(
                gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING) as u32
            )
            .map(glow::NativeFramebuffer),
            read_buffer: gl.get_parameter_i32(glow::READ_BUFFER),
            draw_buffer: gl.get_parameter_i32(glow::DRAW_BUFFER),
            unpack_alignment: gl.get_parameter_i32(glow::UNPACK_ALIGNMENT),
            pack_alignment: gl.get_parameter_i32(glow::PACK_ALIGNMENT),
            unpack_row_length: gl.get_parameter_i32(glow::UNPACK_ROW_LENGTH),
            pack_row_length: gl.get_parameter_i32(glow::PACK_ROW_LENGTH),
            viewport: get_gl_int4(&gl, glow::VIEWPORT),
            scissor_box: get_gl_int4(&gl, glow::SCISSOR_BOX),
            blend_enabled: gl.is_enabled(glow::BLEND),
            cull_face_enabled: gl.is_enabled(glow::CULL_FACE),
            depth_test_enabled: gl.is_enabled(glow::DEPTH_TEST),
            scissor_test_enabled: gl.is_enabled(glow::SCISSOR_TEST),
            stencil_test_enabled: gl.is_enabled(glow::STENCIL_TEST),
            blend_src_rgb: gl.get_parameter_i32(glow::BLEND_SRC_RGB),
            blend_dst_rgb: gl.get_parameter_i32(glow::BLEND_DST_RGB),
            blend_src_alpha: gl.get_parameter_i32(glow::BLEND_SRC_ALPHA),
            blend_dst_alpha: gl.get_parameter_i32(glow::BLEND_DST_ALPHA),
            blend_equation_rgb: gl.get_parameter_i32(glow::BLEND_EQUATION_RGB),
            blend_equation_alpha: gl.get_parameter_i32(glow::BLEND_EQUATION_ALPHA),
            color_mask: get_gl_bool4(&gl, glow::COLOR_WRITEMASK),
            depth_mask: gl.get_parameter_bool(glow::DEPTH_WRITEMASK),
            stencil_mask_front: gl.get_parameter_i32(glow::STENCIL_WRITEMASK),
            stencil_mask_back: gl.get_parameter_i32(glow::STENCIL_BACK_WRITEMASK),
        })
    }
}

pub(super) fn restore_frontend_gl_state(runtime: &HostRuntime, snapshot: &FrontendGlStateSnapshot) {
    let Some(gl) = runtime.hw_render_state.lock().frontend_gl_context.clone() else {
        return;
    };

    unsafe {
        for (unit, state) in snapshot.texture_units.iter().enumerate() {
            gl.active_texture(glow::TEXTURE0 + unit as u32);
            gl.bind_texture(glow::TEXTURE_2D, state.texture_2d);
            gl.bind_sampler(unit as u32, state.sampler);
        }

        gl.active_texture(snapshot.active_texture as u32);
        gl.use_program(snapshot.current_program);
        gl.bind_vertex_array(snapshot.vertex_array);
        gl.bind_buffer(glow::ARRAY_BUFFER, snapshot.array_buffer);
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, snapshot.element_array_buffer);
        gl.bind_buffer(glow::PIXEL_PACK_BUFFER, snapshot.pixel_pack_buffer);
        gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, snapshot.pixel_unpack_buffer);
        gl.bind_renderbuffer(glow::RENDERBUFFER, snapshot.renderbuffer);
        gl.bind_framebuffer(glow::FRAMEBUFFER, snapshot.framebuffer);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, snapshot.read_framebuffer);
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, snapshot.draw_framebuffer);
        gl.read_buffer(snapshot.read_buffer as u32);
        gl.draw_buffer(snapshot.draw_buffer as u32);
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, snapshot.unpack_alignment);
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, snapshot.pack_alignment);
        gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, snapshot.unpack_row_length);
        gl.pixel_store_i32(glow::PACK_ROW_LENGTH, snapshot.pack_row_length);
        set_gl_cap(&gl, glow::BLEND, snapshot.blend_enabled);
        set_gl_cap(&gl, glow::CULL_FACE, snapshot.cull_face_enabled);
        set_gl_cap(&gl, glow::DEPTH_TEST, snapshot.depth_test_enabled);
        set_gl_cap(&gl, glow::SCISSOR_TEST, snapshot.scissor_test_enabled);
        set_gl_cap(&gl, glow::STENCIL_TEST, snapshot.stencil_test_enabled);
        gl.viewport(
            snapshot.viewport[0],
            snapshot.viewport[1],
            snapshot.viewport[2],
            snapshot.viewport[3],
        );
        gl.scissor(
            snapshot.scissor_box[0],
            snapshot.scissor_box[1],
            snapshot.scissor_box[2],
            snapshot.scissor_box[3],
        );
        gl.blend_func_separate(
            snapshot.blend_src_rgb as u32,
            snapshot.blend_dst_rgb as u32,
            snapshot.blend_src_alpha as u32,
            snapshot.blend_dst_alpha as u32,
        );
        gl.blend_equation_separate(
            snapshot.blend_equation_rgb as u32,
            snapshot.blend_equation_alpha as u32,
        );
        gl.color_mask(
            snapshot.color_mask[0],
            snapshot.color_mask[1],
            snapshot.color_mask[2],
            snapshot.color_mask[3],
        );
        gl.depth_mask(snapshot.depth_mask);
        gl.stencil_mask_separate(glow::FRONT, snapshot.stencil_mask_front as u32);
        gl.stencil_mask_separate(glow::BACK, snapshot.stencil_mask_back as u32);
    }
}

fn set_gl_cap(gl: &glow::Context, cap: u32, enabled: bool) {
    unsafe {
        if enabled {
            gl.enable(cap);
        } else {
            gl.disable(cap);
        }
    }
}

fn get_gl_int4(gl: &glow::Context, pname: u32) -> [i32; 4] {
    let mut values = [0_i32; 4];
    let _ = gl;
    #[cfg(target_os = "macos")]
    unsafe {
        extern "C" {
            fn glGetIntegerv(pname: u32, data: *mut i32);
        }
        glGetIntegerv(pname, values.as_mut_ptr());
    }
    values
}

fn get_gl_bool4(gl: &glow::Context, pname: u32) -> [bool; 4] {
    let mut values = [0_u8; 4];
    #[cfg(target_os = "macos")]
    unsafe {
        extern "C" {
            fn glGetBooleanv(pname: u32, data: *mut u8);
        }
        let _ = gl;
        glGetBooleanv(pname, values.as_mut_ptr());
    }
    values.map(|value| value != 0)
}

pub(super) fn force_default_gl_framebuffer(runtime: &HostRuntime) -> bool {
    let _ = runtime;
    std::env::var_os("ARCADE_GL_FORCE_DEFAULT_FRAMEBUFFER").is_some()
}

fn should_use_strict_gl_texture_match(runtime: &HostRuntime) -> bool {
    !runtime
        .environment_context
        .lock()
        .loaded_core_name
        .as_deref()
        .is_some_and(|core| core.eq_ignore_ascii_case("play"))
}

fn should_allow_default_gl_fallback(runtime: &HostRuntime) -> bool {
    !runtime
        .environment_context
        .lock()
        .loaded_core_name
        .as_deref()
        .is_some_and(|core| core.eq_ignore_ascii_case("play"))
}

fn should_allow_generic_gl_texture_scan(runtime: &HostRuntime) -> bool {
    let core_name = runtime.environment_context.lock().loaded_core_name.clone();
    core_name_allows_generic_gl_texture_scan(core_name.as_deref())
}

fn core_name_allows_generic_gl_texture_scan(core_name: Option<&str>) -> bool {
    !core_name.is_some_and(|core| core.eq_ignore_ascii_case("dolphin"))
}

fn is_play_core(runtime: &HostRuntime) -> bool {
    runtime
        .environment_context
        .lock()
        .loaded_core_name
        .as_deref()
        .is_some_and(|core| {
            core.eq_ignore_ascii_case("play") || core.eq_ignore_ascii_case("flycast")
        })
}

fn is_dolphin_core(runtime: &HostRuntime) -> bool {
    runtime
        .environment_context
        .lock()
        .loaded_core_name
        .as_deref()
        .is_some_and(|core| core.eq_ignore_ascii_case("dolphin"))
}

fn sampled_non_black_pixels(pixels: &[u8], width: u32, height: u32) -> usize {
    let (_, non_black_samples, _, _) = summarize_rgba_debug_pixels_grid(pixels, width, height);
    non_black_samples
}

fn read_gl_framebuffer_rgba(
    gl: &glow::Context,
    framebuffer: Option<glow::Framebuffer>,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut pixels = vec![0_u8; width as usize * height as usize * 4];
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, framebuffer);
        if framebuffer.is_some() {
            gl.read_buffer(glow::COLOR_ATTACHMENT0);
        }
        gl.read_pixels(
            0,
            0,
            width as i32,
            height as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut pixels)),
        );
    }
    pixels
}

pub(super) fn should_use_play_direct_gl_texture(runtime: &HostRuntime) -> bool {
    let play_direct =
        is_play_core(runtime) && std::env::var_os("ARCADE_PLAY_GL_CPU_READBACK").is_none();
    let dolphin_direct =
        is_dolphin_core(runtime) && std::env::var_os("ARCADE_DOLPHIN_GL_CPU_READBACK").is_none();
    play_direct || dolphin_direct
}

pub(super) fn take_play_direct_gl_texture_frame(
    runtime: &HostRuntime,
    pending: PendingHardwareFrame,
) -> Result<Option<GlTextureFrame>> {
    if !should_use_play_direct_gl_texture(runtime) {
        return Ok(None);
    }

    let gl = {
        let state = runtime.hw_render_state.lock();
        state.frontend_gl_context.clone()
    }
    .ok_or_else(|| anyhow!("Play direct GL frame requested without an active GL context"))?;

    let mut state = runtime.hw_render_state.lock();
    let Some(target) = state.target.as_mut() else {
        return Ok(None);
    };
    let source_framebuffer = pending.callback_framebuffer.unwrap_or_else(|| {
        if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
            eprintln!("play direct gl: no callback FBO; using host render target FBO");
        }
        target.framebuffer
    });
    if !unsafe { gl.is_framebuffer(source_framebuffer) } {
        return Ok(None);
    }

    let previous_read_framebuffer = unsafe { gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING) };
    let previous_draw_framebuffer = unsafe { gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING) };
    let previous_read_buffer = unsafe { gl.get_parameter_i32(glow::READ_BUFFER) };
    let previous_draw_buffer = unsafe { gl.get_parameter_i32(glow::DRAW_BUFFER) };
    let previous_texture = unsafe { gl.get_parameter_i32(glow::TEXTURE_BINDING_2D) };
    let scissor_was_enabled = unsafe { gl.is_enabled(glow::SCISSOR_TEST) };
    let previous_read_framebuffer =
        NonZeroU32::new(previous_read_framebuffer as u32).map(glow::NativeFramebuffer);
    let previous_draw_framebuffer =
        NonZeroU32::new(previous_draw_framebuffer as u32).map(glow::NativeFramebuffer);
    let previous_texture = NonZeroU32::new(previous_texture as u32).map(glow::NativeTexture);

    unsafe {
        if target.play_present_width != pending.width
            || target.play_present_height != pending.height
        {
            gl.bind_texture(glow::TEXTURE_2D, Some(target.play_present_texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                pending.width as i32,
                pending.height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            target.play_present_width = pending.width;
            target.play_present_height = pending.height;
            target.play_present_generation = 0;
        }

        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(source_framebuffer));
        gl.read_buffer(glow::COLOR_ATTACHMENT0);
        let read_status = gl.check_framebuffer_status(glow::READ_FRAMEBUFFER);
        if read_status != glow::FRAMEBUFFER_COMPLETE {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, previous_read_framebuffer);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, previous_draw_framebuffer);
            gl.read_buffer(previous_read_buffer as u32);
            gl.draw_buffer(previous_draw_buffer as u32);
            gl.bind_texture(glow::TEXTURE_2D, previous_texture);
            if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
                eprintln!(
                    "play direct gl: callback FBO incomplete status=0x{read_status:04x}; skipping CPU readback"
                );
            }
            return Ok(None);
        }

        gl.bind_framebuffer(
            glow::DRAW_FRAMEBUFFER,
            Some(target.play_present_framebuffer),
        );
        gl.draw_buffer(glow::COLOR_ATTACHMENT0);
        let draw_status = gl.check_framebuffer_status(glow::DRAW_FRAMEBUFFER);
        if draw_status != glow::FRAMEBUFFER_COMPLETE {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, previous_read_framebuffer);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, previous_draw_framebuffer);
            gl.read_buffer(previous_read_buffer as u32);
            gl.draw_buffer(previous_draw_buffer as u32);
            gl.bind_texture(glow::TEXTURE_2D, previous_texture);
            return Err(anyhow!(
                "Play presentation framebuffer incomplete (status=0x{draw_status:04x})"
            ));
        }

        if scissor_was_enabled {
            gl.disable(glow::SCISSOR_TEST);
        }
        gl.blit_framebuffer(
            0,
            0,
            pending.width as i32,
            pending.height as i32,
            0,
            0,
            pending.width as i32,
            pending.height as i32,
            glow::COLOR_BUFFER_BIT,
            glow::NEAREST,
        );
        if scissor_was_enabled {
            gl.enable(glow::SCISSOR_TEST);
        }
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, previous_read_framebuffer);
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, previous_draw_framebuffer);
        gl.read_buffer(previous_read_buffer as u32);
        gl.draw_buffer(previous_draw_buffer as u32);
        gl.bind_texture(glow::TEXTURE_2D, previous_texture);
    }

    target.play_present_generation = target.play_present_generation.saturating_add(1);
    if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
        eprintln!(
            "play direct gl: delivered texture={} size={}x{} generation={} bottom_left_origin={}",
            target.play_present_texture.0.get(),
            pending.width,
            pending.height,
            target.play_present_generation,
            pending.bottom_left_origin
        );
    }

    Ok(Some(GlTextureFrame {
        texture: target.play_present_texture,
        width: pending.width,
        height: pending.height,
        bottom_left_origin: pending.bottom_left_origin,
        generation: target.play_present_generation,
    }))
}

#[cfg(target_os = "macos")]
pub(super) fn using_private_play_gl_bridge(runtime: &HostRuntime) -> bool {
    let state = runtime.hw_render_state.lock();
    state.using_private_play_gl_context && is_play_core(runtime)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn using_private_play_gl_bridge(_runtime: &HostRuntime) -> bool {
    false
}

#[cfg(target_os = "macos")]
pub(super) fn take_play_macos_iosurface_frame(
    runtime: &HostRuntime,
    pending: PendingHardwareFrame,
) -> Result<Option<MacosIosurfaceFrame>> {
    if !using_private_play_gl_bridge(runtime) || !should_use_play_direct_gl_texture(runtime) {
        return Ok(None);
    }

    let (gl, raw_context) = {
        let state = runtime.hw_render_state.lock();
        let gl = state.frontend_gl_context.clone().ok_or_else(|| {
            anyhow!("Play IOSurface bridge frame requested without a private GL context")
        })?;
        let raw_context = state
            .private_play_gl_context
            .as_ref()
            .map(|context| context.raw_context)
            .ok_or_else(|| {
                anyhow!("Play IOSurface bridge selected but private CGL context is missing")
            })?;
        (gl, raw_context)
    };
    if !make_gl_ctx_current(raw_context) {
        return Err(anyhow!(
            "failed to make private Play CGL context current before IOSurface blit"
        ));
    }

    let mut state = runtime.hw_render_state.lock();
    let Some(target) = state.target.as_mut() else {
        return Ok(None);
    };
    let source_framebuffer = pending.callback_framebuffer.unwrap_or_else(|| {
        if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
            eprintln!("play iosurface bridge: no callback FBO; using host render target FBO");
        }
        target.framebuffer
    });
    if !unsafe { gl.is_framebuffer(source_framebuffer) } {
        return Ok(None);
    }

    ensure_macos_iosurface_targets(&gl, target, pending.width, pending.height)?;

    let surface_index = target.play_iosurface_index % target.play_iosurface_targets.len();
    target.play_iosurface_index = (surface_index + 1) % target.play_iosurface_targets.len();
    let draw_framebuffer = target.play_iosurface_targets[surface_index].framebuffer;

    let previous_read_framebuffer = unsafe { gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING) };
    let previous_draw_framebuffer = unsafe { gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING) };
    let previous_read_buffer = unsafe { gl.get_parameter_i32(glow::READ_BUFFER) };
    let previous_draw_buffer = unsafe { gl.get_parameter_i32(glow::DRAW_BUFFER) };
    let scissor_was_enabled = unsafe { gl.is_enabled(glow::SCISSOR_TEST) };
    let previous_read_framebuffer =
        NonZeroU32::new(previous_read_framebuffer as u32).map(glow::NativeFramebuffer);
    let previous_draw_framebuffer =
        NonZeroU32::new(previous_draw_framebuffer as u32).map(glow::NativeFramebuffer);

    unsafe {
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(source_framebuffer));
        gl.read_buffer(glow::COLOR_ATTACHMENT0);
        let read_status = gl.check_framebuffer_status(glow::READ_FRAMEBUFFER);
        if read_status != glow::FRAMEBUFFER_COMPLETE {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, previous_read_framebuffer);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, previous_draw_framebuffer);
            gl.read_buffer(previous_read_buffer as u32);
            gl.draw_buffer(previous_draw_buffer as u32);
            return Ok(None);
        }

        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(draw_framebuffer));
        gl.draw_buffer(glow::COLOR_ATTACHMENT0);
        let draw_status = gl.check_framebuffer_status(glow::DRAW_FRAMEBUFFER);
        if draw_status != glow::FRAMEBUFFER_COMPLETE {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, previous_read_framebuffer);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, previous_draw_framebuffer);
            gl.read_buffer(previous_read_buffer as u32);
            gl.draw_buffer(previous_draw_buffer as u32);
            return Err(anyhow!(
                "Play IOSurface bridge framebuffer incomplete (status=0x{draw_status:04x})"
            ));
        }

        if scissor_was_enabled {
            gl.disable(glow::SCISSOR_TEST);
        }
        gl.blit_framebuffer(
            0,
            0,
            pending.width as i32,
            pending.height as i32,
            0,
            0,
            pending.width as i32,
            pending.height as i32,
            glow::COLOR_BUFFER_BIT,
            glow::NEAREST,
        );
        if scissor_was_enabled {
            gl.enable(glow::SCISSOR_TEST);
        }
        gl.finish();
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, previous_read_framebuffer);
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, previous_draw_framebuffer);
        gl.read_buffer(previous_read_buffer as u32);
        gl.draw_buffer(previous_draw_buffer as u32);
    }

    target.play_iosurface_generation = target.play_iosurface_generation.saturating_add(1);
    let surface = target.play_iosurface_targets[surface_index].surface.clone();
    if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
        eprintln!(
            "play iosurface bridge: delivered surface={:?} size={}x{} generation={} bottom_left_origin={}",
            surface.as_ptr(),
            pending.width,
            pending.height,
            target.play_iosurface_generation,
            pending.bottom_left_origin
        );
    }

    Ok(Some(MacosIosurfaceFrame {
        surface,
        width: pending.width,
        height: pending.height,
        bottom_left_origin: pending.bottom_left_origin,
        generation: target.play_iosurface_generation,
    }))
}

#[cfg(target_os = "macos")]
fn ensure_macos_iosurface_targets(
    gl: &glow::Context,
    target: &mut HardwareRenderTarget,
    width: u32,
    height: u32,
) -> Result<()> {
    if target.play_iosurface_targets.len() == MACOS_IOSURFACE_RING_SIZE
        && target
            .play_iosurface_targets
            .iter()
            .all(|surface| surface.width == width && surface.height == height)
    {
        return Ok(());
    }

    unsafe {
        for existing in target.play_iosurface_targets.drain(..) {
            gl.delete_texture(existing.texture);
            gl.delete_framebuffer(existing.framebuffer);
        }
    }
    target.play_iosurface_index = 0;
    target.play_iosurface_generation = 0;

    for _ in 0..MACOS_IOSURFACE_RING_SIZE {
        target
            .play_iosurface_targets
            .push(create_macos_iosurface_target(gl, width, height)?);
    }
    info!(
        target: "arcade_libretro::core_loader",
        "Play IOSurface bridge targets initialized ring={} size={}x{}",
        MACOS_IOSURFACE_RING_SIZE,
        width,
        height
    );
    Ok(())
}

#[cfg(target_os = "macos")]
fn create_macos_iosurface_target(
    gl: &glow::Context,
    width: u32,
    height: u32,
) -> Result<MacosPlayIosurfaceTarget> {
    let surface = unsafe {
        let properties: *mut Object = msg_send![class!(NSMutableDictionary), dictionary];
        let width_value: *mut Object =
            msg_send![class!(NSNumber), numberWithUnsignedLongLong: width as u64];
        let height_value: *mut Object =
            msg_send![class!(NSNumber), numberWithUnsignedLongLong: height as u64];
        let bytes_per_element_value: *mut Object =
            msg_send![class!(NSNumber), numberWithUnsignedLongLong: 4_u64];
        let bgra = u32::from_be_bytes(*b"BGRA");
        let pixel_format_value: *mut Object =
            msg_send![class!(NSNumber), numberWithUnsignedInt: bgra];
        let _: () = msg_send![properties, setObject: width_value forKey: kIOSurfaceWidth];
        let _: () = msg_send![properties, setObject: height_value forKey: kIOSurfaceHeight];
        let _: () =
            msg_send![properties, setObject: pixel_format_value forKey: kIOSurfacePixelFormat];
        let _: () = msg_send![properties, setObject: bytes_per_element_value forKey: kIOSurfaceBytesPerElement];
        let surface = IOSurfaceCreate(properties.cast_const().cast());
        if surface.is_null() {
            return Err(anyhow!(
                "IOSurfaceCreate failed for Play bridge target {}x{}",
                width,
                height
            ));
        }
        MacosIosurfaceHandle::from_retained(surface)
    };

    let texture = unsafe { gl.create_texture() }
        .map_err(|err| anyhow!("failed to create Play IOSurface GL texture: {err}"))?;
    let framebuffer = unsafe { gl.create_framebuffer() }
        .map_err(|err| anyhow!("failed to create Play IOSurface GL framebuffer: {err}"))?;

    unsafe {
        let previous_texture =
            NonZeroU32::new(gl.get_parameter_i32(MACOS_GL_TEXTURE_BINDING_RECTANGLE) as u32)
                .map(glow::NativeTexture);
        let previous_framebuffer =
            NonZeroU32::new(gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING) as u32)
                .map(glow::NativeFramebuffer);

        gl.bind_texture(MACOS_GL_TEXTURE_RECTANGLE, Some(texture));
        gl.tex_parameter_i32(
            MACOS_GL_TEXTURE_RECTANGLE,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            MACOS_GL_TEXTURE_RECTANGLE,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            MACOS_GL_TEXTURE_RECTANGLE,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            MACOS_GL_TEXTURE_RECTANGLE,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        let cgl_context = CGLGetCurrentContext();
        let bind_status = CGLTexImageIOSurface2D(
            cgl_context,
            MACOS_GL_TEXTURE_RECTANGLE,
            glow::RGBA8,
            width as usize,
            height as usize,
            glow::BGRA,
            glow::UNSIGNED_INT_8_8_8_8_REV,
            surface.as_ptr(),
            0,
        );
        if bind_status != 0 {
            gl.delete_texture(texture);
            gl.delete_framebuffer(framebuffer);
            return Err(anyhow!(
                "CGLTexImageIOSurface2D failed for Play bridge target (status={bind_status})"
            ));
        }

        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            MACOS_GL_TEXTURE_RECTANGLE,
            Some(texture),
            0,
        );
        gl.draw_buffer(glow::COLOR_ATTACHMENT0);
        gl.read_buffer(glow::COLOR_ATTACHMENT0);
        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);

        gl.bind_framebuffer(glow::FRAMEBUFFER, previous_framebuffer);
        gl.bind_texture(MACOS_GL_TEXTURE_RECTANGLE, previous_texture);

        if status != glow::FRAMEBUFFER_COMPLETE {
            gl.delete_texture(texture);
            gl.delete_framebuffer(framebuffer);
            return Err(anyhow!(
                "Play IOSurface GL framebuffer incomplete (status=0x{status:04x})"
            ));
        }
    }

    Ok(MacosPlayIosurfaceTarget {
        surface,
        texture,
        framebuffer,
        width,
        height,
    })
}

const PLAY_TEXTURE_REVALIDATE_INTERVAL_FRAMES: u64 = 120;
const PLAY_TEXTURE_SWITCH_COOLDOWN_FRAMES: u64 = 90;
const PLAY_TEXTURE_STALE_RESCAN_STREAK: u32 = 24;
const PLAY_TEXTURE_STALE_INVALIDATE_STREAK: u32 = 64;
const PLAY_TEXTURE_SWITCH_SCORE_MARGIN: f32 = 0.22;
const PLAY_TEXTURE_EXACT_SCORE_EPSILON: f32 = 0.001;

fn play_texture_u64_env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn play_texture_f32_env(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(default)
}

fn play_texture_debug_enabled() -> bool {
    std::env::var_os("ARCADE_PLAY_GL_TEXTURE_DEBUG").is_some()
        || std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some()
}

fn play_texture_revalidate_interval_frames() -> u64 {
    play_texture_u64_env(
        "ARCADE_PLAY_GL_REVALIDATE_INTERVAL_FRAMES",
        PLAY_TEXTURE_REVALIDATE_INTERVAL_FRAMES,
    )
}

fn play_texture_switch_cooldown_frames() -> u64 {
    play_texture_u64_env(
        "ARCADE_PLAY_GL_SWITCH_COOLDOWN_FRAMES",
        PLAY_TEXTURE_SWITCH_COOLDOWN_FRAMES,
    )
}

fn play_texture_switch_score_margin() -> f32 {
    play_texture_f32_env(
        "ARCADE_PLAY_GL_SWITCH_SCORE_MARGIN",
        PLAY_TEXTURE_SWITCH_SCORE_MARGIN,
    )
}

fn play_texture_score_is_exact(score: f32) -> bool {
    score.is_finite() && score.abs() <= PLAY_TEXTURE_EXACT_SCORE_EPSILON
}

fn read_play_callback_framebuffer(
    gl: &glow::Context,
    pending: PendingHardwareFrame,
) -> Option<Vec<u8>> {
    let framebuffer = pending.callback_framebuffer?;
    if !unsafe { gl.is_framebuffer(framebuffer) } {
        return None;
    }

    let previous_binding = unsafe { gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING) };
    let previous_framebuffer =
        NonZeroU32::new(previous_binding as u32).map(glow::NativeFramebuffer);
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
    }
    let status = unsafe { gl.check_framebuffer_status(glow::FRAMEBUFFER) };
    let pixels = if status == glow::FRAMEBUFFER_COMPLETE {
        let pixels = read_gl_framebuffer_rgba(gl, Some(framebuffer), pending.width, pending.height);
        let non_black = sampled_non_black_pixels(&pixels, pending.width, pending.height);
        if non_black > 0 {
            if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
                let (_, _, first_rgba, center_rgba) =
                    summarize_rgba_debug_pixels_grid(&pixels, pending.width, pending.height);
                eprintln!(
                    "play gl framebuffer: using callback FBO {} non_black_samples={non_black}/64 first_rgba={:02x},{:02x},{:02x},{:02x} center_rgba={:02x},{:02x},{:02x},{:02x}",
                    framebuffer.0.get(),
                    first_rgba[0],
                    first_rgba[1],
                    first_rgba[2],
                    first_rgba[3],
                    center_rgba[0],
                    center_rgba[1],
                    center_rgba[2],
                    center_rgba[3],
                );
            }
            Some(pixels)
        } else {
            if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
                eprintln!(
                    "play gl framebuffer: callback FBO {} is black, falling back",
                    framebuffer.0.get()
                );
            }
            None
        }
    } else {
        if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
            eprintln!(
                "play gl framebuffer: callback FBO {} incomplete status=0x{status:x}, falling back",
                framebuffer.0.get()
            );
        }
        None
    };
    unsafe { gl.bind_framebuffer(glow::FRAMEBUFFER, previous_framebuffer) };
    pixels
}

fn play_texture_signature_from_rgba(
    pixels: &[u8],
    width: u32,
    height: u32,
) -> PlayTextureSignature {
    let (checksum, non_black_samples, first_rgba, center_rgba) =
        summarize_rgba_debug_pixels_grid(pixels, width, height);

    let mut min_luma = u8::MAX;
    let mut max_luma = u8::MIN;
    let mut min_r = u8::MAX;
    let mut max_r = u8::MIN;
    let mut min_g = u8::MAX;
    let mut max_g = u8::MIN;
    let mut min_b = u8::MAX;
    let mut max_b = u8::MIN;

    let sample_width = width.min(8);
    let sample_height = height.min(8);
    if sample_width == 0 || sample_height == 0 {
        return PlayTextureSignature::default();
    }

    for sample_y in 0..sample_height {
        let y = ((sample_y as usize * height as usize) / sample_height as usize)
            .min(height.saturating_sub(1) as usize);
        for sample_x in 0..sample_width {
            let x = ((sample_x as usize * width as usize) / sample_width as usize)
                .min(width.saturating_sub(1) as usize);
            let offset = (y * width as usize + x) * 4;
            if offset + 4 > pixels.len() {
                continue;
            }
            let r = pixels[offset];
            let g = pixels[offset + 1];
            let b = pixels[offset + 2];
            let luma = ((u16::from(r) + u16::from(g) + u16::from(b)) / 3) as u8;
            min_luma = min_luma.min(luma);
            max_luma = max_luma.max(luma);
            min_r = min_r.min(r);
            max_r = max_r.max(r);
            min_g = min_g.min(g);
            max_g = max_g.max(g);
            min_b = min_b.min(b);
            max_b = max_b.max(b);
        }
    }

    let uniform = max_r.saturating_sub(min_r) <= 3
        && max_g.saturating_sub(min_g) <= 3
        && max_b.saturating_sub(min_b) <= 3;

    PlayTextureSignature {
        checksum,
        non_black_samples: non_black_samples.min(u16::MAX as usize) as u16,
        luminance_range: u16::from(max_luma.saturating_sub(min_luma)),
        uniform,
        first_rgb: [first_rgba[0], first_rgba[1], first_rgba[2]],
        center_rgb: [center_rgba[0], center_rgba[1], center_rgba[2]],
    }
}

fn color_triplet_close(lhs: [u8; 3], rhs: [u8; 3], tolerance: u8) -> bool {
    lhs.into_iter()
        .zip(rhs)
        .all(|(left, right)| left.abs_diff(right) <= tolerance)
}

fn next_play_stale_signature_streak(
    previous: Option<PlayTextureSignature>,
    current: PlayTextureSignature,
    previous_streak: u32,
) -> u32 {
    if current.non_black_samples == 0 {
        return previous_streak.saturating_add(1);
    }

    let mut near_static = current.uniform;
    if let Some(previous) = previous {
        let non_black_close = previous
            .non_black_samples
            .abs_diff(current.non_black_samples)
            <= 2;
        let luminance_close = previous.luminance_range.abs_diff(current.luminance_range) <= 3;
        let first_close = color_triplet_close(previous.first_rgb, current.first_rgb, 4);
        let center_close = color_triplet_close(previous.center_rgb, current.center_rgb, 4);
        near_static = near_static
            || previous.checksum == current.checksum
            || (non_black_close && luminance_close && first_close && center_close);
    }

    if near_static {
        previous_streak.saturating_add(1)
    } else {
        0
    }
}

fn should_run_play_periodic_rescan(
    frame_counter: u64,
    last_scan_frame: u64,
    last_switch_frame: u64,
) -> bool {
    let revalidate_interval = play_texture_revalidate_interval_frames();
    let switch_cooldown = play_texture_switch_cooldown_frames();
    frame_counter.saturating_sub(last_scan_frame) >= revalidate_interval
        && frame_counter.saturating_sub(last_switch_frame) >= switch_cooldown
}

fn should_switch_play_texture_candidate(
    force_due_to_stale: bool,
    current_score: f32,
    candidate_score: f32,
    frame_counter: u64,
    last_switch_frame: u64,
) -> bool {
    let switch_cooldown = play_texture_switch_cooldown_frames();
    let score_margin = play_texture_switch_score_margin();
    if force_due_to_stale {
        return true;
    }
    if !candidate_score.is_finite() {
        return false;
    }
    if !current_score.is_finite() {
        return true;
    }
    if play_texture_score_is_exact(candidate_score) && !play_texture_score_is_exact(current_score) {
        return true;
    }
    if frame_counter.saturating_sub(last_switch_frame) < switch_cooldown {
        return false;
    }
    candidate_score + score_margin < current_score
}

pub(super) fn read_opengl_render_frame(
    runtime: &HostRuntime,
    pending: PendingHardwareFrame,
) -> Result<FrameBuffer> {
    const PLAY_TEXTURE_SCAN_WARMUP_FRAMES: u32 = 90;

    let is_play = is_play_core(runtime);
    let strict_texture_match = should_use_strict_gl_texture_match(runtime);
    let allow_default_fallback = should_allow_default_gl_fallback(runtime);
    let allow_generic_texture_scan = should_allow_generic_gl_texture_scan(runtime);
    let (
        gl,
        framebuffer,
        emu_ctx_framebuffer,
        color_texture,
        mut emu_game_texture,
        mut play_blank_frame_streak,
        mut play_texture_cache_score,
        mut play_texture_frame_counter,
        mut play_texture_last_signature,
        mut play_texture_stale_signature_streak,
        mut play_texture_last_scan_frame,
        mut play_texture_last_switch_frame,
    ) = {
        let state = runtime.hw_render_state.lock();
        let Some(gl) = state.frontend_gl_context.clone() else {
            return Err(anyhow!(
                "hardware-render frame requested without an active GL context"
            ));
        };
        let (
            framebuffer,
            emu_ctx_framebuffer,
            color_texture,
            emu_game_texture,
            play_blank_frame_streak,
            play_texture_cache_score,
            play_texture_frame_counter,
            play_texture_last_signature,
            play_texture_stale_signature_streak,
            play_texture_last_scan_frame,
            play_texture_last_switch_frame,
        ) = if force_default_gl_framebuffer(runtime) {
            let target = state.target.as_ref();
            (
                None,
                None,
                None,
                target.and_then(|target| target.emu_game_texture),
                target
                    .map(|target| target.play_blank_frame_streak)
                    .unwrap_or(0),
                target
                    .map(|target| target.play_texture_cache_score)
                    .unwrap_or(f32::INFINITY),
                target
                    .map(|target| target.play_texture_frame_counter)
                    .unwrap_or(0),
                target.and_then(|target| target.play_texture_last_signature),
                target
                    .map(|target| target.play_texture_stale_signature_streak)
                    .unwrap_or(0),
                target
                    .map(|target| target.play_texture_last_scan_frame)
                    .unwrap_or(0),
                target
                    .map(|target| target.play_texture_last_switch_frame)
                    .unwrap_or(0),
            )
        } else {
            let Some(target) = state.target.as_ref() else {
                return Err(anyhow!(
                    "hardware-render frame requested before framebuffer setup"
                ));
            };
            (
                Some(target.framebuffer),
                target.emu_ctx_framebuffer,
                Some(target.color_texture),
                target.emu_game_texture,
                target.play_blank_frame_streak,
                target.play_texture_cache_score,
                target.play_texture_frame_counter,
                target.play_texture_last_signature,
                target.play_texture_stale_signature_streak,
                target.play_texture_last_scan_frame,
                target.play_texture_last_switch_frame,
            )
        };
        (
            gl,
            framebuffer,
            emu_ctx_framebuffer,
            color_texture,
            emu_game_texture,
            play_blank_frame_streak,
            play_texture_cache_score,
            play_texture_frame_counter,
            play_texture_last_signature,
            play_texture_stale_signature_streak,
            play_texture_last_scan_frame,
            play_texture_last_switch_frame,
        )
    };
    if is_play {
        play_texture_frame_counter = play_texture_frame_counter.saturating_add(1);
    }
    let play_debug = is_play && play_texture_debug_enabled();
    let mut play_force_texture_scan = false;
    let mut play_force_invalidate_cached_texture = false;
    let mut play_scan_reason: Option<&'static str> = None;
    let mut play_cached_fast_path_pixels: Option<Vec<u8>> = None;

    if is_play {
        if let Some(pixels) = read_play_callback_framebuffer(&gl, pending) {
            emu_game_texture = None;
            play_texture_cache_score = f32::INFINITY;
            play_blank_frame_streak = 0;
            play_cached_fast_path_pixels = Some(pixels);
        }
    }

    // Fast path: if a previous scan found the core's game-frame texture (visible here because
    // the core uses a shared GL context), read from it directly via a temporary FBO.
    if let Some(game_tex) = emu_game_texture.filter(|_| play_cached_fast_path_pixels.is_none()) {
        let mut pixels = vec![0_u8; pending.width as usize * pending.height as usize * 4];
        if let Ok(temp_fbo) = unsafe { gl.create_framebuffer() } {
            let mut fbo_complete = false;
            unsafe {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(temp_fbo));
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(game_tex),
                    0,
                );
                if gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE {
                    fbo_complete = true;
                    gl.read_buffer(glow::COLOR_ATTACHMENT0);
                    gl.read_pixels(
                        0,
                        0,
                        pending.width as i32,
                        pending.height as i32,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelPackData::Slice(Some(&mut pixels)),
                    );
                }
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                gl.delete_framebuffer(temp_fbo);
            }
            // Return whenever the FBO is complete — even if the first 64 pixels are dark
            // (e.g. letterbox / sky row).  The non_black guard was too conservative and
            // caused the fast path to fall through on frames where the bottom row is black.
            if fbo_complete {
                if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
                    static FAST_PATH_LOGGED: std::sync::atomic::AtomicBool =
                        std::sync::atomic::AtomicBool::new(false);
                    if !FAST_PATH_LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                        // Sample pixels at every 10% of height to show where content lives.
                        let w = pending.width as usize;
                        let h = pending.height as usize;
                        for frac in [0, 10, 25, 50, 75, 90, 100_usize] {
                            let y = ((h - 1) * frac / 100).min(h - 1);
                            let cx = w / 2;
                            let off = (y * w + cx) * 4;
                            if off + 3 < pixels.len() {
                                let p = &pixels[off..off + 4];
                                eprintln!(
                                    "  fast_path_sample y={y} (gl, {frac}%) cx={cx} rgba={:02x},{:02x},{:02x},{:02x}",
                                    p[0], p[1], p[2], p[3]
                                );
                            }
                        }
                        let total_non_black = pixels
                            .chunks_exact(4)
                            .filter(|p| p[0] != 0 || p[1] != 0 || p[2] != 0)
                            .count();
                        let total = pixels.len() / 4;
                        eprintln!(
                            "  fast_path total_non_black={total_non_black}/{total} ({:.1}%)",
                            total_non_black as f64 / total as f64 * 100.0
                        );
                    }
                }
                if pending.bottom_left_origin {
                    let row_len = pending.width as usize * 4;
                    let mut flipped =
                        vec![0_u8; pending.width as usize * pending.height as usize * 4];
                    for row in 0..pending.height as usize {
                        let src_row = pending.height as usize - 1 - row;
                        flipped[row * row_len..(row + 1) * row_len]
                            .copy_from_slice(&pixels[src_row * row_len..(src_row + 1) * row_len]);
                    }
                    pixels = flipped;
                }
                // Force alpha: GL render textures commonly leave alpha undefined or zero.
                // egui's from_rgba_unmultiplied premultiplies by alpha, so alpha=0 pixels
                // would be rendered as transparent black even when RGB is non-zero.
                for pixel in pixels.chunks_exact_mut(4) {
                    pixel[3] = 255;
                }
                if !is_play {
                    return Ok(FrameBuffer {
                        width: pending.width,
                        height: pending.height,
                        pitch: pending.width as usize * 4,
                        data: pixels,
                        pixel_format: PixelFormat::Rgba8888,
                    });
                }

                let signature =
                    play_texture_signature_from_rgba(&pixels, pending.width, pending.height);
                let stale_signature_streak = next_play_stale_signature_streak(
                    play_texture_last_signature,
                    signature,
                    play_texture_stale_signature_streak,
                );
                let periodic_rescan = should_run_play_periodic_rescan(
                    play_texture_frame_counter,
                    play_texture_last_scan_frame,
                    play_texture_last_switch_frame,
                );
                let stale_rescan = stale_signature_streak >= PLAY_TEXTURE_STALE_RESCAN_STREAK;
                let stale_invalidate =
                    stale_signature_streak >= PLAY_TEXTURE_STALE_INVALIDATE_STREAK;
                play_blank_frame_streak = if signature.non_black_samples > 0 {
                    0
                } else {
                    play_blank_frame_streak.saturating_add(1)
                };

                if stale_invalidate {
                    play_force_invalidate_cached_texture = true;
                    play_scan_reason = Some("stale-signature");
                } else if stale_rescan {
                    play_scan_reason = Some("stale-signature");
                } else if periodic_rescan {
                    play_scan_reason = Some("periodic");
                }
                play_force_texture_scan = stale_rescan || stale_invalidate || periodic_rescan;

                if !play_force_texture_scan {
                    play_texture_last_signature = Some(signature);
                    play_texture_stale_signature_streak = stale_signature_streak;
                    if let Some(target) = runtime.hw_render_state.lock().target.as_mut() {
                        target.emu_game_texture = emu_game_texture;
                        target.play_blank_frame_streak = play_blank_frame_streak;
                        target.play_texture_cache_score = play_texture_cache_score;
                        target.play_texture_frame_counter = play_texture_frame_counter;
                        target.play_texture_last_signature = play_texture_last_signature;
                        target.play_texture_stale_signature_streak =
                            play_texture_stale_signature_streak;
                        target.play_texture_last_scan_frame = play_texture_last_scan_frame;
                        target.play_texture_last_switch_frame = play_texture_last_switch_frame;
                    }
                    if play_debug {
                        eprintln!(
                            "play gl texture: kept cached tex={} streak={} frame={}",
                            game_tex.0.get(),
                            stale_signature_streak,
                            play_texture_frame_counter
                        );
                    }
                    return Ok(FrameBuffer {
                        width: pending.width,
                        height: pending.height,
                        pitch: pending.width as usize * 4,
                        data: pixels,
                        pixel_format: PixelFormat::Rgba8888,
                    });
                }

                if play_force_invalidate_cached_texture {
                    emu_game_texture = None;
                    play_texture_cache_score = f32::INFINITY;
                }
                if play_debug {
                    eprintln!(
                        "play gl texture: revalidate cached tex={} reason={} stale_streak={} frame={}",
                        game_tex.0.get(),
                        play_scan_reason.unwrap_or("unknown"),
                        stale_signature_streak,
                        play_texture_frame_counter
                    );
                }
                play_cached_fast_path_pixels = Some(pixels);
            }
        } else if is_play {
            play_force_texture_scan = true;
            play_force_invalidate_cached_texture = true;
            play_scan_reason = Some("cached-fbo-incomplete");
            emu_game_texture = None;
            play_texture_cache_score = f32::INFINITY;
            if play_debug {
                eprintln!(
                    "play gl texture: invalidated cached tex={} reason=cached-fbo-incomplete frame={}",
                    game_tex.0.get(),
                    play_texture_frame_counter
                );
            }
        }
        // FBO was incomplete (texture deleted/invalidated) — fall through to normal path.
    }

    let read_framebuffer = |framebuffer: Option<glow::Framebuffer>| -> Vec<u8> {
        read_gl_framebuffer_rgba(&gl, framebuffer, pending.width, pending.height)
    };

    // Flush pending GPU work so read_pixels captures the completed frame.
    unsafe { gl.finish() };

    // Capture what the core left bound after retro_run() — some cores (e.g. mupen64plus-next
    // + GLideN64) render into their own internal FBO rather than the one returned by
    // get_current_framebuffer(), and leave that FBO bound when they call retro_video_refresh.
    let raw_fbo_binding = unsafe { gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING) };
    let core_bound_framebuffer =
        NonZeroU32::new(raw_fbo_binding as u32).map(glow::NativeFramebuffer);

    if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
        let custom_fbo_id = framebuffer.map(|f| f.0.get()).unwrap_or(0);
        eprintln!(
            "gl readback fbo_on_entry={raw_fbo_binding} custom_fbo={custom_fbo_id} thread={:?}",
            std::thread::current().id(),
        );
    }

    let mut pixels = play_cached_fast_path_pixels
        .take()
        .unwrap_or_else(|| read_framebuffer(framebuffer));

    // Restore whatever the core had bound (read_framebuffer changes the binding).
    unsafe { gl.bind_framebuffer(glow::FRAMEBUFFER, core_bound_framebuffer) };

    let mut non_black_pixels = sampled_non_black_pixels(&pixels, pending.width, pending.height);

    // If our dedicated FBO is empty, try the core's last-bound FBO (its internal render target).
    if non_black_pixels == 0 {
        if let Some(core_fbo) = core_bound_framebuffer {
            if Some(core_fbo) != framebuffer {
                let candidate = read_framebuffer(Some(core_fbo));
                unsafe { gl.bind_framebuffer(glow::FRAMEBUFFER, core_bound_framebuffer) };
                let candidate_non_black =
                    sampled_non_black_pixels(&candidate, pending.width, pending.height);
                if candidate_non_black > 0 {
                    if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
                        eprintln!(
                            "gl readback switching to core-bound FBO {} non_black_samples={candidate_non_black}/64",
                            core_fbo.0.get()
                        );
                    }
                    pixels = candidate;
                }
            }
        }
    }

    non_black_pixels = sampled_non_black_pixels(&pixels, pending.width, pending.height);
    if is_play {
        play_blank_frame_streak = if non_black_pixels > 0 {
            0
        } else {
            play_blank_frame_streak.saturating_add(1)
        };
    }
    let allow_texture_scan = !is_play || play_blank_frame_streak >= PLAY_TEXTURE_SCAN_WARMUP_FRAMES;
    let force_play_texture_scan = is_play && play_force_texture_scan;
    if force_play_texture_scan {
        play_texture_last_scan_frame = play_texture_frame_counter;
    }
    let should_texture_scan = force_play_texture_scan
        || (allow_generic_texture_scan
            && allow_texture_scan
            && non_black_pixels == 0
            && emu_game_texture.is_none());

    // Texture scan: when the dedicated FBO (texture 33) is empty and the core hasn't left a
    // useful FBO bound, scan visible texture handles for game content.  When mupen64plus-next
    // + GLideN64 uses a *shared* GL context (FBOs not shared, textures ARE shared), its
    // render textures are visible from eframe's context.  We find the one with game content,
    // cache it in target.emu_game_texture, and return it.  On subsequent frames the fast path
    // at the top of this function uses the cached handle directly, skipping the scan.
    if should_texture_scan {
        static TEXTURE_SCAN_DONE: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        let scan_first_time = !TEXTURE_SCAN_DONE.swap(true, std::sync::atomic::Ordering::Relaxed);

        let known_tex_ids: [u32; 4] = [color_texture.map(|t| t.0.get()).unwrap_or(0), 35, 40, 41];

        let trace = std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some();
        if trace && scan_first_time {
            eprintln!("gl readback texture scan (shared-context, first frame only):");
        } else if play_debug && force_play_texture_scan {
            eprintln!(
                "play gl texture: scanning candidates reason={} frame={} current_cached={} current_score={:.3}",
                play_scan_reason.unwrap_or("unknown"),
                play_texture_frame_counter,
                emu_game_texture.map(|texture| texture.0.get()).unwrap_or(0),
                play_texture_cache_score
            );
        }

        let mut best_tex: Option<glow::NativeTexture> = None;
        let mut best_pixels: Option<Vec<u8>> = None;
        let mut best_score = f32::INFINITY;
        let pending_aspect = pending.width as f32 / pending.height.max(1) as f32;

        for tex_id in 1u32..=256 {
            if known_tex_ids.contains(&tex_id) {
                continue;
            }
            let Some(tex_nz) = NonZeroU32::new(tex_id) else {
                continue;
            };
            let tex = glow::NativeTexture(tex_nz);
            if !unsafe { gl.is_texture(tex) } {
                continue;
            }
            // Only match textures whose dimensions equal the expected output size.  Small
            // utility / noise / cache textures will be skipped here, avoiding false positives
            // where the first 64 pixels of a tiny texture happen to be non-black.
            // glow 0.16 doesn't expose glGetTexLevelParameteriv through its trait, so on macOS
            // (which already links OpenGL.framework via build.rs) we call the C symbol directly.
            #[cfg(target_os = "macos")]
            let (tex_w, tex_h) = {
                extern "C" {
                    fn glGetTexLevelParameteriv(
                        target: u32,
                        level: i32,
                        pname: u32,
                        params: *mut i32,
                    );
                }
                let mut w = 0i32;
                let mut h = 0i32;
                unsafe {
                    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    glGetTexLevelParameteriv(glow::TEXTURE_2D, 0, glow::TEXTURE_WIDTH, &mut w);
                    glGetTexLevelParameteriv(glow::TEXTURE_2D, 0, glow::TEXTURE_HEIGHT, &mut h);
                    gl.bind_texture(glow::TEXTURE_2D, None);
                }
                (w as u32, h as u32)
            };
            #[cfg(not(target_os = "macos"))]
            let (tex_w, tex_h) = (pending.width, pending.height);
            if trace && scan_first_time && (tex_w > 0 || tex_h > 0) {
                eprintln!("  tex={tex_id} size={tex_w}x{tex_h}");
            }
            // Default behavior keeps a strict width match. For play, allow larger backing
            // render targets so we can pick the real scene texture on macOS GL.
            if strict_texture_match {
                // Accept textures whose width matches exactly and height is at least the output
                // height. GLideN64 often uses a slightly oversized render texture
                // (e.g. 640×580 for a 640×480 output); the extra rows are padding/unused.
                if tex_w != pending.width || tex_h < pending.height {
                    continue;
                }
            } else {
                if tex_w < pending.width || tex_h < pending.height {
                    continue;
                }
                let width_ratio = tex_w as f32 / pending.width.max(1) as f32;
                let height_ratio = tex_h as f32 / pending.height.max(1) as f32;
                if width_ratio > 3.0 || height_ratio > 3.0 {
                    continue;
                }
            }
            // Attach to an FBO and read the full frame directly.  Skipping the 8×8 corner
            // probe avoids false-rejects when the game uses a letterbox or dark edge at GL(0,0).
            // The uniform-color check below is the real gate.
            let fbo = match unsafe { gl.create_framebuffer() } {
                Ok(f) => f,
                Err(_) => continue,
            };
            let mut full_pixels = vec![0_u8; pending.width as usize * pending.height as usize * 4];
            let fbo_ok = unsafe {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(tex),
                    0,
                );
                let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
                let ok = status == glow::FRAMEBUFFER_COMPLETE;
                if ok {
                    gl.read_buffer(glow::COLOR_ATTACHMENT0);
                    gl.read_pixels(
                        0,
                        0,
                        pending.width as i32,
                        pending.height as i32,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelPackData::Slice(Some(&mut full_pixels)),
                    );
                } else if trace && scan_first_time {
                    eprintln!("  tex={tex_id} fbo_incomplete=0x{status:x}");
                }
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                gl.delete_framebuffer(fbo);
                ok
            };
            if fbo_ok {
                let full_non_black =
                    sampled_non_black_pixels(&full_pixels, pending.width, pending.height);
                // Reject uniform-color textures (all pixels same RGB) — these are solid
                // clear targets (e.g. eframe's background color) rather than game renders.
                let is_uniform = full_pixels.chunks_exact(4).all(|p| {
                    p[0] == full_pixels[0] && p[1] == full_pixels[1] && p[2] == full_pixels[2]
                });
                if trace && scan_first_time {
                    eprintln!(
                        "  tex={tex_id} non_black={full_non_black} uniform={is_uniform} first=({},{},{},{})",
                        full_pixels[0], full_pixels[1], full_pixels[2], full_pixels[3]
                    );
                }
                if full_non_black > 0 && !is_uniform {
                    let tex_aspect = tex_w as f32 / tex_h.max(1) as f32;
                    let aspect_penalty = (tex_aspect - pending_aspect).abs() * 4.0;
                    let width_ratio = tex_w.max(1) as f32 / pending.width.max(1) as f32;
                    let height_ratio = tex_h.max(1) as f32 / pending.height.max(1) as f32;
                    let size_penalty = width_ratio.ln().abs() + height_ratio.ln().abs();
                    let score = aspect_penalty + size_penalty;
                    if trace {
                        eprintln!(
                            "gl readback texture scan candidate tex={tex_id} non_black={full_non_black} score={score:.3} tex={}x{} pending={}x{}",
                            tex_w,
                            tex_h,
                            pending.width,
                            pending.height,
                        );
                    }
                    if score < best_score {
                        best_score = score;
                        best_tex = Some(tex);
                        best_pixels = Some(full_pixels);
                    }
                }
            }
        }
        unsafe { gl.bind_framebuffer(glow::FRAMEBUFFER, core_bound_framebuffer) };

        if let (Some(game_tex), Some(game_pixels)) = (best_tex, best_pixels) {
            if trace {
                eprintln!(
                    "gl readback texture scan selected tex={} score={best_score:.3}",
                    game_tex.0.get()
                );
            }
            if !is_play {
                emu_game_texture = Some(game_tex);
                pixels = game_pixels;
            } else {
                let current_cached_texture = emu_game_texture;
                let force_due_to_stale = play_force_invalidate_cached_texture;
                let should_accept = if current_cached_texture.is_none()
                    || current_cached_texture == Some(game_tex)
                {
                    true
                } else {
                    should_switch_play_texture_candidate(
                        force_due_to_stale,
                        play_texture_cache_score,
                        best_score,
                        play_texture_frame_counter,
                        play_texture_last_switch_frame,
                    )
                };
                if should_accept {
                    let switched_texture = current_cached_texture != Some(game_tex);
                    emu_game_texture = Some(game_tex);
                    play_texture_cache_score = best_score;
                    pixels = game_pixels;
                    if switched_texture {
                        play_texture_last_switch_frame = play_texture_frame_counter;
                        play_texture_last_signature = None;
                        play_texture_stale_signature_streak = 0;
                    }
                    if play_debug {
                        let action = if switched_texture {
                            "switched texture"
                        } else {
                            "kept cached"
                        };
                        eprintln!(
                            "play gl texture: {} tex={} score={:.3} frame={}",
                            action,
                            game_tex.0.get(),
                            best_score,
                            play_texture_frame_counter
                        );
                    }
                } else if play_debug {
                    eprintln!(
                        "play gl texture: rescanned, kept cached tex={} cached_score={:.3} candidate_tex={} candidate_score={:.3} frame={}",
                        current_cached_texture.map(|texture| texture.0.get()).unwrap_or(0),
                        play_texture_cache_score,
                        game_tex.0.get(),
                        best_score,
                        play_texture_frame_counter
                    );
                }
            }
        } else if play_debug && force_play_texture_scan {
            eprintln!(
                "play gl texture: rescanned, no candidate reason={} frame={}",
                play_scan_reason.unwrap_or("unknown"),
                play_texture_frame_counter
            );
        }
    } else if non_black_pixels == 0 && emu_game_texture.is_none() {
        let trace = std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some();
        if trace && is_play {
            eprintln!(
                "gl readback: play warmup waiting for core-linked output (blank_streak={play_blank_frame_streak}/{PLAY_TEXTURE_SCAN_WARMUP_FRAMES}), skipping texture scan fallback"
            );
        } else if trace && !allow_generic_texture_scan {
            eprintln!(
                "gl readback: skipping generic texture scan for this core; host framebuffer remains black"
            );
        }
    }

    non_black_pixels = sampled_non_black_pixels(&pixels, pending.width, pending.height);
    if non_black_pixels == 0 && allow_default_fallback {
        let fallback_pixels = read_framebuffer(None);
        unsafe { gl.bind_framebuffer(glow::FRAMEBUFFER, core_bound_framebuffer) };
        let fallback_non_black_pixels =
            sampled_non_black_pixels(&fallback_pixels, pending.width, pending.height);
        if fallback_non_black_pixels > 0 {
            if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
                eprintln!(
                    "gl readback switching to default framebuffer fallback non_black_samples={fallback_non_black_pixels}/64"
                );
            }
            pixels = fallback_pixels;
        }
    } else if non_black_pixels == 0
        && !allow_default_fallback
        && std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some()
    {
        eprintln!("gl readback: skipping default framebuffer fallback for play core");
    }

    let final_non_black = sampled_non_black_pixels(&pixels, pending.width, pending.height);
    if is_play {
        let final_signature =
            play_texture_signature_from_rgba(&pixels, pending.width, pending.height);
        play_texture_stale_signature_streak = next_play_stale_signature_streak(
            play_texture_last_signature,
            final_signature,
            play_texture_stale_signature_streak,
        );
        play_texture_last_signature = Some(final_signature);
        if final_non_black > 0 {
            play_blank_frame_streak = 0;
        }
    }
    if let Some(target) = runtime.hw_render_state.lock().target.as_mut() {
        target.emu_game_texture = emu_game_texture;
        if is_play {
            target.play_blank_frame_streak = play_blank_frame_streak;
            target.play_texture_cache_score = play_texture_cache_score;
            target.play_texture_frame_counter = play_texture_frame_counter;
            target.play_texture_last_signature = play_texture_last_signature;
            target.play_texture_stale_signature_streak = play_texture_stale_signature_streak;
            target.play_texture_last_scan_frame = play_texture_last_scan_frame;
            target.play_texture_last_switch_frame = play_texture_last_switch_frame;
        }
    }

    // Diagnostic: inspect the emu-side FBO (the one we gave to get_current_framebuffer),
    // but only if it is valid in the current (main) context.  After the is_framebuffer fix,
    // emu_ctx_framebuffer may point to an FBO that lives in the core's shared context — trying
    // to bind it here (main context) would be a GL error.
    if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
        if let Some(emu_fbo) = emu_ctx_framebuffer {
            let emu_fbo_valid = unsafe { gl.is_framebuffer(emu_fbo) };
            if emu_fbo_valid {
                unsafe {
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(emu_fbo));
                    let attached_obj = gl.get_framebuffer_attachment_parameter_i32(
                        glow::FRAMEBUFFER,
                        glow::COLOR_ATTACHMENT0,
                        glow::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME,
                    );
                    let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
                    eprintln!(
                        "gl readback emu_fbo={} attached_obj={attached_obj} status=0x{status:x}",
                        emu_fbo.0.get()
                    );
                }
                let emu_pixels = read_framebuffer(Some(emu_fbo));
                let (_, emu_non_black, first) = summarize_rgba_debug_pixels(&emu_pixels);
                eprintln!(
                    "gl readback emu_fbo_direct non_black={emu_non_black}/64 first_rgba={:02x},{:02x},{:02x},{:02x}",
                    first[0], first[1], first[2], first[3]
                );
                unsafe { gl.bind_framebuffer(glow::FRAMEBUFFER, core_bound_framebuffer) };
            } else {
                eprintln!(
                    "gl readback emu_fbo={} not valid in main context (shared-context FBO — skipping direct read)",
                    emu_fbo.0.get()
                );
            }
        }
    }

    // Brute-force scan: find which FBO actually contains game content.
    // Only log on the first invocation to avoid flooding the console.
    if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
        static SCAN_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !SCAN_DONE.swap(true, std::sync::atomic::Ordering::Relaxed) {
            eprintln!("gl readback fbo scan (first frame only):");
            for fbo_id in 1u32..=64 {
                if let Some(fbo_nz) = NonZeroU32::new(fbo_id) {
                    let fbo = glow::NativeFramebuffer(fbo_nz);
                    let is_fbo = unsafe { gl.is_framebuffer(fbo) };
                    if !is_fbo {
                        continue;
                    }
                    unsafe { gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo)) };
                    let status = unsafe { gl.check_framebuffer_status(glow::FRAMEBUFFER) };
                    if status != glow::FRAMEBUFFER_COMPLETE {
                        eprintln!("  fbo={fbo_id} status=0x{status:x} (incomplete)");
                        continue;
                    }
                    let attached_obj = unsafe {
                        gl.get_framebuffer_attachment_parameter_i32(
                            glow::FRAMEBUFFER,
                            glow::COLOR_ATTACHMENT0,
                            glow::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME,
                        )
                    };
                    let mut scan_pixels = vec![0u8; 8 * 8 * 4];
                    unsafe {
                        gl.read_buffer(glow::COLOR_ATTACHMENT0);
                        gl.read_pixels(
                            0,
                            0,
                            8,
                            8,
                            glow::RGBA,
                            glow::UNSIGNED_BYTE,
                            glow::PixelPackData::Slice(Some(&mut scan_pixels)),
                        );
                    }
                    let non_black = scan_pixels
                        .chunks_exact(4)
                        .filter(|p| p[0] != 0 || p[1] != 0 || p[2] != 0)
                        .count();
                    eprintln!(
                        "  fbo={fbo_id} attached_obj={attached_obj} non_black={non_black}/64 \
                        first=({},{},{},{})",
                        scan_pixels[0], scan_pixels[1], scan_pixels[2], scan_pixels[3]
                    );
                }
            }
            unsafe { gl.bind_framebuffer(glow::FRAMEBUFFER, core_bound_framebuffer) };
        }
    }

    if pending.bottom_left_origin {
        let row_len = pending.width as usize * 4;
        let half_height = pending.height as usize / 2;
        for row in 0..half_height {
            let top = row * row_len;
            let bottom = (pending.height as usize - 1 - row) * row_len;
            for offset in 0..row_len {
                pixels.swap(top + offset, bottom + offset);
            }
        }
    }

    // Some OpenGL cores leave alpha undefined or zero, which makes the UI texture effectively
    // invisible even when RGB data is valid.
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[3] = 255;
    }

    if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
        let (checksum, non_black_pixels, first_rgba) = summarize_rgba_debug_pixels(&pixels);
        eprintln!(
            "gl readback size={}x{} pitch={} checksum=0x{checksum:016x} non_black_samples={}/64 first_rgba={:02x},{:02x},{:02x},{:02x} bottom_left_origin={}",
            pending.width,
            pending.height,
            pending.width as usize * 4,
            non_black_pixels,
            first_rgba[0],
            first_rgba[1],
            first_rgba[2],
            first_rgba[3],
            pending.bottom_left_origin,
        );
    }

    Ok(FrameBuffer {
        width: pending.width,
        height: pending.height,
        pitch: pending.width as usize * 4,
        data: pixels,
        pixel_format: PixelFormat::Rgba8888,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        core_name_allows_generic_gl_texture_scan, next_play_stale_signature_streak,
        should_run_play_periodic_rescan, should_switch_play_texture_candidate,
        PlayTextureSignature,
    };

    fn signature(seed: u64) -> PlayTextureSignature {
        PlayTextureSignature {
            checksum: seed,
            non_black_samples: 48,
            luminance_range: 24,
            uniform: false,
            first_rgb: [10, 20, 30],
            center_rgb: [100, 110, 120],
        }
    }

    #[test]
    fn stale_streak_increments_for_near_static_signatures() {
        let current = signature(7);
        let streak_1 = next_play_stale_signature_streak(Some(current), current, 0);
        let streak_2 = next_play_stale_signature_streak(Some(current), current, streak_1);
        assert_eq!(streak_1, 1);
        assert_eq!(streak_2, 2);
    }

    #[test]
    fn stale_streak_resets_for_changing_signatures() {
        let previous = signature(1);
        let current = PlayTextureSignature {
            checksum: 2,
            non_black_samples: 12,
            luminance_range: 72,
            uniform: false,
            first_rgb: [220, 10, 40],
            center_rgb: [15, 200, 25],
        };
        let streak = next_play_stale_signature_streak(Some(previous), current, 5);
        assert_eq!(streak, 0);
    }

    #[test]
    fn periodic_rescan_requires_interval_and_cooldown() {
        assert!(!should_run_play_periodic_rescan(80, 0, 0));
        assert!(!should_run_play_periodic_rescan(140, 0, 120));
        assert!(should_run_play_periodic_rescan(240, 0, 0));
    }

    #[test]
    fn switch_decision_obeys_cooldown_and_margin() {
        assert!(!should_switch_play_texture_candidate(
            false, 1.0, 0.6, 40, 0
        ));
        assert!(!should_switch_play_texture_candidate(
            false, 1.0, 0.85, 200, 0
        ));
        assert!(should_switch_play_texture_candidate(
            false, 1.0, 0.5, 200, 0
        ));
    }

    #[test]
    fn switch_decision_prefers_exact_candidate_without_waiting_for_cooldown() {
        assert!(should_switch_play_texture_candidate(
            false, 1.507, 0.0, 115, 90
        ));
    }

    #[test]
    fn switch_decision_keeps_cooldown_for_non_exact_candidates() {
        assert!(!should_switch_play_texture_candidate(
            false, 1.507, 0.4, 115, 90
        ));
    }

    #[test]
    fn switch_decision_allows_force_due_to_stale() {
        assert!(should_switch_play_texture_candidate(true, 1.0, 10.0, 5, 4));
    }

    #[test]
    fn generic_texture_scan_is_disabled_for_dolphin() {
        assert!(!core_name_allows_generic_gl_texture_scan(Some("dolphin")));
        assert!(!core_name_allows_generic_gl_texture_scan(Some("DOLPHIN")));
        assert!(core_name_allows_generic_gl_texture_scan(Some(
            "mupen64plus_next"
        )));
        assert!(core_name_allows_generic_gl_texture_scan(None));
    }
}
