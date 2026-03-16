# Conversation Context

This file captures the visible working context from the current chat session for handoff and continuation.

## User Requests

1. "take a look at my rust app"
2. "first lests work on the mappings for the n64 it doest seem to match entirely a regurlar n64 remote"
3. User provided an image of an N64 controller and clarified that the C section is the button cluster on the right side of the controller.
4. "lets do a refactor pass on the libretro host now, we'll make sure not to break anything"
5. "show me the vulkan linux path code"
6. "now show me the vulkan windows path (currently not working crashes with black screen)"
7. "yes go ahead, consult the documentation online if needed"
8. "place the whole context of this chat on a CONTEXT.md file"

## High-Level App Review

- The project is a Rust Cargo workspace with these crates:
  - `arcade-app`
  - `arcade-ui`
  - `arcade-libretro`
  - `arcade-domain`
  - `arcade-data`
  - `arcade-services`
- Initial review found the structure coherent and the app compiling cleanly.
- Verification completed successfully at that stage with:
  - `cargo check`
  - `cargo test`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`

## N64 Mapping Work

### Problem

- The user reported that the N64 mapping did not match a regular N64 controller.
- Initial assumption incorrectly mapped the C cluster too literally onto the modern right stick.
- User corrected this by explicitly pointing to the original N64 right-side C-button cluster.

### Final Mapping Changes

Updated default N64 controller mapping in:
- `/home/jules/personalWebArcade/native/crates/arcade-domain/src/models.rs`

Current default N64 mapping:
- `A` -> `South`
- `B` -> `West`
- `L` -> `LeftShoulder`
- `R` -> `RightShoulder`
- `Z` -> `LeftTrigger`
- `Start` -> `Start`
- `C-Up` -> `North`
- `C-Right` -> `East`
- `C-Left` -> `RightStickX -`
- `C-Down` -> `RightStickY +`

Rationale:
- `A`, `B`, `C-Up`, and `C-Right` preserve the physical relative layout of the N64 right-side cluster on a modern four-button diamond.
- `C-Left` and `C-Down` are placed on right-stick left/down because a modern controller does not expose six distinct right-side face buttons.

### Input Translation Fix

The work also uncovered a deeper issue: the N64 libretro action translation was partially wrong.

Updated input translation in:
- `/home/jules/personalWebArcade/native/crates/arcade-ui/src/input.rs`

Current N64 action-to-libretro mapping:
- `A` -> libretro joypad `B`
- `B` -> libretro joypad `Y`
- `C-Up` -> libretro joypad `X`
- `C-Right` -> libretro joypad `A`
- `C-Left` -> right analog X `-1.0`
- `C-Down` -> right analog Y `+1.0`
- `Z` -> libretro joypad `L2`

This was implemented by introducing an explicit `RetroActionBinding` enum so the N64 path can emit either joypad or analog bindings instead of forcing everything through the generic joypad mapping.

### Tests Added

Added tests in:
- `/home/jules/personalWebArcade/native/crates/arcade-domain/src/models.rs`
- `/home/jules/personalWebArcade/native/crates/arcade-ui/src/input.rs`

Verified with:
- `cargo test -p arcade-domain -p arcade-ui`
- `cargo check`

### Important Runtime Note

- Existing saved N64 system mappings or device overrides still take precedence over defaults.
- To see the new defaults in the running app, the user may need to reset the N64 mapping in the controller panel.

## Libretro Host Refactor

### Goal

- Perform a low-risk refactor of the libretro host without breaking behavior.
- Avoid touching the most fragile Vulkan/runtime core paths.

### Refactor Boundary Chosen

Extracted two helper areas from the monolithic `arcade-libretro/src/lib.rs`:

1. Core variable / environment-default logic
2. ROM/archive loading and launch staging logic

### New Internal Modules

Created:
- `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/core_variables.rs`
- `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/content.rs`

### What Moved

Into `core_variables.rs`:
- `default_core_variables_for`
- `default_core_variables` (test helper)
- `store_default_variable`
- related tests for N64 core variables

Into `content.rs`:
- `prepare_launch_session`
- arcade archive staging helpers
- `inspect_core_requirements`
- `build_game_info`
- `prepare_game_content`
- ZIP extraction helpers
- related launch-session tests

### Result

- `arcade-libretro/src/lib.rs` was reduced from about 6249 lines to about 5768 lines.
- The runtime, Vulkan negotiation, audio, and frame loop remained in place.

### Verification

Verified after refactor with:
- `cargo fmt --all`
- `cargo test -p arcade-libretro`
- `cargo check`

## Linux Vulkan Path Review

The Linux/X11 Vulkan path was identified in these major areas of:
- `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/lib.rs`

Key sections:
- X11 external fullscreen presentation window
- external-window eligibility checks for Xlib
- Vulkan Xlib surface creation
- swapchain creation and GPU-side present path
- frame-loop hook-up
- CPU readback fallback path

The Linux path is considered the current working high-performance path for `parallel_n64`.

## Windows Vulkan Path Review

The user reported:
- Windows Vulkan path currently crashes with a black screen.

The README already states:
- Windows defaults to the safer in-window fallback path.
- External Win32 Vulkan present is experimental.
- It is gated behind `ARCADE_WINDOWS_EXTERNAL_VULKAN_PRESENT=1`.

Relevant files:
- `/home/jules/personalWebArcade/native/README.md`
- `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/lib.rs`

### Win32 Vulkan Path Components

Reviewed these code sections:
- Win32 external popup window creation
- Win32 surface creation via `VK_KHR_win32_surface`
- swapchain creation
- `vkAcquireNextImageKHR`
- present path using `vkCmdBlitImage` + `vkQueuePresentKHR`
- frame-loop visibility toggling for the external present window

### Current Findings

The current highest-confidence bug candidates are:

1. No swapchain recreation on `VK_ERROR_OUT_OF_DATE_KHR` or `VK_SUBOPTIMAL_KHR`
   - `vkAcquireNextImageKHR` errors are treated as fatal.
   - `vkQueuePresentKHR` errors are treated as fatal.
   - The present window can be resized/shown/maximized, but the swapchain extent is not rebuilt to match.
   - This is a strong match for "black screen then crash."

2. `vkCmdBlitImage(..., VK_FILTER_LINEAR)` is used without verifying format feature support
   - The code picks a surface format but does not query whether blit and linear-filtered blit are valid for the source/destination formats.
   - This is a known portability hazard and can fail on some Windows drivers.

3. Teardown/rebuild risk from destroying Vulkan present resources without first idling the device/queue
   - There is a queue-idle helper in the file, but it is not used before present resource destruction.
   - This can turn a WSI/present failure into a crash during cleanup.

### External References Consulted

During the Windows crash review, Vulkan and Win32 documentation was consulted, including:
- Vulkan `vkAcquireNextImageKHR`
- Vulkan `vkQueuePresentKHR`
- Vulkan `vkCmdBlitImage`
- Vulkan `VkFormatFeatureFlagBits`
- Vulkan `vkDestroySwapchainKHR`
- Vulkan `vkDestroyDevice`
- Vulkan `vkDeviceWaitIdle`
- Win32 `CreateWindowExA`
- Win32 `ShowWindow`

These references were used to validate:
- required swapchain recreation behavior
- format/blit requirements
- lifecycle requirements for destroying swapchain/device resources

## Current Architecture Goals

The next refactor direction agreed in chat is:

- define a backend interface inside `arcade-libretro`
- move software / GL / Vulkan into separate backend modules implementing it
- reduce `arcade-ui` to passing only the minimum frontend capabilities, not backend-specific behavior
- move OS/core/backend selection into one policy point

Interpretation of these goals:
- keep `arcade-libretro` as the libretro/core host boundary
- formalize the existing software/OpenGL/Vulkan paths into explicit backend implementations instead of large internal branching
- keep `arcade-ui` primarily as the shell for menus, overlays, browser, settings, and session UI
- avoid tying backend selection to scattered OS checks, env vars, and ad hoc core-specific branches

## Files Changed During This Chat

- `/home/jules/personalWebArcade/native/crates/arcade-domain/src/models.rs`
- `/home/jules/personalWebArcade/native/crates/arcade-ui/src/input.rs`
- `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/lib.rs`
- `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/content.rs`
- `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/core_variables.rs`
- `/home/jules/personalWebArcade/native/CONTEXT.md`

## Commands Run for Verification

- `cargo check`
- `cargo test`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test -p arcade-domain`
- `cargo test -p arcade-domain -p arcade-ui`
- `cargo test -p arcade-libretro`
- `cargo fmt --all`

## Windows Vulkan Continuation After CONTEXT.md Was Created

After the initial context file was created, the rest of the chat focused almost entirely on getting the Windows Vulkan path for `parallel_n64` to a usable state.

### User Requests After CONTEXT.md Creation

Additional visible user requests in this stretch included:
- implement the lowest-risk Win32 Vulkan fix first
- create compressed test zips of the `native/` folder excluding `target/` and `dist/`
- inspect repeated new `vulkan-debug.log` runs from Windows

## macOS `parallel_n64` Source Build And Dynarec Work

After the Windows Vulkan work, the chat moved to macOS with the goal of building and testing `parallel_n64` from source on Apple Silicon.

### Source Build Setup

- Upstream source was fetched into:
  - `/Users/jules/Downloads/native-build-snapshot-20260314-134938/third_party/parallel-n64`
- Built core output is staged at:
  - `/Users/jules/Downloads/native-build-snapshot-20260314-134938/target/debug/cores/parallel_n64_libretro.dylib`
- A focused smoke harness was added at:
  - `/Users/jules/Downloads/native-build-snapshot-20260314-134938/crates/arcade-libretro/tests/parallel_n64_smoke.rs`

### Smoke-Test Results On macOS

Verified repeatedly with:
- `cargo test -p arcade-libretro parallel_n64_macos_loads_and_unloads -- --ignored --exact`
- `cargo test -p arcade-libretro parallel_n64_macos_smoke_runs_frames -- --ignored --exact`

Current behavior:
- load/unload test passes
- frame smoke test fails because the core produces no CPU frame on macOS in this host

### Important Findings

1. The ROM is not the issue
- The user explicitly clarified that the ROM works with `parallel_n64` on Linux.
- Debugging therefore shifted to the macOS runtime path rather than content handling.

2. Upstream Apple Silicon dynarec did not build cleanly at first
- The upstream makefile disables useful hardware-render paths on macOS and does not provide a clean working Apple Silicon dynarec build out of the box.
- The arm64 dynarec assembly had to be patched so:
  - `make -j8 platform=osx WITH_DYNAREC=aarch64`
    succeeds on this Mac.

3. A real dynarec runtime crash was reached after the build fixes
- Once the build linked, the core no longer stopped immediately in cached interpreter.
- It reached:
  - `Starting R4300 emulator: Dynamic Recompiler`
  - `Init new dynarec`
- The first runtime failure was a crash caused by a macOS-incompatible fixed-address JIT allocation assumption.

4. The arm64 dynarec assumes tight branch ranges
- The arm64 emitter uses direct `b`, `bl`, and conditional branches with constraints around:
  - +/-128 MB for unconditional branches/calls
  - +/-1 MB for conditional branches / `adr`
- This means the JIT code cache must be placed close enough to both the dynarec helper text and the expected `extra_memory`-relative data layout.

### Code Changes Kept

The following changes are currently present in the workspace and should be preserved:

1. Runtime cache flush uses the real JIT base
- File:
  - `/Users/jules/Downloads/native-build-snapshot-20260314-134938/third_party/parallel-n64/mupen64plus-core/src/r4300/new_dynarec/arm64/assem_arm64.c`
- In `do_clear_cache()`, cache invalidation now uses `(uintptr_t)base_addr` instead of the compile-time `BASE_ADDR`.

2. Apple JIT write-protection helpers and allocator scaffolding
- File:
  - `/Users/jules/Downloads/native-build-snapshot-20260314-134938/third_party/parallel-n64/mupen64plus-core/src/r4300/new_dynarec/new_dynarec_64.c`
- Current Apple-specific work includes:
  - `pthread_jit_write_protect_np()` helpers
  - `dynarec_alloc_jit_region_exact()`
  - `dynarec_alloc_jit_region_near()`
  - guarded handling of `MAP_FAILED`
  - `new_dynarec_available` tracking

3. Clean fallback when dynarec is unavailable
- Files:
  - `/Users/jules/Downloads/native-build-snapshot-20260314-134938/third_party/parallel-n64/mupen64plus-core/src/r4300/new_dynarec/new_dynarec.h`
  - `/Users/jules/Downloads/native-build-snapshot-20260314-134938/third_party/parallel-n64/mupen64plus-core/src/r4300/r4300.c`
- Added:
  - `new_dynarec_is_available()`
- `r4300_init()` now falls back cleanly to cached interpreter if dynarec initialization fails, instead of continuing with `base_addr == MAP_FAILED` and crashing.

### Precise Current macOS State

Current verified runtime behavior with the staged rebuilt core:
- `parallel_n64` source build succeeds on macOS
- the core loads successfully in the Rust host
- the load/unload smoke test passes
- the frame smoke test still fails

Current log sequence for the frame smoke:
- `Starting R4300 emulator: Dynamic Recompiler`
- `Init new dynarec`
- `mmap() failed to remap reserved dynarec slot with MAP_JIT: Invalid argument`
- `mmap() failed to allocate MAP_JIT region within branch range of dynarec data`
- `Dynamic Recompiler unavailable, falling back to Cached Interpreter`
- `R4300 emulator finished.`

This means:
- the prior macOS segfault is fixed
- the remaining blocker is executable code-cache placement, not a crash in generated code

### Failed Experiments Already Tried

These paths were tested and did not solve the problem:
- `MAP_JIT` allocation at an arbitrary address
  - this reached generated code but later crashed because branch targets exceeded arm64 range assumptions
- remapping the reserved `extra_memory` slot directly with:
  - `MAP_FIXED | MAP_JIT`
  - result on this Mac: `EINVAL`
- reducing the Apple code-cache size to 16 MB
  - this did not change allocator success and was reverted

### Recommended Next Step

The next practical debugging step is to inspect the live VM layout around `extra_memory` in the loaded dylib and determine whether:
- there is genuinely no `MAP_JIT` hole inside the required branch window
- or the current allocator window/selection logic is wrong for the actual dyld layout on macOS
- explain the required PowerShell environment variables
- be more assertive with the Windows path instead of mirroring Linux too literally
- go directly at the Vulkan image handoff and frame-size path
- describe the direction implied by the latest log
- proceed with a debug readback/checksum of the Vulkan present output
- update `CONTEXT.md` with the full visible context

