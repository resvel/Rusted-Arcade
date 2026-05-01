# Session Log

## 2026-05-01 - Direct press-to-bind controller mapping

### Goal

Replace the system-mapper “select action, then choose from a list” flow with a direct listening mode so clicking a system action captures the next controller input for the selected device.

### Findings

- The visual mapper already had the right abstraction boundary for one-to-one rebinding: `assign_physical_input_to_action()` clears the old action and reassigns the control, so listen-mode can reuse the same conflict policy as manual list binding.
- `capture_gamepad_state()` computes canonical input state for every connected playable controller before filtering down to assigned player slots. That makes it the right place to detect mapper captures, including controllers that are connected but not currently assigned to a gameplay slot.
- The main UX hazard is frontend navigation stealing the same controller input used for mapping. Suppressing gamepad-driven menu navigation while the mapper is listening avoids accidental activation while preserving the existing manual fallback list.
- A baseline pass is required when listening starts so buttons already held down do not bind until they are released and pressed again.

### Changes

- `crates/arcade-ui/src/state/controller_mapping.rs`: added transient listening state, baseline tracking, and automatic cancellation on system/device/load/reset/save transitions.
- `crates/arcade-ui/src/input.rs`: added direct press-to-bind start/cancel helpers, per-frame capture logic for the selected controller, deterministic control detection based on the existing physical hotspot order, and focused listen-mode tests.
- `crates/arcade-ui/src/views/library_chrome.rs`: clicking a hotspot or action row now starts listening immediately; the assignment panel shows an active listening card with a cancel button; manual assign/unassign still works and cancels listening.
- `crates/arcade-ui/src/views/settings.rs`: system changes now go through controller-mapping state so listening is canceled cleanly when the edited system changes.

### Tests

- `cargo test -p arcade-ui` — passed.

### Next Steps

- Manually verify the settings-screen mapper with a connected controller: hotspot click, text-fallback action click, cancel via Escape, trigger/stick-direction capture, and no accidental menu navigation while listening.

## 2026-05-01 - SVG hotspot overlay refinement (Saturn)

### Goal

Finish the remaining Saturn system overlay so the visual mapper targets the actual button art instead of the placeholder grid.

### Findings

- Saturn uses six distinct face-button centers arranged in two slanted rows, not a flat 3x2 grid. The top row is `X (42.53,29.84)`, `Y (47.74,26.64)`, `Z (53.38,25.14)`. The bottom row is `A (44.11,37.57)`, `B (50.35,33.85)`, `C (57.43,31.89)`.
- The top row circles are the smaller `r≈2.43` buttons; the lower row uses larger `r≈3.22` buttons with inner cutouts.
- Saturn Start is the rounded rectangle centered near `(31.90,32.87)`. The previous hotspot at `y≈42.69` sat on the shell below the real button.
- Saturn D-pad arm centers are `Up (13.89,26.27)`, `Down (13.90,34.98)`, `Left (9.57,30.65)`, and `Right (18.24,30.46)`.

### Changes

- `public/gamepads/hotspots/outline/saturn.hotspots.svg`: rewrote Saturn hotspot positions to the slanted six-button layout, corrected D-pad arm centers, and moved Start up to the actual center button.
- `crates/arcade-ui/src/controller_mapper.rs`: updated `SATURN_SYSTEM_HOTSPOTS` fallback geometry to match the corrected overlay.
- `docs/llm-wiki/debugging/svg-hotspot-overlays.md`: added durable notes on Saturn geometry and overlay-analysis rules.
- `docs/llm-wiki/index.md`: linked the new debugging note.

### Tests

- `cargo test -p arcade-ui overlay_files_cover_required_actions_for_supported_systems` — passed.

### Next Steps

- Visually verify the Saturn overlay in-app against the slanted six-button layout and the higher Start button.

## 2026-05-01 - SVG hotspot overlay refinement (PS2, Dreamcast)

