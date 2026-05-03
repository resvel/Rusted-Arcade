# Session Log

## 2026-05-02 - Normalize Game Boy SVG into bundled asset

### Goal

Convert `nintendo-gameboy-8633.svg` into a repo-style bundled controller asset instead of leaving it as a raw stock SVG.

### Findings

- The source asset was already a true vector, so normalization mainly required packaging rather than retracing.
- Because the source is a single monolithic path, the simplest faithful conversion is to preserve that path and normalize it with a compact `64 x 64` viewBox plus a transform.
- This creates a usable `gb.svg` base asset, but it does not solve hotspot placement or mapper wiring by itself.

### Changes

- `public/gamepads/controllercons.2.1/svg/outline/gb.svg`: added a normalized bundled Game Boy asset with `ccsvg` classes, compact viewBox, and the original vector path transformed into repo scale.
- `docs/llm-wiki/decisions/game-boy-controller-art-evaluation.md`: updated status and recommendation now that a first-pass bundled asset exists.

### Tests

- Source geometry extracted from `/Users/jules/Downloads/nintendo-gameboy-8633.svg`.
- No Rust tests were needed because this pass only added a bundled art asset and wiki updates.

### Next Steps

- Do a visual pass on `gb.svg`.
- If it looks good, the next implementation step is a Game Boy system layout plus `gb.hotspots.svg`.

## 2026-05-02 - Evaluate Game Boy SVG candidate

### Goal

Inspect `nintendo-gameboy-8633.svg` to decide whether it is good source material for a future Game Boy visual mapper controller asset.

### Findings

- The file is a true vector SVG, which already makes it much better source material than raster-wrapped SVG candidates.
- It is encoded as a single giant filled `<path>` with `fill="black"` and `stroke="none"`, so it is structurally harder to adapt than multi-shape or grouped vector art.
- The geometry likely contains the right Game Boy front-view elements, but it would still need normalization into the repo's compact bundled controller-art style before mapper use.

### Changes

- `docs/llm-wiki/decisions/game-boy-controller-art-evaluation.md`: added a decision page documenting the asset’s suitability and tradeoffs.
- `docs/llm-wiki/index.md`: linked the new Game Boy evaluation page from the Decisions section.

### Tests

- `file /Users/jules/Downloads/nintendo-gameboy-8633.svg`
- `sed -n '1,260p' /Users/jules/Downloads/nintendo-gameboy-8633.svg`
- Structural inspection showed `1` path and no separate `circle`, `rect`, or `text` elements.

### Next Steps

- If we move forward on a Game Boy mapper layout, use this SVG as source material but plan on a normalization pass plus manual hotspot placement.

## 2026-05-02 - Wire PC Engine into the visual mapper

### Goal

Activate the new PC Engine controller art in the system mapper by adding a real `SystemControllerLayout` entry, fallback hotspots, and a dedicated overlay file.

### Findings

- `PCECD` was already fully supported in input binding logic, so the remaining gap was purely the visual mapper layer.
- The new TurboPad-based art was sufficient to place stable hotspots for `Up`, `Down`, `Left`, `Right`, `I`, `II`, `Select`, and `Run`.
- The overlay file format was enough for PC Engine with only circles and rects; no polygon hotspots were needed.

### Changes

- `crates/arcade-ui/src/controller_mapper.rs`: added `SystemControllerLayout::PcEngine`, mapped `PCECD`/`PC_ENGINE`/`PCENGINE`, added bundled art/overlay routing, and defined `PC_ENGINE_SYSTEM_HOTSPOTS`.
- `public/gamepads/hotspots/outline/pc-engine.hotspots.svg`: added the PC Engine overlay file aligned to the new controller art.
- `docs/llm-wiki/decisions/pc-engine-controller-support.md`: updated status and implementation notes now that the mapper path is active.

### Tests

- `cargo fmt --all`
- `cargo test -p arcade-ui overlay_files_cover_required_actions_for_supported_systems`
- `cargo test -p arcade-ui pcecd_action_bindings_map_to_expected_retropad_ids`

### Next Steps

- Do a quick in-app visual pass on the PC Engine mapper screen to confirm hotspot placement against the latest art.