### Windows Vulkan Changes Made

The following visible work was completed after `CONTEXT.md` was first created.

1. Win32 swapchain recreation and recovery
- Added Win32-specific recovery for `VK_ERROR_OUT_OF_DATE_KHR`, `VK_ERROR_SURFACE_LOST_KHR`, and `SUBOPTIMAL_KHR`.
- Rebuild is isolated to the Windows external-present path and idles the queue before rebuilding present resources.

2. Logging and startup diagnostics
- Added detailed Vulkan debug logging around:
  - frontend handle probing
  - environment gating
  - external present activation
  - `retro_run`
  - `retro_vulkan_set_image`
  - fallback size resolution
  - semaphore/sync behavior
  - present steps

3. Windows external window behavior changes
- The external Win32 Vulkan helper window is now created hidden instead of aggressively visible/fullscreen.
- It only becomes visible when external present is actually active.
- `WM_CLOSE` hides the helper instead of forcing focus/foreground behavior.
- The message pump was narrowed so it only processes messages for the helper window instead of draining the whole UI thread queue.
- Visibility is now tracked so the helper window is not force-shown every frame.

4. Windows viewport/UI coordination changes
- The Windows build now skips the Linux-style egui/winit viewport choreography.
- The main frontend window is left alone on Windows while the external Vulkan helper window is handled independently.

5. Vulkan handoff fixes
- `PendingVulkanImage` now preserves the core-provided subresource information from the libretro Vulkan image metadata instead of assuming mip `0`/layer `0` color slices.
- The core-provided `image_view` is now kept and used explicitly.

6. Geometry and frame-size handling
- Implemented `RETRO_ENVIRONMENT_SET_GEOMETRY` handling to update the runtime Vulkan fallback frame size dynamically.
- Startup and runtime now share the same geometry helper.
- Logs showed that geometry updates were firing, but the actual hardware frame callback still reported `1x1`.

7. Conservative sync fallback
- Added a fallback path that forces a Vulkan queue idle before consuming a pending image when the core provides no semaphores, no signal semaphore, and no explicit command buffers.
- This improved confidence that the missing image was not just a semaphore-handshake problem.

8. Present path rewrite
- Replaced the old Windows/Linux external-present blit path with a shader-based Vulkan render pass that samples the core-provided `image_view` directly and renders a fullscreen triangle into the swapchain.
- This moved the host closer to the intended libretro Vulkan frontend model.

9. Present-output checksum instrumentation
- Added swapchain-present debug readback for the first few frames when the swapchain supports `TRANSFER_SRC`.
- The present path now copies the rendered swapchain image into a staging buffer and logs:
  - checksum
  - number of non-black sampled pixels
  - first RGBA pixel

## Build Snapshot Created Before Backend Refactor

Before starting the larger backend architecture refactor, a trimmed buildable snapshot of the repo was created to preserve the current state.

Snapshot directory:
- `/home/jules/personalWebArcade/native/snapshots/native-build-snapshot-20260314-125412`

Archive:
- `/home/jules/personalWebArcade/native/snapshots/native-build-snapshot-20260314-125412.zip`

Included in the snapshot:
- `Cargo.toml`
- `Cargo.lock`
- `README.md`
- `CORES.md`
- `config.example.toml`
- `config.toml`
- `build`
- `crates/`
- `public/`
- `sql/`
- `third_party/`

Excluded from the snapshot:
- `target/`
- `dist/`
- `.git/`
- logs and packaging-only extras

## Backend Architecture Refactor Progress

The agreed architecture direction was then implemented as a real code refactor instead of remaining only as a plan.

### New Video Layer

Created a new backend-oriented video module tree in:
- `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/video/mod.rs`
- `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/video/policy.rs`
- `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/video/software.rs`
- `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/video/gl.rs`
- `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/video/vulkan.rs`

New shared types introduced there include:
- `FrontendCapabilities`
- `VideoBackendKind`
- `BackendSelection`
- `FrameDelivery`
- `VideoSessionInfo`
- `VideoCoordinator`

### Host Contract Changes

`LibretroHost` in:
- `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/lib.rs`

was refactored to:
- use `VideoCoordinator` from runtime state
- replace the old split frontend setters with a single:
  - `set_frontend_capabilities(FrontendCapabilities)`
- keep the UI-facing queries/actions:
  - `using_external_vulkan_present_window()`
  - `has_external_vulkan_present_window()`
  - `set_external_overlay_message()`
  but route them through the coordinator

The frame loop now:
- asks the coordinator to prepare the frame
- runs `retro_run`
- asks the coordinator to consume the frame
- records perf from normalized `FrameDelivery`

### Backend Selection Policy

Selection logic was centralized into:
- `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/video/policy.rs`

Current preference order is:
- `Vulkan -> OpenGL -> Software`

with these first-pass rules:
- `parallel_n64`
  - Linux: prefer Vulkan when frontend raw window/display handles are present
  - Windows: prefer Vulkan only when the explicit experimental gate is enabled
  - otherwise fall back to Software
- GL hardware-render cores: use OpenGL when a frontend GL-capable renderer is present
- otherwise use Software

This keeps the current behavior matrix but moves policy into one place.

### Core Variable / Runtime Default Integration

`core_variables.rs` was rewritten so core defaults are driven by the selected backend instead of scattered OS/env probing.

Current important behavior:
- `parallel_n64`
  - `VideoBackendKind::Vulkan` => `parallel` renderer + ParaLLEl-RDP upscaling variables
  - `VideoBackendKind::Software` => `angrylion`
- `mupen64plus_next`
  - Software backend still forces the existing angrylion fallback path

Windows Vulkan runtime env defaults for `parallel_n64` are now backend-driven too.

### UI Integration Changes

`arcade-ui` now publishes one minimal frontend capability snapshot from:
- `/home/jules/personalWebArcade/native/crates/arcade-ui/src/app/mod.rs`

It no longer calls separate backend-specific host setters for:
- glow context
- frontend windowing strings

It now only builds `FrontendCapabilities` from `eframe` and sends that to the host.

### Verification After Refactor

Verified successfully with:
- `cargo test -p arcade-libretro -p arcade-ui`
- `cargo check`

At the point of this update, the structural refactor is compiling and tests are passing.

### Important Current Constraint

The backend architecture refactor is structural. It does not by itself solve the existing Windows `parallel_n64` Vulkan black-frame problem.

The current state after the refactor is:
- the video/backend seam is much cleaner
- UI capability flow is reduced
- backend selection is centralized
- future Vulkan work is now localized mainly to the Vulkan backend/policy path instead of the whole host/UI stack

## What Should Happen Next

According to the current direction in this file, the next practical work should be:

1. Resume focused work on the Vulkan backend itself, not on the old monolithic host path.
2. Re-test the known platform matrix against the refactored structure:
   - Linux `parallel_n64` Vulkan path
   - Windows `parallel_n64` software fallback path
   - Windows experimental `parallel_n64` Vulkan path
3. Fix the remaining Windows experimental Vulkan issue inside:
   - `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/video/vulkan.rs`
   and policy interactions in:
   - `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/video/policy.rs`
4. After the Vulkan backend is stable enough, reduce more leftover video-specific branching that still remains in:
   - `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/lib.rs`

The immediate next engineering step is therefore:
- do backend-specific smoke validation and then continue debugging/fixing the Windows Vulkan backend within the new `video::vulkan` structure.

## macOS Port Pass

After the backend refactor landed, the next pass was explicitly redirected toward getting the app onto a macOS `compile and boot` path.

### What Was Changed

1. Cargo / `eframe` feature wiring
- The workspace `eframe` dependency was changed so its base features are now platform-neutral:
  - `default_fonts`
  - `glow`
- Linux-specific `x11` and `wayland` features were moved out of the workspace default and into target-specific dependency sections for:
  - `/home/jules/personalWebArcade/native/crates/arcade-app/Cargo.toml`
  - `/home/jules/personalWebArcade/native/crates/arcade-ui/Cargo.toml`

This removes the Linux-only assumption from the shared dependency graph and makes macOS a valid frontend target shape.

2. macOS OpenGL symbol loading
- `GlProcLoader` in:
  - `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/lib.rs`
  was updated so:
  - Linux/Unix keeps the `libGL.so.1` / `libEGL.so.1` path
  - Windows keeps the `opengl32.dll` path
  - macOS now loads OpenGL symbols from:
    - `/System/Library/Frameworks/OpenGL.framework/OpenGL`

This removes the old incorrect assumption that “not Windows” means Linux-style OpenGL loader behavior.

3. UI font fallback candidates
- `/home/jules/personalWebArcade/native/crates/arcade-ui/src/app/mod.rs`
  now includes macOS font candidates in addition to the Linux ones, so the UI can boot with a better chance of finding a usable proportional, display, and monospace font on macOS.

4. Documentation
- `/home/jules/personalWebArcade/native/README.md`
  was updated so macOS is now called out explicitly in:
  - prerequisites
  - core filename extension expectations
  - run examples
  - current platform notes

### What Was Verified

Verified locally on the current Linux machine:
- `cargo check`

Additional attempted verification:
- installed the Rust target:
  - `x86_64-apple-darwin`
- attempted:
  - `cargo check --target x86_64-apple-darwin`

### Current macOS Limitation

The macOS target check did not complete on this Linux machine because the environment does not have an Apple-capable C toolchain / linker.

The failing point was in `ring`'s native build step, where the host `cc` does not understand macOS flags such as:
- `-arch`
- `-mmacosx-version-min=10.7`
- `-gfull`

So the current blocker is now:
- not the obvious repo-level Linux-only `eframe` or GL loader assumptions
- but the lack of a real macOS build environment or osxcross-style Apple toolchain on this machine

### macOS Direction After This Pass

The intended macOS bring-up order is now:

1. boot the app on macOS with the safe path
   - frontend via `eframe` + `glow`
   - video via `OpenGl` or `Software`
2. validate core loading with `.dylib`
3. validate play-session boot for software / CPU-frame paths
4. only after that, consider any future macOS Vulkan or MoltenVK work

### What Should Happen Next Now

The next practical step for macOS is:
- test this refactored code on an actual macOS machine or a proper macOS cross-build toolchain
- confirm that the app compiles, opens, and reaches the UI shell
- then validate one safe libretro core boot path before touching any macOS-specific rendering expansion

So the immediate next engineering step is no longer another Linux-side architecture pass.
It is:
- run the app on real macOS hardware and fix whatever first native build/boot issues remain there.

## Real macOS Runtime Result

The app was then tested on a real Apple Silicon Mac after the macOS build snapshot was transferred.

### macOS Core Setup

A `macosCores/` directory was created locally and the relevant supported libretro arm64 macOS cores were downloaded from:
- `https://buildbot.libretro.com/nightly/apple/osx/arm64/latest/`

Downloaded/extracted successfully:
- `fceumm_libretro.dylib`
- `snes9x_libretro.dylib`
- `genesis_plus_gx_libretro.dylib`
- `gambatte_libretro.dylib`
- `mgba_libretro.dylib`
- `mupen64plus_next_libretro.dylib`
- `fbneo_libretro.dylib`
- `mame2003_libretro.dylib`
- `mame2003_plus_libretro.dylib`

Important exception:
- `parallel_n64_libretro.dylib` was not present in the arm64 nightly buildbot feed, so it could not be downloaded from that source.

### Confirmed macOS App Result

The user reported that the macOS build ran successfully and `mupen64plus_next` launched an N64 title correctly.

Visible runtime evidence from the user log:
- the app launched on macOS
- the libretro core loaded successfully
- `mupen64plus_next` executed the ROM correctly
- the play session ran and emitted normal perf logs
- the core shut down cleanly

Important console/log signals from that run:
- `plugin_start_gfx`
- `Started play session system=N64 ... core=mupen64plus_next`
- `Starting R4300 emulator: Cached Interpreter`
- repeated `arcade_ui::perf: play_tick ...`
- clean shutdown:
  - `Stopping emulation.`
  - `R4300 emulator finished.`
  - `Rom closed.`

### Current macOS Conclusion

This is now confirmed, not speculative:
- the app builds on Apple Silicon macOS
- the UI shell works on macOS
- `.dylib` core loading works on macOS
- the safe N64 path through `mupen64plus_next` works on macOS

The only obvious limitation observed in that successful run was:
- `Audio output unavailable, continuing without sound: audio feature is disabled at compile time`

So the macOS bring-up goal has advanced from:
- “make it compile and boot”

to:
- “the app already boots and runs at least one real N64 core on macOS”

### What This Means For The Next Step

The remaining macOS uncertainty is no longer the app as a whole.
It is specifically:
- getting `parallel_n64` available and tested on macOS arm64

Given that:
- `parallel_n64` is missing from the arm64 nightly buildbot feed
- the user previously had to build `parallel-n64` from source on Linux

the working assumption is now:
- `parallel_n64` will likely need to be built from source on macOS as well

### Immediate Next Direction

The next step after this confirmed macOS success should be:

1. inspect upstream `parallel-n64` build requirements for macOS arm64
2. build `parallel_n64_libretro.dylib` from source on the Mac
3. place it in the macOS cores directory
4. test it through the app

At this point, the macOS app port is no longer the blocker.
`parallel_n64` availability on macOS is the blocker.

## Latest macOS `parallel_n64` Dynarec Allocator Findings

Work continued on the real Apple Silicon Mac after the earlier dynarec bring-up pass.

### What Was Verified

1. `MAP_JIT` itself works on this Mac
- A small local probe confirmed:
  - `mmap(NULL, ..., MAP_JIT)` succeeds
  - hinted `mmap(addr, ..., MAP_JIT)` succeeds
  - `mmap(addr, ..., MAP_FIXED | MAP_JIT)` fails with `EINVAL`

This means the earlier Apple allocator was structurally wrong to depend on `MAP_FIXED | MAP_JIT`.

2. The dynarec text anchor still does not yield a usable nearby JIT mapping
- The allocator was changed to stop trying to remap `extra_memory` in place.
- The Apple path now searches relative to `new_dyna_start` instead of `BASE_ADDR`.
- The allocator was further changed to use non-fixed hinted `MAP_JIT` mappings and verify the returned address is still within the arm64 branch window.

3. The failure mode is now precise and stable
- Repeated LLDB runs of:
  - `target/debug/deps/parallel_n64_smoke-58ab2af2f5b6b685 parallel_n64_macos_smoke_runs_frames --ignored --exact --nocapture`
  now consistently show:
  - `Starting R4300 emulator: Dynamic Recompiler`
  - `Init new dynarec`
  - `mmap() failed to allocate MAP_JIT region within branch range of dynarec text: Result too large`
  - `Dynamic Recompiler unavailable, falling back to Cached Interpreter`

