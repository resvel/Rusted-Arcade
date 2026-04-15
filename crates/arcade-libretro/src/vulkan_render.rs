use super::*;

impl PendingVulkanImage {
    fn color_subresource_range(
        create_info: &vk::ImageViewCreateInfo<'static>,
    ) -> vk::ImageSubresourceRange {
        let range = create_info.subresource_range;
        vk::ImageSubresourceRange::default()
            .aspect_mask(if range.aspect_mask.is_empty() {
                vk::ImageAspectFlags::COLOR
            } else {
                range.aspect_mask
            })
            .base_mip_level(range.base_mip_level)
            .level_count(range.level_count.max(1))
            .base_array_layer(range.base_array_layer)
            .layer_count(range.layer_count.max(1))
    }

    fn color_subresource_layers(
        create_info: &vk::ImageViewCreateInfo<'static>,
    ) -> vk::ImageSubresourceLayers {
        let range = Self::color_subresource_range(create_info);
        vk::ImageSubresourceLayers::default()
            .aspect_mask(range.aspect_mask)
            .mip_level(range.base_mip_level)
            .base_array_layer(range.base_array_layer)
            .layer_count(range.layer_count)
    }
}

pub(super) fn pending_vulkan_image_delay(vulkan: &VulkanInterfaceState) -> usize {
    #[cfg(target_os = "macos")]
    if vulkan.present.is_some() {
        // External present on macOS should consume the latest callback image
        // immediately; extra delay just increases latency and can look like
        // intermittent visual stalls.
        return 0;
    }
    vulkan.sync_frames.saturating_sub(1).min(2) as usize
}

pub(super) fn current_pending_vulkan_image(
    vulkan: &VulkanInterfaceState,
) -> Option<&PendingVulkanImage> {
    let delay = pending_vulkan_image_delay(vulkan);
    (vulkan.pending_images.len() > delay)
        .then(|| vulkan.pending_images.front())
        .flatten()
}

pub(super) fn take_current_pending_vulkan_image(
    vulkan: &mut VulkanInterfaceState,
) -> Option<PendingVulkanImage> {
    let delay = pending_vulkan_image_delay(vulkan);
    if vulkan.pending_images.len() > delay {
        vulkan.pending_images.pop_front()
    } else {
        None
    }
}

pub(super) fn should_sample_pending_vulkan_image_directly(image: &PendingVulkanImage) -> bool {
    let _ = image;
    false
}

pub(super) fn create_sampling_image_view_for_pending_image(
    device: &ash::Device,
    image: &PendingVulkanImage,
) -> Result<vk::ImageView> {
    let create_info = vk::ImageViewCreateInfo::default()
        .image(image.image)
        .view_type(image.view_type)
        .format(image.format)
        .components(image.components)
        .subresource_range(image.subresource_range);
    unsafe {
        device
            .create_image_view(&create_info, None)
            .map_err(|err| anyhow!("failed to create Vulkan sampling image view: {err:?}"))
    }
}

impl ExternalVulkanWindow {
    fn create() -> std::result::Result<Self, String> {
        unsafe {
            // Ensure NSApplication exists (idempotent).
            let ns_app_class = class!(NSApplication);
            let _: *mut Object = msg_send![ns_app_class, sharedApplication];

            // Get screen dimensions via CoreGraphics (avoids NSRect return from msg_send).
            let display_id = CGMainDisplayID();
            let screen_w = CGDisplayPixelsWide(display_id) as u32;
            let screen_h = CGDisplayPixelsHigh(display_id) as u32;
            let width = screen_w.max(640);
            let height = screen_h.max(480);

            let content_rect = NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: width as f64,
                    height: height as f64,
                },
            };

            // NSBorderlessWindowMask = 0 — gives a frameless fullscreen window.
            let style_mask: usize = 0;
            // NSBackingStoreBuffered = 2.
            let backing: usize = 2;

            let ns_window_class = class!(NSWindow);
            let ns_window: *mut Object = msg_send![ns_window_class, alloc];
            let ns_window: *mut Object = msg_send![
                ns_window,
                initWithContentRect:content_rect
                styleMask:style_mask
                backing:backing
                defer:NO
            ];
            if ns_window.is_null() {
                return Err(String::from(
                    "failed to create NSWindow for Vulkan presentation",
                ));
            }

            // Create a CAMetalLayer (autoreleased — must retain).
            let ca_metal_layer_class = class!(CAMetalLayer);
            let metal_layer: *mut Object = msg_send![ca_metal_layer_class, layer];
            if metal_layer.is_null() {
                let _: () = msg_send![ns_window, release];
                return Err(String::from("failed to create CAMetalLayer"));
            }
            let _: () = msg_send![metal_layer, retain];

            // Attach the metal layer to the window's content view.
            let content_view: *mut Object = msg_send![ns_window, contentView];
            let _: () = msg_send![content_view, setWantsLayer: YES];
            let _: () = msg_send![content_view, setLayer: metal_layer];

            // Configure the window.
            let title = CString::new("Personal Arcade N64 (Vulkan)").unwrap();
            let ns_string_class = class!(NSString);
            let ns_title: *mut Object =
                msg_send![ns_string_class, stringWithUTF8String: title.as_ptr()];
            let _: () = msg_send![ns_window, setTitle: ns_title];

            let ns_color_class = class!(NSColor);
            let black: *mut Object = msg_send![ns_color_class, blackColor];
            let _: () = msg_send![ns_window, setBackgroundColor: black];

            Ok(Self {
                ns_window,
                metal_layer,
                width,
                height,
                visible: false,
            })
        }
    }

    pub(super) fn descriptor(&self) -> ExternalVulkanWindowDescriptor {
        ExternalVulkanWindowDescriptor::Metal {
            layer: self.metal_layer as *const c_void,
            width: self.width,
            height: self.height,
        }
    }

    pub(super) fn set_overlay_message(&mut self, _message: Option<&str>) {}

    fn pump_events(&mut self) {
        // No-op on macOS: eframe/winit manages the NSApplication event loop
        // and dispatches events to all NSWindows including this one.
        // Explicitly draining NSApp events here would re-enter winit's
        // event handler, causing a panic.
    }

    fn set_visible(&mut self, visible: bool) {
        if visible {
            if self.visible {
                return;
            }
            unsafe {
                let fullscreen_rect = NSRect {
                    origin: NSPoint { x: 0.0, y: 0.0 },
                    size: NSSize {
                        width: self.width as f64,
                        height: self.height as f64,
                    },
                };
                let _: () = msg_send![self.ns_window, setFrame:fullscreen_rect display:YES];
                let null: *mut Object = std::ptr::null_mut();
                let _: () = msg_send![self.ns_window, makeKeyAndOrderFront: null];
            }
            self.visible = true;
        } else {
            if !self.visible {
                return;
            }
            unsafe {
                let null: *mut Object = std::ptr::null_mut();
                let _: () = msg_send![self.ns_window, orderOut: null];
            }
            self.visible = false;
        }
    }
}

pub(super) fn destroy_external_vulkan_window(window: ExternalVulkanWindow) {
    unsafe {
        let _: () = msg_send![window.metal_layer, release];
        let _: () = msg_send![window.ns_window, close];
    }
}

pub(super) fn frontend_supports_external_vulkan_window(state: &HardwareRenderState) -> bool {
    let _ = state;
    let _ = active_runtime();
    // macOS creates its own NSWindow — no frontend window handle needed.
    true
}

pub(super) fn vulkan_debug_enabled() -> bool {
    std::env::var_os("ARCADE_VULKAN_DEBUG").is_some()
}

pub(super) fn vulkan_handoff_trace_enabled() -> bool {
    env_flag_enabled("ARCADE_VULKAN_HANDOFF_TRACE")
}

pub(super) fn vulkan_handoff_trace_should_log_callback(
    index: u64,
    width: u32,
    height: u32,
) -> bool {
    index < 64 || index % 120 == 0 || width <= 1 || height <= 1
}

pub(super) fn env_flag_enabled(name: &str) -> bool {
    match std::env::var(name) {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            !matches!(normalized.as_str(), "0" | "false" | "off" | "no")
        }
        Err(_) => false,
    }
}

pub(super) fn vulkan_test_metrics_enabled() -> bool {
    env_flag_enabled("ARCADE_VULKAN_TEST_METRICS")
}

pub(super) fn vulkan_force_black_test_mode() -> bool {
    env_flag_enabled("ARCADE_VULKAN_TEST_FORCE_BLACK")
}

pub(super) fn vulkan_force_1x1_source_frame_test_mode() -> bool {
    env_flag_enabled("ARCADE_VULKAN_TEST_FORCE_1X1")
}

pub(super) fn vulkan_fail_fast_disabled() -> bool {
    env_flag_enabled("ARCADE_VULKAN_DISABLE_FAIL_FAST")
}

pub(super) fn vulkan_fail_fast_sampling_enabled() -> bool {
    // Keep the default present path minimal: fail-fast readback sampling is only
    // enabled when explicitly requested (or when diagnostics/test modes are on).
    env_flag_enabled("ARCADE_VULKAN_FAIL_FAST_SAMPLING")
        || vulkan_debug_enabled()
        || vulkan_test_metrics_enabled()
}

pub(super) fn vulkan_black_fail_fast_threshold_frames() -> u64 {
    // Keep fail-fast below the known dynarec crash window seen in unhealthy
    // black-frame runs so we surface an explicit Vulkan error first.
    const DEFAULT_THRESHOLD_FRAMES: u64 = 240;
    if vulkan_fail_fast_disabled() {
        return 0;
    }
    match std::env::var("ARCADE_VULKAN_BLACK_FAIL_FAST_FRAMES") {
        Ok(raw) => raw
            .trim()
            .parse::<u64>()
            .unwrap_or(DEFAULT_THRESHOLD_FRAMES),
        Err(_) => DEFAULT_THRESHOLD_FRAMES,
    }
}

pub(super) fn vulkan_tiny_frame_fail_fast_threshold_frames() -> u64 {
    const DEFAULT_THRESHOLD_FRAMES: u64 = 120;
    if vulkan_fail_fast_disabled() {
        return 0;
    }
    match std::env::var("ARCADE_VULKAN_TINY_FRAME_FAIL_FAST_FRAMES") {
        Ok(raw) => raw
            .trim()
            .parse::<u64>()
            .unwrap_or(DEFAULT_THRESHOLD_FRAMES),
        Err(_) => DEFAULT_THRESHOLD_FRAMES,
    }
}

pub(super) fn vulkan_should_sample_for_fail_fast(
    runtime: &HostRuntime,
    already_non_black: bool,
) -> bool {
    if !vulkan_fail_fast_sampling_enabled() {
        return false;
    }
    let threshold = vulkan_black_fail_fast_threshold_frames();
    if threshold == 0 || already_non_black {
        return false;
    }

    let state = runtime.vulkan_present_metrics.lock();
    if state.external_present_deliveries >= threshold {
        return false;
    }

    state.external_present_deliveries < 8 || state.external_present_deliveries % 60 == 0
}

pub(super) fn vulkan_force_fallback_idle() -> bool {
    match std::env::var("ARCADE_VULKAN_FORCE_FALLBACK_IDLE") {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            !matches!(normalized.as_str(), "0" | "false" | "off" | "no")
        }
        Err(_) => false,
    }
}

pub(super) fn vulkan_force_present_idle() -> bool {
    match std::env::var("ARCADE_VULKAN_FORCE_PRESENT_IDLE") {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            !matches!(normalized.as_str(), "0" | "false" | "off" | "no")
        }
        Err(_) => false,
    }
}