## 2026-05-02 - Replace PC Engine draft with TurboPad-based vector

### Goal

Swap out the unsatisfactory hand-built `pc-engine.svg` draft and rebuild the bundled PC Engine asset from the stronger `pc-engine-turbopad-inspired-outline.svg` source.

### Findings

- The TurboPad-inspired file was the right base because it already had clean vector geometry for the body, D-pad, center buttons, face buttons, and turbo controls.
- The main normalization work was subtractive rather than additive: remove poster-style text and extra panel decoration, keep the controller geometry, and compress it into the repo's compact bundled asset format.
- Converting the source into a `64 x 64` asset by scaling the original geometry inside a transformed group is a practical way to preserve the stronger proportions without hand-redrawing every feature again.

### Changes

- `public/gamepads/controllercons.2.1/svg/outline/pc-engine.svg`: replaced the earlier hand-built draft with a normalized TurboPad-inspired vector asset using compact monochrome geometry, no font-dependent labels, and retained turbo-switch/button structure.
- `docs/llm-wiki/decisions/pc-engine-controller-support.md`: updated status/path-forward notes to reflect that the current local base now comes from the TurboPad-inspired vector source.

### Tests

- Compared `pc-engine-turbopad-inspired-outline.svg` structure against the existing bundled controller SVG style.
- `sed -n '1,220p' public/gamepads/controllercons.2.1/svg/outline/pc-engine.svg`

### Next Steps

- Do a final visual pass on the new `pc-engine.svg`.
- Once the art is approved, add `SystemControllerLayout::PcEngine` and create `pc-engine.hotspots.svg`.

## 2026-05-02 - Inspect alternate TurboPad-inspired SVG

### Goal

Evaluate whether `pc-engine-turbopad-inspired-outline.svg` is a better base asset for PC Engine controller art than the earlier wrapped-raster candidate and the first local draft.

### Findings

- `pc-engine-turbopad-inspired-outline.svg` is a true vector SVG, unlike the earlier `PC-EngineController.svg` wrapper that only embedded a PNG.
- It is a stronger geometry/source-material candidate because it already separates the body, D-pad, center buttons, face buttons, and turbo controls into clean vector groups.
- It still does not match the repo's bundled `controllercons.2.1` style directly because it relies on CSS classes, explicit fills, font-based labels, and a large `1200 x 620` presentation-oriented canvas.

### Changes

- `docs/llm-wiki/decisions/pc-engine-controller-support.md`: noted that the TurboPad-inspired SVG is a better true-vector base, but still needs normalization into bundled asset style.

### Tests

- `file /Users/jules/Downloads/pc-engine-turbopad-inspired-outline.svg`
- `sed -n '1,260p' /Users/jules/Downloads/pc-engine-turbopad-inspired-outline.svg`
- Compared structure against `public/gamepads/controllercons.2.1/svg/outline/nes.svg`

### Next Steps

- If chosen as the new base, simplify `pc-engine-turbopad-inspired-outline.svg` into a compact monochrome bundled asset with text removed or converted to shape-only controller marks.

## 2026-05-02 - Refine PC Engine SVG proportions

### Goal

Improve the first-pass `pc-engine.svg` so its proportions and browser preview better match the supplied controller reference.

### Findings

- The biggest visual mismatch in raw browser previews came from the asset being stroke-based: a normal SVG stroke scales aggressively when the standalone file is displayed large in the browser.
- Adding `vector-effect="non-scaling-stroke"` is a practical fit for this hand-built outline because it keeps the line weight closer to the original reference image during large previews.
- The draft also benefited from tighter face-button, start-button, and top-arch proportions rather than just lowering the stroke width alone.

### Changes

- `public/gamepads/controllercons.2.1/svg/outline/pc-engine.svg`: refined the outer shell, cable arch, D-pad, center buttons, and face-button pod; added non-scaling strokes so standalone preview weight stays controlled.
- `docs/llm-wiki/decisions/pc-engine-controller-support.md`: noted that the current draft relies on non-scaling strokes during preview.

### Tests

