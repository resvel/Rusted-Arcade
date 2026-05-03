---
name: PC Engine Controller Support
description: Evaluation of a candidate raster image for PC Engine controller art and what is required to add PC Engine as a supported system layout
type: decision
---

# PC Engine Controller Support

## Status

Implemented for the visual mapper path. `PCECD` now resolves to `SystemControllerLayout::PcEngine`, uses `public/gamepads/controllercons.2.1/svg/outline/pc-engine.svg`, and has a dedicated overlay at `public/gamepads/hotspots/outline/pc-engine.hotspots.svg`.

## Controller Layout

PC Engine uses a minimal button set — same count as NES:

| Action | Physical control |
| :----- | :--------------- |
| Up / Down / Left / Right | D-pad |
| I | Right face button |
| II | Left face button |
| Select | Left oval center button |
| Run | Right oval center button (Start equivalent) |

No analog sticks, no shoulder buttons (standard pad).

## Candidate Art Evaluation

A clean line-art PNG of the PC Engine standard pad was evaluated. The image shows the correct layout (D-pad in circular housing left, Select/Run ovals center, I/II circles right) and matches the `controllercons.2.1` style.

A later candidate file, `PC-EngineController.svg`, was inspected and turned out to be an Inkscape SVG wrapper containing a single embedded base64 PNG on a large `297 x 210` canvas. That means it is still effectively raster art for mapper purposes, even though the file extension is `.svg`.

Another later candidate, `pc-engine-turbopad-inspired-outline.svg`, is a true vector SVG and a much better structural starting point than the wrapped-raster file. However, it is more of a custom poster-style controller diagram than a `controllercons.2.1` asset: it uses large stroke-based geometry, inline text labels, explicit fills/colors, and a wide `1200 x 620` canvas instead of the compact monochrome bundled style used by the existing controller art.

**Earlier blockers:**

- Format is raster PNG — the mapper requires SVG (assets.rs rasterizes SVG at runtime via `resvg`).
- The inspected `PC-EngineController.svg` does not remove that blocker because it is not true vector outline art.
- No PC Engine file exists in `public/gamepads/controllercons.2.1/svg/outline/` — the image is from a different source.
- The curved arch at the top (HuCard slot / cable protrusion) adds visual mass not present on other supported controllers; hotspot placement needs care around it.
- The true-vector `pc-engine-turbopad-inspired-outline.svg` is usable as source material, but would still need normalization into repo style:
  - remove font-dependent `<text>` labels
  - collapse custom CSS/fills into a monochrome outline treatment
  - normalize proportions/viewBox for parity with the existing `controllercons.2.1` assets

**Path forward:** Continue refining the normalized TurboPad-based local SVG or hotspot placement if visual alignment issues show up in the mapper UI.

## Implemented Changes

The mapper integration now includes:

1. `SystemControllerLayout::PcEngine` — new enum variant
2. `system_controller_layout_for_system()` — supports `PCECD`, `PC_ENGINE`, and `PCENGINE`
3. `system_controller_mapper_art_asset()` — points to the bundled PC Engine SVG
4. `system_controller_hotspot_overlay_asset()` — points to `public/gamepads/hotspots/outline/pc-engine.hotspots.svg`
5. `system_controller_hotspots()` — return a new `PC_ENGINE_SYSTEM_HOTSPOTS` constant (8 entries matching NES count)
6. `PC_ENGINE_SYSTEM_HOTSPOTS` — 8 entries: Up, Down, Left, Right, I, II, Select, Run
7. `public/gamepads/hotspots/outline/pc-engine.hotspots.svg` — new hotspot overlay file

## Reference

NES is the closest analog — same 8-action layout. See `NES_SYSTEM_HOTSPOTS` at `crates/arcade-ui/src/controller_mapper.rs:791`.
