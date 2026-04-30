# Libretro Integration

This document describes how the `arcade-libretro` crate integrates the Libretro API into the Rust frontend.

## Core Lifecycle

The `LibretroHost` struct is the primary entry point for managing a core's lifecycle.

1.  **Core Discovery**: The host searches for compatible `.dylib` (macOS) or `.so` (Linux) files in the `cores/` directory. It uses specific naming conventions for certain cores (e.g., searching for ARM64-specific binaries for `mupen64plus_next` on Apple Silicon).
2.  **Loading**: The host uses `libloading` to dynamically load the core library and extract the `RetroApi` function pointers.
3.  **Initialization**: The host calls `retro_init()` and sets up the environment via `retro_set_environment()`.
4.  **Execution Loop**: The host calls `retro_run()` once per frame.
5.  **Unloading**: The host calls `retro_deinit()` and unloads the library.

## Video Pipeline

The project implements a sophisticated multi-backend video system managed by a `VideoCoordinator`.

### Backend Selection

When a core is loaded, the `VideoCoordinator` plans a session based on:
- **Core Requirements**: Whether the core requests hardware rendering (`RETRO_HW_CONTEXT_VULKAN`).
- **Frontend Capabilities**: Whether the frontend can provide a Vulkan surface or a GL context.

### Backends

1.  **Software/GL Backend**: Uses `glow` (OpenGL ES) to render frames provided by the core via `retro_video_refresh`.
2.0 **Vulkan Backend**: A high-performance path for cores requesting hardware rendering.
    - **macOS Integration**: On macOS, the frontend creates a standalone `NSWindow` with a `CAMetalLayer`. This allows the core to present directly to a window managed by the host's Vulkan instance.
    - **Synchronization**: Uses Vulkan semaphores and fences to synchronize frame delivery between the emulator thread and the UI thread.
    - **Debug Readback**: Implements specialized logic to sample swapchain images to detect if the core is actually rendering content, preventing "black screen" hangs.

## Audio Pipeline

The audio system uses `cpal` for cross-platform audio output.

- **Resampling**: A resampler bridges the gap between the core's native sample rate and the host's output device rate.
- **Pacing**: For "audio-master" cores (e.g., Flycast, Mupen64Plus), the audio callback drives the emulation cadence to prevent buffer underruns/overruns.

## Input Handling

Input is managed via `gilrs`. The host polls input devices and pushes the state to the core using `retro_set_input_state` and `retro_set_input_poll`. Keyboard events are dispatched via `retro_set_input_event`.

## VFS (Virtual File System)

The host implements the Libretro VFS interface, allowing cores to access `system/`, `save/`, and `content/` directories through a unified, sandboxed interface.
