# Current State

This document tracks the current architectural understanding and implementation status of the project.

## Core Architecture

The project follows a host-plugin architecture where a Rust frontend (`arcade-app`) manages the lifecycle of Libretro cores (`arcade-libretro`).

### 1. Core Loading Path
**Path**: `crates/arcade-app/src/main.rs` $\to$ `crates/arcade-libretro/src/lib.rs`

1.  **App Entry**: `main()` in `arcade-app/src/main.rs` initializes `AppConfig` and `LibretroHost`.
2.  **Host Initialization**: `LibretroHost::new()` initializes the `HostRuntime`.
3.  **Core Selection**: `LibretroHost::load_for_rom()` resolves the core name to a path using `resolve_core()`.
4.  **Dynamic Loading**: `LibretroHost::load_core()` uses `libloading::Library::new(core_path)` to load the `.dylib`/`.so`.
5.  **API Extraction**: `load_api()` extracts the `RetroApi` function pointers from the loaded library.
6.  **Core Initialization**: The host calls the core's `retro_init()` and `retro_set_environment()` functions.

### 2. Video Rendering Pipeline

The rendering pipeline varies based on the core's requirements and frontend capabilities.

**Path**: `crates/arcade-libretro/src/video/mod.rs` $\to$ `VideoBackend::consume_frame`

1.  **Frame Preparation**: `LibretroHost::run_frame()` calls `VideoCoordinator::prepare_frame()`.
    - For **Vulkan**, this involves `vulkan_render.rs:prepare_vulkan_sync_for_frame()`.
2.  **Core Execution**: `LibretroHost::run_frame()` calls the core's `retro_run()`.
3.  **Frame Capture**:
    - **Software/GL Path**: The core calls `retro_video_refresh()`. The host captures the buffer and returns `FrameDelivery::CpuFrame`.
    - **Vulkan/Hardware Path**: The core calls `retro_vulkan_set_image()`. The host stores the `PendingVulkanImage`.
4.  **Frame Consumption**: `LibretroHost::run_frame()` calls `VideoCoordinator::consume_frame()`.
    - **Vulkan Backend**: `vulkan.rs:consume_frame()` checks for pending hardware images. If found, it processes the image (potentially via `take_vulkan_render_frame()`) and returns `FrameDelivery::ExternalPresent` or `FrameDelivery::CpuFrame` (after readback).
    - **GL Backend**: `gl.rs:consume_frame()` reads from the GL texture/framebuffer and returns `FrameDelivery::CpuFrame`.

### 3. Hardware Rendering Configuration

Hardware rendering is configured during the core loading phase based on the core's requirements and the host's capabilities.

- **Discovery**: `LibretroHost::load_core()` calls `inspect_core_requirements(core_path)`.
- **Policy Decision**: `VideoCoordinator::plan_session()` uses `policy.rs` to select the backend.
- **Configuration by Core**:
    - **Mupen64Plus (N64)**: Often requests `RETRO_HW_CONTEXT_VULKAN`. The policy selects `VulkanBackend` with `allows_external_present: true`.
    - **Play! (PS2)**: Specifically handled in `policy.rs` to force `OpenGl` and disable external presentation to prevent instability.
    - **Mednafen PSX**: Uses `VulkanBackend` on native ARM64 macOS, but falls back to `Software` on Rosetta (x86_64) to avoid driver crashes.

### 4. Platform-Specific Implementations

- **macOS Rendering**:
    - **Windowing**: `vulkan_render.rs` implements `ExternalVulkanWindow` using `objc` to create an `NSWindow` and `CAMetalLayer`.
    - **GL Context**: `hardware_render.rs:current_gl_ctx_id()` uses `CGLGetCurrentContext` to identify the thread-local GL context.
    - **Library Loading**: `vulkan_render.rs:macos_vulkan_loader_candidates()` searches specific paths like `/opt/homebrew/lib/libvulkan.dylib`.
- **Linux/Windows**: Standard `libloading` and `ash` (Vulkan) / `glow` (OpenGL) paths.

### 5. Environment & Build Flags

| Variable | Purpose |
| :---_ | :--- |
| `ARCADE_VULKAN_DEBUG` | Enables verbose Vulkan tracing and debug synchronization logs. |
| `ARCADE_MACOS_RENDERER` | Forces macOS renderer to `wgpu` (default) or `glow` (OpenGL). |
 
| `LIBRETRO_TRACE_GL_READBACK` | Enables tracing of OpenGL texture/framebuffer readback operations. |
| `LIBRETRO_TRACE_GL_CONTEXT` | Logs details of the active OpenGL context (version, vendor, etc.). |
| `LIBRETRO_TRACE_GL_ERROR` | Logs OpenGL error states during frame processing. |
| `ARCADE_MUPEN64PLUS_NEXT_VIDEO_BACKEND` | Forces Mupen64Plus to use `software` or `vulkan` backend. |
| `ARCADE_LOG_FILE` | Redirects all `tracing` logs to a specified file. |
