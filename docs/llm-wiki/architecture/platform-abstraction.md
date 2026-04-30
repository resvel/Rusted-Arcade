# Platform Abstraction

This document describes how `arcade-libretro` abstracts platform-specific functionalities, particularly for macOS, to support high-performance hardware rendering.

## Overview

The project uses conditional compilation (`#[cfg(target_os = "macos")]`) and runtime checks (e.g., `is_running_under_rosetta()`) to manage the differences between Linux/Windows and macOS (both Intel and Apple Silicon).

## Key Abstractions

### 1. Windowing and Rendering (macOS)

The most significant abstraction is the creation of a secondary, "external" window for Vulkan hardware rendering.

- **The Problem**: Standard `eframe`/`egui` windows are managed by the `wgpu` or `glow` backend. However, Libretro cores requesting Vulkan hardware rendering need to present to a surface they control.
- **The Solution**: `crates/arcade-lib_retro/src/video/vulkan_render.rs` implements `ExternalVulkanWindow`.
    - It uses the `objc` crate to interact with Apple's `AppKit` and `CoreGraphics` frameworks.
    - It creates an `NSWindow` with a `CAMetalLayer`.
    - This layer is attached to the window's content view, allowing the Vulkan/MoltenVK backend to present directly to this layer.
- **Implementation Details**:
    - `NSWindow` creation: `vulkan_render.rs:ExternalVulkanWindow::create()`.
    - `CAMetalLayer` attachment: `vulkan_render.rs:ExternalVulkanWindow::create()`.
    - Window resizing/visibility: `vulkan_render.rs:ExternalVulkanWindow::set_visible()`.

### 2. OpenGL Context Management

To handle cores that render in a separate OpenGL context (common in `glow`-based backends), the host must track the "active" context.

- **Mechanism**: `crates/arcade-libretro/src/hardware_render.rs:current_gl_ctx_id()`.
- **macOS Implementation**: Uses the `CGLGetCurrentContext` C function to retrieve a pointer to the current thread's CGL context.
- **Purpose**: This allows the `VideoCoordinator` to detect when a core has switched to a different (core-owned) GL context, preventing the host from attempting to read from the wrong framebuffer.

### 3. Library Loading (Dynamic Loading)

The host must find and load `.dylib` (macOS) or `.so` (Linux) files.

- **Mechanism**: `crates/arcade-libretro/src/lib.rs:load_core()`.
- **Implementation**: Uses the `libloading` crate.
- **macOS Specifics**: The host implements specialized logic in `core_library_filename_candidates()` to find Apple Silicon-optimized binaries (e.g., `mupen64plus_next_dynarec_arm64_libretro.dylib`) when running on `aarch64`.

### 4. Architecture-Specific Logic

The project differentiates between native ARM64 and x86_64 (Rosetta 2) on macOS.

- **Rosetta Detection**: `crates/arcade_domain/src/platform.rs:is_running_under_rosetta()`.
- **Impact**: 
    - For `mednafen_psx_hw`, the host forces a **Software** backend when running under Rosetta to avoid MoltenVK/driver crashes, but uses **Vulkan** on native ARM64.

## Summary of Platform Dependencies

| Feature | macOS (ARM64/Intel) | Linux / Windows |
| :--- | :--- | :--- |
| **Windowing** | `AppKit` (`NSWindow`) | `winit` / `eframe` |
| **Vulkan Surface** | `CAMetalLayer` | `X11`/`Wayland` or `Win32` |
| **GL Context ID** | `CGLGetCurrentContext` | `0` (not implemented) |
| **Core Binary** | `.dylib` (with ARM64 variants) | `.so` |
