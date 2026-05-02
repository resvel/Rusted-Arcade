# Upstream Libretro Notes

This document tracks the architectural differences and integration requirements between the `arcade-libretro` host implementation and the standard Libretro API specification.

## Core Integration Differences

While `arcade-libretro` adheres to the standard Libretro API, it implements several specialized behaviors to optimize for a modern, high-performance desktop environment.

### 1. Video Presentation (The "External" Model)
Standard Libretro often expects the host to provide a window and the core to simply "push" pixels to it. `arcade-libretro` implements a more advanced "External Present" model for Vulkan:
- **The Host-Managed Surface**: For Vulkan-capable cores, the host provides a pre-configured `CAMetalLayer` (on macOS) or a Vulkan surface.
- **The Core's Responsibility**: The core is responsible for performing the actual `vkQueuePresentKHR` or equivalent operations on the provided surface.
- **Benefit**: This allows the host to maintain full control over the `egui` windowing loop and prevents the core from creating disruptive, separate windows.

### 2. Audio Pacing Models
The host must handle two distinct audio threading models:
- **Push Model**: The core calls `retro_audio_sample` or `retro_audio_sample_batch` to push samples to the host.
- **Pull Model (Audio-Master)**: For cores like `flycast` or `mupen64plus_next`, the host must implement a callback that the core calls to "pull" samples.
- **Implementation**: `arcade-libretro` uses `cpal` to bridge these models, implementing a resampler to handle the frequency mismatch between the core's native rate and the host's hardware rate.

### 3. Virtual File System (VFS)
To ensure security and portability, the host implements a sandboxed VFS.
- **Abstraction**: The core does not access the real filesystem directly. Instead, it uses the `RETRO_ENVIRONMENT_SET_PATH` and `RETRO_ENVIRONMENT_GET_PATH` interfaces.
- **Mapping**: The host maps these virtual paths (e.g., `system/`, `saves/`) to specific subdirectories within the project's `data/` or `roms/` directories.

## Integration Challenges

### macOS/ARM64 Specifics
- **Library Loading**: The host must be aware of ARM64-specific naming conventions (e.g., `..._arm64_libretro.dylib`) to ensure the correct dynamic recompiler is loaded on Apple Silicon.
- **Vulkan Loader**: On macOS, the host must proactively search for `libvulkan.dylib` or `libMoltenVK.dylib` in common paths (like Homebrew) to ensure the Vulkan backend can initialize.

### Threading and Synchronization
- **The Emulator Thread vs. UI Thread**: The core runs on a dedicated emulation thread. All interactions with the `egui` UI thread (like updating the video frame or audio queue) must be synchronized using thread-safe primitives (`Arc`, `Mutex`, `Atomic`).
- **GPU Synchronization**: When using the Vulkan backend, the host uses Vulkan semaphores and fences to coordinate the handover of textures from the emulator thread to the presentation queue.