pub(super) fn ensure_external_vulkan_window_for(
    runtime: &HostRuntime,
) -> Option<ExternalVulkanWindowDescriptor> {
    let mut state = runtime.hw_render_state.lock();
    let should_use_external = frontend_supports_external_vulkan_window(&state);
    if !should_use_external {
        if vulkan_debug_enabled() && !state.external_vulkan_probe_logged {
            let capabilities = runtime
                .video_coordinator
                .lock()
                .frontend_capabilities()
                .clone();
            info!(
                target: "arcade_libretro::vulkan_debug",
                "external Vulkan window disabled renderer={:?} window_handle={:?} display_handle={:?}",
                capabilities.renderer_name,
                capabilities.window_handle_kind,
                capabilities.display_handle_kind
            );
            state.external_vulkan_probe_logged = true;
        }
        return None;
    }

    if state.external_vulkan_window.is_none() {
        match ExternalVulkanWindow::create() {
            Ok(window) => {
                if vulkan_debug_enabled() {
                    let descriptor = window.descriptor();
                    let (width, height) = descriptor.size();
                    info!(
                        target: "arcade_libretro::vulkan_debug",
                        "created external Vulkan window {}x{}",
                        width,
                        height
                    );
                }
                state.external_vulkan_window = Some(window);
            }
            Err(err) => {
                warn!("libretro hw-render: external Vulkan window unavailable: {err}");
                return None;
            }
        }
    }
    state
        .external_vulkan_window
        .as_ref()
        .map(ExternalVulkanWindow::descriptor)
}

pub(super) fn pump_external_vulkan_window_events(runtime: &HostRuntime) {
    let mut state = runtime.hw_render_state.lock();
    if let Some(window) = state.external_vulkan_window.as_mut() {
        window.pump_events();
    }
}

pub(super) fn update_external_vulkan_present_state(runtime: &HostRuntime, active: bool) {
    let mut state = runtime.hw_render_state.lock();
    state.external_vulkan_present_active = active;
    if let Some(window) = state.external_vulkan_window.as_mut() {
        window.set_visible(active);
    }
}

fn build_vulkan_instance_extensions(
    entry: &ash::Entry,
    external_surface: bool,
) -> std::result::Result<Vec<*const i8>, String> {
    fn push_extension_if_available(
        enabled_extensions: &mut Vec<&'static CStr>,
        available_names: &HashSet<Vec<u8>>,
        extension_name: &'static CStr,
    ) {
        if available_names.contains(extension_name.to_bytes())
            && !enabled_extensions
                .iter()
                .any(|enabled| enabled.to_bytes() == extension_name.to_bytes())
        {
            enabled_extensions.push(extension_name);
        }
    }

    let available_extensions = unsafe { entry.enumerate_instance_extension_properties(None) }
        .map_err(|err| format!("failed to enumerate Vulkan instance extensions: {err:?}"))?;
    let available_names: HashSet<Vec<u8>> = available_extensions
        .iter()
        .map(|properties| unsafe { CStr::from_ptr(properties.extension_name.as_ptr()) })
        .map(|name| name.to_bytes().to_vec())
        .collect();

    let mut enabled_extensions: Vec<&'static CStr> = Vec::new();

    if external_surface {
        enabled_extensions.push(ash::vk::KHR_SURFACE_NAME);
        push_extension_if_available(
            &mut enabled_extensions,
            &available_names,
            ash::vk::EXT_METAL_SURFACE_NAME,
        );
    }

    push_extension_if_available(
        &mut enabled_extensions,
        &available_names,
        ash::vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_NAME,
    );
    push_extension_if_available(
        &mut enabled_extensions,
        &available_names,
        ash::vk::KHR_EXTERNAL_MEMORY_CAPABILITIES_NAME,
    );
    push_extension_if_available(
        &mut enabled_extensions,
        &available_names,
        ash::vk::KHR_EXTERNAL_SEMAPHORE_CAPABILITIES_NAME,
    );
    push_extension_if_available(
        &mut enabled_extensions,
        &available_names,
        ash::vk::KHR_EXTERNAL_FENCE_CAPABILITIES_NAME,
    );
    push_extension_if_available(
        &mut enabled_extensions,
        &available_names,
        ash::vk::KHR_PORTABILITY_ENUMERATION_NAME,
    );
    if vulkan_debug_enabled() {
        push_extension_if_available(
            &mut enabled_extensions,
            &available_names,
            ash::vk::EXT_DEBUG_UTILS_NAME,
        );
        let enabled_names: Vec<String> = enabled_extensions
            .iter()
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        info!(
            target: "arcade_libretro::vulkan_debug",
            "enabling Vulkan instance extensions: {}",
            enabled_names.join(", ")
        );
    }

    Ok(enabled_extensions
        .into_iter()
        .map(|extension_name| extension_name.as_ptr())
        .collect())
}

#[cfg(target_os = "macos")]
fn macos_vulkan_loader_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(path) = std::env::var_os("ARCADE_VULKAN_LOADER") {
        candidates.push(PathBuf::from(path));
    }
    if let Some(vulkan_sdk) = std::env::var_os("VULKAN_SDK") {
        let sdk = PathBuf::from(vulkan_sdk);
        candidates.push(sdk.join("lib").join("libvulkan.dylib"));
        candidates.push(sdk.join("macOS").join("lib").join("libvulkan.dylib"));
        candidates.push(sdk.join("macOS").join("lib").join("libMoltenVK.dylib"));
    }
    for path in [
        "/opt/homebrew/lib/libvulkan.dylib",
        "/opt/homebrew/lib/libvulkan.1.dylib",
        "/opt/homebrew/lib/libMoltenVK.dylib",
        "/usr/local/lib/libvulkan.dylib",
        "/usr/local/lib/libvulkan.1.dylib",
        "/usr/local/lib/libMoltenVK.dylib",
        "/usr/local/lib/libMoltenVK_all.dylib",
    ] {
        candidates.push(PathBuf::from(path));
    }

    let mut unique = Vec::new();
    let mut seen = HashSet::new();
    for candidate in candidates {
        if seen.insert(candidate.clone()) {
            unique.push(candidate);
        }
    }
    unique
}

fn load_vulkan_entry() -> std::result::Result<ash::Entry, String> {
    #[cfg(target_os = "macos")]
    {
        let mut attempted_paths = vec![PathBuf::from("libvulkan.dylib")];
        let mut last_error = match unsafe { ash::Entry::load() } {
            Ok(entry) => return Ok(entry),
            Err(err) => err.to_string(),
        };

        for candidate in macos_vulkan_loader_candidates() {
            if attempted_paths
                .iter()
                .any(|existing| existing == &candidate)
            {
                continue;
            }
            attempted_paths.push(candidate.clone());
            match unsafe { ash::Entry::load_from(&candidate) } {
                Ok(entry) => {
                    if vulkan_debug_enabled() {
                        info!(
                            target: "arcade_libretro::vulkan_debug",
                            "loaded Vulkan entry points from {}",
                            candidate.display()
                        );
                    }
                    return Ok(entry);
                }
                Err(err) => last_error = err.to_string(),
            }
        }

        let attempted = attempted_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let mut message = format!(
            "failed to load Vulkan entry points on macOS. Tried: {attempted}. \
Install Vulkan loader + MoltenVK (Homebrew: `brew install vulkan-loader molten-vk`) and ensure \
`libvulkan.dylib` or `libMoltenVK.dylib` is discoverable \
(for example `DYLD_FALLBACK_LIBRARY_PATH=/opt/homebrew/lib:/usr/local/lib`)."
        );
        if vulkan_debug_enabled() {
            message.push_str(&format!(" Last loader error: {last_error}"));
        }
        Err(message)
    }

    #[cfg(not(target_os = "macos"))]
    {
        unsafe { ash::Entry::load() }
            .map_err(|err| format!("failed to load Vulkan entry points: {err}"))
    }
}

impl VulkanInterfaceState {
    fn create(
        runtime: &HostRuntime,
        negotiation: Option<VulkanNegotiationCallbacks>,
        external_window: Option<ExternalVulkanWindowDescriptor>,
    ) -> std::result::Result<Self, String> {
        let app_name = CString::new("Let's Play")
            .map_err(|_| String::from("failed to build Vulkan app name"))?;
        let engine_name = CString::new("arcade-libretro")
            .map_err(|_| String::from("failed to build Vulkan engine name"))?;

        let entry = load_vulkan_entry()?;

        let default_app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .application_version(0)
            .engine_name(&engine_name)
            .engine_version(0)
            .api_version(vk::API_VERSION_1_0);
        let app_info = if let Some(get_application_info) =
            negotiation.and_then(|callbacks| callbacks.get_application_info)
        {
            let raw_app_info = unsafe { get_application_info() };
            if raw_app_info.is_null() {
                &default_app_info
            } else {
                unsafe { &*(raw_app_info as *const vk::ApplicationInfo<'static>) }
            }
        } else {
            &default_app_info
        };
        if vulkan_debug_enabled() {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "creating Vulkan interface negotiation={} app_api_version=0x{:x} external_surface={}",
                negotiation.is_some(),
                app_info.api_version,
                external_window.is_some()
            );
            let loader_version = unsafe { entry.try_enumerate_instance_version() }
                .ok()
                .flatten()
                .unwrap_or(vk::API_VERSION_1_0);
            info!(
                target: "arcade_libretro::vulkan_debug",
                "Vulkan loader instance version=0x{:x}",
                loader_version
            );
        }
        let instance_extensions =
            build_vulkan_instance_extensions(&entry, external_window.is_some())?;
        let portability_enumeration_enabled = instance_extensions
            .iter()
            .any(|&name| name == ash::vk::KHR_PORTABILITY_ENUMERATION_NAME.as_ptr());
        let mut create_info = vk::InstanceCreateInfo::default()
            .application_info(app_info)
            .enabled_extension_names(&instance_extensions);
        #[cfg(target_os = "macos")]
        if portability_enumeration_enabled {
            create_info = create_info.flags(vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR);
        }

        let instance = match unsafe { entry.create_instance(&create_info, None) } {
            Ok(instance) => instance,
            Err(err) => {
                #[cfg(target_os = "macos")]
                {
                    // MoltenVK setups can report incompatible driver for higher API versions
                    // requested by cores; retry with Vulkan 1.0 to maximize compatibility.
                    if err == vk::Result::ERROR_INCOMPATIBLE_DRIVER
                        && app_info.api_version > vk::API_VERSION_1_0
                    {
                        warn!(
                            "Vulkan instance creation failed with {:?} at api_version=0x{:x}; retrying with Vulkan 1.0",
                            err,
                            app_info.api_version
                        );
                        let fallback_app_info = default_app_info.api_version(vk::API_VERSION_1_0);
                        let mut fallback_create_info = vk::InstanceCreateInfo::default()
                            .application_info(&fallback_app_info)
                            .enabled_extension_names(&instance_extensions);
                        if portability_enumeration_enabled {
                            fallback_create_info = fallback_create_info
                                .flags(vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR);
                        }
                        if let Ok(instance) =
                            unsafe { entry.create_instance(&fallback_create_info, None) }
                        {
                            if vulkan_debug_enabled() {
                                info!(
                                    target: "arcade_libretro::vulkan_debug",
                                    "Vulkan instance retry succeeded with api_version=0x{:x}",
                                    vk::API_VERSION_1_0
                                );
                            }
                            instance
                        } else {
                            return Err(format!("failed to create Vulkan instance: {err:?}"));
                        }
                    } else {
                        return Err(format!("failed to create Vulkan instance: {err:?}"));
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    return Err(format!("failed to create Vulkan instance: {err:?}"));
                }
            }
        };

        let surface = external_window
            .map(|external_window| {
                create_external_vulkan_surface(&entry, &instance, external_window)
            })
            .transpose()?;

        let physical_devices = unsafe { instance.enumerate_physical_devices() }
            .map_err(|err| format!("failed to enumerate Vulkan physical devices: {err:?}"))?;
        let mut physical_device = *physical_devices
            .first()
            .ok_or_else(|| String::from("no Vulkan physical devices were reported"))?;

        let mut destroy_device_callback = None;
        let (
            device,
            queue,
            queue_family_index,
            presentation_queue,
            presentation_queue_family_index,
        ) = if let Some(create_device) = negotiation.and_then(|callbacks| callbacks.create_device) {
            let required_features = vk::PhysicalDeviceFeatures::default();
            let required_device_extensions = [ash::vk::KHR_SWAPCHAIN_NAME.as_ptr()];
            let mut frontend_context = RetroVulkanContext {
                gpu: vk::PhysicalDevice::null(),
                device: vk::Device::null(),
                queue: vk::Queue::null(),
                queue_family_index: 0,
                presentation_queue: vk::Queue::null(),
                presentation_queue_family_index: 0,
            };
            let created = unsafe {
                create_device(
                    &mut frontend_context,
                    instance.handle(),
                    physical_device,
                    surface.unwrap_or(vk::SurfaceKHR::null()),
                    entry.static_fn().get_instance_proc_addr,
                    required_device_extensions.as_ptr(),
                    required_device_extensions.len() as u32,
                    std::ptr::null(),
                    0,
                    &required_features,
                )
            };
            if vulkan_debug_enabled() {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "core Vulkan create_device returned={} surface={} required_extensions={}",
                    created,
                    surface.is_some(),
                    required_device_extensions.len()
                );
            }

            if created {
                if frontend_context.gpu == vk::PhysicalDevice::null()
                    || frontend_context.device == vk::Device::null()
                    || frontend_context.queue == vk::Queue::null()
                {
                    unsafe {
                        instance.destroy_instance(None);
                    }
                    return Err(String::from(
                        "core Vulkan create_device reported success without a valid GPU/device/queue",
                    ));
                }

                physical_device = frontend_context.gpu;
                destroy_device_callback =
                    negotiation.and_then(|callbacks| callbacks.destroy_device);
                let device =
                    unsafe { ash::Device::load(instance.fp_v1_0(), frontend_context.device) };
                (
                    device,
                    frontend_context.queue,
                    frontend_context.queue_family_index,
                    if frontend_context.presentation_queue == vk::Queue::null() {
                        frontend_context.queue
                    } else {
                        frontend_context.presentation_queue
                    },
                    if frontend_context.presentation_queue == vk::Queue::null() {
                        frontend_context.queue_family_index
                    } else {
                        frontend_context.presentation_queue_family_index
                    },
                )
            } else {
                let queue_family_index =
                    choose_vulkan_queue_family_index(&instance, physical_device)?;

                let queue_priorities = [1.0_f32];
                let queue_infos = [vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(queue_family_index)
                    .queue_priorities(&queue_priorities)];
                let device_extensions = surface
                    .is_some()
                    .then_some(vec![ash::vk::KHR_SWAPCHAIN_NAME.as_ptr()])
                    .unwrap_or_default();
                let device_create_info = vk::DeviceCreateInfo::default()
                    .queue_create_infos(&queue_infos)
                    .enabled_extension_names(&device_extensions);

                let device =
                    unsafe { instance.create_device(physical_device, &device_create_info, None) }
                        .map_err(|err| format!("failed to create Vulkan logical device: {err:?}"))?;
                let queue = unsafe { device.get_device_queue(queue_family_index, 0) };
                (device, queue, queue_family_index, queue, queue_family_index)
            }
        } else {
            let queue_family_index = choose_vulkan_queue_family_index(&instance, physical_device)?;

            let queue_priorities = [1.0_f32];
            let queue_infos = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_family_index)
                .queue_priorities(&queue_priorities)];
            let device_extensions = surface
                .is_some()
                .then_some(vec![ash::vk::KHR_SWAPCHAIN_NAME.as_ptr()])
                .unwrap_or_default();
            let device_create_info = vk::DeviceCreateInfo::default()
                .queue_create_infos(&queue_infos)
                .enabled_extension_names(&device_extensions);

            let device =
                unsafe { instance.create_device(physical_device, &device_create_info, None) }
                    .map_err(|err| format!("failed to create Vulkan logical device: {err:?}"))?;
            let queue = unsafe { device.get_device_queue(queue_family_index, 0) };
            (device, queue, queue_family_index, queue, queue_family_index)
        };

