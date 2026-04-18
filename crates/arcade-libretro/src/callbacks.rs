use super::*;

pub(super) fn record_runtime_load_error(message: impl Into<String>) {
    let message = message.into();
    let _ = with_active_runtime(|runtime| {
        runtime.environment_context.lock().last_load_error = Some(message.clone());
    });
}

pub(super) fn configure_environment_context(
    runtime: &HostRuntime,
    system_root: &Path,
    save_root: &Path,
    core_name: &str,
    backend: VideoBackendKind,
    emulation: &EmulationConfig,
) {
    {
        let mut context = runtime.environment_context.lock();
        context.system_dir = StableCStringBuffer::from_path(system_root);
        context.save_dir = StableCStringBuffer::from_path(save_root);
        context.variables = default_core_variables_for(core_name, backend, emulation);
        context.variables_updated = false;
        context.allow_vfs = !core_name.eq_ignore_ascii_case("fbneo");
        context.controller_info.clear();
        context.requested_hw_render = false;
        context.requested_hw_context_type = None;
        context.last_load_error = None;
        context.last_negotiation_interface = None;
        context.loaded_core_name = Some(core_name.to_string());
        context.loaded_backend = Some(backend);
        context.keyboard_event_cb = None;
        context.audio_callback = None;
        context.audio_set_state_callback = None;
        context.audio_callback_enabled = false;
        let should_log_core_vars = vulkan_debug_enabled()
            || vulkan_handoff_trace_enabled()
            || env_flag_enabled("ARCADE_PARALLEL_RDP_SAFE_DIAG")
            || std::env::var_os("LIBRETRO_TRACE_VARIABLES").is_some();
        if core_name.eq_ignore_ascii_case("mupen64plus_next") {
            let rdp = context
                .variables
                .get("mupen64plus-rdp-plugin")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("<unset>");
            let rsp = context
                .variables
                .get("mupen64plus-rsp-plugin")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("<unset>");
            let cpucore = context
                .variables
                .get("mupen64plus-cpucore")
                .or_else(|| context.variables.get("mupen64plus-cpu-core"))
                .and_then(|value| value.to_str().ok())
                .unwrap_or("<unset>");
            let count_per_op = context
                .variables
                .get("mupen64plus-CountPerOp")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("<unset>");
            let count_per_op_denom = context
                .variables
                .get("mupen64plus-CountPerOpDenomPot")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("<unset>");

            eprintln!(
                "[CORE-VARS] backend={:?} mupen64plus-rdp-plugin={} mupen64plus-rsp-plugin={} mupen64plus-cpucore={} mupen64plus-CountPerOp={} mupen64plus-CountPerOpDenomPot={}",
                backend,
                rdp,
                rsp,
                cpucore,
                count_per_op,
                count_per_op_denom
            );

            if should_log_core_vars {
                info!(
                    target: "arcade_libretro::core_loader",
                    "configured mupen64plus_next core vars backend={:?} mupen64plus-rdp-plugin={} mupen64plus-rsp-plugin={} mupen64plus-cpucore={} mupen64plus-CountPerOp={} mupen64plus-CountPerOpDenomPot={}",
                    backend,
                    rdp,
                    rsp,
                    cpucore,
                    count_per_op,
                    count_per_op_denom
                );
            }
        }
    }
    runtime.hw_render_state.lock().vulkan_negotiation = None;
}

pub(super) fn core_has_embedded_software_video_fallback(core_name: &str) -> bool {
    matches!(core_name, "mupen64plus_next" | "mednafen_psx_hw")
}

