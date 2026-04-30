# Useful Commands for Debugging

This page contains a collection of environment variables and commands useful for debugging the `arcade-libretro` integration, particularly for macOS and Vulkan issues.

## Environment Variables

Use these to enable detailed tracing and diagnostics in the logs.

### General Logging
| Variable | Purpose |
| :--- | :--- |
| `ARCADE_LOG_FILE=<path>` | Redirects all `tracing` logs to the specified file. |
| `RUST_LOG=<filter>` | Standard `tracing-subscriber` filter (e.g., `info`, `debug`, `trace`). |

### Libretro Core & Video Debugging
| Variable | Purpose |
| :--- | :--- |
| `LIBRETRO_TRACE_GL_READBACK` | Enables verbose logging for OpenGL texture/framebuffer readback operations. |
| `LIBRETRO_TRACE_GL_CONTEXT` | Logs details of the active OpenGL context (version, vendor, etc.) on startup. |
| `LIBRETRO_TRACE_GL_ERROR` | Logs OpenGL error states during frame processing. |
| `LIBRETRO_TRACE_BACKEND` | Logs the chosen video backend and fallback attempts during core loading. |
| `ARCADE_VULKAN_DEBUG` | Enables detailed Vulkan synchronization and debug-step logging. |
| `ARCADE_VULKAN_HANDOFF_TRACE` | Traces the handoff of frames between the core and the host. |
| `ARCADE_PLAY_GL_TEXTURE_DEBUG` | Enables debugging for the `Play!` core's complex texture scanning logic. |

### Core-Specific Overrides
| Variable | Purpose |
| :--- | :--- |
| `ARCADE_MUPEN64PLUS_NEXT_VIDEO_BACKEND=<software\|vulkan>` | Forces Mupen64Plus to use a specific backend. |
| `ARCADE_MACOS_RENDERER=<wgpu\|glow>` | Forces the macOS frontend renderer to use `wgpu` (default) or `glow` (OpenGL). |
| `ARCADE_MACOS_GL_PROFILE=<legacy\|modern>` | Sets the OpenGL profile for the macOS GL renderer. |

## LLDB Debugging (macOS)

When debugging the application with `lldb`, use these commands to inspect the state of the Libretro integration.

### Inspecting the Active Runtime
If you have a breakpoint in a function that has access to the `runtime` variable:
```lldb
# Print the current emulation configuration
p runtime.emulation

# Check if the core is currently loaded
p runtime.loaded.as_ref().is_some()

# Inspect the current video backend
p runtime.video_coordinator.lock().current_backend_kind()
```

### Inspecting Hardware Render State
If you have a breakpoint in a function that has access to `state` (from `hw_render_state.lock()`):
```lldd
# Check if the Vulkan context is active
p state.context_type == Some(6)

# Check if the external Vulkan window is initialized
p state.external_vulkan_window.is_some()

# Inspect the current target framebuffer/texture
p state.target
```

### Inspecting Core Callbacks
If you are at a breakpoint inside a Libretro callback (e.s. `retro_video_refresh`):
```lldd
# Inspect the incoming frame data
p width
p height
p pitch
```