        if vulkan_debug_enabled() {
            let queue_properties =
                unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
            let queue_flags = queue_properties
                .get(queue_family_index as usize)
                .map(|properties| properties.queue_flags)
                .unwrap_or(vk::QueueFlags::empty());
            let present_queue_flags = queue_properties
                .get(presentation_queue_family_index as usize)
                .map(|properties| properties.queue_flags)
                .unwrap_or(vk::QueueFlags::empty());
            info!(
                target: "arcade_libretro::vulkan_debug",
                "Vulkan device selection queue_family={} queue_flags={:?} present_queue_family={} present_queue_flags={:?}",
                queue_family_index,
                queue_flags,
                presentation_queue_family_index,
                present_queue_flags
            );
        }

        let interface = RetroHwRenderInterfaceVulkan {
            interface_type: RETRO_HW_RENDER_INTERFACE_VULKAN,
            interface_version: 5,
            handle: runtime as *const HostRuntime as *mut c_void,
            instance: instance.handle(),
            gpu: physical_device,
            device: device.handle(),
            get_device_proc_addr: instance.fp_v1_0().get_device_proc_addr,
            get_instance_proc_addr: entry.static_fn().get_instance_proc_addr,
            queue,
            queue_index: queue_family_index,
            set_image: Some(retro_vulkan_set_image),
            get_sync_index: Some(retro_vulkan_get_sync_index),
            get_sync_index_mask: Some(retro_vulkan_get_sync_index_mask),
            set_command_buffers: Some(retro_vulkan_set_command_buffers),
            wait_sync_index: Some(retro_vulkan_wait_sync_index),
            lock_queue: Some(retro_vulkan_lock_queue),
            unlock_queue: Some(retro_vulkan_unlock_queue),
            set_signal_semaphore: Some(retro_vulkan_set_signal_semaphore),
        };

        let present = if let (Some(surface), Some(external_window)) = (surface, external_window) {
            match create_vulkan_present_state(VulkanPresentConfig {
                entry: &entry,
                instance: &instance,
                physical_device,
                device: &device,
                surface,
                external_window,
                present_queue: presentation_queue,
                present_queue_family_index: presentation_queue_family_index,
            }) {
                Ok(present) => Some(present),
                Err(err) => {
                    warn!("libretro hw-render: external Vulkan presentation unavailable: {err}");
                    let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
                    unsafe {
                        surface_loader.destroy_surface(surface, None);
                    }
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            _entry: entry,
            instance,
            physical_device,
            device,
            _queue: queue,
            queue_family_index,
            presentation_queue,
            sync_index: present
                .as_ref()
                .map(|_| 0)
                .unwrap_or(VULKAN_FALLBACK_SYNC_FRAMES.saturating_sub(1)),
            sync_frames: present
                .as_ref()
                .map(|present| present.images.len() as u32)
                .unwrap_or(VULKAN_FALLBACK_SYNC_FRAMES)
                .max(1),
            waiting_for_core_wait_sync: false,
            wait_sync_generation: 0,
            queue_locked: false,
            destroy_device_callback,
            interface,
            pending_images: VecDeque::new(),
            present,
            readback: None,
        })
    }

    fn summary(&self) -> String {
        let properties = unsafe {
            self.instance
                .get_physical_device_properties(self.physical_device)
        };
        let device_name = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        format!(
            "gpu={device_name}, queue_family={}",
            self.queue_family_index
        )
    }
}

pub(super) fn prepare_vulkan_sync_for_frame(runtime: &HostRuntime) -> Result<()> {
    let mut state = runtime.hw_render_state.lock();
    let external_window = state
        .external_vulkan_window
        .as_ref()
        .map(ExternalVulkanWindow::descriptor);
    let Some(vulkan) = state.vulkan.as_mut() else {
        return Ok(());
    };
    if vulkan.present.is_none() {
        let frames = vulkan.sync_frames.max(1);
        vulkan.sync_index = (vulkan.sync_index + 1) % frames;
        #[cfg(target_os = "macos")]
        {
            // The macOS mupen64plus-next Vulkan path relies on non-blocking fallback sync.
            vulkan.waiting_for_core_wait_sync = false;
        }
        #[cfg(not(target_os = "macos"))]
        {
            vulkan.waiting_for_core_wait_sync = true;
        }
        if vulkan_debug_enabled() {
            let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
            if debug_step < 16 {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "prepare fallback sync index={} frames={}",
                    vulkan.sync_index,
                    frames
                );
            }
        }
        return Ok(());
    }

    let mut retried_recreate = false;
    loop {
        let Some(present) = vulkan.present.as_mut() else {
            return Ok(());
        };

        if let Some(image_index) = present.acquired_image_index {
            vulkan.sync_index = image_index;
            vulkan.sync_frames = present.images.len().max(1) as u32;
            #[cfg(target_os = "macos")]
            {
                // On macOS some Vulkan cores (including mupen64plus-next) do not invoke
                // wait_sync_index for present-mode images; blocking here adds a 50ms/frame stall.
                vulkan.waiting_for_core_wait_sync = false;
            }
            #[cfg(not(target_os = "macos"))]
            {
                vulkan.waiting_for_core_wait_sync = true;
            }
            return Ok(());
        }

        let acquire_result = unsafe {
            present.swapchain_loader.acquire_next_image(
                present.swapchain,
                5_000_000_000,
                present.acquire_semaphore,
                vk::Fence::null(),
            )
        };

        match acquire_result {
            Ok((_, true))
                if !retried_recreate
                    && recreate_vulkan_present_state(
                        vulkan,
                        external_window,
                        "vkAcquireNextImageKHR returned SUBOPTIMAL_KHR",
                    )? =>
            {
                retried_recreate = true;
            }
            Ok((image_index, _)) => {
                if let Some(present) = vulkan.present.as_mut() {
                    present.acquired_image_index = Some(image_index);
                    vulkan.sync_index = image_index;
                    vulkan.sync_frames = present.images.len().max(1) as u32;
                    #[cfg(target_os = "macos")]
                    {
                        // Mirror the non-blocking macOS behavior above for freshly-acquired images.
                        vulkan.waiting_for_core_wait_sync = false;
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        vulkan.waiting_for_core_wait_sync = true;
                    }
                }
                return Ok(());
            }
            Err(err)
                if !retried_recreate
                    && vulkan_present_requires_recreate(err)
                    && recreate_vulkan_present_state(
                        vulkan,
                        external_window,
                        &format!("vkAcquireNextImageKHR returned {err:?}"),
                    )? =>
            {
                retried_recreate = true;
            }
            Err(err) => {
                return Err(anyhow!("failed to acquire Vulkan swapchain image: {err:?}"));
            }
        }
    }
}

struct VulkanPresentConfig<'a> {
    entry: &'a ash::Entry,
    instance: &'a ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: &'a ash::Device,
    surface: vk::SurfaceKHR,
    external_window: ExternalVulkanWindowDescriptor,
    present_queue: vk::Queue,
    present_queue_family_index: u32,
}

