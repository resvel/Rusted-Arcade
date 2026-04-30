# Rendering Pipeline

This document describes the data flow from the Libretro core's rendering output to the final display on the user's screen.

## Overview

The rendering pipeline is split into two distinct paths: the **Software/GL Path** (CPU-to-GPU copy) and the **Hardware Rendering Path** (Direct GPU presentation). The choice of path is determined by the `VideoCoordinator` during the core loading phase.

---

## 1. Software/GL Path (CPU-to-GPU)

Used by cores that do not support hardware rendering or when the host cannot provide a compatible hardware context.

**Data Flow**:
1.  **Core Output**: The core renders into a memory buffer or an internal OpenGL texture.
2.  **Callback**: The core invokes `retro_video_refresh(data, width, height, pitch)`.
3.  **Host Capture**: `arcade-libretro` captures this data into a `FrameBuffer` (a `Vec<u8>` of raw pixels).
4.  **GPU Upload**: In the next frame, the `OpenGlBackend` (or `SoftwareBackend`) takes this `FrameBuffer`.
5.  **Display**: The pixels are uploaded to a GPU texture and rendered as a full-screen quad in the `egui` viewport.

**Key Files**:
- `crates/arcade-libretro/src/video/software.rs`
- `crates/arcade-libretro/src/video/gl.rs`

---

## 2. Hardware Rendering Path (Direct GPU)

Used by high-performance cores (e.g., Mupen64Plus, Play!) that request a Vulkan or OpenGL hardware context.

### A. Vulkan Path (macOS/Linux)
This path allows the core to render directly into a swapchain image managed by the host.

**Data Flow**:
1.  **Negotiation**: The core requests `RETRO_HW_CONTEXT_VULKAN` via `retro_set_environment`.
2.  **Window Setup**: On macOS, the host creates an `NSWindow` with a `CAMetalLayer` (`vulkan_render.rs`).
3.  **Core Rendering**: The core renders directly into a Vulkan `VkImage` provided by the host via `retro_vulkan_set_image`.
4.  **Synchronization**: The host uses `VkSemaphore` and `VkFence` to ensure the host doesn't present the image until the core has finished rendering.
5.  **Presentation**: The host calls `vkQueuePresentKHR` on the `ExternalVulkanWindow`'s swapchain.

**Key Files**:
- `crates/arcade-libretro/src/video/vulkan.rs`
- `crates/arcade-libretro/src/video/vulkan_render.rs`

### B. OpenGL Path (Direct GPU)
Used when a core requests an OpenGL context and the host can provide one (e.s. `eframe_glow`).

**Data Flow**:
1.  **Context Sharing**: The host provides a `glow` context to the core.
2.  **Core Rendering**: The core renders into a texture/framebuffer within that shared context.
3.  **Host Readback/Capture**: The host uses `glReadPixels` or texture sampling (for `Play!`) to "capture" the rendered content.
4.  **Display**: The captured content is then presented in the main application window.

**Key Files**:
- `crates/arcade-libretro/src/video/gl.rs`

---

## Summary Table

| Feature | Software/GL Path | Vulkan Path |
| :--- | :--- | :--- |
| **Primary Mechanism** | CPU Buffer Copy | GPU Swapchain Presentation |
| **Latency** | Higher (due to CPU/GPU sync) | Lower (Direct GPU path) |
| **Complexity** | Low | High (requires `NSWindow` management) |
| **Core Requirement** | None | `RETRO_HW_CONTEXT_VULKAN` |
| **macOS Specifics** | Standard GL context | `CAMetalLayer` + `NSWindow` |