pub(super) unsafe fn load_api(library: &Library) -> Result<CoreApi> {
    let load = |name: &[u8]| -> Result<*const ()> {
        let symbol: Symbol<*const ()> = library.get(name).with_context(|| {
            format!(
                "missing required libretro symbol {}",
                String::from_utf8_lossy(name)
            )
        })?;
        Ok(*symbol)
    };
    let load_optional = |name: &[u8]| -> Option<*const ()> {
        let symbol: std::result::Result<Symbol<*const ()>, _> = library.get(name);
        symbol.ok().map(|symbol| *symbol)
    };

    Ok(CoreApi {
        set_environment: std::mem::transmute::<*const (), RetroSetEnvironment>(load(
            b"retro_set_environment",
        )?),
        set_video_refresh: std::mem::transmute::<*const (), RetroSetVideoRefresh>(load(
            b"retro_set_video_refresh",
        )?),
        set_audio_sample: std::mem::transmute::<*const (), RetroSetAudioSample>(load(
            b"retro_set_audio_sample",
        )?),
        set_audio_sample_batch: std::mem::transmute::<*const (), RetroSetAudioSampleBatch>(load(
            b"retro_set_audio_sample_batch",
        )?),
        set_input_poll: std::mem::transmute::<*const (), RetroSetInputPoll>(load(
            b"retro_set_input_poll",
        )?),
        set_input_state: std::mem::transmute::<*const (), RetroSetInputState>(load(
            b"retro_set_input_state",
        )?),
        set_controller_port_device: load_optional(b"retro_set_controller_port_device")
            .map(|symbol| std::mem::transmute::<*const (), RetroSetControllerPortDevice>(symbol)),
        init: std::mem::transmute::<*const (), RetroInit>(load(b"retro_init")?),
        deinit: std::mem::transmute::<*const (), RetroDeinit>(load(b"retro_deinit")?),
        api_version: std::mem::transmute::<*const (), RetroApiVersion>(load(b"retro_api_version")?),
        get_system_info: std::mem::transmute::<*const (), RetroGetSystemInfo>(load(
            b"retro_get_system_info",
        )?),
        get_system_av_info: std::mem::transmute::<*const (), RetroGetSystemAvInfo>(load(
            b"retro_get_system_av_info",
        )?),
        load_game: std::mem::transmute::<*const (), RetroLoadGame>(load(b"retro_load_game")?),
        unload_game: std::mem::transmute::<*const (), RetroUnloadGame>(load(b"retro_unload_game")?),
        reset: std::mem::transmute::<*const (), RetroReset>(load(b"retro_reset")?),
        run: std::mem::transmute::<*const (), RetroRun>(load(b"retro_run")?),
        serialize_size: std::mem::transmute::<*const (), RetroSerializeSize>(load(
            b"retro_serialize_size",
        )?),
        serialize: std::mem::transmute::<*const (), RetroSerialize>(load(b"retro_serialize")?),
        unserialize: std::mem::transmute::<*const (), RetroUnserialize>(load(
            b"retro_unserialize",
        )?),
    })
}