fn create_vulkan_present_state(
    config: VulkanPresentConfig<'_>,
) -> std::result::Result<VulkanPresentState, String> {
    let VulkanPresentConfig {
        entry,
        instance,
        physical_device,
        device,
        surface,
        external_window,
        present_queue,
        present_queue_family_index,
    } = config;
    for function_name in [
        b"vkCreateSwapchainKHR\0".as_slice(),
        b"vkDestroySwapchainKHR\0".as_slice(),
        b"vkGetSwapchainImagesKHR\0".as_slice(),
        b"vkAcquireNextImageKHR\0".as_slice(),
        b"vkQueuePresentKHR\0".as_slice(),
    ] {
        let function_name = CStr::from_bytes_with_nul(function_name)
            .map_err(|_| String::from("invalid Vulkan swapchain entrypoint name"))?;
        let proc =
            unsafe { instance.get_device_proc_addr(device.handle(), function_name.as_ptr()) };
        if proc.is_none() {
            return Err(format!(
                "core-provided Vulkan device does not expose required swapchain entrypoint {}",
                function_name.to_string_lossy()
            ));
        }
    }

    let surface_loader = ash::khr::surface::Instance::new(entry, instance);
    let present_supported = unsafe {
        surface_loader.get_physical_device_surface_support(
            physical_device,
            present_queue_family_index,
            surface,
        )
    }
    .map_err(|err| format!("failed to query Vulkan surface present support: {err:?}"))?;
    if !present_supported {
        return Err(format!(
            "Vulkan queue family {present_queue_family_index} does not support presentation to the external surface"
        ));
    }
    let surface_capabilities = unsafe {
        surface_loader.get_physical_device_surface_capabilities(physical_device, surface)
    }
    .map_err(|err| format!("failed to query Vulkan surface capabilities: {err:?}"))?;
    let surface_formats =
        unsafe { surface_loader.get_physical_device_surface_formats(physical_device, surface) }
            .map_err(|err| format!("failed to query Vulkan surface formats: {err:?}"))?;
    let surface_format = surface_formats
        .iter()
        .find(|format| {
            matches!(
                format.format,
                vk::Format::B8G8R8A8_UNORM | vk::Format::R8G8B8A8_UNORM
            )
        })
        .copied()
        .or_else(|| surface_formats.first().copied())
        .ok_or_else(|| String::from("no Vulkan surface formats were reported"))?;
    let present_modes = unsafe {
        surface_loader.get_physical_device_surface_present_modes(physical_device, surface)
    }
    .map_err(|err| format!("failed to query Vulkan present modes: {err:?}"))?;
    let present_mode = present_modes
        .into_iter()
        .find(|mode| *mode == vk::PresentModeKHR::FIFO)
        .unwrap_or(vk::PresentModeKHR::FIFO);
    let extent = if surface_capabilities.current_extent.width != u32::MAX {
        surface_capabilities.current_extent
    } else {
        let (width, height) = external_window.size();
        vk::Extent2D {
            width: width.max(1),
            height: height.max(1),
        }
    };
    let mut image_count = surface_capabilities.min_image_count.max(2);
    if surface_capabilities.max_image_count > 0 {
        image_count = image_count.min(surface_capabilities.max_image_count);
    }
    if !surface_capabilities
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
    {
        return Err(String::from(
            "Vulkan surface does not support COLOR_ATTACHMENT swapchain images",
        ));
    }

    let vulkan_test_metrics = vulkan_test_metrics_enabled();
    let fail_fast_sampling_enabled = vulkan_black_fail_fast_threshold_frames() > 0;
    let debug_or_test_metrics =
        vulkan_debug_enabled() || vulkan_test_metrics || fail_fast_sampling_enabled;
    let debug_swapchain_readback_supported = debug_or_test_metrics
        && surface_capabilities
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::TRANSFER_SRC);
    if vulkan_test_metrics && !debug_swapchain_readback_supported {
        return Err(String::from(
            "Vulkan test metrics mode requires TRANSFER_SRC swapchain usage support",
        ));
    }
    let mut swapchain_usage = vk::ImageUsageFlags::COLOR_ATTACHMENT;
    if debug_swapchain_readback_supported {
        swapchain_usage |= vk::ImageUsageFlags::TRANSFER_SRC;
    } else if vulkan_debug_enabled() {
        info!(
            target: "arcade_libretro::vulkan_debug",
            "swapchain debug readback unavailable supported_usage={:?}",
            surface_capabilities.supported_usage_flags
        );
    }

    let swapchain_loader = ash::khr::swapchain::Device::new(instance, device);
    let create_info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(surface_format.format)
        .image_color_space(surface_format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(swapchain_usage)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(surface_capabilities.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(present_mode)
        .clipped(true);
    let swapchain = unsafe { swapchain_loader.create_swapchain(&create_info, None) }
        .map_err(|err| format!("failed to create Vulkan swapchain: {err:?}"))?;
    let images = unsafe { swapchain_loader.get_swapchain_images(swapchain) }
        .map_err(|err| format!("failed to enumerate Vulkan swapchain images: {err:?}"))?;
    let render_pass = create_vulkan_present_render_pass(device, surface_format.format)?;
    let descriptor_set_layout = create_vulkan_present_descriptor_set_layout(device)?;
    let sampler = create_vulkan_present_sampler(device)?;
    let (pipeline_layout, pipeline) =
        create_vulkan_present_pipeline(device, extent, render_pass, descriptor_set_layout)?;
    let descriptor_pool_sizes = [
        vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::SAMPLER)
            .descriptor_count(images.len() as u32),
        vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(images.len() as u32),
    ];
    let descriptor_pool_info = vk::DescriptorPoolCreateInfo::default()
        .pool_sizes(&descriptor_pool_sizes)
        .max_sets(images.len() as u32);
    let descriptor_pool = unsafe { device.create_descriptor_pool(&descriptor_pool_info, None) }
        .map_err(|err| format!("failed to create Vulkan descriptor pool: {err:?}"))?;
    let descriptor_layouts = vec![descriptor_set_layout; images.len()];
    let descriptor_set_info = vk::DescriptorSetAllocateInfo::default()
        .descriptor_pool(descriptor_pool)
        .set_layouts(&descriptor_layouts);
    let descriptor_sets = unsafe { device.allocate_descriptor_sets(&descriptor_set_info) }
        .map_err(|err| format!("failed to allocate Vulkan descriptor sets: {err:?}"))?;
    let debug_readback = if debug_swapchain_readback_supported {
        Some(create_vulkan_present_debug_readback(
            instance,
            physical_device,
            device,
            extent,
        )?)
    } else {
        None
    };
    let mut image_views = Vec::with_capacity(images.len());
    let mut framebuffers = Vec::with_capacity(images.len());
    for image in &images {
        let view_info = vk::ImageViewCreateInfo::default()
            .image(*image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(surface_format.format)
            .components(vk::ComponentMapping::default())
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1),
            );
        let image_view = unsafe { device.create_image_view(&view_info, None) }
            .map_err(|err| format!("failed to create Vulkan swapchain image view: {err:?}"))?;
        let framebuffer_info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass)
            .attachments(std::slice::from_ref(&image_view))
            .width(extent.width)
            .height(extent.height)
            .layers(1);
        let framebuffer = unsafe { device.create_framebuffer(&framebuffer_info, None) }
            .map_err(|err| format!("failed to create Vulkan framebuffer: {err:?}"))?;
        image_views.push(image_view);
        framebuffers.push(framebuffer);
    }
    let semaphore_info = vk::SemaphoreCreateInfo::default();
    let acquire_semaphore = unsafe { device.create_semaphore(&semaphore_info, None) }
        .map_err(|err| format!("failed to create Vulkan acquire semaphore: {err:?}"))?;
    let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

    let mut frames = Vec::with_capacity(images.len());
    for _ in 0..images.len() {
        let command_pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(present_queue_family_index)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = unsafe { device.create_command_pool(&command_pool_info, None) }
            .map_err(|err| format!("failed to create Vulkan present command pool: {err:?}"))?;
        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let command_buffer = match unsafe { device.allocate_command_buffers(&alloc_info) } {
            Ok(buffers) => buffers.into_iter().next().ok_or_else(|| {
                String::from("failed to allocate a Vulkan present command buffer")
            })?,
            Err(err) => {
                unsafe {
                    device.destroy_command_pool(command_pool, None);
                }
                return Err(format!(
                    "failed to allocate Vulkan present command buffer: {err:?}"
                ));
            }
        };
        let render_semaphore = match unsafe { device.create_semaphore(&semaphore_info, None) } {
            Ok(semaphore) => semaphore,
            Err(err) => {
                unsafe {
                    device.free_command_buffers(command_pool, &[command_buffer]);
                    device.destroy_command_pool(command_pool, None);
                }
                return Err(format!("failed to create Vulkan render semaphore: {err:?}"));
            }
        };
        let fence = match unsafe { device.create_fence(&fence_info, None) } {
            Ok(fence) => fence,
            Err(err) => {
                unsafe {
                    device.destroy_semaphore(render_semaphore, None);
                    device.free_command_buffers(command_pool, &[command_buffer]);
                    device.destroy_command_pool(command_pool, None);
                }
                return Err(format!("failed to create Vulkan present fence: {err:?}"));
            }
        };
        frames.push(VulkanPresentFrameResources {
            command_pool,
            command_buffer,
            render_semaphore,
            fence,
            transient_image_view: None,
        });
    }

    if vulkan_debug_enabled() {
        info!(
            target: "arcade_libretro::vulkan_debug",
            "created Vulkan present state extent={}x{} format={:?} images={} queue_family={} present_queue_family={}",
            extent.width,
            extent.height,
            surface_format.format,
            images.len(),
            present_queue_family_index,
            present_queue_family_index
        );
    }

    Ok(VulkanPresentState {
        surface_loader,
        surface,
        swapchain_loader,
        swapchain,
        extent,
        format: surface_format.format,
        image_initialized: vec![false; images.len()],
        images,
        image_views,
        framebuffers,
        present_queue,
        acquire_semaphore,
        present_queue_family_index,
        acquired_image_index: None,
        render_pass,
        descriptor_set_layout,
        descriptor_pool,
        descriptor_sets,
        pipeline_layout,
        pipeline,
        sampler,
        debug_readback,
        frames,
    })
}

fn create_vulkan_present_debug_readback(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: &ash::Device,
    extent: vk::Extent2D,
) -> std::result::Result<VulkanPresentDebugReadback, String> {
    let staging_capacity = extent.width as usize * extent.height as usize * 4;
    let buffer_info = vk::BufferCreateInfo::default()
        .size(staging_capacity as u64)
        .usage(vk::BufferUsageFlags::TRANSFER_DST)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let staging_buffer = unsafe { device.create_buffer(&buffer_info, None) }
        .map_err(|err| format!("failed to create Vulkan present debug buffer: {err:?}"))?;
    let requirements = unsafe { device.get_buffer_memory_requirements(staging_buffer) };
    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(physical_device) };
    let memory_type_index = find_memory_type_index(
        &memory_properties,
        requirements.memory_type_bits,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )
    .ok_or_else(|| String::from("no suitable Vulkan memory type for present debug readback"))?;
    let alloc_info = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type_index);
    let staging_memory = unsafe { device.allocate_memory(&alloc_info, None) }
        .map_err(|err| format!("failed to allocate Vulkan present debug memory: {err:?}"))?;
    unsafe {
        device
            .bind_buffer_memory(staging_buffer, staging_memory, 0)
            .map_err(|err| format!("failed to bind Vulkan present debug memory: {err:?}"))?;
    }
    Ok(VulkanPresentDebugReadback {
        staging_buffer,
        staging_memory,
        staging_capacity,
    })
}

fn create_external_vulkan_surface(
    entry: &ash::Entry,
    instance: &ash::Instance,
    external_window: ExternalVulkanWindowDescriptor,
) -> std::result::Result<vk::SurfaceKHR, String> {
    match external_window {
        ExternalVulkanWindowDescriptor::Metal { layer, .. } => {
            let metal_surface = ash::ext::metal_surface::Instance::new(entry, instance);
            let create_info = vk::MetalSurfaceCreateInfoEXT::default().layer(layer.cast());
            unsafe { metal_surface.create_metal_surface(&create_info, None) }
                .map_err(|err| format!("failed to create Vulkan Metal surface: {err:?}"))
        }
    }
}

fn choose_vulkan_queue_family_index(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> std::result::Result<u32, String> {
    let properties =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
    properties
        .iter()
        .enumerate()
        .find_map(|(index, properties)| {
            (properties.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                && properties.queue_flags.contains(vk::QueueFlags::COMPUTE))
            .then_some(index as u32)
        })
        .or_else(|| {
            properties
                .iter()
                .enumerate()
                .find_map(|(index, properties)| {
                    properties
                        .queue_flags
                        .contains(vk::QueueFlags::GRAPHICS)
                        .then_some(index as u32)
                })
        })
        .ok_or_else(|| String::from("no Vulkan graphics queue family was reported"))
}

fn create_vulkan_shader_module(
    device: &ash::Device,
    bytes: &[u8],
) -> std::result::Result<vk::ShaderModule, String> {
    let words = ash::util::read_spv(&mut std::io::Cursor::new(bytes))
        .map_err(|err| format!("failed to decode embedded SPIR-V shader: {err}"))?;
    let create_info = vk::ShaderModuleCreateInfo::default().code(&words);
    unsafe { device.create_shader_module(&create_info, None) }
        .map_err(|err| format!("failed to create Vulkan shader module: {err:?}"))
}

fn create_vulkan_present_render_pass(
    device: &ash::Device,
    format: vk::Format,
) -> std::result::Result<vk::RenderPass, String> {
    let attachments = [vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];
    let color_attachment_refs = [vk::AttachmentReference::default()
        .attachment(0)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
    let subpasses = [vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_attachment_refs)];
    let dependencies = [vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)];
    let create_info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(&subpasses)
        .dependencies(&dependencies);
    unsafe { device.create_render_pass(&create_info, None) }
        .map_err(|err| format!("failed to create Vulkan render pass: {err:?}"))
}