### Findings
- PS2 sub-path bounding boxes (from Z-delimited segments) give reliable D-pad arm centers; arc endpoint math gives reliable circle centers. Always use cursor-tracking arc parser (not naive bbox) for circles — the bbox only captures arc endpoints, not the full arc extent.
- PS2 Select and Start are at different Y levels (Select y≈26.5, Start y≈32.3); the original hotspot had both at y=29.12.
- PS2 Start button was originally placed on the analog LED indicator (path 14 at center 37.65, 26.42); the real Start is path 12 (center 30.34–33.67, y=31.06–33.56). Path 14 is the LED.
- Dreamcast SVG has the analog stick as a round disc (sub1/2, center 9.22, 20.22, r=4.94) in the upper-left and the D-pad as a cross/plus shape (sub3/4, center 12.79, 34.77) in the lower-left — opposite of what the element shapes suggest visually.
- Dreamcast `DREAMCAST_SYSTEM_HOTSPOTS` in `controller_mapper.rs` had only 9 entries with no stick actions, causing the overlay to reject any SVG containing `Stick Up/Down/Left/Right` and fall back to text rendering.
- Dreamcast Start button is the triangle shape at sub5–8, center (32.00, 46.52), not the element at y≈40.

### Changes
- `public/gamepads/hotspots/outline/ps2.hotspots.svg`: full rewrite — D-pad arms from bbox centers, face buttons from arc endpoints, analog stick centers from cursor-tracking arc parser (L3: 22.58,35.82; R3: 41.09,35.82), Select/Start from path bboxes, stick directional rects at outer circle edges.
- `public/gamepads/hotspots/outline/dreamcast.hotspots.svg`: full rewrite — D-pad on cross shape (12.79,34.77), analog stick rects on disc edges (9.22,20.22), face buttons Y/X/A/B from arc analysis, Start moved to triangle shape (32.00,46.52).
- `crates/arcade-ui/src/controller_mapper.rs`: `DREAMCAST_SYSTEM_HOTSPOTS` expanded from 9 to 13 entries to include Stick Up/Down/Left/Right.

### Next Steps
- Saturn hotspot work remaining.

## 2026-05-01 - SVG hotspot overlay refinement (N64, PSX)

### Findings
- SVG circle centers must be derived from arc math, not the raw M coordinate — M is the arc START POINT (top or bottom of circle), not the center. Off-by-r errors (up to 9 units) are the dominant mistake in existing hotspots.
- SVG paths use implicit lineto after `m` — a sequence like `m dx1 dy1 dx2 dy2` means moveto + lineto. Missing the implicit lineto caused the PSX Start button to be placed 2.83 units too far right.
- Even-odd fill paths draw buttons twice (once solid, once as cutout). The first occurrence gives the correct center; the second is the body-subtraction duplicate.
- N64 SVG: all button positions were wrong by ~11 units (D-pad), ~5 units (A, B, face buttons), and ~18 units (Start). Root cause: hotspots were placed using raw M coordinates without arc-radius correction.
- PSX D-pad center is at (13.62, 26.44), not (12.80, 26.72). Left arm center is cx≈10.33, not 8.45 (old value was at the outer tip, not the arm center).

### Changes
- `public/gamepads/hotspots/outline/n64.hotspots.svg`: full rewrite — D-pad, analog stick rects, A, B, Start, C buttons, L/R shoulders all corrected from SVG path analysis.
- `public/gamepads/hotspots/outline/psx.hotspots.svg`: face buttons (Triangle/Square/Circle/Cross), D-pad, Select, Start all corrected. Select moved up 5.7 units; Start moved up ~1 unit and left 2.83 units.

### Next Steps
- Remaining systems still need hotspot work: PS2, Dreamcast, Saturn.

## 2026-04-30 - Initialize Project Wiki

### Goal

Initialize the `docs/llm-wiki/` directory and populate it with the initial project structure, architecture overview, and core-specific information based on the initial repository inspection.

### Findings

- The project is a Rust workspace with several specialized crates (`arcade-app`, `libretro`, etc.).
- The `arcade-libretro` crate contains complex logic for Libretro integration, including a sophisticated Vulkan backend for macOS.
- The project uses a multi-backend video system (Software/GL and Vulkan).
- The `mupen64plus-libretro-nx` core is a primary target and requires specific handling for ARM64 macOS (dynarec binaries).
- The `docs/llm-wiki/` directory was missing several key files.

### Changes

- Created `docs/llm-wiki/architecture/libretro-integration.md`.
- Created `docs/llm-wiki/cores/mupen64plus-libretro-nx.md`.
- Updated `docs/llm-wiki/repo-overview.md` with crate structure details.
- Initialized `docs/llm-wiki/log.md`.

### Tests

- Verified file existence and content for all newly created/updated wiki pages.

### Next Steps

- Continue expanding the wiki as new architectural decisions or debugging findings emerge.