pub(super) unsafe extern "C" fn retro_environment(cmd: u32, data: *mut c_void) -> bool {
    const RETRO_ENVIRONMENT_GET_CAN_DUPE: u32 = 3;
    const RETRO_ENVIRONMENT_SET_PERFORMANCE_LEVEL: u32 = 8;
    const RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY: u32 = 9;
    const RETRO_ENVIRONMENT_SET_PIXEL_FORMAT: u32 = 10;
    const RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS: u32 = 11;
    const RETRO_ENVIRONMENT_SET_HW_RENDER: u32 = 14;
    const RETRO_ENVIRONMENT_GET_VARIABLE: u32 = 15;
    const RETRO_ENVIRONMENT_SET_VARIABLES: u32 = 16;
    const RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE: u32 = 17;
    const RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME: u32 = 18;
    const RETRO_ENVIRONMENT_SET_AUDIO_CALLBACK: u32 = 22;
    const RETRO_ENVIRONMENT_GET_LOG_INTERFACE: u32 = 27;
    const RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY: u32 = 31;
    const RETRO_ENVIRONMENT_SET_SYSTEM_AV_INFO: u32 = 32;
    const RETRO_ENVIRONMENT_SET_GEOMETRY: u32 = 37;
    const RETRO_ENVIRONMENT_SET_HW_RENDER_CONTEXT_NEGOTIATION_INTERFACE: u32 =
        43 | RETRO_ENVIRONMENT_EXPERIMENTAL;
    const RETRO_ENVIRONMENT_SET_CONTROLLER_INFO: u32 = 35;
    const RETRO_ENVIRONMENT_GET_LANGUAGE: u32 = 39;
    const RETRO_ENVIRONMENT_SET_SUPPORT_ACHIEVEMENTS: u32 = 42 | 0x10000;
    const RETRO_ENVIRONMENT_GET_VFS_INTERFACE: u32 = 45 | 0x10000;
    const RETRO_ENVIRONMENT_GET_INPUT_BITMASKS: u32 = 51 | 0x10000;
    const RETRO_ENVIRONMENT_GET_MESSAGE_INTERFACE_VERSION: u32 = 59;
    const RETRO_ENVIRONMENT_SET_FASTFORWARDING_OVERRIDE: u32 = 64;
    const RETRO_ENVIRONMENT_SET_CORE_OPTIONS_UPDATE_DISPLAY_CALLBACK: u32 = 69;
    const RETRO_ENVIRONMENT_SET_KEYBOARD_CALLBACK: u32 = 12;
    const RETRO_ENVIRONMENT_SET_VARIABLE: u32 = 70;

    const RETRO_PIXEL_FORMAT_0RGB1555: u32 = 0;
    const RETRO_PIXEL_FORMAT_XRGB8888: u32 = 1;
    const RETRO_PIXEL_FORMAT_RGB565: u32 = 2;
    const RETRO_HW_RENDER_CONTEXT_NEGOTIATION_INTERFACE_VULKAN: u32 = 0;

    if std::env::var_os("LIBRETRO_TRACE_ENV").is_some() {
        eprintln!("libretro env cmd={cmd}");
    }

    match cmd {
        RETRO_ENVIRONMENT_GET_CAN_DUPE => {
            if data.is_null() {
                return false;
            }
            unsafe {
                *(data as *mut bool) = true;
            }
            true
        }
        RETRO_ENVIRONMENT_SET_PERFORMANCE_LEVEL
        | RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS
        | RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME
        | RETRO_ENVIRONMENT_SET_SUPPORT_ACHIEVEMENTS
        | RETRO_ENVIRONMENT_SET_FASTFORWARDING_OVERRIDE
        | RETRO_ENVIRONMENT_SET_CORE_OPTIONS_UPDATE_DISPLAY_CALLBACK => true,
        RETRO_ENVIRONMENT_SET_KEYBOARD_CALLBACK => {
            if data.is_null() {
                return false;
            }
            // The struct is a single function-pointer field.
            #[repr(C)]
            struct RetroKeyboardCallback {
                callback: RetroKeyboardEventFn,
            }
            let kb = unsafe { &*(data as *const RetroKeyboardCallback) };
            if let Some(runtime) = active_runtime() {
                runtime.environment_context.lock().keyboard_event_cb = Some(kb.callback);
            }
            true
        }
        RETRO_ENVIRONMENT_SET_AUDIO_CALLBACK => {
            if data.is_null() {
                return false;
            }
            #[repr(C)]
            struct RetroAudioCallback {
                callback: Option<RetroAudioCallbackFn>,
                set_state: Option<RetroAudioSetStateCallbackFn>,
            }
            let audio = unsafe { &*(data as *const RetroAudioCallback) };
            if let Some(runtime) = active_runtime() {
                let mut context = runtime.environment_context.lock();
                context.audio_callback = audio.callback;
                context.audio_set_state_callback = audio.set_state;
                context.audio_callback_enabled = audio.callback.is_some();
                if let Some(set_state) = audio.set_state {
                    unsafe { set_state(true) };
                }
            }
            info!(
                target: "arcade_libretro::audio",
                callback_registered = audio.callback.is_some(),
                set_state_registered = audio.set_state.is_some(),
                "core requested RETRO_ENVIRONMENT_SET_AUDIO_CALLBACK"
            );
            true
        }
        RETRO_ENVIRONMENT_GET_HW_RENDER_INTERFACE => {
            if data.is_null() {
                record_runtime_load_error(
                    "core requested hardware-render interface with a null payload",
                );
                return false;
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let state = runtime.hw_render_state.lock();
            if state.context_type != Some(RETRO_HW_CONTEXT_VULKAN) {
                record_runtime_load_error(
                    "core requested Vulkan render interface before Vulkan SET_HW_RENDER completed",
                );
                return false;
            }
            let Some(vulkan) = state.vulkan.as_ref() else {
                record_runtime_load_error(
                    "core requested Vulkan render interface before the frontend Vulkan backend was initialized",
                );
                return false;
            };
            unsafe {
                *(data as *mut *const RetroHwRenderInterface) = &vulkan.interface
                    as *const RetroHwRenderInterfaceVulkan
                    as *const RetroHwRenderInterface;
            }
            true
        }
        RETRO_ENVIRONMENT_SET_HW_RENDER => {
            if data.is_null() {
                record_runtime_load_error(
                    "core requested hardware render with a null callback payload",
                );
                warn!("libretro hw-render: SET_HW_RENDER denied because callback data was null");
                return false;
            }
            let callback = unsafe { &mut *(data as *mut RetroHwRenderCallback) };
            let runtime = active_runtime();
            if let Some(runtime) = runtime.as_ref() {
                let chosen_backend = runtime.video_coordinator.lock().current_backend_kind();
                let mut context = runtime.environment_context.lock();
                context.requested_hw_render = true;
                context.requested_hw_context_type = Some(callback.context_type);
                if vulkan_debug_enabled() {
                    info!(
                        target: "arcade_libretro::vulkan_debug",
                        "SET_HW_RENDER requested context_type={} ({}) chosen_backend={:?}",
                        callback.context_type,
                        hw_context_type_name(callback.context_type),
                        chosen_backend
                    );
                }
                if chosen_backend == VideoBackendKind::Vulkan
                    && callback.context_type != RETRO_HW_CONTEXT_VULKAN
                {
                    warn!(
                        "libretro hw-render: core requested {} ({}) while host selected Vulkan; \
                        this core binary is likely built without Vulkan hardware-render support",
                        callback.context_type,
                        hw_context_type_name(callback.context_type)
                    );
                }
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            if callback.context_type == RETRO_HW_CONTEXT_VULKAN {
                // If the policy already chose a non-Vulkan backend (e.g. OpenGL for
                // mednafen_psx_hw when a GL context is available), reject this Vulkan
                // request so the core can retry with an OpenGL context instead.
                let chosen_backend = runtime.video_coordinator.lock().current_backend_kind();
                if chosen_backend != VideoBackendKind::Vulkan {
                    info!(
                        target: "arcade_libretro::vulkan_debug",
                        "SET_HW_RENDER: rejecting Vulkan because policy chose {:?}; core should retry with OpenGL",
                        chosen_backend
                    );
                    return false;
                }
                let existing = {
                    let mut state = runtime.hw_render_state.lock();
                    state.vulkan.take()
                };
                if let Some(existing) = existing {
                    destroy_vulkan_interface_state(existing);
                }
            } else {
                // OpenGL context request — reject when the policy chose Software so the
                // core falls through to its own software renderer.
                let chosen_backend = runtime.video_coordinator.lock().current_backend_kind();
                if chosen_backend == VideoBackendKind::Software {
                    info!(
                        target: "arcade_libretro::vulkan_debug",
                        "SET_HW_RENDER: rejecting GL context type {} because policy chose Software; core should use software renderer",
                        callback.context_type
                    );
                    return false;
                }
                if !hardware_render_frontend_available() {
                    record_runtime_load_error(
                        "core requested OpenGL hardware render, but no frontend GL context is available",
                    );
                    warn!("libretro hw-render: SET_HW_RENDER denied because no frontend GL context is available");
                    return false;
                }
            }
            if callback.context_type != RETRO_HW_CONTEXT_VULKAN
                && !hw_context_type_supported(callback.context_type)
            {
                let message = format!(
                    "core requested unsupported hardware context {} ({})",
                    callback.context_type,
                    hw_context_type_name(callback.context_type)
                );
                record_runtime_load_error(message.clone());
                warn!(
                    "libretro hw-render: rejecting unsupported context type {} ({})",
                    callback.context_type,
                    hw_context_type_name(callback.context_type)
                );
                return false;
            }
            if callback.context_type != RETRO_HW_CONTEXT_VULKAN {
                callback.get_current_framebuffer = Some(retro_hw_get_current_framebuffer);
                callback.get_proc_address = Some(retro_hw_get_proc_address);
            }
            let mut hw_state = runtime.hw_render_state.lock();
            hw_state.context_type = Some(callback.context_type);
            hw_state.callbacks = Some(HardwareRenderCallbacks {
                context_reset: callback.context_reset,
                context_destroy: callback.context_destroy,
                bottom_left_origin: callback.bottom_left_origin,
            });
            hw_state.context_ready = false;
            true
        }
        RETRO_ENVIRONMENT_SET_HW_RENDER_CONTEXT_NEGOTIATION_INTERFACE => {
            if data.is_null() {
                record_runtime_load_error(
                    "core requested hardware-render negotiation with a null payload",
                );
                return false;
            }
            let negotiation =
                unsafe { &*(data as *const RetroHwRenderContextNegotiationInterface) };
            let runtime = active_runtime();
            if let Some(runtime) = runtime.as_ref() {
                runtime
                    .environment_context
                    .lock()
                    .last_negotiation_interface =
                    Some((negotiation.interface_type, negotiation.interface_version));
            }
            if negotiation.interface_type == RETRO_HW_RENDER_CONTEXT_NEGOTIATION_INTERFACE_VULKAN {
                let Some(runtime) = runtime else {
                    return false;
                };
                let negotiation =
                    unsafe { &*(data as *const RetroHwRenderContextNegotiationInterfaceVulkan) };
                if vulkan_debug_enabled() {
                    info!(
                        target: "arcade_libretro::vulkan_debug",
                        "core requested Vulkan negotiation interface version={} get_application_info={} create_device={} destroy_device={}",
                        negotiation.interface_version,
                        negotiation.get_application_info.is_some(),
                        negotiation.create_device.is_some(),
                        negotiation.destroy_device.is_some()
                    );
                }
                let should_create = {
                    let mut state = runtime.hw_render_state.lock();
                    // Store negotiation callbacks unconditionally. Cores often call
                    // SET_HW_RENDER_CONTEXT_NEGOTIATION_INTERFACE before SET_HW_RENDER,
                    // so context_type may not be VULKAN yet. The callbacks will be
                    // consumed later when ensure_vulkan_interface_state_for() runs.
                    state.vulkan_negotiation = Some(VulkanNegotiationCallbacks {
                        get_application_info: negotiation.get_application_info,
                        create_device: negotiation.create_device,
                        destroy_device: negotiation.destroy_device,
                    });
                    if state.context_type != Some(RETRO_HW_CONTEXT_VULKAN) {
                        // Accept and store, but don't create yet — will be used when
                        // SET_HW_RENDER arrives with a Vulkan context.
                        return true;
                    }
                    state.vulkan.is_some()
                };
                if should_create {
                    return match rebuild_vulkan_interface_state_for(&runtime) {
                        Ok(_) => true,
                        Err(err) => {
                            let message = format!(
                                "core requested Vulkan render negotiation, but frontend Vulkan backend initialization failed: {err}"
                            );
                            record_runtime_load_error(message.clone());
                            warn!("libretro hw-render: {message}");
                            false
                        }
                    };
                }
                return true;
            }
            let message = format!(
                "core requested unsupported render negotiation interface {} ({}) v{}",
                negotiation.interface_type,
                hw_render_interface_type_name(negotiation.interface_type),
                negotiation.interface_version
            );
            record_runtime_load_error(message.clone());
            warn!("libretro hw-render: {message}");
            false
        }
        RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY => {
            if data.is_null() {
                return false;
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let context = runtime.environment_context.lock();
            let out = data as *mut *const c_char;
            if let Some(path) = context.system_dir.as_ref() {
                unsafe {
                    *out = path.as_ptr();
                }
                true
            } else {
                false
            }
        }
        RETRO_ENVIRONMENT_SET_PIXEL_FORMAT => {
            if data.is_null() {
                return false;
            }
            let requested = unsafe { *(data as *mut u32) };
            let pixel_format = match requested {
                RETRO_PIXEL_FORMAT_0RGB1555 => PixelFormat::Argb1555,
                RETRO_PIXEL_FORMAT_XRGB8888 => PixelFormat::Xrgb8888,
                RETRO_PIXEL_FORMAT_RGB565 => PixelFormat::Rgb565,
                _ => return false,
            };
            let Some(runtime) = active_runtime() else {
                return false;
            };
            runtime.callback_state.lock().pixel_format = pixel_format;
            true
        }
        RETRO_ENVIRONMENT_SET_SYSTEM_AV_INFO => {
            // The core is updating both geometry and timing mid-run (e.g. an
            // N64 game changing video mode or the audio plugin reporting a
            // different sample rate after dynarec kicks in).
            if data.is_null() {
                return false;
            }
            let av_info = unsafe { *(data as *const RetroSystemAvInfo) };
            let Some(runtime) = active_runtime() else {
                return false;
            };
            runtime
                .video_coordinator
                .lock()
                .handle_geometry_update(&runtime, av_info.geometry);
            if av_info.timing.sample_rate.is_finite() && av_info.timing.sample_rate > 0.0 {
                set_audio_source_sample_rate(av_info.timing.sample_rate);
                eprintln!(
                    "[AUDIO-RS] SET_SYSTEM_AV_INFO updated source sample rate to {}",
                    av_info.timing.sample_rate,
                );
            }
            true
        }
        RETRO_ENVIRONMENT_SET_GEOMETRY => {
            if data.is_null() {
                return false;
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let geometry = unsafe { *(data as *const RetroGameGeometry) };
            runtime
                .video_coordinator
                .lock()
                .handle_geometry_update(&runtime, geometry);
            true
        }
        RETRO_ENVIRONMENT_GET_VARIABLE => {
            if data.is_null() {
                return false;
            }
            let variable = unsafe { &mut *(data as *mut RetroVariable) };
            if variable.key.is_null() {
                return false;
            }
            let Ok(key) = unsafe { CStr::from_ptr(variable.key) }.to_str() else {
                variable.value = std::ptr::null();
                return true;
            };
            let Some(runtime) = active_runtime() else {
                variable.value = std::ptr::null();
                return true;
            };
            let context = runtime.environment_context.lock();
            let aliased_key = match key {
                "mupen64plus-cpucore" => Some("mupen64plus-cpu-core"),
                "mupen64plus-cpu-core" => Some("mupen64plus-cpucore"),
                _ => None,
            };
            let value = context
                .variables
                .get(key)
                .or_else(|| aliased_key.and_then(|alias| context.variables.get(alias)))
                .map(|value| value.as_ptr())
                .unwrap_or(std::ptr::null());
            if vulkan_debug_enabled() && key.starts_with("mupen64plus-") {
                let printable = context
                    .variables
                    .get(key)
                    .or_else(|| aliased_key.and_then(|alias| context.variables.get(alias)))
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("<unset>");
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "GET_VARIABLE key={} value={}",
                    key,
                    printable
                );
            }
            if std::env::var_os("LIBRETRO_TRACE_VARIABLES").is_some()
                && key.starts_with("mupen64plus-")
            {
                let printable = context
                    .variables
                    .get(key)
                    .or_else(|| aliased_key.and_then(|alias| context.variables.get(alias)))
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("<unset>");
                eprintln!("env GET_VARIABLE key={key} value={printable}");
            }
            variable.value = value;
            true
        }
        RETRO_ENVIRONMENT_SET_VARIABLES => {
            if data.is_null() {
                return false;
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let mut context = runtime.environment_context.lock();
            let mut variable = data as *const RetroVariable;
            while !variable.is_null() {
                let current = unsafe { &*variable };
                if current.key.is_null() {
                    break;
                }
                let Ok(key) = unsafe { CStr::from_ptr(current.key) }.to_str() else {
                    break;
                };
                let value = if current.value.is_null() {
                    String::new()
                } else {
                    unsafe { CStr::from_ptr(current.value) }
                        .to_string_lossy()
                        .to_string()
                };
                store_default_variable(&mut context, key, &value);
                variable = unsafe { variable.add(1) };
            }
            context.variables_updated = true;
            true
        }
        RETRO_ENVIRONMENT_SET_CONTROLLER_INFO => {
            if data.is_null() {
                return false;
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let mut context = runtime.environment_context.lock();
            context.controller_info = parse_controller_info(data as *const RetroControllerInfo);
            true
        }
        RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE => {
            if data.is_null() {
                return false;
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let mut context = runtime.environment_context.lock();
            unsafe {
                *(data as *mut bool) = context.variables_updated;
            }
            context.variables_updated = false;
            true
        }
        RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY => {
            if data.is_null() {
                return false;
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let context = runtime.environment_context.lock();
            let out = data as *mut *const c_char;
            if let Some(path) = context.save_dir.as_ref() {
                unsafe {
                    *out = path.as_ptr();
                }
                true
            } else {
                false
            }
        }
        RETRO_ENVIRONMENT_GET_LANGUAGE => {
            if data.is_null() {
                return false;
            }
            unsafe {
                *(data as *mut u32) = 0;
            }
            true
        }
        RETRO_ENVIRONMENT_GET_INPUT_BITMASKS => {
            if data.is_null() {
                return false;
            }
            unsafe {
                *(data as *mut bool) = false;
            }
            true
        }
        RETRO_ENVIRONMENT_GET_LOG_INTERFACE => {
            if data.is_null() {
                return false;
            }

            let callback = data as *mut RetroLogCallback;
            unsafe {
                (*callback).log = arcade_libretro_log_printf as *const c_void;
            }
            true
        }
        RETRO_ENVIRONMENT_GET_VFS_INTERFACE => {
            if data.is_null() {
                return false;
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let allow_vfs = runtime.environment_context.lock().allow_vfs;
            if !allow_vfs {
                vfs_trace("environment GET_VFS_INTERFACE refused for fbneo");
                return false;
            }
            if vfs_disabled() {
                vfs_trace("environment GET_VFS_INTERFACE refused by LIBRETRO_DISABLE_VFS");
                return false;
            }
            let info = unsafe { &mut *(data as *mut RetroVfsInterfaceInfo) };
            if info.required_interface_version > 2 {
                return false;
            }
            info.iface = &VFS_INTERFACE as *const RetroVfsInterface as *mut RetroVfsInterface;
            true
        }
        RETRO_ENVIRONMENT_GET_MESSAGE_INTERFACE_VERSION => {
            if data.is_null() {
                return false;
            }
            unsafe {
                *(data as *mut u32) = 0;
            }
            true
        }
        RETRO_ENVIRONMENT_SET_VARIABLE => {
            if data.is_null() {
                return false;
            }
            let variable = unsafe { &*(data as *const RetroVariable) };
            if variable.key.is_null() || variable.value.is_null() {
                return false;
            }
            let Ok(key) = unsafe { CStr::from_ptr(variable.key) }.to_str() else {
                return false;
            };
            let value = unsafe { CStr::from_ptr(variable.value) }
                .to_string_lossy()
                .to_string();
            let Ok(value) = CString::new(value) else {
                return false;
            };
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let mut context = runtime.environment_context.lock();
            if vulkan_debug_enabled() && key.starts_with("mupen64plus-") {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "SET_VARIABLE key={} value={}",
                    key,
                    value.to_str().unwrap_or("<invalid>")
                );
            }
            if std::env::var_os("LIBRETRO_TRACE_VARIABLES").is_some()
                && key.starts_with("mupen64plus-")
            {
                eprintln!(
                    "env SET_VARIABLE key={key} value={}",
                    value.to_str().unwrap_or("<invalid>")
                );
            }
            context.variables.insert(key.to_string(), value);
            context.variables_updated = true;
            true
        }
        _ => {
            if std::env::var_os("LIBRETRO_TRACE_ENV").is_some() {
                eprintln!("Unhandled libretro environment cmd={cmd}");
            }
            false
        }
    }
}

pub(super) unsafe extern "C" fn retro_hw_get_current_framebuffer() -> usize {
    with_active_runtime(|runtime| {
        if force_default_gl_framebuffer(runtime) {
            return 0;
        }
        #[cfg(not(target_os = "macos"))]
        {
            return runtime
                .hw_render_state
                .lock()
                .target
                .as_ref()
                .map(|target| target.framebuffer.0.get() as usize)
                .unwrap_or(0);
        }

        // Extract state without holding the lock across GL calls.
        let (gl, cached_emu_fbo, color_texture, width, height) = {
            let state = runtime.hw_render_state.lock();
            let Some(gl) = state.frontend_gl_context.clone() else {
                return 0;
            };
            let Some(target) = state.target.as_ref() else {
                return 0;
            };
            (
                gl,
                target.emu_ctx_framebuffer,
                target.color_texture,
                target.width,
                target.height,
            )
        };

        // Fast path: if a cached emu-side FBO exists AND is valid in the CURRENT GL context,
        // return it immediately.  The validity check is critical: mupen64plus-next (and other
        // cores) may use a shared GL context for rendering that is different from the frontend's
        // main context.  FBOs are NOT shared between GL contexts in a share group — so
        // is_framebuffer() returns false when called from a different context than the one that
        // created the FBO.  When this happens we fall through and create a new emu-side FBO in
        // the current (shared) context, attaching the same color_texture (which IS shared).
        // That way GLideN64 renders into color_texture 33 on the shared context, and the
        // frontend reads it back from FBO 1 (main context) which also wraps color_texture 33.
        if let Some(emu_fbo) = cached_emu_fbo {
            if unsafe { gl.is_framebuffer(emu_fbo) } {
                return emu_fbo.0.get() as usize;
            }
            // Context mismatch — recreate in the current context below.
            if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
                eprintln!(
                    "get_current_framebuffer: cached FBO {} invalid in current context (context switch?), recreating on thread {:?}",
                    emu_fbo.0.get(),
                    std::thread::current().id(),
                );
            }
        }

        // Slow path: lazily create the emu-side FBO in whatever GL context is current on
        // this thread (the emu thread's shared context for cores like mupen64plus-next).
        // gl is just a collection of function pointers — it works on any thread that has a
        // GL context current.  color_texture is a shared object (textures are shared across
        // contexts in the same share group), so it is valid here even though the FBO that
        // wraps it in the main context is not.
        let emu_fbo = match unsafe { gl.create_framebuffer() } {
            Ok(fbo) => fbo,
            Err(err) => {
                if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
                    eprintln!("get_current_framebuffer: failed to create emu-side FBO: {err}");
                }
                return 0;
            }
        };

        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(emu_fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(color_texture),
                0,
            );
            gl.draw_buffer(glow::COLOR_ATTACHMENT0);

            // Depth-stencil: renderbuffers are not shared between GL contexts, so create a
            // new one in the emu thread's context.  We don't track this for cleanup because
            // it is lightweight and tied to the core's session lifetime.
            if let Ok(depth_stencil) = gl.create_renderbuffer() {
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
                gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            }

            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }

        if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
            eprintln!(
                "get_current_framebuffer: created emu-side FBO {} for color_texture {} on thread {:?}",
                emu_fbo.0.get(),
                color_texture.0.get(),
                std::thread::current().id(),
            );
        }

        // Cache the emu-side FBO so subsequent calls return immediately.
        {
            let mut state = runtime.hw_render_state.lock();
            if let Some(target) = state.target.as_mut() {
                target.emu_ctx_framebuffer = Some(emu_fbo);
            }
        }

        emu_fbo.0.get() as usize
    })
    .unwrap_or(0)
}

pub(super) unsafe extern "C" fn retro_hw_get_proc_address(sym: *const c_char) -> *const c_void {
    if sym.is_null() {
        return std::ptr::null();
    }
    let symbol = unsafe { CStr::from_ptr(sym) };
    GL_PROC_LOADER.lock().get(symbol)
}

pub(super) fn parse_default_variable_value(spec: &str) -> Option<&str> {
    let (_, values) = spec.split_once(';')?;
    let default = values.split('|').next()?.trim();
    if default.is_empty() {
        None
    } else {
        Some(default)
    }
}

pub(super) fn parse_controller_info(mut info: *const RetroControllerInfo) -> Vec<Vec<u32>> {
    let mut ports = Vec::new();
    while !info.is_null() {
        let current = unsafe { &*info };
        if current.types.is_null() || current.num_types == 0 {
            break;
        }

        let mut supported = Vec::with_capacity(current.num_types as usize);
        for index in 0..current.num_types as usize {
            let description = unsafe { &*current.types.add(index) };
            supported.push(description.id);
        }
        ports.push(supported);
        info = unsafe { info.add(1) };
    }
    ports
}

pub(super) fn preferred_controller_device(devices: Option<&[u32]>) -> u32 {
    const RETRO_DEVICE_JOYPAD: u32 = 1;
    const RETRO_DEVICE_MASK: u32 = 0xff;

    let Some(devices) = devices else {
        return RETRO_DEVICE_JOYPAD;
    };

    devices
        .iter()
        .copied()
        .find(|device| *device == RETRO_DEVICE_JOYPAD)
        .or_else(|| {
            devices
                .iter()
                .copied()
                .find(|device| *device != 0 && (*device & RETRO_DEVICE_MASK) == RETRO_DEVICE_JOYPAD)
        })
        .or_else(|| devices.iter().copied().find(|device| *device != 0))
        .unwrap_or(RETRO_DEVICE_JOYPAD)
}

pub(super) unsafe extern "C" fn retro_video_refresh(
    data: *const c_void,
    width: u32,
    height: u32,
    pitch: usize,
) {
    const RETRO_HW_FRAME_BUFFER_VALID: usize = usize::MAX;

    if width == 0 || height == 0 {
        return;
    }

    #[cfg(not(target_os = "macos"))]
    {
        if data as usize == RETRO_HW_FRAME_BUFFER_VALID {
            let Some(runtime) = active_runtime() else {
                return;
            };
            let bottom_left_origin = runtime
                .hw_render_state
                .lock()
                .callbacks
                .map(|callbacks| callbacks.bottom_left_origin)
                .unwrap_or(true);
            let mut state = runtime.callback_state.lock();
            state.latest_hw_frame = Some(PendingHardwareFrame {
                width,
                height,
                bottom_left_origin,
            });
            return;
        }
    }

    #[cfg(target_os = "macos")]
    {
        let hw_callback = data as usize == RETRO_HW_FRAME_BUFFER_VALID;
        let callback_trace_index = if vulkan_handoff_trace_enabled() {
            Some(VULKAN_VIDEO_REFRESH_TRACE_COUNTER.fetch_add(1, Ordering::Relaxed))
        } else {
            None
        };
        if let Some(index) = callback_trace_index
            .filter(|index| vulkan_handoff_trace_should_log_callback(*index, width, height))
        {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "handoff trace: retro_video_refresh raw idx={} hw={} data={:p} width={} height={} pitch={}",
                index,
                hw_callback,
                data,
                width,
                height,
                pitch
            );
        }

        if hw_callback {
            let Some(runtime) = active_runtime() else {
                return;
            };
            if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
                let (fbo, emu_fbo) = {
                    let state = runtime.hw_render_state.lock();
                    let fbo = state
                        .frontend_gl_context
                        .as_ref()
                        .map(|gl| unsafe { gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING) })
                        .unwrap_or(-1);
                    let emu_fbo = state
                        .target
                        .as_ref()
                        .and_then(|t| t.emu_ctx_framebuffer)
                        .map(|f| f.0.get() as i32)
                        .unwrap_or(0);
                    (fbo, emu_fbo)
                };
                let (eframe_ctx_id, current_ctx_id) = {
                    (
                        runtime.hw_render_state.lock().eframe_gl_ctx_id,
                        current_gl_ctx_id(),
                    )
                };
                eprintln!(
                    "retro_video_refresh HW thread={:?} FRAMEBUFFER_BINDING={fbo} emu_ctx_fbo={emu_fbo} \
                     ctx: current=0x{current_ctx_id:x} eframe=0x{eframe_ctx_id:x} same_ctx={}",
                    std::thread::current().id(),
                    current_ctx_id == eframe_ctx_id || eframe_ctx_id == 0,
                );
            }
            let (bottom_left_origin, gl, is_vulkan_hw_context) = {
                let state = runtime.hw_render_state.lock();
                let blo = state
                    .callbacks
                    .map(|callbacks| callbacks.bottom_left_origin)
                    .unwrap_or(true);
                let gl = state.frontend_gl_context.clone();
                let is_vulkan = state.context_type == Some(RETRO_HW_CONTEXT_VULKAN);
                (blo, gl, is_vulkan)
            };
            if !is_vulkan_hw_context {
                if let Some(gl) = gl {
                    unsafe { gl.finish() };
                }
            }
            let (source_width, source_height) = if vulkan_force_1x1_source_frame_test_mode() {
                (1, 1)
            } else {
                (width, height)
            };
            if is_vulkan_hw_context {
                if let Some(index) = callback_trace_index.filter(|index| {
                    vulkan_handoff_trace_should_log_callback(*index, source_width, source_height)
                }) {
                    info!(
                        target: "arcade_libretro::vulkan_debug",
                        "handoff trace: retro_video_refresh hw idx={} raw={}x{} stored={}x{} pitch={} forced_1x1={}",
                        index,
                        width,
                        height,
                        source_width,
                        source_height,
                        pitch,
                        vulkan_force_1x1_source_frame_test_mode()
                    );
                }
                if vulkan_handoff_trace_enabled() && (source_width <= 1 || source_height <= 1) {
                    let trace_index = VULKAN_HANDOFF_TRACE_COUNTER.fetch_add(1, Ordering::Relaxed);
                    if trace_index < 32 || trace_index % 60 == 0 {
                        info!(
                            target: "arcade_libretro::vulkan_debug",
                            "handoff trace: retro_video_refresh hw tiny idx={} source={}x{}",
                            trace_index,
                            source_width,
                            source_height
                        );
                    }
                }
                record_vulkan_source_frame_size(&runtime, source_width, source_height);
            }
            let mut state = runtime.callback_state.lock();
            state.latest_hw_frame = Some(PendingHardwareFrame {
                width: source_width,
                height: source_height,
                bottom_left_origin,
            });
            return;
        }
    }

    if data.is_null() || pitch == 0 {
        return;
    }

    let byte_len = pitch.saturating_mul(height as usize);
    let src = unsafe { std::slice::from_raw_parts(data as *const u8, byte_len) };
    let Some(runtime) = active_runtime() else {
        return;
    };
    let mut state = runtime.callback_state.lock();
    let pixel_format = state.pixel_format;
    state.latest_frame = Some(FrameBuffer {
        width,
        height,
        pitch,
        data: src.to_vec(),
        pixel_format,
    });
}

pub(super) unsafe extern "C" fn retro_input_poll() {}

pub(super) unsafe extern "C" fn retro_input_state(
    port: u32,
    device: u32,
    index: u32,
    id: u32,
) -> i16 {
    const RETRO_DEVICE_MASK: u32 = 0xff;

    let Some(runtime) = active_runtime() else {
        return 0;
    };
    let state = runtime.callback_state.lock();
    if let Some(value) = state.input_state.get(&(port, device, index, id)) {
        return *value;
    }

    let base_device = device & RETRO_DEVICE_MASK;
    if base_device != device {
        return state
            .input_state
            .get(&(port, base_device, index, id))
            .copied()
            .unwrap_or(0);
    }

    0
}