fn create_vulkan_present_descriptor_set_layout(
    device: &ash::Device,
) -> std::result::Result<vk::DescriptorSetLayout, String> {
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    ];
    let create_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    unsafe { device.create_descriptor_set_layout(&create_info, None) }
        .map_err(|err| format!("failed to create Vulkan descriptor set layout: {err:?}"))
}

fn create_vulkan_present_sampler(device: &ash::Device) -> std::result::Result<vk::Sampler, String> {
    let create_info = vk::SamplerCreateInfo::default()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .min_lod(0.0)
        .max_lod(0.0);
    unsafe { device.create_sampler(&create_info, None) }
        .map_err(|err| format!("failed to create Vulkan sampler: {err:?}"))
}

fn create_vulkan_present_pipeline(
    device: &ash::Device,
    extent: vk::Extent2D,
    render_pass: vk::RenderPass,
    descriptor_set_layout: vk::DescriptorSetLayout,
) -> std::result::Result<(vk::PipelineLayout, vk::Pipeline), String> {
    let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(std::slice::from_ref(&descriptor_set_layout));
    let pipeline_layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }
        .map_err(|err| format!("failed to create Vulkan pipeline layout: {err:?}"))?;

    let vertex_module = create_vulkan_shader_module(device, VULKAN_PRESENT_VERT_SPV)?;
    let fragment_module = match create_vulkan_shader_module(device, VULKAN_PRESENT_FRAG_SPV) {
        Ok(module) => module,
        Err(err) => {
            unsafe {
                device.destroy_pipeline_layout(pipeline_layout, None);
                device.destroy_shader_module(vertex_module, None);
            }
            return Err(err);
        }
    };
    let entry_point =
        CString::new("main").map_err(|_| String::from("invalid Vulkan shader entry point"))?;
    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_module)
            .name(&entry_point),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_module)
            .name(&entry_point),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let color_blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(
            vk::ColorComponentFlags::R
                | vk::ColorComponentFlags::G
                | vk::ColorComponentFlags::B
                | vk::ColorComponentFlags::A,
        )];
    let color_blend =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(&color_blend_attachments);
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .subpass(0);
    let pipeline = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|(_, err)| format!("failed to create Vulkan graphics pipeline: {err:?}"))?
    .into_iter()
    .next()
    .ok_or_else(|| String::from("Vulkan graphics pipeline creation returned no pipeline"))?;

    unsafe {
        device.destroy_shader_module(fragment_module, None);
        device.destroy_shader_module(vertex_module, None);
    }

    let _ = extent;
    Ok((pipeline_layout, pipeline))
}

fn destroy_vulkan_present_state(device: &ash::Device, state: VulkanPresentState) {
    unsafe {
        device.destroy_semaphore(state.acquire_semaphore, None);
        device.destroy_sampler(state.sampler, None);
        device.destroy_pipeline(state.pipeline, None);
        device.destroy_pipeline_layout(state.pipeline_layout, None);
        device.destroy_descriptor_pool(state.descriptor_pool, None);
        device.destroy_descriptor_set_layout(state.descriptor_set_layout, None);
        for framebuffer in state.framebuffers {
            device.destroy_framebuffer(framebuffer, None);
        }
        for image_view in state.image_views {
            device.destroy_image_view(image_view, None);
        }
        device.destroy_render_pass(state.render_pass, None);
        for frame in state.frames {
            if let Some(image_view) = frame.transient_image_view {
                device.destroy_image_view(image_view, None);
            }
            device.destroy_fence(frame.fence, None);
            device.destroy_semaphore(frame.render_semaphore, None);
            device.free_command_buffers(frame.command_pool, &[frame.command_buffer]);
            device.destroy_command_pool(frame.command_pool, None);
        }
        if let Some(debug_readback) = state.debug_readback {
            device.destroy_buffer(debug_readback.staging_buffer, None);
            device.free_memory(debug_readback.staging_memory, None);
        }
        state
            .swapchain_loader
            .destroy_swapchain(state.swapchain, None);
        state.surface_loader.destroy_surface(state.surface, None);
    }
}

pub(super) fn present_vulkan_image(
    runtime: &HostRuntime,
    vulkan: &mut VulkanInterfaceState,
    source_size: (u32, u32),
    external_window: Option<ExternalVulkanWindowDescriptor>,
) -> Result<bool> {
    if vulkan.present.is_none() {
        if vulkan_debug_enabled() {
            let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
            if debug_step < 16 {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "no present step={} reason=present_state_unavailable pending_images={}",
                    debug_step,
                    vulkan.pending_images.len()
                );
            }
        }
        return Ok(false);
    }
    if current_pending_vulkan_image(vulkan).is_none() {
        if vulkan_debug_enabled() {
            let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
            if debug_step < 16 {
                let acquired_image_index = vulkan
                    .present
                    .as_ref()
                    .and_then(|present| present.acquired_image_index);
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "no present step={} reason=pending_image_missing pending_images={} acquired_image_index={:?}",
                    debug_step,
                    vulkan.pending_images.len(),
                    acquired_image_index
                );
            }
        }
        return Ok(false);
    }

    let debug_enabled = vulkan_debug_enabled();
    let test_metrics_enabled = vulkan_test_metrics_enabled();
    let swapchain_non_black_seen = runtime
        .vulkan_present_metrics
        .lock()
        .swapchain_non_black_seen;
    let debug_step = if debug_enabled {
        Some(VULKAN_DEBUG_STEP_COUNTER.fetch_add(1, Ordering::Relaxed))
    } else {
        None
    };
    let (image_index, present_result, debug_present_readback) = {
        let mut image =
            take_current_pending_vulkan_image(vulkan).expect("pending image checked above");
        let present = vulkan
            .present
            .as_mut()
            .expect("present state checked above");

        if let Some(debug_step) = debug_step.filter(|step| *step < 8) {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "present step={} begin src={}x{} dst={}x{} semaphores={} layout={:?}",
                debug_step,
                source_size.0,
                source_size.1,
                present.extent.width,
                present.extent.height,
                image.semaphores.len(),
                image.image_layout
            );
        }

        let image_index = present.acquired_image_index.take().ok_or_else(|| {
            anyhow!("no acquired Vulkan swapchain image was prepared for this frame")
        })?;

        if let Some(debug_step) = debug_step.filter(|step| *step < 8) {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "present step={} acquired swapchain image {}",
                debug_step,
                image_index
            );
        }

        let framebuffer = present
            .framebuffers
            .get(image_index as usize)
            .copied()
            .ok_or_else(|| anyhow!("invalid Vulkan framebuffer index: {image_index}"))?;
        let descriptor_set = present
            .descriptor_sets
            .get(image_index as usize)
            .copied()
            .ok_or_else(|| anyhow!("invalid Vulkan descriptor set index: {image_index}"))?;
        let frame = present
            .frames
            .get_mut(image_index as usize)
            .ok_or_else(|| anyhow!("invalid Vulkan present frame index: {image_index}"))?;

        let wait_semaphores = std::mem::take(&mut image.semaphores);
        let signal_semaphore = image.signal_semaphore.take();
        let image_subresource_range = image.subresource_range;
        let direct_image_view_sampling = should_sample_pending_vulkan_image_directly(&image);
        let retired_image_view = frame.transient_image_view.take();
        let debug_present_readback = present.debug_readback.as_ref().and_then(|debug| {
            let should_sample_for_debug =
                debug_enabled && VULKAN_PRESENT_DEBUG_COUNTER.load(Ordering::Relaxed) < 8;
            let should_sample_for_test_metrics = test_metrics_enabled && !swapchain_non_black_seen;
            let should_sample_for_fail_fast =
                vulkan_should_sample_for_fail_fast(runtime, swapchain_non_black_seen);
            (should_sample_for_debug
                || should_sample_for_test_metrics
                || should_sample_for_fail_fast)
                .then_some((
                    frame.fence,
                    debug.staging_memory,
                    debug.staging_buffer,
                    debug.staging_capacity,
                    present.extent,
                    present.format,
                    present.images[image_index as usize],
                ))
        });
        let src_queue_family_index = if image.src_queue_family == u32::MAX {
            vk::QUEUE_FAMILY_IGNORED
        } else {
            image.src_queue_family
        };

        unsafe {
            vulkan
                .device
                .wait_for_fences(&[frame.fence], true, 5_000_000_000)
                .map_err(|err| anyhow!("failed waiting for Vulkan present fence: {err:?}"))?;
            if let Some(image_view) = retired_image_view {
                vulkan.device.destroy_image_view(image_view, None);
            }
            vulkan
                .device
                .reset_fences(&[frame.fence])
                .map_err(|err| anyhow!("failed resetting Vulkan present fence: {err:?}"))?;
            vulkan
                .device
                .reset_command_pool(frame.command_pool, vk::CommandPoolResetFlags::empty())
                .map_err(|err| anyhow!("failed resetting Vulkan present command pool: {err:?}"))?;

            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            vulkan
                .device
                .begin_command_buffer(frame.command_buffer, &begin_info)
                .map_err(|err| anyhow!("failed to begin Vulkan present command buffer: {err:?}"))?;

            let image_view = if direct_image_view_sampling {
                let view = create_sampling_image_view_for_pending_image(&vulkan.device, &image)?;
                frame.transient_image_view = Some(view);
                view
            } else {
                image.image_view
            };

            if !direct_image_view_sampling {
                let source_to_sample = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .old_layout(image.image_layout)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_queue_family_index(src_queue_family_index)
                    .dst_queue_family_index(present.present_queue_family_index)
                    .image(image.image)
                    .subresource_range(image_subresource_range);
                vulkan.device.cmd_pipeline_barrier(
                    frame.command_buffer,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[source_to_sample],
                );
            }

            let sampler_info = vk::DescriptorImageInfo::default().sampler(present.sampler);
            let image_info = vk::DescriptorImageInfo::default()
                .image_view(image_view)
                .image_layout(if direct_image_view_sampling {
                    image.image_layout
                } else {
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
                });
            let descriptor_writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(std::slice::from_ref(&sampler_info)),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(std::slice::from_ref(&image_info)),
            ];
            vulkan
                .device
                .update_descriptor_sets(&descriptor_writes, &[]);

            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            }];
            let render_pass_info = vk::RenderPassBeginInfo::default()
                .render_pass(present.render_pass)
                .framebuffer(framebuffer)
                .render_area(vk::Rect2D::default().extent(present.extent))
                .clear_values(&clear_values);
            vulkan.device.cmd_begin_render_pass(
                frame.command_buffer,
                &render_pass_info,
                vk::SubpassContents::INLINE,
            );
            let viewport = vk::Viewport::default()
                .x(0.0)
                .y(0.0)
                .width(present.extent.width as f32)
                .height(present.extent.height as f32)
                .min_depth(0.0)
                .max_depth(1.0);
            let scissor = vk::Rect2D::default().extent(present.extent);
            vulkan.device.cmd_set_viewport(
                frame.command_buffer,
                0,
                std::slice::from_ref(&viewport),
            );
            vulkan
                .device
                .cmd_set_scissor(frame.command_buffer, 0, std::slice::from_ref(&scissor));
            vulkan.device.cmd_bind_pipeline(
                frame.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                present.pipeline,
            );
            vulkan.device.cmd_bind_descriptor_sets(
                frame.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                present.pipeline_layout,
                0,
                std::slice::from_ref(&descriptor_set),
                &[],
            );
            vulkan.device.cmd_draw(frame.command_buffer, 3, 1, 0, 0);
            vulkan.device.cmd_end_render_pass(frame.command_buffer);

            if let Some((_, _, staging_buffer, _, extent, _, swapchain_image)) =
                debug_present_readback
            {
                let to_transfer_src = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(swapchain_image)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .base_mip_level(0)
                            .level_count(1)
                            .base_array_layer(0)
                            .layer_count(1),
                    );
                vulkan.device.cmd_pipeline_barrier(
                    frame.command_buffer,
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_transfer_src],
                );

                let copy_region = vk::BufferImageCopy::default()
                    .buffer_offset(0)
                    .buffer_row_length(0)
                    .buffer_image_height(0)
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .mip_level(0)
                            .base_array_layer(0)
                            .layer_count(1),
                    )
                    .image_extent(vk::Extent3D {
                        width: extent.width,
                        height: extent.height,
                        depth: 1,
                    });
                vulkan.device.cmd_copy_image_to_buffer(
                    frame.command_buffer,
                    swapchain_image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    staging_buffer,
                    std::slice::from_ref(&copy_region),
                );

                let to_present = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                    .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(swapchain_image)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .base_mip_level(0)
                            .level_count(1)
                            .base_array_layer(0)
                            .layer_count(1),
                    );
                vulkan.device.cmd_pipeline_barrier(
                    frame.command_buffer,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_present],
                );
            }

            if !direct_image_view_sampling {
                let source_restore = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_READ)
                    .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
                    .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .new_layout(image.image_layout)
                    .src_queue_family_index(present.present_queue_family_index)
                    .dst_queue_family_index(src_queue_family_index)
                    .image(image.image)
                    .subresource_range(image_subresource_range);
                vulkan.device.cmd_pipeline_barrier(
                    frame.command_buffer,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[source_restore],
                );
            }

            vulkan
                .device
                .end_command_buffer(frame.command_buffer)
                .map_err(|err| anyhow!("failed to end Vulkan present command buffer: {err:?}"))?;

            let mut submit_wait_semaphores = Vec::with_capacity(wait_semaphores.len() + 1);
            submit_wait_semaphores.push(present.acquire_semaphore);
            submit_wait_semaphores.extend(wait_semaphores.iter().copied());
            let submit_wait_stages =
                vec![vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT; submit_wait_semaphores.len()];
            let mut submit_signal_semaphores = Vec::with_capacity(2);
            submit_signal_semaphores.push(frame.render_semaphore);
            if let Some(signal_semaphore) = signal_semaphore {
                submit_signal_semaphores.push(signal_semaphore);
            }
            let submit_info = vk::SubmitInfo::default()
                .wait_semaphores(&submit_wait_semaphores)
                .wait_dst_stage_mask(&submit_wait_stages)
                .command_buffers(std::slice::from_ref(&frame.command_buffer))
                .signal_semaphores(&submit_signal_semaphores);
            vulkan
                .device
                .queue_submit(vulkan.presentation_queue, &[submit_info], frame.fence)
                .map_err(|err| anyhow!("failed submitting Vulkan present work: {err:?}"))?;

            if let Some(debug_step) = debug_step.filter(|step| *step < 8) {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "present step={} submitted present work",
                    debug_step
                );
            }

            let present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(std::slice::from_ref(&frame.render_semaphore))
                .swapchains(std::slice::from_ref(&present.swapchain))
                .image_indices(std::slice::from_ref(&image_index));
            record_vulkan_queue_present_attempt(runtime);
            let present_result = present
                .swapchain_loader
                .queue_present(present.present_queue, &present_info);
            (image_index, present_result, debug_present_readback)
        }
    };

    match present_result {
        Ok(suboptimal) => {
            record_vulkan_queue_present_success(runtime);
            if suboptimal
                && recreate_vulkan_present_state(
                    vulkan,
                    external_window,
                    "vkQueuePresentKHR returned SUBOPTIMAL_KHR",
                )?
            {
                return Ok(false);
            }
        }
        Err(err)
            if vulkan_present_requires_recreate(err)
                && recreate_vulkan_present_state(
                    vulkan,
                    external_window,
                    &format!("vkQueuePresentKHR returned {err:?}"),
                )? =>
        {
            return Ok(false);
        }
        Err(err) => {
            return Err(anyhow!("failed presenting Vulkan swapchain image: {err:?}"));
        }
    }

    if let Some(debug_step) = debug_step.filter(|step| *step < 8) {
        info!(
            target: "arcade_libretro::vulkan_debug",
            "present step={} queue_present completed",
            debug_step
        );
    }

    if let Some(initialized) = vulkan
        .present
        .as_mut()
        .and_then(|present| present.image_initialized.get_mut(image_index as usize))
    {
        *initialized = true;
    }

    if let Some((fence, staging_memory, _, staging_capacity, extent, format, _)) =
        debug_present_readback
    {
        unsafe {
            vulkan
                .device
                .wait_for_fences(&[fence], true, 5_000_000_000)
                .map_err(|err| anyhow!("failed waiting for Vulkan present debug fence: {err:?}"))?;
            let mapped = vulkan
                .device
                .map_memory(
                    staging_memory,
                    0,
                    staging_capacity as u64,
                    vk::MemoryMapFlags::empty(),
                )
                .map_err(|err| anyhow!("failed to map Vulkan present debug memory: {err:?}"))?;
            let pixels = std::slice::from_raw_parts(mapped as *const u8, staging_capacity);
            let mut normalized = pixels.to_vec();
            if matches!(
                format,
                vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB
            ) {
                for pixel in normalized.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
            }
            let (checksum, non_black_pixels, first_rgba, center_rgba) =
                summarize_rgba_debug_pixels_grid(&normalized, extent.width, extent.height);
            let swapchain_non_black_seen = !vulkan_force_black_test_mode() && non_black_pixels > 0;
            record_vulkan_swapchain_non_black_sample(runtime, swapchain_non_black_seen);
            if debug_enabled {
                let frame_index = VULKAN_PRESENT_DEBUG_COUNTER.fetch_add(1, Ordering::Relaxed);
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "present readback frame={} size={}x{} format={:?} checksum=0x{checksum:016x} non_black_samples={}/64 first_rgba={:02x},{:02x},{:02x},{:02x} center_rgba={:02x},{:02x},{:02x},{:02x}",
                    frame_index,
                    extent.width,
                    extent.height,
                    format,
                    non_black_pixels,
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
            vulkan.device.unmap_memory(staging_memory);
        }
    }

    Ok(true)
}