So the crash path is gone, but dynarec still cannot get an executable region close enough to its helper text.

### Additional Experiments Tried

1. Replaced Mach-region gap inference with direct hint probing
- The Apple allocator no longer trusts only Mach VM region inspection.
- It now repeatedly asks `mmap(..., MAP_JIT)` for nearby hinted addresses and rejects mappings returned outside the branch range.
- Result: still no valid nearby executable mapping on this process layout.

2. Reduced the arm64 dynarec cache size on macOS
- `TARGET_SIZE_2` in:
  - `/Users/jules/Downloads/native-build-snapshot-20260314-134938/third_party/parallel-n64/mupen64plus-core/src/r4300/new_dynarec/arm64/assem_arm64.h`
  was tried at:
  - `24` (16 MB)
  - then `23` (8 MB)
- Result: even an 8 MB cache still cannot be placed within branch range of the dynarec text on this Mac.

### Current Relevant Files

The current kept macOS dynarec changes now include:
- `/Users/jules/Downloads/native-build-snapshot-20260314-134938/third_party/parallel-n64/mupen64plus-core/src/r4300/new_dynarec/new_dynarec_64.c`
  - `MAP_FAILED` handling and dynarec availability tracking
  - Apple `pthread_jit_write_protect_*` handling
  - non-fixed hinted `MAP_JIT` allocator path
  - branch-range verification for returned mappings
  - fallback to cached interpreter when allocation fails
- `/Users/jules/Downloads/native-build-snapshot-20260314-134938/third_party/parallel-n64/mupen64plus-core/src/r4300/new_dynarec/arm64/assem_arm64.c`
  - runtime `base_addr` cache flush fix
- `/Users/jules/Downloads/native-build-snapshot-20260314-134938/third_party/parallel-n64/mupen64plus-core/src/r4300/new_dynarec/arm64/assem_arm64.h`
  - temporary macOS-only reduced `TARGET_SIZE_2` experiments
- `/Users/jules/Downloads/native-build-snapshot-20260314-134938/third_party/parallel-n64/mupen64plus-core/src/r4300/r4300.c`
  - clean fallback to cached interpreter when dynarec init is unavailable

### Verification State Now

Verified repeatedly with:
- `make -j8 platform=osx WITH_DYNAREC=aarch64`
- restaging:
  - `/Users/jules/Downloads/native-build-snapshot-20260314-134938/target/debug/cores/parallel_n64_libretro.dylib`
- LLDB-driven smoke runs of:
  - `parallel_n64_macos_smoke_runs_frames`

Current exact status:
- source build works
- core load/unload works
- dynarec build/link works
- dynarec startup no longer segfaults
- dynarec still cannot obtain a usable nearby `MAP_JIT` region on this Mac, even with an 8 MB cache
- host therefore falls back to cached interpreter
- the smoke test still fails because no CPU frame is produced

### Current Best Next Step

The next useful step is no longer another allocator tweak of the same kind.

The remaining likely options are:

1. remove the tight branch-range dependency
- audit the arm64 dynarec helper call/jump sites and replace some direct `b`/`bl` assumptions with long-range trampolines or register-indirect call sequences

2. change the loader / mapping topology
- move or split executable helper/code layout so the JIT cache does not need such a large contiguous nearby hole relative to the loaded dylib text

Right now the evidence suggests the blocker is:
- not ROM correctness
- not host load/unload behavior
- not `MAP_JIT` support itself
- but the inability to place the dynarec code cache within the arm64 branch-distance constraints of the loaded helper text in this macOS process layout
- This is meant to answer whether the sampled Vulkan output is actually black before it reaches the window, versus being visible data shown with the wrong viewport/size assumptions.

### What the Windows Logs Established

Across the repeated Windows runs, the visible log analysis established this progression:

1. Early runs did not enter the Win32 external Vulkan path at all because `ARCADE_WINDOWS_EXTERNAL_VULKAN_PRESENT=1` was not set.
2. After correcting the environment, the path reached:
   - frontend probe success
   - external Vulkan window creation
   - Vulkan interface state creation
3. After the helper-window and message-pump fixes, the Windows build stopped hanging.
4. After the present-path rewrite, `retro_run` returned and `queue_present` completed repeatedly.
5. The major remaining signal in the logs is still:
   - `retro_vulkan_set_image ... semaphores=0`
   - no `set_signal_semaphore`
   - no `set_command_buffers`
   - `pending=1x1`
   - fallback frame size resolved to `640x480`

This means the problem area has narrowed substantially:
- It is no longer primarily a Win32 surface/swapchain/window bring-up failure.
- It is now in the Vulkan frame contract itself:
  - image content
  - effective source rect / viewport
  - or how this core reports Vulkan frame dimensions to the frontend

### Current Direction

The current direction at the end of the chat is:

1. Use the new present-output checksum logs to determine whether the fullscreen sampled output is actually black.
2. If the new `present readback frame=...` logs show all-black output, keep debugging upstream Vulkan content/layout/readiness.
3. If those logs show non-black output, shift immediately to viewport/UV/frame-size semantics rather than synchronization or swapchain logic.

### Current Relevant Files

In addition to the files already listed above, the visible chat after `CONTEXT.md` creation also changed:

- `/home/jules/personalWebArcade/native/crates/arcade-ui/src/app/viewport.rs`
- `/home/jules/personalWebArcade/native/crates/arcade-libretro/build.rs`
- `/home/jules/personalWebArcade/native/crates/arcade-libretro/Cargo.toml`

### Current Windows Run Environment

The Windows Vulkan tests were being run from PowerShell with:

```powershell
$env:ARCADE_VULKAN_DEBUG = "1"
$env:ARCADE_WINDOWS_EXTERNAL_VULKAN_PRESENT = "1"
$env:PARALLEL_RDP_SUBGROUP = "0"
$env:RUST_LOG = "info,arcade_libretro::vulkan_debug=info,arcade_ui::video_debug=info"
$env:ARCADE_LOG_FILE = ".\vulkan-debug.log"
Remove-Item .\vulkan-debug.log -ErrorAction SilentlyContinue
.\personal-arcade-native.exe
```

### Latest Verification Status

The latest implemented change at the end of the chat was verified locally with:

- `cargo fmt --all`
- `cargo test -p arcade-libretro`
- `cargo check`

### Latest Test Archive

The latest test archive created during the chat was:

- `/home/jules/personalWebArcade/native-windows-test-20260308-155537.zip`

## Further Windows Vulkan Debugging After The First Append

After the previous `CONTEXT.md` update, the chat continued and the Windows Vulkan investigation narrowed significantly.

### Additional Work Completed

1. Source-image debug readback
- Added a source Vulkan image readback before the shader-present pass samples the image.
- This reuses the existing Vulkan readback path and logs checksum / non-black samples / first RGBA pixel for the first few unsignaled frames.
- Goal: determine whether the source image itself is black, rather than only the presented swapchain output.

2. Sync ordering instrumentation
- Added tracking for whether the core has acknowledged the current sync slot via `wait_sync_index`.
- Added detailed logging for:
  - `wait_sync_index`
  - `lock_queue`
  - `unlock_queue`
  - sync slot index values
- Added a short wait in the unsignaled-image path to give the core’s `wait_sync_index` callback a chance to happen before falling back to queue idle.

3. Vulkan negotiation / device-selection improvements
- Added logging for the Vulkan negotiation interface:
  - negotiation interface version
  - presence of `get_application_info`
  - presence of `create_device`
  - presence of `destroy_device`
- Added logging during Vulkan interface creation to show:
  - whether negotiation is active
  - requested application API version
  - whether an external surface is involved
  - whether the core’s `create_device` callback returned `true`
- Improved frontend fallback Vulkan device creation to:
  - prefer a queue family with both `GRAPHICS` and `COMPUTE`
  - enable `VK_KHR_swapchain` when a surface exists
  - log selected queue family flags

### What The Newer Logs Established

The later Windows logs answered several important open questions.

1. Present path output was confirmed black
- The swapchain-present debug readback showed:
  - checksum `0x0000000000000000`
  - `non_black_samples=0/64`
  - `first_rgba=00,00,00,00`
- This proved the output is black before it reaches the Win32 window.

2. Source image was also confirmed black
- The new source-image readback showed the same all-zero result.
- This ruled out the fullscreen sampling/present shader path as the primary bug.
- The black output now clearly originates upstream of presentation.

3. `wait_sync_index` and queue lock/unlock are active
- Later logs showed `wait_sync_index completed ... sync_index=...` repeatedly.
- `lock_queue` and `unlock_queue` also fired repeatedly.
- Despite that, the source image remained black.
- This ruled out missing callback ordering as the main blocker.

4. The bad frame contract signal still remains
- The Vulkan hardware frame path continues to see:
  - `pending=1x1`
  - fallback to `640x480`
  - `retro_vulkan_set_image ... semaphores=0`
- There are still no useful semaphores from the core in the observed Windows runs.

### Updated Diagnosis

By the end of the visible chat, the following had been ruled out as primary causes:

- Win32 external window creation / visibility behavior
- swapchain creation and present submission
- swapchain recreation handling
- fullscreen shader sampling/present path
- missing `wait_sync_index`
- missing queue lock/unlock participation

The remaining likely problem area is now upstream Vulkan negotiation / render-target setup, especially the contract between `parallel_n64` and the frontend during Vulkan device/context setup on Windows.

### Current Direction At End Of Chat

The current intended next step is:

1. Run the latest Windows build with the new negotiation logs.
2. Inspect whether:
   - the core requested Vulkan negotiation
   - `create_device` was actually used successfully
   - the host fell back to its own device creation
   - the chosen queue family has the expected flags
3. Use that result to decide whether the Windows Vulkan path is failing because the core is not getting the device/context shape it expects.

### More Files Changed In This Later Stretch

- `/home/jules/personalWebArcade/native/crates/arcade-libretro/src/lib.rs`

### More Verification Performed

The later changes in this stretch were also verified repeatedly with:

- `cargo fmt --all`
- `cargo test -p arcade-libretro`
- `cargo check`

### Later Test Archives Created

Additional Windows test archives created later in the chat included:

- `/home/jules/personalWebArcade/native-windows-test-20260308-155537.zip`
- `/home/jules/personalWebArcade/native-windows-test-20260308-160642.zip`
- `/home/jules/personalWebArcade/native-windows-test-20260308-161828.zip`
- `/home/jules/personalWebArcade/native-windows-test-20260308-162808.zip`

## Current Recommended Next Step

If continuing from this point, the best next implementation step is:

1. Run the latest Windows build and inspect the new negotiation/device-selection logs.
2. Determine whether `parallel_n64` is actually getting the Vulkan device/context shape it expects on Windows.
3. If not, focus the next fix on Vulkan negotiation and device creation rather than any more present-path work.

## Notes

- This file captures the visible working context from the chat.
- It does not include hidden system or developer instructions.

## 2026-03-14 macOS parallel_n64 continuation

### Latest files changed

- `/Users/jules/Downloads/native-build-snapshot-20260314-134938/third_party/parallel-n64/mupen64plus-core/src/r4300/new_dynarec/arm64/assem_arm64.c`
- `/Users/jules/Downloads/native-build-snapshot-20260314-134938/third_party/parallel-n64/mupen64plus-core/src/r4300/new_dynarec/arm64/assem_arm64.h`
- `/Users/jules/Downloads/native-build-snapshot-20260314-134938/third_party/parallel-n64/mupen64plus-core/src/r4300/new_dynarec/new_dynarec_64.c`
- `/Users/jules/Downloads/native-build-snapshot-20260314-134938/third_party/parallel-n64/mupen64plus-rsp-hle/src/alist.c`
- `/Users/jules/Downloads/native-build-snapshot-20260314-134938/crates/arcade-libretro/src/core_variables.rs`

### Dynarec/runtime state reached in this stretch

1. Apple Silicon dynarec now builds, links, allocates `MAP_JIT`, and reaches runtime execution.
2. Long-range helper transfers were added on macOS in `assem_arm64.c` so generated code can call/jump outside the normal arm64 branch window.
3. Apple JIT allocation in `new_dynarec_64.c` was relaxed to plain `mmap(NULL, ..., MAP_JIT, ...)`.
4. Runtime linker patch sites in `dynamic_linker()` and `dynamic_linker_ds()` were wrapped with `dynarec_begin_code_write()` / `dynarec_end_code_write()`.
5. After those fixes, the previous startup blocker changed from:
   - `mmap() failed to allocate MAP_JIT region within branch range of dynarec text`
   to real runtime execution inside the core.

### New macOS failures observed

Two distinct failure modes were reproduced after the allocator/linker work:

1. Intermittent crash in RSP HLE audio filter
   - Original crash was inside `alist_filter` in `mupen64plus-rsp-hle/src/alist.c`.
   - A narrow Apple-only workaround was added:
     - `__attribute__((optnone))` on `alist_filter`
   - That removed the immediate crash in some runs.

2. Intermittent later crash in `memmove`
   - LLDB later caught:
     - `EXC_BAD_ACCESS (code=1, address=0xc)`
     - top frame in `libsystem_platform.dylib` `_platform_memmove`
     - LR pointed back into `alist_filter`
   - This lines up with `alist_filter`'s tail copies:
     - `memcpy(hle->dram + address, in2 - 8, 16);`
     - `memcpy(hle->alist_buffer + dmem, outbuff, count);`
   - So even after disabling optimization on `alist_filter`, the broader macOS execution path can still corrupt the filter inputs or surrounding state.

### Important control result

A host-side diagnostic override was added in `crates/arcade-libretro/src/core_variables.rs`:

- on macOS software backend only:
  - `parallel-n64-cpucore = cached_interpreter`

This was used to test whether the remaining problem was dynarec-specific.

Result:

- forcing `parallel_n64` to `cached_interpreter` still produced:
  - `Starting R4300 emulator: Cached Interpreter`
  - then `R4300 emulator finished.`
  - and the smoke test still failed with:
    - `parallel_n64 did not produce a CPU frame`

This is important because it means the current macOS blocker is **not only** the Apple dynarec path anymore.

### Current best diagnosis

The current evidence now points to a broader macOS `parallel_n64` runtime/integration problem:

- dynarec-specific allocator/linker blockers were real and were fixed far enough to allow execution
- but even with cached interpreter forced, the core still exits without producing a CPU frame
- and with dynarec enabled, the run is unstable enough to sometimes wander into RSP HLE memory corruption