- Compared the rendered browser screenshot against the extracted reference image from `/tmp/pc-engine-controller.png`.
- No Rust tests were needed because this pass only adjusted the SVG art and wiki notes.

### Next Steps

- Do one more visual browser pass on `pc-engine.svg` and tweak any remaining spacing issues if they stand out.
- Once the art feels stable, proceed with PC Engine mapper wiring and `pc-engine.hotspots.svg`.

## 2026-05-02 - Draft PC Engine outline SVG

### Goal

Turn the provided raster-wrapped `PC-EngineController.svg` reference into a proper local vector outline asset that can serve as the base for future PC Engine mapper support.

### Findings

- The embedded PNG is clean enough to hand-trace into a simple controller outline without depending on external tracing tools.
- A manual SVG built from strokes and primitives is a practical interim asset even if it does not yet perfectly match every `controllercons.2.1` authoring convention.
- PC Engine is still not wired into `SystemControllerLayout`, so the new art file is preparatory rather than active.

### Changes

- `public/gamepads/controllercons.2.1/svg/outline/pc-engine.svg`: added a first-pass clean vector outline based on the supplied reference image.
- `docs/llm-wiki/decisions/pc-engine-controller-support.md`: updated status to note that a draft local outline asset now exists.

### Tests

- Visual source inspected by extracting the embedded PNG from `/Users/jules/Downloads/PC-EngineController.svg`.
- No Rust tests were required because this pass only added a new art asset and wiki updates.

### Next Steps

- Render-check and refine `pc-engine.svg` until it feels consistent with the other `controllercons.2.1` outlines.
- Add `SystemControllerLayout::PcEngine`, mapper asset routing, and a `pc-engine.hotspots.svg` overlay when ready to activate the system in the visual mapper.

## 2026-05-02 - Inspect PC Engine SVG candidate

### Goal

Check whether a provided `PC-EngineController.svg` file can be used as the PC Engine controller image for the visual mapper.

### Findings

- PC Engine is still not wired into the mapper layout selection or asset lookup paths in `crates/arcade-ui/src/controller_mapper.rs`.
- The provided `PC-EngineController.svg` is not true vector outline art; it is an Inkscape SVG containing a single embedded base64 PNG on a `297 x 210` canvas.
- Because the current mapper art pipeline expects clean SVG assets that rasterize well and pair with hotspot overlays, this file is better treated as a reference or temporary fallback than a final bundled outline asset.

### Changes

- `docs/llm-wiki/decisions/pc-engine-controller-support.md`: recorded that the inspected SVG candidate is a raster image wrapped in SVG markup, so the vector-art blocker still applies.

### Tests

- `file /Users/jules/Downloads/PC-EngineController.svg` — confirmed the file is SVG markup.
- `sed -n '1,220p' /Users/jules/Downloads/PC-EngineController.svg` — showed the SVG contains a single `<image>` element with a base64 PNG payload.
- `rg --files public/gamepads/controllercons.2.1/svg/outline` — confirmed there is still no bundled PC Engine outline asset.

### Next Steps

- Prefer a true vector PC Engine outline before implementing mapper support.
- If needed, use the inspected file as tracing/reference material rather than the final shipped art.

## 2026-05-02 - PC Engine controller art evaluation

### Findings
- Candidate raster PNG matches PC Engine standard pad layout (D-pad, I, II, Select, Run — 8 actions, same as NES).
- Image is PNG, not SVG — blocked from use until converted; no PC Engine file exists in the `controllercons.2.1` pack.
- The curved HuCard/cable arch at the top is a hotspot placement concern not present on other supported controllers.
- NES is the closest implementation reference (`NES_SYSTEM_HOTSPOTS` at `controller_mapper.rs:791`).

### Changes
- `docs/llm-wiki/decisions/pc-engine-controller-support.md`: created, documenting layout, blockers, and required code changes.
- `docs/llm-wiki/index.md`: linked new decision page.

### Next Steps
- Convert or source an SVG version of the PC Engine pad art before implementation.

## 2026-05-02 - Housekeeping commits and wiki directory tracking

### Findings
- `assets.rs` and `controller_mapper.rs` had accumulated rustfmt drift — pure formatting, no logic changes.
- `docs/llm-wiki/bugs/`, `decisions/`, and `external/` directories were untracked; each already contained one file.