fn vulkan_present_requires_recreate(result: vk::Result) -> bool {
    matches!(
        result,
        vk::Result::ERROR_OUT_OF_DATE_KHR | vk::Result::ERROR_SURFACE_LOST_KHR
    )
}

fn recreate_vulkan_present_state(
    vulkan: &mut VulkanInterfaceState,
    external_window: Option<ExternalVulkanWindowDescriptor>,
    trigger: &str,
) -> Result<bool> {
    let Some(external_window) = external_window else {
        return Ok(false);
    };
    let Some(old_present) = vulkan.present.take() else {
        return Ok(false);
    };

    warn!(
        target: "arcade_libretro::vulkan_debug",
        "recreating Vulkan present state after {trigger}"
    );

    wait_for_vulkan_device_idle(vulkan)?;

    let surface = create_external_vulkan_surface(&vulkan._entry, &vulkan.instance, external_window)
        .map_err(|err| anyhow!("failed to recreate Vulkan surface: {err}"))?;
    let present_queue_family_index = old_present.present_queue_family_index;
    destroy_vulkan_present_state(&vulkan.device, old_present);

    match create_vulkan_present_state(VulkanPresentConfig {
        entry: &vulkan._entry,
        instance: &vulkan.instance,
        physical_device: vulkan.physical_device,
        device: &vulkan.device,
        surface,
        external_window,
        present_queue: vulkan.presentation_queue,
        present_queue_family_index,
    }) {
        Ok(present) => {
            if vulkan_debug_enabled() {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "recreated Vulkan present state {}x{} images={}",
                    present.extent.width,
                    present.extent.height,
                    present.images.len()
                );
            }
            vulkan.sync_index = 0;
            vulkan.sync_frames = present.images.len().max(1) as u32;
            vulkan.pending_images.clear();
            vulkan.present = Some(present);
            Ok(true)
        }
        Err(err) => {
            let surface_loader = ash::khr::surface::Instance::new(&vulkan._entry, &vulkan.instance);
            unsafe {
                surface_loader.destroy_surface(surface, None);
            }
            Err(anyhow!("failed to recreate Vulkan swapchain: {err}"))
        }
    }
}

pub(super) fn destroy_vulkan_interface_state(state: VulkanInterfaceState) {
    if let Some(present) = state.present {
        destroy_vulkan_present_state(&state.device, present);
    }
    if let Some(readback) = state.readback {
        unsafe {
            state.device.destroy_fence(readback.fence, None);
            state
                .device
                .free_command_buffers(readback.command_pool, &[readback.command_buffer]);
            state
                .device
                .destroy_command_pool(readback.command_pool, None);
            state.device.destroy_buffer(readback.staging_buffer, None);
            state.device.free_memory(readback.staging_memory, None);
        }
    }
    if let Some(destroy_device) = state.destroy_device_callback {
        unsafe {
            destroy_device();
        }
    }
    unsafe {
        state.device.destroy_device(None);
        state.instance.destroy_instance(None);
    }
}

pub(super) fn session_allows_external_vulkan_present(runtime: &HostRuntime) -> bool {
    runtime
        .video_coordinator
        .lock()
        .current_selection()
        .map(|selection| selection.allows_external_present)
        .unwrap_or(false)
}

pub(super) fn ensure_vulkan_interface_state_for(
    runtime: &HostRuntime,
) -> std::result::Result<String, String> {
    let negotiation = {
        let state = runtime.hw_render_state.lock();
        if let Some(vulkan) = state.vulkan.as_ref() {
            return Ok(vulkan.summary());
        }
        state.vulkan_negotiation
    };
    let external_window = if session_allows_external_vulkan_present(runtime) {
        ensure_external_vulkan_window_for(runtime)
    } else {
        None
    };

    let vulkan = VulkanInterfaceState::create(runtime, negotiation, external_window)?;
    let summary = vulkan.summary();
    if vulkan_debug_enabled() {
        info!(
            target: "arcade_libretro::vulkan_debug",
            "created Vulkan interface state {summary}"
        );
    }

    let mut state = runtime.hw_render_state.lock();
    if let Some(existing) = state.vulkan.as_ref() {
        return Ok(existing.summary());
    }
    state.vulkan = Some(vulkan);
    Ok(summary)
}

pub(super) fn rebuild_vulkan_interface_state_for(
    runtime: &HostRuntime,
) -> std::result::Result<String, String> {
    let existing = {
        let mut state = runtime.hw_render_state.lock();
        state.vulkan.take()
    };
    if let Some(existing) = existing {
        destroy_vulkan_interface_state(existing);
    }
    ensure_vulkan_interface_state_for(runtime)
}

pub(super) fn find_memory_type_index(
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    memory_type_bits: u32,
    required_flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    (0..memory_properties.memory_type_count).find(|index| {
        let supported = (memory_type_bits & (1_u32 << index)) != 0;
        let flags = memory_properties.memory_types[*index as usize].property_flags;
        supported && flags.contains(required_flags)
    })
}

pub(super) fn ensure_vulkan_readback_resources(
    vulkan: &mut VulkanInterfaceState,
    required_size: usize,
) -> Result<()> {
    if vulkan
        .readback
        .as_ref()
        .map(|readback| readback.staging_capacity >= required_size)
        .unwrap_or(false)
    {
        return Ok(());
    }

    if let Some(readback) = vulkan.readback.take() {
        unsafe {
            vulkan.device.destroy_fence(readback.fence, None);
            vulkan
                .device
                .free_command_buffers(readback.command_pool, &[readback.command_buffer]);
            vulkan
                .device
                .destroy_command_pool(readback.command_pool, None);
            vulkan.device.destroy_buffer(readback.staging_buffer, None);
            vulkan.device.free_memory(readback.staging_memory, None);
        }
    }

    let command_pool_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(vulkan.queue_family_index)
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
    let command_pool = unsafe { vulkan.device.create_command_pool(&command_pool_info, None) }
        .map_err(|err| anyhow!("failed to create Vulkan command pool: {err:?}"))?;

    let alloc_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    let command_buffer = unsafe { vulkan.device.allocate_command_buffers(&alloc_info) }
        .map_err(|err| anyhow!("failed to allocate Vulkan command buffer: {err:?}"))?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("failed to allocate a Vulkan command buffer"))?;

    let fence_info = vk::FenceCreateInfo::default();
    let fence = unsafe { vulkan.device.create_fence(&fence_info, None) }
        .map_err(|err| anyhow!("failed to create Vulkan fence: {err:?}"))?;

    let buffer_info = vk::BufferCreateInfo::default()
        .size(required_size as u64)
        .usage(vk::BufferUsageFlags::TRANSFER_DST)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let staging_buffer = unsafe { vulkan.device.create_buffer(&buffer_info, None) }
        .map_err(|err| anyhow!("failed to create Vulkan staging buffer: {err:?}"))?;
    let requirements = unsafe { vulkan.device.get_buffer_memory_requirements(staging_buffer) };
    let memory_properties = unsafe {
        vulkan
            .instance
            .get_physical_device_memory_properties(vulkan.physical_device)
    };
    let memory_type_index = find_memory_type_index(
        &memory_properties,
        requirements.memory_type_bits,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )
    .ok_or_else(|| anyhow!("no suitable Vulkan host-visible memory type found"))?;
    let alloc_info = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type_index);
    let staging_memory = unsafe { vulkan.device.allocate_memory(&alloc_info, None) }
        .map_err(|err| anyhow!("failed to allocate Vulkan staging memory: {err:?}"))?;
    unsafe {
        vulkan
            .device
            .bind_buffer_memory(staging_buffer, staging_memory, 0)
            .map_err(|err| anyhow!("failed to bind Vulkan staging memory: {err:?}"))?;
    }

    vulkan.readback = Some(VulkanReadbackState {
        command_pool,
        command_buffer,
        fence,
        staging_buffer,
        staging_memory,
        staging_capacity: required_size,
    });

    Ok(())
}