So the remaining issue is currently best described as:

- `parallel_n64` on this macOS host still does not deliver a stable software frame path
- and the Apple dynarec path remains additionally unstable on top of that

### Strongest latest frame-delivery proof

Additional tracing was added in this stretch:

- host-side env-gated trace in `crates/arcade-libretro/src/lib.rs`:
  - `LIBRETRO_TRACE_VIDEO=1`
- core-side env-gated trace in `third_party/parallel-n64/libretro/libretro.c`:
  - `LIBRETRO_TRACE_RETURNS=1`

Observed result with macOS software backend and cached interpreter forced:

- many repeated:
  - `retro_return just_flipping=0 stop=0`
- final:
  - `retro_return just_flipping=0 stop=1`
- no `retro_video_refresh` callback trace at all

This is the clearest current signal:

- the core is yielding back to the frontend repeatedly
- but only on non-flipping paths
- and it never reaches a frame-producing libretro video callback before emulation stops

So the next investigation should focus on why the software angrylion path on macOS never reaches a valid flip/sync (`retro_return(true)` / `video_cb`) rather than on frontend frame-consumption logic.

### Most useful next step from here

The next high-value investigation is host/core handoff around frame delivery rather than more allocator work:

1. trace `retro_return(just_flipping)` and `retro_video_refresh()` on macOS
2. determine whether `parallel_n64` is ever delivering a non-null CPU frame callback before emulation stops
3. if not, inspect why the software angrylion path is yielding/terminating before a valid frame lands
4. only after that boundary is understood, continue dynarec-specific correctness work

## Latest continuation: boot-path narrowing on macOS

This continuation moved the diagnosis materially beyond "no frame callback".

### New core and host instrumentation added

- `third_party/parallel-n64/mupen64plus-core/src/r4300/interrupt.c`
  - env-gated tracing for:
    - interrupt queue events: `LIBRETRO_TRACE_INTERRUPTS`
    - general exceptions: `LIBRETRO_TRACE_EXCEPTIONS`
- `third_party/parallel-n64/libretro/libretro.c`
  - `LIBRETRO_TRACE_RETURNS` now also logs:
    - current PC
    - current instruction word
    - CP0 cause/status/epc/badvaddr
    - SP/MI/PI state
- `third_party/parallel-n64/mupen64plus-core/src/rsp/rsp_core.c`
  - env-gated tracing for:
    - `write_rsp_regs`
    - `write_rsp_regs2`
    - `do_SP_Task`
    - via `LIBRETRO_TRACE_RSP`
- `third_party/parallel-n64/mupen64plus-core/src/si/cic.c`
  - `LIBRETRO_TRACE_BOOT` now logs detected CIC version/seed and IPL3 checksum
- `third_party/parallel-n64/mupen64plus-core/src/pifbootrom/pifbootrom.c`
  - `LIBRETRO_TRACE_BOOT` now logs boot HLE setup

### Strongest new runtime facts

Using:

- `LIBRETRO_TRACE_RETURNS=1`
- `LIBRETRO_TRACE_INTERRUPTS=1`
- `LIBRETRO_TRACE_RSP=1`
- `LIBRETRO_TRACE_BOOT=1`

the macOS source build now shows:

1. this is **not** a CP0 exception loop
   - `CAUSE` stays `0`
   - `EPC` and `BADVADDR` stay unset (`0xffffffff`)
   - `wrapped_exception_general()` never fires

2. VI interrupts continue, but the core never reaches a valid video state
   - `VI_INT` keeps firing
   - no `retro_video_refresh()` callback occurs
   - VI registers remain effectively uninitialized for rendering (`VI_ORIGIN = 0`, geometry/scales zero)

3. the CPU ends in an intentional IPL3 fail loop
   - traced PCs eventually settle at `0x800001c8`
   - current instruction there is `0x0411ffff`
   - that is the classic self-branch / park loop pattern in MIPS boot code

4. the RSP is never even launched
   - `SP_STATUS` remains `0x1` (`HALT`)
   - `SP_PC` remains `0`
   - `LIBRETRO_TRACE_RSP=1` produced no `write_rsp_regs`, `write_rsp_regs2`, or `do_SP_Task` events

This is important: the failure happens **before** first RSP startup, so the remaining macOS blocker is earlier than the software renderer and earlier than the RSP plugin path.

### CIC / boot setup result

`LIBRETRO_TRACE_BOOT=1` showed:

- IPL3 checksum detected as `0xd057c85244`
- CIC detected as `CIC_X102`
- CIC seed `0x3f`
- normal PIF boot baseline:
  - `rom_type=0`
  - `tv_type=1`
  - `sp_status=0x1`
  - `pi_status=0`
  - `vi_intr=0x3ff`

So the build is not obviously choosing the wrong CIC or starting from a malformed PIF HLE state.

### CPU backend experiment result

The macOS host-side diagnostic override in `crates/arcade-libretro/src/core_variables.rs` was changed again:

- from `cached_interpreter`
- to `pure_interpreter`

Result:

- pure interpreter behaves the same way as cached interpreter
- it still reaches the IPL3 fail loop and never produces a CPU frame

This rules out the remaining issue being specific to cached interpreter or dynarec execution.

### Optimization experiment result

Because an Apple-only optimizer issue already existed in `alist_filter`, a compiler-miscompile hypothesis was tested:

1. `make -B -j8 DEBUG=1 platform=osx WITH_DYNAREC=aarch64`
   - this failed to link on macOS because non-optimized builds expose missing `safe_rdram` symbols

2. `make -B -j8 platform=osx WITH_DYNAREC=aarch64 HAVE_LTCG=0 CPUOPTS='-O1 -DNDEBUG'`
   - this built successfully
   - after staging and codesigning, the smoke test still failed in the same way

So lowering optimization from `-Ofast` to `-O1` did **not** change the macOS boot failure.

### Current best diagnosis after this continuation

The most defensible current statement is:

- the macOS source build reaches normal PIF/CIC setup
- then parks in IPL3 before first RSP launch
- this is reproducible in both cached and pure interpreter
- and it still happens with an `-O1` build

That means the remaining blocker is now best treated as an early boot/ROM-bootcode compatibility issue in this macOS source path, not a libretro video callback issue, not a dynarec-only issue, and not an RSP-launch issue.

### Best next step from here

The next high-value investigation is now one of:

1. inspect the IPL3 fail path more directly and determine whether it is failing a checksum/bootcode condition for this ROM image
2. compare against a known-good clean N64 ROM in this workspace or against a known-good upstream core build on macOS
3. if the goal is pragmatic macOS validation rather than bootcode archaeology, add a temporary diagnostic path to bypass IPL3 and see whether the rest of the core stack runs correctly after boot

## 2026-03-14 continuation: macOS OpenGL path is now working

This continuation changed the macOS status materially. `parallel_n64` now builds from source on this Mac, loads in the host, and passes the macOS smoke test when driven through the explicit OpenGL path with `gln64` plus `pure_interpreter`.

### Upstream source changes kept

- `third_party/parallel-n64/Makefile`
  - local opt-in macOS OpenGL build path remains:
    - `ALLOW_OSX_OPENGL=1`
    - `-framework OpenGL`
    - Apple-specific flags to keep the GL path building on arm64
- `third_party/parallel-n64/libretro/libretro.c`
  - `gl_inited` / `vulkan_inited` are now set from the actual return value of `retro_init_gl()` / `retro_init_vulkan()` instead of being forced to `true`
- existing earlier fixes remain in place, especially:
  - `third_party/parallel-n64/mupen64plus-core/src/r4300/mips_instructions.def`
    - explicit unsigned 32-bit wraparound for `ADDU` / `SUBU`

### Host-side changes that unlocked the macOS GL test

- `crates/arcade-libretro/src/video/policy.rs`
  - macOS `parallel_n64` now prefers `VideoBackendKind::OpenGl` when the frontend exposes GL capability
- `crates/arcade-libretro/src/core_variables.rs`
  - on macOS `parallel_n64` + `OpenGl`, the host now selects:
    - `parallel-n64-gfxplugin = gln64`
    - `parallel-n64-cpucore = pure_interpreter`
  - this is intentional for now because Apple dynarec is still not stable
- `crates/arcade-libretro/tests/parallel_n64_smoke.rs`
  - added a macOS headless CGL context helper
  - the smoke test now sets real frontend GL capabilities before loading `parallel_n64`
- `crates/arcade-libretro/build.rs`
  - links `OpenGL.framework` on macOS so the CGL-based smoke harness can link

### Important host bugs fixed during this continuation

Two host bugs were the real blockers once the GL path existed:

1. planned backend session was being destroyed during load
   - `load_core()` planned `OpenGl`
   - but `configure_environment_context()` called `destroy_hw_render_session_for()`
   - that cleared `VideoCoordinator.session`
   - `run_frame()` then silently fell back to `Software`
   - fix: removed that session teardown from `configure_environment_context()`

2. OpenGL backend deadlocked on coordinator re-lock
   - once the session-lifetime bug was fixed, `OpenGlBackend.prepare_frame()` started running
   - it then deadlocked because `ensure_hw_render_target()` tried to lock `video_coordinator` while `run_frame()` already held that lock
   - fix: store the frontend GL context in `HardwareRenderState` and use that in:
     - `ensure_hw_render_target()`
     - `read_opengl_render_frame()`
   - this removed the recursive coordinator lock path

### What the traces proved

With:

- `LIBRETRO_TRACE_BACKEND=1`
- `LIBRETRO_TRACE_GL_READBACK=1`
- `LIBRETRO_TRACE_RETURNS=1`
- `LIBRETRO_TRACE_VIDEO=1`

the working macOS OpenGL run showed:

- backend planner selected `OpenGl`
- `OpenGlBackend.prepare_frame()` ran each frame with `context_type=Some(1)`
- early frames had no hardware frame yet
- later the core emitted:
  - `retro_return just_flipping=1 stop=0`
  - `libretro video data=0xffffffffffffffff width=640 height=480 pitch=0`
- the host then consumed that as a hardware frame:
  - `gl backend consume_frame pending_hw=true`
  - `gl backend consume_frame readback=640x480`

This is the key result:

- the macOS GL path is real
- the host now correctly reads back libretro hardware frames into a CPU `FrameBuffer`
- the old macOS smoke failure was no longer a ROM issue and no longer just an upstream core issue; the embedded host had two real OpenGL-path bugs

### Verification status after fixes

Source build used:

- `make -B -j8 platform=osx WITH_DYNAREC=aarch64 ALLOW_OSX_OPENGL=1 HAVE_NEON=0 HAVE_RICE=0 HAVE_GLIDE64=0 HAVE_GLN64=1`

Staged core:

- `target/debug/cores/parallel_n64_libretro.dylib`

Codesign:

- `codesign --force --sign - target/debug/cores/parallel_n64_libretro.dylib`

Passing checks:

- `cargo test -p arcade-libretro parallel_n64_macos_loads_and_unloads -- --ignored --exact`
- `cargo test -p arcade-libretro parallel_n64_macos_smoke_runs_frames -- --ignored --exact`

The smoke test now passes cleanly on macOS.

### Remaining limitation

Apple dynarec is still not ready.

- with the same macOS OpenGL build but without the host forcing `pure_interpreter`, the core reaches:
  - `Starting R4300 emulator: Dynamic Recompiler`
  - `Init new dynarec`
- and then crashes with `SIGBUS`

So the current practical macOS status is:

- `parallel_n64` source build: yes
- macOS host load/unload: yes
- macOS frame-producing smoke test: yes
- current stable macOS runtime path: `gln64` + `pure_interpreter`
- Apple dynarec: still broken / not yet usable

### Best next step from here

If the next turn is about stability rather than more host work, the highest-value task is now:

1. get a real backtrace for the Apple dynarec `SIGBUS`
2. keep the new macOS OpenGL smoke path as the regression test for successful frame production

## 2026-03-14 Follow-up dynarec investigation

### Dynarec stub parsing fix

Apple arm64 dynarec already had long-range fallbacks in `emit_call()` / `emit_jmp()`:

- direct `bl` / `b` only when within +/-128 MB
- otherwise `movimm64 + blr x16` / `movimm64 + br x16`

However, several dirty-block parser helpers in:

- `third_party/parallel-n64/mupen64plus-core/src/r4300/new_dynarec/arm64/assem_arm64.c`

still assumed a direct `bl` when scanning dirty stubs:

- `get_clean_addr()`
- `verify_dirty()`
- `isclean()`
- `get_bounds()`

Fix applied:

- added helpers to decode either:
  - direct `bl`
  - Apple `movimm64 + blr x16`
- updated those dirty-stub parser helpers to use the new decoder instead of assuming a single `bl`

This was a real correctness fix for Apple far-call stubs, even though it did not eliminate the remaining dynarec failure by itself.

### Current dynarec behavior after that fix

Forced dynarec run:

- `ARCADE_PARALLEL_N64_CPUCORE=dynamic_recompiler cargo test -p arcade-libretro parallel_n64_macos_smoke_runs_frames -- --ignored --exact --nocapture`

still fails on macOS.

Two distinct behaviors are now observed:

1. normal run:
   - core reaches:
     - `Starting R4300 emulator: Dynamic Recompiler`
     - `Init new dynarec`
   - then often dies with `SIGBUS`

2. traced or LLDB-instrumented runs:
   - the same dynarec path can avoid the `SIGBUS`
   - but still exits without producing a CPU frame
   - smoke test then fails with:
     - `parallel_n64 did not produce a CPU frame`

That means the `SIGBUS` is not the only blocker. Even when the crash disappears under instrumentation, dynarec still does not reach a working frame-producing state on macOS.

### Latest crash signature

Newest crash report:

- `~/Library/Logs/DiagnosticReports/parallel_n64_smoke-58ab2af2f5b6b685-2026-03-14-181255.ips`

Important details:

- faulting PC:
  - `0x10cea0fac`
- `parallel_n64_libretro.dylib` `__TEXT` base in that run:
  - `0x10ceac000`
- so the crash PC is still `45140` bytes before dylib `__TEXT`
- thread state again showed dynarec globals in expected places:
  - `PC`
  - `fake_pc`
  - `readmem_dword`
  - `mini_ht`
  - `invalid_code`

So the old signature remains:

- execution jumps into a bogus non-executable mapped-file region immediately before the dylib text image

### Dynarec layout remains far from helper text/data

Recent traced run:

- `ARCADE_PARALLEL_N64_CPUCORE=dynamic_recompiler LIBRETRO_TRACE_DYNAREC_LAYOUT=1 cargo test -p arcade-libretro parallel_n64_macos_smoke_runs_frames -- --ignored --exact --nocapture`

logged:

- `base=0x119b10000`
- `PC=0x1117b74f0`
- `fake_pc=0x1117b74f8`
- `readmem_dword=0x1117b7128`
- `mini_ht=0x1117b75d0`
- `invalid_code=0x1141f8000`

deltas:

- `PC-base=-137726736`
- `fake_pc-base=-137726728`
- `readmem_dword-base=-137727704`
- `mini_ht-base=-137726512`
- `invalid_code-base=-93421568`

So the Apple JIT mapping is still landing roughly 137.7 MB away from the key dylib dynarec symbols and roughly 172 MB away from helper text call targets.

### Far-branch tracing result

Added env-gated tracing in:

- `third_party/parallel-n64/mupen64plus-core/src/r4300/new_dynarec/arm64/assem_arm64.c`

with:

- `LIBRETRO_TRACE_DYNAREC_FAR_BRANCHES=1`

This logs every Apple long-range `movimm64 + blr/br x16` emission.

Result from:

- `ARCADE_PARALLEL_N64_CPUCORE=dynamic_recompiler LIBRETRO_TRACE_DYNAREC_LAYOUT=1 LIBRETRO_TRACE_DYNAREC_FAR_BRANCHES=1 cargo test -p arcade-libretro parallel_n64_macos_smoke_runs_frames -- --ignored --exact --nocapture`

Key finding:

- all traced far call/jump targets in that run were valid helper symbols inside the dylib, e.g. addresses around:
  - `0x1116ac8c4`
  - `0x1116ac914`
  - `0x1116ac994`
  - `0x1116aca08`
  - `0x1116acb48`
  - `0x1116acb4c`
  - `0x1116e09f8`
  - `0x111733e98`
  - `0x11173455c`
- no emitted far branch target matched the bogus crash PC near `0x10cea0fac`

That strongly suggests:

- the `SIGBUS` is not explained by `emit_call()` / `emit_jmp()` materializing an obviously wrong far-helper address
- the remaining bogus control flow is likely coming from:
  - a runtime indirect jump target
  - memory corruption
  - or another untraced control-flow path

### Mini-HT experiment

I also tried temporarily disabling `USE_MINI_HT` for Apple arm64 in:

- `third_party/parallel-n64/mupen64plus-core/src/r4300/new_dynarec/arm64/assem_arm64.h`

to test whether the crash came from the mini hash-table fast path.

Result:

- dynarec still hit the same `SIGBUS`

That experiment was reverted.

### Current best interpretation

At this point:

- the host OpenGL path is working
- source-built `parallel_n64` works on macOS in `gln64 + pure_interpreter`
- Apple dynarec still has two observable failure modes:
  - sometimes `SIGBUS` into a bogus mapped-file address just before dylib text
  - sometimes no crash, but immediate non-frame-producing exit

So the next useful dynarec step is no longer “prove whether `emit_call()` / `emit_jmp()` are using bad far targets” because that has now been checked.

The next likely debugging targets are:

1. runtime indirect jumps / jump-table paths
2. corruption of control-flow state after code emission
3. the older early-boot / no-frame path that still reproduces under LLDB and tracing

## 2026-03-14: macOS OpenGL state restore fix for `egui_glow` VAO errors

The user then ran the macOS app with `parallel_n64` and reported repeated frontend errors like:

- `egui_glow: ... bind_vertex_array: GL_INVALID_OPERATION (0x502)`

Key interpretation:

- this is separate from ROM loading and separate from the earlier dynarec work
- the app log still showed a normal `parallel_n64` launch on macOS:
  - `plugin_start_gfx success.`
  - `Gfx RomOpen.`
  - `mupen64plus: Starting R4300 emulator: Pure Interpreter`
- `Pure Interpreter` is currently expected on macOS in this workspace because the host forces the stable path for `parallel_n64`

Most likely cause:

- the libretro OpenGL helper path was mutating shared frontend GL state in the same context used by `egui_glow`
- in particular, host-side framebuffer / texture / renderbuffer bindings were being changed and then reset to `None` instead of being restored to the previous bindings

Host fix applied in:

- `crates/arcade-libretro/src/lib.rs`

Changes:

- in `ensure_hw_render_target(...)`, save and restore:
  - `GL_TEXTURE_BINDING_2D`
  - `GL_FRAMEBUFFER_BINDING`
  - `GL_RENDERBUFFER_BINDING`
- in `read_opengl_render_frame(...)`, save and restore:
  - `GL_FRAMEBUFFER_BINDING`
- restore those values using `glow::NativeTexture`, `glow::NativeFramebuffer`, and `glow::NativeRenderbuffer`

Verification completed:

- `cargo test -p arcade-libretro parallel_n64_macos_smoke_runs_frames -- --ignored --exact`
- `cargo check -p arcade-ui`

Result:

- both commands passed after the GL-state restore patch
- there is still no automated UI-level repro for the `egui_glow` VAO error in this workspace, so the remaining confirmation step is a manual app rerun on macOS

If the `GL_INVALID_OPERATION` spam persists after this patch, the next GL state to preserve is likely one of:

1. vertex array binding
2. active program
3. active texture unit
4. viewport / scissor
5. pixel pack / unpack alignment

### Follow-up after manual app rerun

The user reran the macOS app and the `egui_glow` spam persisted, now alternating between:

- `vao.rs:79 (bind_vertex_array): GL_INVALID_OPERATION`
- `painter.rs:594 (tex_parameter): GL_INVALID_OPERATION`

At the same time, the app was still making forward progress:

- `arcade_libretro::perf` reported active frame production
- example counters included `frames=180`, `fps=41.7`, `cpu_frames=55`, `empty_frames=125`, `errors=0`

That combination shifted the likely diagnosis:

- less likely that `egui_glow` itself is issuing invalid calls
- more likely that the libretro core leaves one or more GL errors pending in the shared context each frame
- `egui_glow` then trips over those inherited errors on its own next GL call and logs them at whichever call site it happens to touch first

Host fix applied after that observation:

- added `drain_frontend_gl_errors(runtime, stage)` in `crates/arcade-libretro/src/lib.rs`
- it drains up to 16 queued `glGetError()` values from the shared frontend GL context
- it runs after `retro_run`
- it runs again after frame delivery / GL readback
- optional trace hook:
  - `LIBRETRO_TRACE_GL_ERRORS=1`

Verification completed:

- `cargo test -p arcade-libretro parallel_n64_macos_smoke_runs_frames -- --ignored --exact`
- `cargo check -p arcade-ui`

Result:

- both commands passed
- next confirmation still requires a manual macOS app rerun
- if the spam remains even after draining GL errors, the next most likely issue is actual state pollution rather than sticky errors, with the best candidates still being VAO / program / active texture / viewport-scissor state

### Follow-up after black-screen report

The user reran the app and reported:

- the repeated `egui_glow` spam was effectively gone, reduced to a one-off startup error
- audio was still unavailable because the build still had audio disabled at compile time
- the game appeared to run, but the display stayed black

New host-side experiments in `crates/arcade-libretro/src/lib.rs`:

- OpenGL readback now forces alpha to `255`, matching the Vulkan path
- `LIBRETRO_TRACE_GL_READBACK=1` now logs a readback summary for the GL path
- `LIBRETRO_TRACE_GL_ERRORS=1` drains and logs inherited frontend GL errors around `retro_run` and frame delivery
- a temporary default-framebuffer experiment was tried, then reverted back behind env opt-in only:
  - `ARCADE_GL_FORCE_DEFAULT_FRAMEBUFFER=1`

What the local macOS smoke traces proved:

- for many frames, `parallel_n64` produces no hardware frame at all:
  - `gl backend consume_frame pending_hw=false`
- when a hardware frame finally appears, the host reads back a fully black image:
  - `non_black_samples=0/64`
  - `first_rgba=00,00,00,ff`
- the core is emitting `GL_INVALID_FRAMEBUFFER_OPERATION` (`0x506`) during its render path:
  - drained after `retro_run`
- the host also hits `0x506` after frame delivery / GL readback
- forcing the default framebuffer did not change the local smoke result, so the simple “core is rendering only to FBO 0” theory is not sufficient

Current best interpretation:

- this is no longer a UI upload problem
- this is no longer an alpha-only invisibility problem
- the remaining blocker is in the macOS OpenGL hardware-render path itself
- specifically, `parallel_n64` / `gln64` is ending up with invalid framebuffer state on this frontend/context and only ever handing back black hardware frames

Most likely next debugging targets:

1. inspect the exact GL context/profile/version the frontend is exposing on macOS
2. trace upstream `gln64` / `glsm` framebuffer setup and check where `0x506` first appears
3. determine whether this core/plugin requires a compatibility-style context that `eframe_glow` is not actually providing on macOS

### Follow-up: frontend GL context tracing and host surface request

Additional investigation clarified two things:

- the OpenGL plugin used on macOS here is the `parallel-n64-gfxplugin=gln64` option, but in this checkout that path is implemented by the `gles2n64` renderer sources
- `gles2n64` is not obviously fixed-function legacy GL; it uses shaders with desktop GLSL 120 in `third_party/parallel-n64/gles2n64/src/ShaderCombiner.c`

New host changes:

- added a one-shot frontend GL context dump in `crates/arcade-libretro/src/lib.rs`
- gated by:
  - `LIBRETRO_TRACE_GL_CONTEXT=1`
- it logs:
  - `GL_VERSION`
  - `GL_SHADING_LANGUAGE_VERSION`
  - `GL_VENDOR`
  - `GL_RENDERER`
  - major/minor version
  - `GL_CONTEXT_PROFILE_MASK`
  - depth/stencil bits
  - samples
  - current framebuffer bindings

Additional host/app change:

- `crates/arcade-app/src/main.rs` now requests:
  - `renderer: eframe::Renderer::Glow`
  - `depth_buffer: 24`
  - `stencil_buffer: 8`

Reasoning:

- `eframe` defaults to `depth_buffer = 0` and `stencil_buffer = 0`
- even though the host creates its own offscreen GL target, requesting a real default depth/stencil surface is a low-risk compatibility improvement for cores/plugins that assume those buffers exist during setup or intermediate rendering

Verification after these changes:

- `cargo check -p arcade-app`
- `cargo test -p arcade-libretro parallel_n64_macos_smoke_runs_frames -- --ignored --exact`

Status:

- both passed
- the black-screen root cause is still unresolved
- the highest-value next datum is now the actual app-side macOS GL context dump from `LIBRETRO_TRACE_GL_CONTEXT=1`

### Follow-up: actual macOS app context confirmed as `4.1 core`

The user ran the real app with `LIBRETRO_TRACE_GL_CONTEXT=1` and got:

- `version="4.1 Metal - 90.5"`
- `glsl="4.10"`
- `vendor="Apple"`
- `renderer="Apple M1"`
- `profile_mask=0x1`
- `depth_bits=0`
- `stencil_bits=0`
- `samples=0`

Interpretation:

- the live frontend context is an Apple OpenGL 4.1 core-profile context
- `gles2n64` is not being given a legacy/compatibility-style desktop GL context
- `gles2n64` in this checkout emits desktop GLSL 120 shaders and also calls `glGetString(GL_EXTENSIONS)`, which is invalid in a core profile
- this lines up with the observed startup `GL_INVALID_ENUM` and later `GL_INVALID_FRAMEBUFFER_OPERATION`

Additional app-side result:

- audio now works after enabling the app's default `native-av` feature set in `crates/arcade-app/Cargo.toml`

Most important new architecture change:

- vendored `glutin` into:
  - `third_party/glutin-0.32.3`
- added a workspace patch in:
  - `Cargo.toml`
- patched macOS CGL config selection in:
  - `third_party/glutin-0.32.3/src/api/cgl/config.rs`
- new behavior:
  - if `ARCADE_MACOS_GL_PROFILE=legacy`, CGL profile selection prefers `NSOpenGLProfileVersionLegacy` before `4.1Core` / `3.2Core`
- set that env var automatically from:
  - `crates/arcade-app/src/main.rs`
  - before `eframe::run_native(...)`

Verification after this change:

- `cargo check -p arcade-app`
- `cargo test -p arcade-libretro parallel_n64_macos_smoke_runs_frames -- --ignored --exact`

Status after code changes:

- build/check passed with the patched local `glutin`
- next required confirmation is another real macOS app launch with `LIBRETRO_TRACE_GL_CONTEXT=1`
- success criterion is that the frontend GL context line changes away from `4.1 core` to the legacy profile path

### Follow-up: legacy `2.1` context confirmed, audio works, black screen remains

The user reran the real app after the `glutin` patch and got:

- `frontend GL context version="2.1 Metal - 90.5"`
- `glsl="1.20"`
- `vendor="Apple"`
- `renderer="Apple M1"`
- `profile_mask=0x0`
- `depth_bits=32`
- `stencil_bits=8`

Interpretation:

- the app is now using the intended legacy/compatibility-style OpenGL context on macOS
- the earlier `4.1 core` mismatch hypothesis is resolved as a prerequisite, not the final blocker

Additional runtime status:

- audio now works in the app after the earlier `native-av` default-feature change
- video is still black even though:
  - `parallel_n64` launches
  - `mupen64plus: Starting R4300 emulator: Pure Interpreter` appears
  - perf logs report real hardware frames such as `last_frame=640x480`

Host-side diagnostics added in `crates/arcade-libretro/src/lib.rs`:

- env-gated framebuffer trace:
  - `LIBRETRO_TRACE_GL_FRAMEBUFFER=1`
- explicit FBO draw/read buffer initialization for the host hw-render target:
  - `glDrawBuffer(GL_COLOR_ATTACHMENT0)`
  - `glReadBuffer(GL_COLOR_ATTACHMENT0)`
- readback path now also forces `GL_COLOR_ATTACHMENT0` before `glReadPixels()` on the host FBO

Most important new finding from the smoke harness after rebuilding the core:

- `glsm` inside the core now logs that it captures and binds the host offscreen framebuffer correctly:
  - `glsm framebuffer stage=setup default_framebuffer=1 ...`
  - `glsm framebuffer stage=bind default_framebuffer=1 framebuffer=1 ... status=0x8cd5`
- `0x8cd5` is `GL_FRAMEBUFFER_COMPLETE`
- therefore the core is *not* simply falling back to framebuffer `0` during rendering in the smoke harness

Corollary:

- the black-screen bug is now narrower:
  - not the ROM
  - not the app audio path
  - not the macOS context profile anymore
  - not the basic host hw-render FBO contract
- remaining likely area is inside `gles2n64` render/shader/draw behavior on macOS legacy OpenGL

Core-side diagnostics added in upstream source:

- file:
  - `third_party/parallel-n64/libretro-common/glsm/glsm.c`
- env-gated logs:
  - setup/bind/unbind framebuffer state
  - explicit `rglBindFramebuffer(...)` calls

Rebuild/stage notes:

- the staged core at `target/debug/cores/parallel_n64_libretro.dylib` was rebuilt successfully with:
  - `make -j8 platform=osx WITH_DYNAREC=aarch64 ALLOW_OSX_OPENGL=1 HAVE_NEON=0 HAVE_RICE=0 HAVE_GLIDE64=0 HAVE_GLN64=1`
- then copied and re-signed:
  - `codesign --force --sign - target/debug/cores/parallel_n64_libretro.dylib`

Current best next step:

- run the real app with:
  - `LIBRETRO_TRACE_GL_FRAMEBUFFER=1 LIBRETRO_TRACE_GL_READBACK=1 cargo run -p arcade-app`
- capture the first few:
  - `glsm framebuffer ...`
  - `frontend GL framebuffer ...`
  - `gl readback ...`
- that will confirm whether the live windowed app follows the same successful FBO bind path as the smoke harness

### Follow-up: live app confirms good FBO bind path too

The user ran the real app with:

- `LIBRETRO_TRACE_GL_FRAMEBUFFER=1 LIBRETRO_TRACE_GL_READBACK=1 cargo run -p arcade-app`

Important results from that run:

- `glsm framebuffer stage=setup default_framebuffer=1 ... status=0x8cd5`
- `glsm framebuffer stage=bind default_framebuffer=1 framebuffer=1 ... draw_buffer=0x8ce0 read_buffer=0x8ce0 status=0x8cd5`
- `glsm framebuffer stage=unbind default_framebuffer=1 framebuffer=0 ... draw_buffer=0x405 read_buffer=0x405 status=0x8cd5`
- `frontend GL framebuffer stage=retro_run framebuffer=0 ... draw_buffer=0x405 read_buffer=0x405 status=0x8cd5`

Interpretation:

- the real windowed app behaves like the smoke harness with respect to framebuffer binding
- `parallel_n64` / `glsm` captures the host offscreen FBO as framebuffer `1`
- that FBO is complete when bound for rendering
- after core rendering, `glsm` unbinds back to the window/default framebuffer `0`
- the default framebuffer is also complete in the live app

This rules out:

- wrong FBO id being returned by the frontend
- incomplete host offscreen framebuffer
- incomplete default/window framebuffer
- the black screen being caused by a basic FBO bind/unbind contract mismatch

Most important remaining observation from the same run:

- the host repeatedly logged:
  - `gl backend consume_frame pending_hw=false`
  - `gl backend consume_frame fallback=NoFrame`
- so in the live app path, the host usually is not receiving `RETRO_HW_FRAME_BUFFER_VALID` at all in those frames

Current hypothesis after this run:

- the real blocker is earlier in the frame pipeline:
  - the core is not reaching a real flip/swap path often enough
  - or it is yielding via `retro_return(false)` rather than `retro_return(true)`
- in other words, the black screen is now best treated as a flip scheduling / video callback problem, not a framebuffer completeness problem

Additional instrumentation added after this discovery:

- `third_party/parallel-n64/gles2n64/src/OpenGL.c`
  - env-gated log in `OGL_SwapBuffers()` via `LIBRETRO_TRACE_SWAPS`
- `third_party/parallel-n64/mupen64plus-core/src/api/vidext_libretro.c`
  - env-gated log in `VidExt_GL_SwapBuffers()` via `LIBRETRO_TRACE_SWAPS`

Rebuild/stage status:

- rebuilt staged core successfully with:
  - `make -j8 platform=osx WITH_DYNAREC=aarch64 ALLOW_OSX_OPENGL=1 HAVE_NEON=0 HAVE_RICE=0 HAVE_GLIDE64=0 HAVE_GLN64=1`
- restaged and re-signed:
  - `target/debug/cores/parallel_n64_libretro.dylib`

Next best run:

- `LIBRETRO_TRACE_SWAPS=1 LIBRETRO_TRACE_RETURNS=1 cargo run -p arcade-app`
- then launch the game and capture the first lines containing:
  - `gles2n64 OGL_SwapBuffers`
  - `vidext VidExt_GL_SwapBuffers`
  - `retro_return just_flipping=`

### Follow-up: swap/flip path confirmed alive in the live app

The user ran the real app with:

- `LIBRETRO_TRACE_SWAPS=1 LIBRETRO_TRACE_RETURNS=1 cargo run -p arcade-app`

Important results:

- repeated:
  - `gles2n64 OGL_SwapBuffers renderCallback=0`
- repeated real flip returns:
  - `retro_return just_flipping=1 ...`
- many interleaved non-flip returns:
  - `retro_return just_flipping=0 ...`

Interpretation:

- the GL plugin is definitely reaching its swap path on macOS
- the libretro core is definitely producing real flip events (`just_flipping=1`)
- therefore the black screen is *not* caused by:
  - missing swaps
  - `retro_return(true)` never happening
  - the game never reaching presentation

Additional note:

- `renderCallback=0` appears every time, but inspection of upstream source shows that `SetRenderingCallback(...)` is declared in the plugin API and exported by `gln64`, yet nothing in this upstream tree actually invokes `gfx.setRenderingCallback(...)`
- so `renderCallback=0` appears to be normal for this codebase and is not currently treated as the root cause

Best current hypothesis after these traces:

- the remaining bug is in the actual GL rendering output itself
- i.e. the plugin is swapping frames, but those frames are black

New diagnostic change staged after this:

- `third_party/parallel-n64/gles2n64/src/OpenGL.c`
- per-frame env-gated draw accounting via:
  - `LIBRETRO_TRACE_DRAWS=1`
- at each `OGL_SwapBuffers()` it now logs:
  - draw call count
  - vertex count
  - clear count
  - current program
  - array buffer binding
  - element array buffer binding
  - current render state
  - `screenUpdate`
  - `mustRenderDlist`

Staged core status:

- rebuilt again with:
  - `make -j8 platform=osx WITH_DYNAREC=aarch64 ALLOW_OSX_OPENGL=1 HAVE_NEON=0 HAVE_RICE=0 HAVE_GLIDE64=0 HAVE_GLN64=1`
- copied and signed:
  - `target/debug/cores/parallel_n64_libretro.dylib`

Current best next run:

- `LIBRETRO_TRACE_DRAWS=1 LIBRETRO_TRACE_SWAPS=1 cargo run -p arcade-app`
- then launch the game and capture the first few lines containing:
  - `gles2n64 frame draws=`
  - `gles2n64 OGL_SwapBuffers`

Decision point from that run:

- if draw count is near zero, the bug is “nothing is being rendered”
- if draw count is nonzero, the bug is “rendered output is black” and the next step becomes shader/texture/blend-state tracing

Update after the above:

- the first swapped `gln64` frame really was just clears:
  - local smoke with `LIBRETRO_TRACE_DRAWS=1 LIBRETRO_TRACE_SWAPS=1`
  - logged:
    - `gles2n64 frame draws=0 vertices=0 clears=2 ... screen_update=1 must_render=1`
- tracing the source explains why DKR is a bad fit for this path:
  - `third_party/parallel-n64/gles2n64/src/FrameBuffer_gles2n64.c`
  - key framebuffer-emulation entry points are stubs:
    - `FrameBuffer_RenderBuffer(...)`
    - `FrameBuffer_CopyFromRDRAM(...)`
    - `FrameBuffer_CopyToRDRAM(...)`
    - `FrameBuffer_CopyDepthBuffer(...)`
- I also patched a likely DKR DMA-triangle bug in:
  - `third_party/parallel-n64/gles2n64/src/gSP_gles2n64.c`
  - `gln64gSPDMATriangles()` now queues triangles via `gln64gSPTriangle(v0, v1, v2);`
  - this did not change the first swapped frame behavior in the smoke test

New macOS direction tried after that:

- enabled Rice in the upstream macOS build:
  - `make -B -j8 platform=osx WITH_DYNAREC=aarch64 ALLOW_OSX_OPENGL=1 HAVE_NEON=0 HAVE_GLIDE64=0 HAVE_GLN64=1 HAVE_RICE=1`
- switched the macOS OpenGL default in:
  - `crates/arcade-libretro/src/core_variables.rs`
  - from `gln64` to `rice`
- restaged and re-signed:
  - `target/debug/cores/parallel_n64_libretro.dylib`

Important clarification from tracing:

- the host really is returning `parallel-n64-gfxplugin=rice`
  - verified with `LIBRETRO_TRACE_VARIABLES=1`
- after staging the Rice-capable dylib, the old `[gles2n64] Loading Config ...` log disappeared in the smoke test
  - so the staged core is now actually running the Rice path

Current result with Rice:

- smoke test still passes on macOS load/run:
  - `cargo test -p arcade-libretro parallel_n64_macos_smoke_runs_frames -- --ignored --exact --nocapture`
- first hardware frame under Rice is still black:
  - with `LIBRETRO_TRACE_GL_READBACK=1`
  - host logged:
    - `gl backend consume_frame pending_hw=true`
    - `gl readback size=640x480 ... non_black_samples=0/64 first_rgba=00,00,00,ff`

Current state after this pass:

- `gln64` path is structurally suspect for DKR on macOS because framebuffer emulation is stubbed
- `rice` is now the active macOS OpenGL plugin in the staged core
- despite that, the first readback frame is still black in the smoke harness
- next useful step is a real app rerun on the staged Rice build to see whether the live app now behaves any differently than the headless smoke path

Update after real app rerun with `LIBRETRO_TRACE_GL_READBACK=1`:

- live app behavior diverges from the headless smoke path in an important way
- in the real app, Rice repeatedly reports:
  - `gl backend consume_frame pending_hw=true`
  - `gl readback switching to default framebuffer fallback non_black_samples=64/64`
  - `gl readback size=640x480 ... non_black_samples=64/64 first_rgba=09,09,09,ff`
- this means:
  - the host offscreen FBO path is still black
  - the window/default framebuffer path is the only one producing non-black pixels in the live app
- the checksum stayed constant across frames, so what Rice is presenting there may still be mostly a static clear/background, but it is not the all-black offscreen result anymore
- the app also eventually crashed with:
  - `Segmentation fault: 11`

Host change made after that:

- `crates/arcade-libretro/src/core_variables.rs`
- on macOS, for `parallel_n64` + `OpenGl`, the runtime now sets:
  - `ARCADE_GL_FORCE_DEFAULT_FRAMEBUFFER=1`
- rationale:
  - stop advertising the offscreen FBO to this core/backend pair
  - make the core use framebuffer `0` from the start instead of only discovering a usable image via readback fallback
- verification:
  - `cargo test -p arcade-libretro parallel_n64_macos_smoke_runs_frames -- --ignored --exact`
  - `cargo check -p arcade-app`

Current staged macOS direction:

- source-built `parallel_n64` with Rice enabled
- macOS OpenGL default plugin set to `rice`
- macOS runtime default now forces default-framebuffer mode for this core/backend pair

Verification after the default-framebuffer host override:

- reran:
  - `LIBRETRO_TRACE_GL_READBACK=1 cargo test -p arcade-libretro parallel_n64_macos_smoke_runs_frames -- --ignored --exact --nocapture`
- result:
  - the host no longer logs `gl readback switching to default framebuffer fallback ...`
  - the first hardware frame is now read directly from framebuffer `0`
  - in the headless smoke harness that direct read is still black:
    - `gl readback size=640x480 ... non_black_samples=0/64 first_rgba=00,00,00,ff`
- interpretation:
  - the fallback-heavy live-app log from before this change reflects the pre-patch host path
  - the current binary should be rerun before drawing conclusions from that older log

Rice crash work after switching the macOS OpenGL path to framebuffer `0`:

- reproduced the macOS app crash in a headless long-run smoke by adding:
  - `parallel_n64_macos_smoke_runs_many_frames()`
  - in `crates/arcade-libretro/tests/parallel_n64_smoke.rs`
- this proved the failure is not specific to `egui` or the app shell; the core/host OpenGL path itself can crash under sustained execution

First Rice crash fixed:

- LLDB initially stopped in:
  - `ricegDPLoadTLUT(...)`
  - while storing into `g_wRDPTlut`
- source bug in:
  - `third_party/parallel-n64/gles2rice/src/gDP_rice.cpp`
- root cause:
  - `tile->tmem - 256` underflowed when `tile->tmem < 0x100`
  - this produced an out-of-range palette index and `SIGSEGV`
- fix applied:
  - return early if `tile->tmem < 0x100`
  - clamp the palette write loop so `i + dwTMEMOffset < 0x200`
- verified in staged dylib disassembly:
  - `cmp w10, #0x100`
  - `b.lo ...`

Second Rice crash identified:

- after the TLUT fix, the long-run smoke no longer dies in `ricegDPLoadTLUT`
- the remaining crash now stops in:
  - `DLParser_Process(OSTask*) + 904`
  - on the `ldrb w9, [x0, #0x3]` fetch of the next display-list command
- source location:
  - `third_party/parallel-n64/gles2rice/src/RSP_Parser.cpp`
- Rice was missing the basic RDRAM bounds check that the `gln64` parser already has
- fix applied:
  - before decoding `Gfx *pgfx`, added:
    - `uint32_t pc = __RSP.PC[__RSP.PCi];`
    - `if (pc + 8 > g_dwRamSize) break;`
- result:
  - this parser guard alone is not sufficient
  - the long-run smoke still crashes later inside `DLParser_Process`, which means Rice is still following a bad display-list pointer/state transition after the initial bounds check

Current macOS `parallel_n64` OpenGL state:

- staged core is source-built and signed
- macOS runtime forces:
  - `parallel-n64-gfxplugin=rice`
  - `parallel-n64-cpucore=pure_interpreter`
  - `ARCADE_GL_FORCE_DEFAULT_FRAMEBUFFER=1`
- audio works in the app
- repeated `egui_glow` GL spam is mostly gone
- video is still not correct:
  - headless smoke sees only black hardware frames from framebuffer `0`
  - live app previously showed a stable dark-gray framebuffer checksum (`09,09,09,ff`) before crashing
- remaining blocker:
  - Rice display-list parser stability / command stream handling on macOS, not the host FBO plumbing