### Changes
- `527d09f` `style: rustfmt assets.rs`
- `c6d075f` `style: rustfmt controller_mapper.rs`
- `1262ba8` `docs: add initial llm-wiki bug, decision, and external pages`

### Next Steps
- Keep wiki directories tracked as new pages are added.

## 2026-05-02 - Add wiki commit reference page

### Goal

Create a durable wiki page that references the repository commit history with hashes and descriptions so future sessions can quickly cross-reference past work.

### Findings

- The repository history currently contains `83` commits through `61e6c25`.
- The most stable compact representation for the wiki is the chronological git subject line: `short-hash | date | commit subject`.
- The source command for regenerating the page is `git log --reverse --date=short --pretty=format:'%h|%ad|%s'`.

### Changes

- `docs/llm-wiki/commit-reference.md`: added a new commit-reference page containing the full chronological commit list with dates and subjects.
- `docs/llm-wiki/index.md`: linked the new commit-reference page from the Start Here section.

### Tests

- `git rev-list --count HEAD` — reported `83`.
- `git log --reverse --date=short --pretty=format:'%h|%ad|%s'` — used to generate and verify the commit-reference contents.

### Next Steps

- Refresh `docs/llm-wiki/commit-reference.md` whenever new commits land so the wiki remains a faithful reference to repository history.

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

## 2026-05-02 - Game Boy hotspot placement and mapper wiring

### Goal

Add a visual mapper layout for Game Boy using the normalized `gb.svg` asset, wire `GB` into the supported system-layout path, and create a hotspot overlay for the native controls.

### Findings

- The normalized Game Boy art remains a single transformed source path, so hotspot placement had to be calibrated from the original source subpath bounds instead of reading separate SVG primitives.
- The original source-to-bundled transform is `translate(8.5384 -2.6946) scale(0.0440362)`, which was used to carry D-pad, A/B, and Start/Select centers into the `64x64` overlay space.
- Game Boy does not need custom runtime input-binding logic in `input.rs`; the existing generic action mapping already covers `A`, `B`, `Start`, and `Select`.

### Changes

- `crates/arcade-ui/src/controller_mapper.rs`: added `SystemControllerLayout::GameBoy`, `GB` system routing, bundled art/overlay asset paths, fallback hotspot geometry, and test coverage.
- `public/gamepads/hotspots/outline/gb.hotspots.svg`: added the calibrated Game Boy overlay for `Up`, `Down`, `Left`, `Right`, `A`, `B`, `Select`, and `Start`.
- `docs/llm-wiki/decisions/game-boy-controller-art-evaluation.md`: updated the Game Boy controller-art note to reflect that the mapper is now live and documented the hotspot derivation trail.

### Tests

- `cargo fmt --all -- crates/arcade-ui/src/controller_mapper.rs` — passed.
- `cargo test -p arcade-ui supported_systems_resolve_to_system_controller_layouts` — passed.
- `cargo test -p arcade-ui system_native_action_detection_matches_supported_visual_layouts` — passed.
- `cargo test -p arcade-ui overlay_files_cover_required_actions_for_supported_systems` — passed.

### Next Steps

- Do one visual in-app pass on the Game Boy mapper screen to confirm the D-pad cross and the slanted A/B buttons feel centered against `gb.svg`.

## 2026-05-02 - GBA SVG source inspection

### Goal

Inspect a candidate Game Boy Advance SVG and decide whether it is a good source for a future visual mapper asset.

### Findings

- `/Users/jules/Downloads/gameboy_advance_exact_style_centered.svg` is a real vector source with separate primitives for the shell, bezel, D-pad, two small left-side circles, and `A/B` face buttons.
- This structure is much easier to normalize and hotspot than the original monolithic Game Boy SVG source.
- `GBA` already exists elsewhere in the app, but `crates/arcade-ui/src/controller_mapper.rs` still explicitly treats it as unsupported in the mapper tests.

### Changes

- `docs/llm-wiki/decisions/gba-controller-art-evaluation.md`: added the source evaluation and recommendation.
- `docs/llm-wiki/index.md`: linked the new GBA decision page.