pub(super) fn debug_readback_vulkan_source_image(
    runtime: &HostRuntime,
    vulkan: &mut VulkanInterfaceState,
    frame_size: (u32, u32),
) -> Result<()> {
    let debug_enabled = vulkan_debug_enabled();
    let test_metrics_enabled = vulkan_test_metrics_enabled();
    let source_non_black_seen = runtime.vulkan_present_metrics.lock().source_non_black_seen;
    let should_sample_for_debug =
        debug_enabled && VULKAN_SOURCE_IMAGE_DEBUG_COUNTER.load(Ordering::Relaxed) < 8;
    let should_sample_for_test_metrics = test_metrics_enabled && !source_non_black_seen;
    let should_sample_for_fail_fast = vulkan.present.is_some()
        && vulkan_should_sample_for_fail_fast(runtime, source_non_black_seen);
    if !should_sample_for_debug && !should_sample_for_test_metrics && !should_sample_for_fail_fast {
        return Ok(());
    }
    if frame_size.0 <= 1 || frame_size.1 <= 1 {
        if vulkan_handoff_trace_enabled() {
            let trace_index = VULKAN_HANDOFF_TRACE_COUNTER.fetch_add(1, Ordering::Relaxed);
            if trace_index < 32 || trace_index % 60 == 0 {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "handoff trace: skipping source image probe for tiny frame={}x{}",
                    frame_size.0,
                    frame_size.1
                );
            }
        }
        return Ok(());
    }

    let Some(image) = current_pending_vulkan_image(vulkan) else {
        return Ok(());
    };
    if should_sample_pending_vulkan_image_directly(image) {
        return Ok(());
    }
    if !image.semaphores.is_empty() || image.signal_semaphore.is_some() {
        return Ok(());
    }

    let format = image.format;
    let image_handle = image.image;
    let image_layout = image.image_layout;
    let image_subresource_range = image.subresource_range;
    let image_subresource_layers = image.subresource_layers;
    let src_queue_family_index = if image.src_queue_family == u32::MAX {
        vk::QUEUE_FAMILY_IGNORED
    } else {
        image.src_queue_family
    };

    let needs_bgra_swizzle = match format {
        vk::Format::R8G8B8A8_UNORM | vk::Format::R8G8B8A8_SRGB => false,
        vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB => true,
        _ => {
            if debug_enabled {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "source image debug readback skipped unsupported_format={:?}",
                    format
                );
            }
            return Ok(());
        }
    };

    let required_size = frame_size.0 as usize * frame_size.1 as usize * 4;
    ensure_vulkan_readback_resources(vulkan, required_size)?;

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

    unsafe {
        vulkan
            .device
            .reset_fences(&[fence])
            .map_err(|err| anyhow!("failed to reset Vulkan source debug fence: {err:?}"))?;
        vulkan
            .device
            .reset_command_pool(readback.command_pool, vk::CommandPoolResetFlags::empty())
            .map_err(|err| anyhow!("failed to reset Vulkan source debug command pool: {err:?}"))?;

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        vulkan
            .device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| {
                anyhow!("failed to begin Vulkan source debug command buffer: {err:?}")
            })?;

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
            .map_err(|err| anyhow!("failed to end Vulkan source debug command buffer: {err:?}"))?;

        let submit_info =
            vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&command_buffer));
        vulkan
            .device
            .queue_submit(queue, &[submit_info], fence)
            .map_err(|err| anyhow!("failed to submit Vulkan source debug readback: {err:?}"))?;
        vulkan
            .device
            .wait_for_fences(&[fence], true, 5_000_000_000)
            .map_err(|err| anyhow!("failed waiting for Vulkan source debug fence: {err:?}"))?;

        let mapped = vulkan
            .device
            .map_memory(
                staging_memory,
                0,
                required_size as u64,
                vk::MemoryMapFlags::empty(),
            )
            .map_err(|err| anyhow!("failed to map Vulkan source debug staging memory: {err:?}"))?;
        let src = std::slice::from_raw_parts(mapped as *const u8, required_size);
        let mut pixels = src.to_vec();
        vulkan.device.unmap_memory(staging_memory);

        if needs_bgra_swizzle {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }

        let (checksum, non_black_pixels, first_rgba, center_rgba) =
            summarize_rgba_debug_pixels_grid(&pixels, frame_size.0, frame_size.1);
        let source_non_black_seen = !vulkan_force_black_test_mode() && non_black_pixels > 0;
        record_vulkan_source_non_black_sample(runtime, source_non_black_seen);
        if debug_enabled {
            let frame_index = VULKAN_SOURCE_IMAGE_DEBUG_COUNTER.fetch_add(1, Ordering::Relaxed);
            info!(
                target: "arcade_libretro::vulkan_debug",
                "source image readback frame={} size={}x{} format={:?} checksum=0x{checksum:016x} non_black_samples={}/64 first_rgba={:02x},{:02x},{:02x},{:02x} center_rgba={:02x},{:02x},{:02x},{:02x}",
                frame_index,
                frame_size.0,
                frame_size.1,
                format,
                non_black_pixels,
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
    }

    Ok(())
}

#[cfg(test)]
pub(super) fn probe_basic_vulkan_bootstrap() -> std::result::Result<String, String> {
    let app_name =
        CString::new("Let's Play").map_err(|_| String::from("failed to build Vulkan app name"))?;
    let engine_name = CString::new("arcade-libretro")
        .map_err(|_| String::from("failed to build Vulkan engine name"))?;

    let entry = load_vulkan_entry()?;

    let app_info = vk::ApplicationInfo::default()
        .application_name(&app_name)
        .application_version(0)
        .engine_name(&engine_name)
        .engine_version(0)
        .api_version(vk::API_VERSION_1_0);
    let create_info = vk::InstanceCreateInfo::default().application_info(&app_info);

    let instance = unsafe { entry.create_instance(&create_info, None) }
        .map_err(|err| format!("failed to create Vulkan instance: {err:?}"))?;

    let result = (|| {
        let physical_devices = unsafe { instance.enumerate_physical_devices() }
            .map_err(|err| format!("failed to enumerate Vulkan physical devices: {err:?}"))?;
        let physical_device = *physical_devices
            .first()
            .ok_or_else(|| String::from("no Vulkan physical devices were reported"))?;

        let queue_family_index =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) }
                .iter()
                .enumerate()
                .find_map(|(index, properties)| {
                    properties
                        .queue_flags
                        .contains(vk::QueueFlags::GRAPHICS)
                        .then_some(index as u32)
                })
                .ok_or_else(|| String::from("no Vulkan graphics queue family was reported"))?;

        let queue_priorities = [1.0_f32];
        let queue_infos = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family_index)
            .queue_priorities(&queue_priorities)];
        let device_create_info = vk::DeviceCreateInfo::default().queue_create_infos(&queue_infos);

        let device = unsafe { instance.create_device(physical_device, &device_create_info, None) }
            .map_err(|err| format!("failed to create Vulkan logical device: {err:?}"))?;

        let summary = {
            let properties = unsafe { instance.get_physical_device_properties(physical_device) };
            let device_name = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            let _queue = unsafe { device.get_device_queue(queue_family_index, 0) };
            format!("gpu={device_name}, queue_family={queue_family_index}")
        };

        unsafe {
            device.destroy_device(None);
        }

        Ok(summary)
    })();

    unsafe {
        instance.destroy_instance(None);
    }

    result
}

#[cfg(test)]
pub(super) fn vulkan_unimplemented_message(runtime: Option<&HostRuntime>) -> String {
    let frontend_summary = runtime.and_then(frontend_windowing_summary_for);

    match probe_basic_vulkan_bootstrap() {
        Ok(vulkan_summary) => match frontend_summary {
            Some(frontend_summary) => format!(
                "core requested Vulkan hardware render, and the frontend can expose {frontend_summary}; basic Vulkan bootstrap succeeded ({vulkan_summary}), but this host does not implement Vulkan frame presentation yet"
            ),
            None => format!(
                "core requested Vulkan hardware render; basic Vulkan bootstrap succeeded ({vulkan_summary}), but this host does not implement Vulkan frame presentation yet"
            ),
        },
        Err(probe_error) => match frontend_summary {
            Some(frontend_summary) => format!(
                "core requested Vulkan hardware render, and the frontend can expose {frontend_summary}, but basic Vulkan bootstrap failed: {probe_error}"
            ),
            None => format!(
                "core requested Vulkan hardware render, but basic Vulkan bootstrap failed: {probe_error}"
            ),
        },
    }
}

pub(super) unsafe extern "C" fn retro_vulkan_set_image(
    handle: *mut c_void,
    image: *const RetroVulkanImage,
    num_semaphores: u32,
    semaphores: *const vk::Semaphore,
    src_queue_family: u32,
) {
    if handle.is_null() || image.is_null() {
        return;
    }

    let runtime = unsafe { &*(handle as *const HostRuntime) };
    let image = unsafe { &*image };
    let semaphore_values = if num_semaphores == 0 || semaphores.is_null() {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(semaphores, num_semaphores as usize) }.to_vec()
    };

    let mut state = runtime.hw_render_state.lock();
    let Some(vulkan) = state.vulkan.as_mut() else {
        return;
    };
    let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
    if vulkan_debug_enabled() && debug_step < 8 {
        info!(
            target: "arcade_libretro::vulkan_debug",
            "retro_vulkan_set_image step={} format={:?} src_queue_family={} semaphores={} base_mip={} base_layer={} layers={}",
            debug_step,
            image.create_info.format,
            src_queue_family,
            semaphore_values.len(),
            image.create_info.subresource_range.base_mip_level,
            image.create_info.subresource_range.base_array_layer,
            image.create_info.subresource_range.layer_count.max(1)
        );
    }
    vulkan.pending_images.push_back(PendingVulkanImage {
        image: image.create_info.image,
        image_view: image.image_view,
        image_layout: image.image_layout,
        format: image.create_info.format,
        view_type: image.create_info.view_type,
        components: image.create_info.components,
        subresource_range: PendingVulkanImage::color_subresource_range(&image.create_info),
        subresource_layers: PendingVulkanImage::color_subresource_layers(&image.create_info),
        semaphores: semaphore_values,
        src_queue_family,
        signal_semaphore: None,
    });
    let max_pending_images = vulkan.sync_frames.max(1) as usize + 2;
    while vulkan.pending_images.len() > max_pending_images {
        vulkan.pending_images.pop_front();
    }
}

pub(super) unsafe extern "C" fn retro_vulkan_get_sync_index(_handle: *mut c_void) -> u32 {
    let Some(runtime) = active_runtime() else {
        return 0;
    };
    current_vulkan_sync_index(&runtime)
}

pub(super) unsafe extern "C" fn retro_vulkan_get_sync_index_mask(_handle: *mut c_void) -> u32 {
    let Some(runtime) = active_runtime() else {
        return 1;
    };
    current_vulkan_sync_mask(&runtime)
}