Next run to validate:

- `cargo run -p arcade-app`
- optionally with:
  - `LIBRETRO_TRACE_GL_READBACK=1 cargo run -p arcade-app`

What to look for:

- whether the window is still black
- whether the earlier segfault still happens
- if traced, whether the repeated `switching to default framebuffer fallback` lines disappear now that framebuffer `0` is the direct path

Update (latest pass, 2026-03-15):

- added additional Rice/parallel-n64 hardening and pointer relinking to stop the new macOS segfault chain:
  - `third_party/parallel-n64/gles2rice/src/RSP_Parser.cpp`
    - guarded `DLParser_Process` and `RDP_DLParser_Process` against:
      - out-of-range `__RSP.PCi`
      - out-of-range `pc` before `Gfx` decode
      - null `currentUcodeMap[opcode]` dispatch
  - `third_party/parallel-n64/gles2rice/src/RSP_GBI0.h`
  - `third_party/parallel-n64/gles2rice/src/RSP_GBI2.h`
  - `third_party/parallel-n64/gles2rice/src/RSP_GBI2_ext.h`
  - `third_party/parallel-n64/gles2rice/src/RSP_GBI_Others.h`
    - added stack-bound guards around unbounded `__RSP.PCi++` display-list pushes
  - `third_party/parallel-n64/mupen64plus-core/src/rdp/rdp_core.c`
  - `third_party/parallel-n64/mupen64plus-core/src/rdp/fb.c`
  - `third_party/parallel-n64/mupen64plus-core/src/rsp/rsp_core.c`
  - `third_party/parallel-n64/mupen64plus-core/src/r4300/mi_controller.c`
    - added defensive null checks
    - added lazy relinking of `dp/sp/ri/r4300` pointers to `g_dev` when unexpectedly null

- rebuilt and restaged core:
  - `make -j8 platform=osx WITH_DYNAREC=aarch64 ALLOW_OSX_OPENGL=1 HAVE_NEON=0 HAVE_GLIDE64=0 HAVE_GLN64=1 HAVE_RICE=1`
  - copied `parallel_n64_libretro.dylib` to `target/debug/cores/`
  - `codesign --force --sign - target/debug/cores/parallel_n64_libretro.dylib`

- verification result:
  - `cargo test -p arcade-libretro parallel_n64_macos_smoke_runs_many_frames -- --ignored --exact --nocapture`
  - now passes (no segfault):
    - `parallel_n64 many-frame smoke cpu_frames=557 empty_frames=43`

Current checkpoint:

- the long-running macOS smoke harness is stable again with the staged core
- next required validation is full app rendering behavior (still need to confirm if visible video is now correct in the real UI path)

Update (latest pass, 2026-03-15, black-screen fallback):

- changed macOS default core-variable policy for `parallel_n64` when `VideoBackendKind::OpenGl`:
  - file: `crates/arcade-libretro/src/core_variables.rs`
  - before: `parallel-n64-gfxplugin=rice`
  - now: `parallel-n64-gfxplugin=angrylion`
- rationale:
  - current macOS OpenGL plugin path (`rice`/`gln64`) is still producing audio-only black output in real app runs
  - software renderer fallback is prioritized so users get visible video now while GL path is debugged separately
- kept existing macOS CPU-core override behavior in place (`parallel-n64-cpucore=pure_interpreter`)
- added a macOS unit test coverage point:
  - `default_core_variables_force_parallel_n64_macos_opengl_to_angrylion`
- verification:
  - `cargo test -p arcade-libretro core_variables::tests::default_core_variables_force_parallel_n64_macos_opengl_to_angrylion -- --exact --nocapture`
  - result: pass

Next validation requested from app run:

- run `cargo run -p arcade-app`
- confirm that N64 now renders (expected slower software path) instead of black screen

Update (latest pass, 2026-03-15, user validation confirmed):

- user validated end-to-end: game is now both visible and audible in the app
- startup log confirms macOS fallback path remains active:
  - `arcade_libretro::core_variables: forcing default framebuffer path for parallel_n64 OpenGL on macOS: ARCADE_GL_FORCE_DEFAULT_FRAMEBUFFER=1`
- launch/runtime behavior:
  - play session starts successfully with `core=parallel_n64`
  - emulator enters execution (`M64CMD_EXECUTE`, `Starting R4300 emulator: Pure Interpreter`)
  - no black-screen symptom reported in this run
  - no immediate segfault in this run
- perf profile from this successful run:
  - `arcade_libretro::perf` shows stable CPU-frame delivery after startup (`cpu_frames=180 empty_frames=0`)
  - frame cadence fluctuates (roughly ~26–60 fps windows in sampled intervals), consistent with software-render fallback and current scheduling constraints
  - `last_frame=640x240` in sampled output

Current checkpoint:

- primary blocker resolved for user workflow: N64 now renders and outputs audio on macOS
- remaining work is optimization/cleanup, not functional bring-up:
  - improve pacing consistency/perf under software fallback
  - optionally continue GL-plugin path investigation (`rice`/`gln64`) behind a guarded toggle before re-enabling as default

Update (latest pass, 2026-03-15, performance tuning pass 1):

- changed macOS `parallel_n64` default CPU core from `pure_interpreter` to `cached_interpreter`
  - file: `crates/arcade-libretro/src/core_variables.rs`
  - applied for:
    - `VideoBackendKind::Software`
    - `VideoBackendKind::OpenGl` path (currently paired with `angrylion` fallback)
  - reason: interpreter mode was a major throughput bottleneck after functional bring-up

- fixed N64 play-loop pacing to avoid adding extra delay when emulation work already exceeded frame budget
  - file: `crates/arcade-ui/src/play_session.rs`
  - previous behavior in `parallel_n64_target_pacing` always scheduled an additional delay from frame interval fraction
  - new behavior:
    - if `post_tick_elapsed < frame_interval`: delay only the remaining budget
    - else: request immediate repaint and clear accumulated catch-up debt
  - expected effect: reduced lag/jitter under load, better effective frame cadence

- verification:
  - `cargo test -p arcade-libretro core_variables::tests::default_core_variables_force_parallel_n64_macos_opengl_to_angrylion -- --exact --nocapture` -> pass
  - `cargo test -p arcade-ui --lib --no-run` -> pass (build/test compile OK)

Next validation requested:

- run `cargo run -p arcade-app`
- check whether:
  - UI perf `tick_hz` increases and `avg_gap_ms` drops
  - gameplay input latency/lag is visibly improved

Update (latest pass, 2026-03-15, performance validation success):

- user validation after tuning confirms a strong/stable run
- runtime confirms tuned path is active:
  - `Starting R4300 emulator: Cached Interpreter`
  - macOS OpenGL fallback guard still enabled:
    - `ARCADE_GL_FORCE_DEFAULT_FRAMEBUFFER=1`
- performance profile in long run:
  - `arcade_libretro::perf` repeatedly ~`60.0 fps`
  - `avg_run_ms` mostly in ~`3.4ms` to `7.4ms` range (with occasional lower early samples)
  - after startup warm-up, delivery is fully populated:
    - `cpu_frames=180 empty_frames=0 errors=0`
  - `arcade_ui::perf` is locked at:
    - `tick_hz=60.0`
    - `avg_gap_ms≈16.67`
    - `avg_frames_per_tick=1.00`
- observed user outcome:
  - game is visible + audible
  - lag complaint resolved for this scenario

Current baseline status:

- macOS N64 path is now functionally stable and performance-stable for this ROM/workload
- this is an acceptable checkpoint for moving to optional follow-up work (quality/accuracy toggles, GL plugin bring-up behind feature flag, cleanup)

Update (latest pass, 2026-03-15, macOS window chrome restore fix):

- issue observed:
  - after exiting a play session, the main app window could remain borderless on macOS (missing traffic-light controls/title bar)
- root area:
  - viewport immersive/fullscreen restore sequencing in `crates/arcade-ui/src/app/viewport.rs`
- fixes applied:
  - `crates/arcade-ui/src/app/mod.rs`
    - increased viewport restore retry budget:
      - `VIEWPORT_RESTORE_RETRY_FRAMES`: `12 -> 120`
  - `crates/arcade-ui/src/app/viewport.rs`
    - when leaving immersive mode, now sends restore commands immediately together with `Fullscreen(false)`
    - added non-session self-healing path:
      - if not in session, not fullscreen, and chrome still appears missing, keep reapplying restore commands (`Decorations(true)` + `EnableButtons`) until restored
- verification:
  - `cargo test -p arcade-ui --lib --no-run` passed

Next validation requested:

- launch app -> start game -> exit game
- confirm title bar and traffic-light controls are consistently restored

Update (latest pass, 2026-03-15, macOS chrome restore fix v2):

- follow-up report: chrome/title bar still disappeared after prior fix
- additional macOS-specific hardening applied in `crates/arcade-ui/src/app/viewport.rs`:
  - do not send `Decorations(false)` when entering immersive mode on macOS
    - keep fullscreen transition without explicitly switching to borderless decorations
  - added an inactive-session hard restore guard:
    - if session is inactive and any of these are true:
      - immersive flag still set
      - restore stage still active
      - viewport still fullscreen
      - chrome appears missing
    - then force:
      - `ViewportCommand::Fullscreen(false)`
      - restore commands (`Decorations(true)` + `EnableButtons`)
- intent:
  - aggressively reassert normal framed window state after leaving game session on macOS
  - avoid persistent borderless style-mask state after fullscreen transitions
- verification:
  - `cargo test -p arcade-ui --lib --no-run` passed

Next validation requested:

- launch app -> start game -> exit game
- confirm traffic-light controls/title bar remain present after returning to library

Update (latest pass, 2026-03-15, macOS chrome restore validated):

- user confirmed the v2 viewport/chrome fix works in real usage:
  - after exiting gameplay, the app window keeps macOS chrome/traffic-light controls
- this closes the restore regression for the current macOS flow

Update (latest pass, 2026-03-15, cleanup pass):

- cleaned up cross-target warnings and finalized wording:
  - `crates/arcade-libretro/src/core_variables.rs`
    - removed temporary wording for macOS OpenGL `angrylion` safety default comment
  - `crates/arcade-libretro/src/lib.rs`
    - gated frontend capability locals to linux/windows builds only
    - added non-linux/windows no-op use for Vulkan surface constructor params to avoid unused warnings
  - `crates/arcade-ui/src/app/viewport.rs`
    - gated `EXTERNAL_PRESENT_OFFSCREEN_POSITION` constant to linux only to avoid dead-code warnings on macOS/windows

Update (latest pass, 2026-03-15, TheGamesDB settings simplification):

- simplified the Settings -> TheGamesDB panel to keep only:
  - API Key
  - Default Limit
  - Default Delay (ms)
- removed editable platform-ID rows from the UI
- hardcoded platform IDs at save time in `crates/arcade-ui/src/views/manage.rs`:
  - NES `[7]`, SNES `[6]`, GENESIS `[18]`, GB `[4]`, GBA `[5]`, N64 `[3]`, ARCADE `[23]`
- removed now-unused platform-ID fields from `crates/arcade-ui/src/state/manage.rs`
- verification:
  - `cargo test -p arcade-ui --lib --no-run` passed

Update (latest pass, 2026-03-15, Genesis investigation):

- user reported Genesis titles not visibly playing and shared `genesis_plus_gx` startup logs
- investigation result:
  - those `Game Genie/BIOS should be located at ...` lines are informational startup messages from `genesis_plus_gx`
  - no fatal error was emitted in the provided log
- added targeted smoke test:
  - `crates/arcade-libretro/tests/genesis_smoke.rs`
  - `genesis_plus_gx_smoke_runs_frames` (ignored/local fixture test)
- local verification:
  - `cargo test -p arcade-libretro genesis_plus_gx_smoke_runs_frames -- --ignored --exact --nocapture`
  - pass with:
    - `cpu_frames=300 empty_frames=0`
    - observed output sizes: `(256x192)` and `(320x224)`
- current conclusion:
  - core load/run path is functioning in host runtime
  - remaining issue is likely in live app presentation/interaction path rather than core boot failure

Update (latest pass, 2026-03-15, cross-system session regression report):

- user identified a reproducible lifecycle regression:
  - cold start app -> launch GENESIS game -> game plays
  - exit GENESIS session -> launch a game from a different system -> launch starts, then exits immediately
- impact:
  - major regression in multi-session stability across systems
  - previously other systems were launchable in sequence without app restart
- current working hypothesis:
  - session teardown/reset state is incomplete when transitioning from GENESIS to another core/system in the same app process
  - likely in host unload/reload lifecycle or per-session runtime state reset (not initial cold-boot core load)
- next debugging target:
  - reproduce with detailed launch/unload logs around `stop_play_session`, `host.unload`, next `prepare_launch`, and `load_for_rom`

Update (latest pass, 2026-03-15, cross-system session regression fix attempt):

- likely root cause identified in frontend shortcut lifecycle:
  - when a new play session starts, Exit/Reset/Save/Load shortcuts can still be physically held from the prior session transition
  - this can trigger hold-based Exit shortly after launch, appearing as "game starts then quits"
- fix implemented:
  - `crates/arcade-ui/src/state/play.rs`
    - added `block_frontend_shortcuts_until_release` gate
    - on `begin_session`, frontend shortcuts are now blocked until all shortcut inputs are observed released once
    - `begin_session`/`clear_session` now explicitly reset quick-save/load/slot latches
    - `reset_frontend_shortcut_latches` now also resets return-hold state
    - added helper:
      - `should_block_frontend_shortcuts_until_release(any_shortcut_pressed: bool) -> bool`
  - `crates/arcade-ui/src/input.rs`
    - in `apply_frontend_shortcuts`, added early gate check using:
      - `reset || quick_save || quick_load || next_save_slot || return_pressed`
    - if gate is active and any shortcut is still pressed, ignore shortcut handling for that frame
- intent:
  - prevent stale held inputs from previous session exits from immediately terminating the next launched game
- verification:
  - `cargo check -p arcade-ui` passed

Update (latest pass, 2026-03-15, cross-system session regression fixed in user validation):

- user validated the fix in real app behavior:
  - can launch -> exit -> relaunch games within the same system
  - can launch -> exit -> launch games across different systems
  - no immediate auto-exit regression observed after session transitions
- outcome:
  - multi-session lifecycle is stable again for the tested flows
  - prior major regression ("starts then quits") is considered resolved at current checkpoint

Update (latest pass, 2026-03-15, SNES audio quality improvement):

- issue reported:
  - SNES audio sounded crunchy/rough