### Tests

- `rg -n '"GBA"|GAMEBOY_ADVANCE|GameBoyAdvance|GBA_' crates/arcade-ui/src docs/llm-wiki` — confirmed `GBA` is present in app assets/theme/layout code but not the visual mapper.

### Next Steps

- Normalize the candidate into `public/gamepads/controllercons.2.1/svg/outline/gba.svg` when ready to add GBA mapper support.

## 2026-05-02 - Normalize GBA bundled controller art

### Goal

Convert the inspected Game Boy Advance source SVG into a compact bundled controller asset that matches the repo's outline-art set.

### Findings

- The source was already structured enough to preserve directly: shell contours, bezel, D-pad, two small left-side circles, and `A/B` face buttons.
- A single transformed outline group was the simplest way to keep the original geometry while normalizing it into the repo's `64x64` asset space.
- This step only adds art; GBA is still not enabled in the visual mapper.

### Changes

- `public/gamepads/controllercons.2.1/svg/outline/gba.svg`: added a normalized bundled GBA outline asset using `currentColor`, a `64x64` viewBox, and the original source geometry inside a scaled group.
- `docs/llm-wiki/decisions/gba-controller-art-evaluation.md`: updated the GBA note to reflect that the bundled asset now exists.

### Tests

- No Rust tests were run. This pass only added art/wiki files.

### Next Steps

- Visually inspect `gba.svg`, then add GBA hotspot placement and mapper wiring when ready.

## 2026-05-02 - GBA hotspot placement and mapper wiring

### Goal

Wire `GBA` into the visual mapper and place controller hotspots on the normalized GBA art.

### Findings

- The GBA source exposes clean primitives for the D-pad and `A/B` buttons, and the user clarified that the two buttons under the D-pad should be treated as `Select` and `Start`.
- `L/R` are not explicitly drawn as separate front-facing controls in the source art, but top-edge shoulder hotspots fit the controller silhouette and keep the full original control set on the visual layout.
- `GBA` already used the correct shared action list and generic runtime bindings, so no production input logic changes were needed beyond mapper wiring.

### Changes

- `crates/arcade-ui/src/controller_mapper.rs`: added `SystemControllerLayout::GameBoyAdvance`, `GBA` system routing, bundled art/overlay asset paths, fallback hotspot geometry, and test coverage.
- `public/gamepads/hotspots/outline/gba.hotspots.svg`: added the GBA overlay with `Up`, `Down`, `Left`, `Right`, `A`, `B`, `L`, `R`, `Select`, and `Start`.
- `crates/arcade-ui/src/input.rs`: added a targeted test for `GBA` action-to-retropad bindings.
- `docs/llm-wiki/decisions/gba-controller-art-evaluation.md`: updated the GBA decision note to reflect that mapper support is now live.

### Tests

- `cargo fmt --all -- crates/arcade-ui/src/controller_mapper.rs crates/arcade-ui/src/input.rs` — passed.
- `cargo test -p arcade-ui supported_systems_resolve_to_system_controller_layouts` — passed.
- `cargo test -p arcade-ui system_native_action_detection_matches_supported_visual_layouts` — passed.
- `cargo test -p arcade-ui overlay_files_cover_required_actions_for_supported_systems` — passed.
- `cargo test -p arcade-ui gba_action_bindings_map_to_expected_retropad_ids` — passed.

### Next Steps

- Do one in-app visual pass on the GBA mapper screen and nudge any hotspot that feels off relative to the shell art.

## 2026-05-02 - GBA Start/Select order correction

### Goal

Correct the GBA visual mapper so `Start` appears above `Select`, matching the real hardware.

### Findings

- The first GBA hotspot pass had the two under-D-pad buttons reversed vertically.
- Both the fallback hotspot geometry and the SVG overlay needed the same swap to stay aligned.

### Changes

- `crates/arcade-ui/src/controller_mapper.rs`: swapped the `Start` and `Select` Y positions in `GBA_SYSTEM_HOTSPOTS`.
- `public/gamepads/hotspots/outline/gba.hotspots.svg`: swapped the `Start` and `Select` hotspot positions.

