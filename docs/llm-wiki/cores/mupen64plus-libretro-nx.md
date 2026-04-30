# Mupen64Plus Libretro NX

This page documents the integration and specific requirements for the Mupen64Plus-based N64 core.

## Overview

The `mupen64plus-libretro-nx` core is a critical component of the project, providing N64 emulation. It is located in `third_party/mupen64plus-libretro-nx`.

## Key Requirements

### Hardware Rendering

This core heavily relies on hardware-accelerated rendering (OpenGL/Vulkan). 

- **Vulkan Integration**: On macOS (Apple Silicon), the core is expected to use the Vulkan backend. The host provides a specialized `NSWindow` with a `CAMetalLayer` to facilitate this.
- **Dynamic Recompiler (Dynarec)**: For optimal performance on ARM64 macOS, the host specifically looks for and attempts to load the `mupen64plus_next_dynarec_arm64_libretro.dylib` binary.

### Audio Pacing

The core is identified as an **audio-master** core. This means the audio callback drives the emulation cadence. The `arcade-libretro` crate implements specific pacing logic to ensure the audio buffer remains stable, preventing stuttering or latency spikes.

## Known Issues / Notes

- **Binary Pathing**: Ensure that the correct ARM64-compatible dynarec binary is present in the `cores/` directory to avoid significant performance degradation or crashes on Apple Silicon.
- **Vulkan Setup**: If the core fails to initialize its Vulkan context, check if the `ARCADE_VULKAN_DEBUG` environment variable is set to inspect the bootstrap process.
