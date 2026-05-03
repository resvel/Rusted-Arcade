---
name: Game Boy Controller Art Evaluation
description: Evaluation of a candidate Game Boy SVG as source material for a future visual mapper layout
type: decision
---

# Game Boy Controller Art Evaluation

## Status

Candidate reviewed, normalized into `public/gamepads/controllercons.2.1/svg/outline/gb.svg`, and now wired into the visual mapper with `SystemControllerLayout::GameBoy` plus `public/gamepads/hotspots/outline/gb.hotspots.svg`.

## Candidate Summary

The inspected file is `nintendo-gameboy-8633.svg`.

What is good:

- It is a real vector SVG, not a raster image wrapped in SVG markup.
- It appears to encode a full original Game Boy style front view, including the screen bezel, D-pad, A/B buttons, Start/Select area, and speaker details.
- Because it is already vectorized, it is a much better starting point than tracing from a bitmap.

What is risky:

- The file is a single monolithic `<path>` with `fill="black"` and `stroke="none"`, not a structured set of shapes or grouped controls.
- The canvas is very large (`1068 x 1578`) and presentation-oriented rather than normalized for the compact bundled controller-art set.
- The single-path approach means individual elements like D-pad, A/B, Start, and Select are harder to isolate or tweak than in the TurboPad-inspired PC Engine source.

## Fit For This Repo

Compared with the bundled `controllercons.2.1` assets, this Game Boy SVG is closer in spirit than the earlier wrapped-raster PC Engine file because it is true vector art. However, it still does not match the repo's current bundled-controller style directly:

- it is a giant filled silhouette/path instead of a compact packed monochrome outline asset
- it does not expose controls as easy-to-edit primitives
- hotspot placement would likely require visual calibration rather than reading obvious source circles/rects

## Activation Notes

The mapper now treats `GB` as a supported visual layout. The current hotspot overlay covers:

- `Up`, `Down`, `Left`, `Right`
- `A`, `B`
- `Select`, `Start`

Hotspot placement was derived by isolating the D-pad, A button, B button, and center-button subpaths from the original monolithic source path, then applying the same transform used in the normalized bundled asset:

- `transform="translate(8.5384 -2.6946) scale(0.0440362)"`

That produced the absolute `64x64` overlay centers used in `gb.hotspots.svg`:

- D-pad: `Up (21.95,39.32)`, `Down (21.95,45.13)`, `Left (19.08,42.22)`, `Right (24.79,42.22)`
- Face buttons: `B (38.65,42.98)`, `A (44.51,40.69)`
- Center buttons: `Select (27.21,50.48)`, `Start (33.03,50.48)`

## Remaining Risk

The bundled art is still a single transformed source path, so any future visual cleanup or coordinate refinement will likely remain a calibration task rather than a simple primitive edit.

## Reference

Current bundled controller assets are compact `viewBox="0 0 64 64"` files such as `public/gamepads/controllercons.2.1/svg/outline/nes.svg`.