### Tests

- `cargo test -p arcade-ui overlay_files_cover_required_actions_for_supported_systems` — passed.

### Next Steps

- No follow-up needed unless another small visual nudge shows up in the mapper screen.

## 2026-05-02 - Arcade SVG source inspection

### Goal

Inspect a candidate arcade-machine SVG and decide whether it is a good source for the arcade system mapper.

### Findings

- `/Users/jules/Downloads/arcade-machine-game-icon.svg` is a real vector SVG, but it is encoded as one giant monolithic path.
- The art depicts a full upright arcade cabinet rather than a focused control panel.
- That makes it a poor fit for the repo's arcade action model of `Up`, `Down`, `Left`, `Right`, `A`, `B`, `C`, `D`, `Start`, and `Coin`.

### Changes

- `docs/llm-wiki/decisions/arcade-controller-art-evaluation.md`: added the source evaluation and recommendation.
- `docs/llm-wiki/index.md`: linked the new arcade decision page.

### Tests

- `rg -n '"ARCADE"|Arcade|Coin|Start|A|B|C|D' crates/arcade-domain/src/models.rs crates/arcade-ui/src/controller_mapper.rs crates/arcade-ui/src/input.rs` — confirmed the current arcade mapper model expects a control-panel style layout.

### Next Steps

- If we add an arcade visual mapper, prefer a purpose-built control-panel SVG rather than this cabinet illustration.

## 2026-05-02 - Build bundled arcade panel art

### Goal

Create a custom repo-style arcade panel SVG after failing to find a good drop-in source.

### Findings

- The arcade system action model is panel-oriented: `Up`, `Down`, `Left`, `Right`, `A`, `B`, `C`, `D`, `Start`, and `Coin`.
- A purpose-built control-panel drawing is a better fit than a full arcade cabinet because it keeps the bindable controls visually central and easy to hotspot later.

### Changes

- `public/gamepads/controllercons.2.1/svg/outline/arcade.svg`: added a custom bundled arcade control-panel asset with joystick, four-button cluster, and centered `Start/Coin` buttons.
- `docs/llm-wiki/decisions/arcade-controller-art-evaluation.md`: updated the arcade decision note to reflect that a custom bundled asset now exists.

### Tests

- No Rust tests were run. This pass only added art/wiki files.

### Next Steps

- Visually inspect `arcade.svg`, then wire arcade into the visual mapper and add `arcade.hotspots.svg` if the panel layout looks right.

## 2026-05-02 - Arcade panel reference comparison

### Goal

Evaluate a cleaner arcade control-panel reference image against the first custom arcade SVG draft.

### Findings

- `bc442dd5-8085-412d-b62b-209bac86e148.png` is a better arcade mapper target than the first custom draft.
- It matches the arcade action model directly with labeled `A`, `B`, `C`, `D`, `Start`, and `Coin` controls.
- The joystick and button spacing are clearer and would make hotspot placement simpler and more accurate.

### Changes

- `docs/llm-wiki/decisions/arcade-controller-art-evaluation.md`: added a comparison note recommending the newer arcade panel image as the preferred visual target.

### Tests

- No code/tests run. This was an art/reference evaluation.

### Next Steps

- Prefer this newer panel layout if we revise `arcade.svg` or proceed to arcade hotspot wiring.

## 2026-05-02 - Refine arcade panel to labeled reference

### Goal

Update the bundled arcade panel asset so it closely matches the newer labeled reference image.

### Findings

- The later reference is a better mapper target because it makes the joystick assembly, `A/B/C/D` layout, and `Start/Coin` buttons visually explicit.
- Matching that panel more literally should make later hotspot placement simpler and more trustworthy.

### Changes

- `public/gamepads/controllercons.2.1/svg/outline/arcade.svg`: replaced the rougher first draft with a cleaner rounded panel, a closer joystick assembly, a 2x2 labeled button grid, and labeled `Start`/`Coin` buttons to match the newer reference.
- `docs/llm-wiki/decisions/arcade-controller-art-evaluation.md`: updated the arcade decision note to reflect that the current bundled asset now follows the later reference closely.

### Tests