- audio pipeline tuning applied in `crates/arcade-libretro/src/lib.rs`:
  - increased output buffering safety margins:
    - `AUDIO_TARGET_LATENCY_SECS`: `0.040 -> 0.060`
    - `AUDIO_MAX_LATENCY_SECS`: `0.120 -> 0.180`
  - increased CPAL callback buffer target from ~8ms to ~16ms in `pick_buffer_size`
    - target changed from `sample_rate / 120` to `sample_rate / 60`
    - clamp widened from `128..1024` to `256..2048`
  - resampler drift correction was softened to reduce audible modulation:
    - added:
      - `AUDIO_RESAMPLE_QUEUE_CORRECTION = 0.005`
      - `AUDIO_RESAMPLE_RATIO_DRIFT_LIMIT = 0.005`
    - ratio now tracks near base `source_rate/output_rate` with only ±0.5% drift window
- attempted underrun fallback smoothing (sample hold) was reverted after user feedback because it made audio sound "8-bit"
- verification:
  - `cargo check -p arcade-libretro` passed after final tuning
- user validation:
  - "that was it, it sounds beautiful now"
  - issue considered resolved at this checkpoint

Update (latest pass, 2026-03-15, ARCADE BIOS/core routing and mslug compatibility):

- user reported ARCADE launch failures around shared BIOS lookup and then `mslug` startup
- initial path symptom was addressed by wiring ARCADE compatibility checks and staging helpers to honor `bios_root`:
  - `crates/arcade-domain/src/arcade_deps.rs`
    - `get_arcade_bios_directory(...)` and `get_missing_arcade_shared_bios_files(...)` now include `bios_root` candidates
  - `crates/arcade-services/src/lib.rs`
    - `prepare_launch(...)` now passes `config.paths.bios_root` into `get_arcade_compatibility(...)`
  - `crates/arcade-libretro/src/content.rs`
    - shared archive staging (`neogeo.zip`, `pgm.zip`, `qsound.zip`) can resolve from BIOS-root locations
- after this, logs showed runtime moved past path errors and into core/romset behavior

- next observed failure:
  - ARCADE entries for `mslug` were being launched with `fbneo` even when the DB row had `emulatorCore=mame2003`
  - this came from ARCADE soft title preference policy in `resolve_effective_core_override(...)`
- a temporary policy change was tested to prefer configured/scanned core first for ARCADE titles
  - result: `mslug` launched with `mame2003` as intended

- next observed failure under `mame2003`:
  - load path moved to real ROM path and progressed deep into ROM loading
  - logs showed concrete romset mismatch signals:
    - `sfix.sfx NOT FOUND`
    - multiple NeoGeo BIOS/content checksum mismatches
    - `000-lo.lo` wrong length/checksum for expected `mame2003` set
    - multiple `201-c*.bin` checksum mismatches
  - this established that the available `mslug.zip`/`neogeo.zip` content matches a different set (works with `fbneo`) and is not `mame2003`-compatible

- launch staging behavior was also tightened:
  - `crates/arcade-libretro/src/content.rs`
    - ARCADE temp-dir staging is now opt-in only via:
      - `ARCADE_STAGE_LAUNCH_ARCHIVES=1`
    - default path is direct ROM loading (avoids temp-path load regression)

- final policy decision requested by user:
  - prefer `fbneo` for stability when available
  - ARCADE core-resolution policy was set back to:
    - hard override -> soft fbneo title preference -> configured core
  - file:
    - `crates/arcade-domain/src/core.rs`
      - `resolve_effective_core_override(...)` now restores fbneo-first soft preference behavior for titles like `mslug`

- tests/verification added in this pass:
  - `crates/arcade-domain/src/arcade_deps.rs`
    - separate BIOS-root lookup coverage
  - `crates/arcade-libretro/src/content.rs`
    - staging from BIOS-root coverage
    - default no-staging coverage
  - `crates/arcade-services/src/lib.rs`
    - ARCADE launch core-resolution regression coverage for `mslug` policy
  - repeated checks/tests passed across:
    - `arcade-domain`
    - `arcade-services`
    - `arcade-libretro`
    - `arcade-app`

Update (latest pass, 2026-03-15, workspace move confirmation):

- active working directory moved to:
  - `/Users/jules/native`
- verified that this workspace contains the same updated ARCADE files/policies described above
- confirmed successful compile in the moved workspace with:
  - `cargo check -p arcade-app -p arcade-services -p arcade-libretro`

Update (latest pass, 2026-03-15, ARCADE/FBNeo pacing tuning):

- after ARCADE launch was fixed, user reported low runtime cadence in perf logs for FBNeo on macOS:
  - `tick_hz` around ~35
  - `avg_gap_ms` around ~28ms
  - `avg_work_ms` low (~2-3ms), indicating scheduling/pacing overhead rather than emulation load
- host/runtime diagnosis:
  - FBNeo was now loading correctly with direct filesystem pathing and BIOS discovery
  - bottleneck appeared in UI play-loop pacing (`request_repaint_after` jitter on macOS ARCADE path)

- pacing change applied in:
  - `crates/arcade-ui/src/play_session.rs`
- new behavior:
  - introduced ARCADE-targeted low-latency pacing branch (`arcade_low_latency_pacing`)
  - when ARCADE is active and `elapsed < frame_interval`, request immediate repaint instead of timer-based `request_repaint_after(...)`
  - after running frame(s), ARCADE path now uses immediate `ctx.request_repaint()` instead of interval sleep scheduling
- rationale:
  - avoid timer quantization/jitter that was producing ~30-40 Hz cadence despite low frame work time
  - keep prior pacing behavior unchanged for non-ARCADE systems and the specialized `parallel_n64` branch

- verification:
  - `cargo fmt --all`
  - `cargo check -p arcade-ui -p arcade-app`
  - both passed

- next validation requested:
  - run `cargo run -p arcade-app`
  - launch FBNeo ARCADE title
  - compare `arcade_ui::perf` logs; expected improvement is materially higher `tick_hz` and lower `avg_gap_ms`

Update (latest pass, 2026-03-15, ARCADE/FBNeo pacing tuning pass 2):

- after the first ARCADE low-latency repaint change, perf improved from ~35 Hz into ~45-50 Hz range, but still below target cadence over long runs
- logs showed:
  - low `avg_work_ms` (~2-3ms)
  - `avg_frames_per_tick` staying near `1.00`
  - suggesting UI tick cadence was still limiting effective frame delivery and catch-up was too conservative

- additional pacing change applied in:
  - `crates/arcade-ui/src/play_session.rs`
- new ARCADE-specific catch-up model:
  - for `arcade_low_latency_pacing`, switched to explicit frame-budget/debt accumulation (similar style to existing `parallel_n64` path)
  - computes:
    - `elapsed_frames = elapsed / frame_interval`
    - `frame_budget = catch_up_debt + elapsed_frames` (clamped)
    - `frames_to_run = floor(frame_budget)` (clamped to `1..=3`)
    - `catch_up_debt = fractional remainder`
  - intent:
    - when UI tick cadence is below target, run occasional extra core frames to maintain real-time emulation speed instead of remaining near 1 frame/tick

- verification:
  - `cargo fmt --all`
  - `cargo check -p arcade-ui -p arcade-app`
  - both passed

- next validation requested:
  - run `cargo run -p arcade-app`
  - launch FBNeo ARCADE title
  - compare `arcade_ui::perf`:
    - expect `avg_frames_per_tick` to rise above 1.0 when needed
    - expect improved effective cadence/game speed versus prior ~45-50 Hz drift

Update (latest pass, 2026-03-15, ARCADE/FBNeo pacing validation and stabilization):

- user reran after ARCADE pacing jitter tuning
- observed sustained perf window:
  - `tick_hz` roughly `45.6 .. 47.3`
  - `avg_gap_ms` roughly `21.16 .. 21.95`
  - `avg_work_ms` roughly `2.23 .. 2.69`
  - `avg_frames_per_tick` roughly `1.27 .. 1.32`
- effective delivered cadence (`tick_hz * avg_frames_per_tick`) stays near `58 .. 61 fps`
- interpretation:
  - real-time gameplay speed target is being met via stable catch-up behavior
  - cadence is now materially improved versus earlier ~35 Hz / ~1.00 frames-per-tick runs
  - no new launch/path regressions observed in this validation window

Current checkpoint:

- ARCADE launch path is functional with `fbneo`
- BIOS/romset discovery works for validated title set (`mslugx` in this run)
- pacing/perf behavior is acceptable and stable for this workload
- this is an acceptable stop point unless further fine-grained frame-time smoothing is explicitly requested

Update (latest pass, 2026-03-16, macOS `parallel_n64` Vulkan default + 2x headroom tuning):

- scope shifted back to N64 on Apple Silicon macOS (`/Users/jules/native`) with goal:
  - make `parallel_n64` Vulkan path the practical default for N64
  - keep other cores on safe frontend OpenGL path
  - stabilize 2x ParaLLEl upscaling performance and frame pacing

- core/source build status:
  - rebuilt staged core with Vulkan support using:
    - `/Users/jules/native/scripts/build_parallel_n64_macos_vulkan.sh`
  - install target confirmed:
    - `/Users/jules/native/target/debug/cores/parallel_n64_libretro.dylib`
  - resulting core variables include expected plugin options (`auto|angrylion|parallel`)

- runtime bring-up progression:
  - initial launch failed on missing Vulkan loader (`libvulkan.dylib`)
  - loader resolution verified via Homebrew loader path:
    - `/opt/homebrew/lib/libvulkan.dylib`
  - subsequent `ERROR_INCOMPATIBLE_DRIVER` instance-creation failure was resolved in host/runtime negotiation path (including portability-related instance extension handling)
  - Vulkan device creation now succeeds on Apple M1 and game sessions start with `parallel_n64`

- backend/default policy now in code:
  - macOS `parallel_n64` defaults to Vulkan backend (experimental gate remains available)
  - non-`parallel_n64` cores continue to default to OpenGL frontend path
  - relevant files:
    - `/Users/jules/native/crates/arcade-libretro/src/video/mod.rs`
    - `/Users/jules/native/crates/arcade-libretro/src/video/policy.rs`
    - `/Users/jules/native/crates/arcade-libretro/src/lib.rs`

- ParaLLEl Vulkan headroom defaults added for macOS:
  - in `/Users/jules/native/crates/arcade-libretro/src/core_variables.rs`
  - for `parallel_n64` + Vulkan:
    - disable VI extras:
      - `parallel-n64-parallel-rdp-vi-aa=disabled`
      - `parallel-n64-parallel-rdp-vi-bilinear=disabled`
      - `parallel-n64-parallel-rdp-dither-filter=disabled`
      - `parallel-n64-parallel-rdp-divot-filter=disabled`
      - `parallel-n64-parallel-rdp-gamma-dither=disabled`
    - dynamic accuracy policy:
      - `gfxplugin-accuracy=high` at `1x`
      - `gfxplugin-accuracy=medium` above `1x` (2x/4x/8x)
  - tests added and passing for these defaults

- frame pacing/runtime tuning added:
  1. `parallel_n64` repaint wait clamp in UI loop to reduce timer-jitter oversleep:
     - `/Users/jules/native/crates/arcade-ui/src/play_session.rs`
  2. fast-path frame upload for `Rgba8888` (row copy instead of per-pixel conversion):
     - `/Users/jules/native/crates/arcade-ui/src/render.rs`
     - dedicated tests added for packed/padded RGBA rows

- fallback-sync default policy updated:
  - `ARCADE_VULKAN_FORCE_FALLBACK_IDLE` now defaults to disabled (false) when unset
  - explicit override remains:
    - `=1` to force conservative idle waits
    - `=0` to force disable
  - updated in:
    - `/Users/jules/native/crates/arcade-libretro/src/lib.rs`
    - `/Users/jules/native/README.md`

- user validation summary (DKR, 2x upscaling):
  - with default/old conservative fallback idle behavior: long dips toward ~52-55 Hz windows remained
  - with `ARCADE_VULKAN_FORCE_FALLBACK_IDLE=0`: materially improved and sustained near-60 windows became more frequent/longer
  - latest run after tuning shows:
    - core perf often ~58-60 fps
    - UI tick commonly ~56-60 Hz with fewer severe low windows
  - user explicitly reported: "big improvement at 2x"

- current practical state:
  - N64 + `parallel_n64` Vulkan on macOS is now viable as default path in this workspace
  - 2x upscaling is close to target and much improved
  - occasional foreground-window cadence dips still appear intermittently and likely involve UI/compositor scheduling sensitivity rather than core-only throughput

Update (latest checkpoint, 2026-03-16, settings/persistence + default behavior lock-in):

- verified scope for Vulkan support in this app:
  - `parallel_n64` is the only N64 core path currently wired for Vulkan in host defaults/policy
  - other cores remain on safe OpenGL/software paths by default

- app settings/config model now persists an N64 ParaLLEl quality preset:
  - `balanced` (default)
  - `performance`
  - files:
    - `/Users/jules/native/crates/arcade-domain/src/config.rs`
    - `/Users/jules/native/crates/arcade-services/src/lib.rs`
    - `/Users/jules/native/crates/arcade-ui/src/state/manage.rs`
    - `/Users/jules/native/crates/arcade-ui/src/views/manage.rs`
    - `/Users/jules/native/crates/arcade-ui/src/state/menu_nav.rs`
    - `/Users/jules/native/crates/arcade-ui/src/input.rs`
    - `/Users/jules/native/crates/arcade-ui/src/views/settings.rs`

- preset behavior in core variable defaults:
  - `balanced`: existing headroom defaults
  - `performance`: lower accuracy (`medium` at 1x, `low` above 1x) with VI extras disabled
  - file:
    - `/Users/jules/native/crates/arcade-libretro/src/core_variables.rs`

- renderer/runtime defaults on macOS:
  - app default frontend renderer is `glow` (OpenGL)
  - optional `wgpu`/Metal path available with `ARCADE_MACOS_RENDERER=wgpu`
  - file:
    - `/Users/jules/native/crates/arcade-app/src/main.rs`

- Vulkan fallback idle env default was confirmed:
  - `ARCADE_VULKAN_FORCE_FALLBACK_IDLE` defaults to `false` when unset
  - explicit `=1` still forces conservative idle waits for debugging
  - file:
    - `/Users/jules/native/crates/arcade-libretro/src/lib.rs`

- docs/context sync:
  - README reflects current macOS Vulkan requirements and defaults
  - this CONTEXT.md section captures the new persisted preset + default backend policy state for handoff