pub(super) unsafe extern "C" fn retro_vulkan_set_command_buffers(
    handle: *mut c_void,
    num_cmd: u32,
    cmd: *const vk::CommandBuffer,
) {
    if handle.is_null() || cmd.is_null() || num_cmd == 0 {
        return;
    }
    let runtime = unsafe { &*(handle as *const HostRuntime) };
    let command_buffers = unsafe { std::slice::from_raw_parts(cmd, num_cmd as usize) };
    if vulkan_debug_enabled() {
        let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
        if debug_step < 8 {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "set_command_buffers step={} count={}",
                debug_step,
                command_buffers.len()
            );
        }
    }
    if let Err(err) = submit_vulkan_command_buffers_immediately(runtime, command_buffers) {
        warn!("libretro hw-render: {err}");
        record_runtime_load_error(err.to_string());
    }
}

pub(super) unsafe extern "C" fn retro_vulkan_wait_sync_index(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }

    let runtime = unsafe { &*(handle as *const HostRuntime) };
    let mut state = runtime.hw_render_state.lock();
    let Some(vulkan) = state.vulkan.as_mut() else {
        return;
    };
    let Some(present) = vulkan.present.as_ref() else {
        let sync_index = vulkan.sync_index;
        #[cfg(target_os = "macos")]
        {
            if vulkan_force_fallback_idle() {
                let wait_result = wait_for_vulkan_device_idle(vulkan);
                if let Err(err) = wait_result {
                    warn!(
                        "libretro hw-render: wait_sync_index failed waiting for fallback queue idle: {err}"
                    );
                    record_runtime_load_error(format!(
                        "wait_sync_index failed waiting for fallback queue idle: {err}"
                    ));
                }
            }
            if vulkan_debug_enabled() {
                let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
                if debug_step < 16 {
                    info!(
                        target: "arcade_libretro::vulkan_debug",
                        "wait_sync_index completed fallback step={} sync_index={} force_idle={}",
                        debug_step,
                        sync_index,
                        vulkan_force_fallback_idle()
                    );
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let wait_result = wait_for_vulkan_device_idle(vulkan);
            if let Err(err) = wait_result {
                warn!(
                    "libretro hw-render: wait_sync_index failed waiting for fallback queue idle: {err}"
                );
                record_runtime_load_error(format!(
                    "wait_sync_index failed waiting for fallback queue idle: {err}"
                ));
            } else if vulkan_debug_enabled() {
                let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
                if debug_step < 16 {
                    info!(
                        target: "arcade_libretro::vulkan_debug",
                        "wait_sync_index completed fallback step={} sync_index={}",
                        debug_step,
                        sync_index
                    );
                }
            }
        }
        vulkan.waiting_for_core_wait_sync = false;
        vulkan.wait_sync_generation = vulkan.wait_sync_generation.wrapping_add(1);
        return;
    };
    let Some(frame) = present
        .frames
        .get(vulkan.sync_index as usize)
        .or_else(|| present.frames.first())
    else {
        return;
    };

    let wait_result = unsafe {
        vulkan
            .device
            .wait_for_fences(&[frame.fence], true, 5_000_000_000)
    };
    if let Err(err) = wait_result {
        warn!("libretro hw-render: wait_sync_index failed waiting for present fence: {err:?}");
        record_runtime_load_error(format!(
            "wait_sync_index failed waiting for present fence: {err:?}"
        ));
    } else if vulkan_debug_enabled() {
        let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
        if debug_step < 16 {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "wait_sync_index completed step={} sync_index={}",
                debug_step,
                vulkan.sync_index
            );
        }
    }
    vulkan.waiting_for_core_wait_sync = false;
    vulkan.wait_sync_generation = vulkan.wait_sync_generation.wrapping_add(1);
}

pub(super) unsafe extern "C" fn retro_vulkan_lock_queue(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    let runtime = unsafe { &*(handle as *const HostRuntime) };
    lock_vulkan_queue(runtime);
}

pub(super) unsafe extern "C" fn retro_vulkan_unlock_queue(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    let runtime = unsafe { &*(handle as *const HostRuntime) };
    unlock_vulkan_queue(runtime);
}

pub(super) unsafe extern "C" fn retro_vulkan_set_signal_semaphore(
    handle: *mut c_void,
    semaphore: vk::Semaphore,
) {
    if handle.is_null() {
        return;
    }
    let runtime = unsafe { &*(handle as *const HostRuntime) };
    let mut state = runtime.hw_render_state.lock();
    let Some(vulkan) = state.vulkan.as_mut() else {
        return;
    };
    if let Some(image) = vulkan.pending_images.back_mut() {
        if vulkan_debug_enabled() {
            let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
            if debug_step < 8 {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "set_signal_semaphore step={} semaphore={:?}",
                    debug_step,
                    semaphore
                );
            }
        }
        image.signal_semaphore = Some(semaphore);
    }
}

pub(super) fn wait_for_unsignaled_vulkan_image(runtime: &HostRuntime) -> Result<()> {
    let mut logged_wait = false;
    let mut logged_wait_sync = false;
    let started_at = std::time::Instant::now();

    loop {
        let mut state = runtime.hw_render_state.lock();
        let Some(vulkan) = state.vulkan.as_mut() else {
            return Ok(());
        };
        let Some(image) = current_pending_vulkan_image(vulkan) else {
            return Ok(());
        };
        if !image.semaphores.is_empty() || image.signal_semaphore.is_some() {
            return Ok(());
        }
        if vulkan.queue_locked {
            drop(state);
            if !logged_wait && vulkan_debug_enabled() {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "waiting for core Vulkan queue unlock before consuming unsignaled image"
                );
                logged_wait = true;
            }
            if started_at.elapsed() > std::time::Duration::from_secs(5) {
                return Err(anyhow!(
                    "timed out waiting for core Vulkan queue unlock for unsignaled image"
                ));
            }
            std::thread::yield_now();
            continue;
        }

        if vulkan.waiting_for_core_wait_sync {
            if vulkan.present.is_none() {
                vulkan.waiting_for_core_wait_sync = false;
                return Ok(());
            }
            drop(state);
            if !logged_wait_sync && vulkan_debug_enabled() {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "waiting for core wait_sync_index before consuming unsignaled image"
                );
                logged_wait_sync = true;
            }
            if started_at.elapsed() > std::time::Duration::from_millis(50) {
                if vulkan_debug_enabled() {
                    let state = runtime.hw_render_state.lock();
                    if let Some(vulkan) = state.vulkan.as_ref() {
                        info!(
                            target: "arcade_libretro::vulkan_debug",
                            "timed out waiting for core wait_sync_index; proceeding without forced device idle sync_index={} generation={} force_idle={}",
                            vulkan.sync_index,
                            vulkan.wait_sync_generation,
                            vulkan_force_fallback_idle()
                        );
                    }
                }
                let mut state = runtime.hw_render_state.lock();
                if let Some(vulkan) = state.vulkan.as_mut() {
                    vulkan.waiting_for_core_wait_sync = false;
                    if vulkan_force_fallback_idle() {
                        if vulkan_debug_enabled()
                            && VULKAN_READBACK_DEBUG_COUNTER.load(Ordering::Relaxed) < 16
                        {
                            info!(
                                target: "arcade_libretro::vulkan_debug",
                                "forcing Vulkan device idle for unsignaled image readiness"
                            );
                        }
                        return wait_for_vulkan_device_idle(vulkan);
                    }
                }
                return Ok(());
            }
            std::thread::yield_now();
            continue;
        }

        if should_sample_pending_vulkan_image_directly(image) {
            return Ok(());
        }

        #[cfg(target_os = "macos")]
        if vulkan.present.is_some() {
            // Minimal-present mode: do not force device-idle synchronization every
            // frame. This keeps frontend overhead low and lets the core own pacing.
            if vulkan_force_present_idle() {
                if vulkan_debug_enabled()
                    && VULKAN_READBACK_DEBUG_COUNTER.load(Ordering::Relaxed) < 16
                {
                    info!(
                        target: "arcade_libretro::vulkan_debug",
                        "forcing Vulkan device idle for present-mode unsignaled image readiness"
                    );
                }
                return wait_for_vulkan_device_idle(vulkan);
            }
            return Ok(());
        }

        if vulkan_force_fallback_idle() {
            if vulkan_debug_enabled() && VULKAN_READBACK_DEBUG_COUNTER.load(Ordering::Relaxed) < 16
            {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "forcing Vulkan device idle for unsignaled image readiness"
                );
            }
            return wait_for_vulkan_device_idle(vulkan);
        }
        return Ok(());
    }
}

pub(super) fn submit_vulkan_command_buffers_immediately(
    runtime: &HostRuntime,
    command_buffers: &[vk::CommandBuffer],
) -> Result<()> {
    if command_buffers.is_empty() {
        return Ok(());
    }

    let mut state = runtime.hw_render_state.lock();
    let Some(vulkan) = state.vulkan.as_mut() else {
        return Err(anyhow!(
            "Vulkan command buffers were submitted before Vulkan interface setup"
        ));
    };

    let fence_info = vk::FenceCreateInfo::default();
    let fence = unsafe { vulkan.device.create_fence(&fence_info, None) }
        .map_err(|err| anyhow!("failed to create Vulkan command fence: {err:?}"))?;

    let submit_info = vk::SubmitInfo::default().command_buffers(command_buffers);
    let submit_result = unsafe {
        vulkan
            .device
            .queue_submit(vulkan.interface.queue, &[submit_info], fence)
    };
    if let Err(err) = submit_result {
        unsafe {
            vulkan.device.destroy_fence(fence, None);
        }
        return Err(anyhow!(
            "failed to submit core Vulkan command buffers: {err:?}"
        ));
    }

    let wait_result = unsafe { vulkan.device.wait_for_fences(&[fence], true, 5_000_000_000) };
    unsafe {
        vulkan.device.destroy_fence(fence, None);
    }
    wait_result.map_err(|err| anyhow!("timed out waiting for core Vulkan command buffers: {err:?}"))
}

pub(super) fn wait_for_vulkan_device_idle(vulkan: &VulkanInterfaceState) -> Result<()> {
    unsafe {
        vulkan
            .device
            .device_wait_idle()
            .map_err(|err| anyhow!("failed waiting for Vulkan device idle: {err:?}"))
    }
}

pub(super) fn lock_vulkan_queue(runtime: &HostRuntime) {
    loop {
        {
            let mut state = runtime.hw_render_state.lock();
            let Some(vulkan) = state.vulkan.as_mut() else {
                return;
            };
            if !vulkan.queue_locked {
                vulkan.queue_locked = true;
                if vulkan_debug_enabled() {
                    let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
                    if debug_step < 16 {
                        info!(
                            target: "arcade_libretro::vulkan_debug",
                            "lock_queue step={} sync_index={}",
                            debug_step,
                            vulkan.sync_index
                        );
                    }
                }
                return;
            }
        }
        std::thread::yield_now();
    }
}

pub(super) fn unlock_vulkan_queue(runtime: &HostRuntime) {
    let mut state = runtime.hw_render_state.lock();
    if let Some(vulkan) = state.vulkan.as_mut() {
        vulkan.queue_locked = false;
        if vulkan_debug_enabled() {
            let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
            if debug_step < 16 {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "unlock_queue step={} sync_index={}",
                    debug_step,
                    vulkan.sync_index
                );
            }
        }
    }
}

pub(super) fn current_vulkan_sync_index(runtime: &HostRuntime) -> u32 {
    let state = runtime.hw_render_state.lock();
    state
        .vulkan
        .as_ref()
        .map(|vulkan| vulkan.sync_index)
        .unwrap_or(0)
}

pub(super) fn current_vulkan_sync_mask(runtime: &HostRuntime) -> u32 {
    let state = runtime.hw_render_state.lock();
    let Some(vulkan) = state.vulkan.as_ref() else {
        return 1;
    };
    let frames = vulkan
        .present
        .as_ref()
        .map(|present| present.images.len() as u32)
        .unwrap_or(vulkan.sync_frames)
        .clamp(1, 31);
    ((1_u32 << frames) - 1).max(1)
}