- No Rust tests were run. This pass only changed art/wiki files.

### Next Steps

- Visually inspect the revised `arcade.svg`, then proceed to arcade hotspot placement and mapper wiring if it looks right.

## 2026-05-02 - Arcade hotspot placement and mapper wiring

### Goal

Wire Arcade into the visual mapper using the revised labeled arcade control-panel asset.

### Findings

- The custom arcade panel maps cleanly to the repo's arcade action set: `Up`, `Down`, `Left`, `Right`, `A`, `B`, `C`, `D`, `Start`, and `Coin`.
- The joystick ring is a workable visual anchor for directional hotspots, while the button labels make the action cluster straightforward to map.
- Runtime input bindings already supported the arcade action set generically, so only mapper wiring and test coverage were needed.

### Changes

- `crates/arcade-ui/src/controller_mapper.rs`: added `SystemControllerLayout::Arcade`, `ARCADE` system routing, bundled art/overlay asset paths, fallback hotspot geometry, and test coverage.
- `public/gamepads/hotspots/outline/arcade.hotspots.svg`: added the Arcade overlay for `Up`, `Down`, `Left`, `Right`, `A`, `B`, `C`, `D`, `Start`, and `Coin`.
- `crates/arcade-ui/src/input.rs`: added a targeted test for Arcade action-to-retropad bindings.
- `docs/llm-wiki/decisions/arcade-controller-art-evaluation.md`: updated the arcade decision note to reflect that the visual mapper is now live.

### Tests

- `cargo fmt --all -- crates/arcade-ui/src/controller_mapper.rs crates/arcade-ui/src/input.rs` — passed.
- `cargo test -p arcade-ui supported_systems_resolve_to_system_controller_layouts` — passed.
- `cargo test -p arcade-ui system_native_action_detection_matches_supported_visual_layouts` — passed.
- `cargo test -p arcade-ui overlay_files_cover_required_actions_for_supported_systems` — passed.
- `cargo test -p arcade-ui arcade_action_bindings_map_to_expected_retropad_ids` — passed.

### Next Steps

- Do one in-app visual pass on the Arcade mapper screen and nudge any hotspot if the joystick directions or `Start/Coin` circles feel off.

## 2026-05-02 - Refresh commit reference

### Goal

Bring the persistent commit reference page up to date with the latest git history.

### Findings

- `docs/llm-wiki/commit-reference.md` was stale and stopped at `61e6c25`.
- The latest recorded commit in the repo history is `43abe7d`.

### Changes

- `docs/llm-wiki/commit-reference.md`: appended the missing entries for `1141800`, `527d09f`, `c6d075f`, `1262ba8`, and `43abe7d`.

### Tests

- `git log --reverse --date=short --pretty=format:'%h | %ad | %s' | tail -n 10` — confirmed the current trailing commit history.

### Next Steps

- Refresh `commit-reference.md` again after the next docs or feature commit so it stays aligned with `git log`.

## 2026-05-03 - Clean Git metadata noise

### Goal

Safely clean repository status noise around macOS `.DS_Store` files and the Mupen64Plus gitlink.

### Findings

- `crates/.DS_Store` was tracked even though the root `.gitignore` already ignores `.DS_Store` globally.
- `third_party/mupen64plus-libretro-nx` was recorded as a gitlink at `4da9fcc`, but the repo had no `.gitmodules`, causing `git submodule status` to fail.
- The nested Mupen checkout was already at the recorded gitlink commit and had one untracked `.DS_Store`.

### Changes

- Added `.gitmodules` for `third_party/mupen64plus-libretro-nx` using its existing origin URL.
- Stopped tracking `crates/.DS_Store` with `git rm --cached`, leaving the local file ignored on disk.
- Removed the untracked `.DS_Store` inside the Mupen checkout.

### Tests

- `git submodule status` now succeeds and reports `4da9fcc95d83d309639f4fffa816689f07c8c665`.
- `git -C third_party/mupen64plus-libretro-nx status --short --branch` now shows only `develop...origin/develop [ahead 4]`.

### Next Steps

- Commit the `.gitmodules` addition and `crates/.DS_Store` removal when ready.
