# Cross-Platform Graphics Strategy

The `arcade-libretro` engine employs a multi-tiered rendering strategy designed to balance high performance on modern hardware with maximum compatibility across different platforms (Linux, macOS, and Windows) and different hardware capabilities.

## Rendering Backends

The system supports three primary backend modes, managed by the `VideoCoordinator`:

1.  **Software Backend**: The fallback of last resort. It uses the CPU to render frames into a buffer, which is then uploaded to the GPU. This is used when no hardware acceleration is available or requested.
2.  **OpenGL (GL) Backend**: Uses `glow` (OpenGL ES) to render. This is the standard path for cores that request a GL context (e.0.g., `play` or `beetle_sgx`). It is highly compatible but subject to driver-specific issues on macOS (e.g., integer texture/float sampler rejection).
3.  **Vulkan Backend**: The high-performance path. This is the preferred backend for modern hardware and is specifically optimized for macOS via the "External Window" integration.

## The "External Window" Strategy (macOS Optimization)

A key architectural decision is how the Vulkan backend handles macOS. Standard Vulkancore integration often struggles with windowing and surface management when the host and the core have different viewports or windowing loops.

### Implementation Details
On macOS, instead of the core managing its own window, the `arcade-libretro` host:
- Creates a standalone, frameless `NSWindow` with a `CAMetalLayer`.
- Exposes this window's surface to the core via the `RETRO_HW_CONTEXT_VULKAN` interface.
- Uses `ash` and `MoltenVK` to allow the core to present directly to the host-managed surface.

This approach avoids the "window fighting" that occurs when an embedded `eframe`/`egui` window and a Libretro-created window attempt to coexist, and it allows for seamless integration of the emulator viewport into the main application UI.

## Backend Selection Policy

The `VideoCoordinator` uses a policy-based approach (`policy.rs`) to select the best backend based on:
- **Core Requirements**: Does the core explicitly request `RETRO_HW_context_vulkan`?
- **Frontend Capabilities**: Does the host provide a valid OpenGL context or a Vulkan-compatible window handle?
- **Platform Nuances**: 
    - **macOS/Rosetta**: If running under Rosetta, the policy forces `Software` for certain cores (like `mednafen_psx_hw`) to avoid MoltenVK/driver crashes.
    - **Core-Specific Overrides**: Certain cores like `play` are forced to OpenGL to prevent unstable startup behavior.

## Synchronization and Debugging

To ensure stability in the asynchronous rendering pipeline, the system implements:
- **Vulkan Sync Primitives**: Uses semapores and fences to synchronize frame delivery between the emulator thread and the host's UI thread.
- **Debug Readback**: A specialized mechanism to sample swapchain images. This allows the host to detect "black screen" or "stale frame" scenarios, which are common when a core's JIT or rendering pipeline stalls.
