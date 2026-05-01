# SVG Hotspot Overlays

This page records durable lessons from aligning system-control hotspot overlays to the bundled `controllercons` SVG art in `public/gamepads/controllercons.2.1/svg/outline/`.

## Saturn Notes

- The Saturn face buttons are not a flat 3x2 grid. They are arranged in two slanted rows:
  - top row: `X (42.53,29.84)`, `Y (47.74,26.64)`, `Z (53.38,25.14)`
  - bottom row: `A (44.11,37.57)`, `B (50.35,33.85)`, `C (57.43,31.89)`
- The top row uses the smaller `r≈2.43` circles. The bottom row uses larger `r≈3.22` circles with inner cutouts around `r≈2.47`.
- The Saturn Start button is the rounded rectangle centered near `(31.90,32.87)`. The old overlay at `y≈42.7` was placed on the lower shell, not the button.
- The Saturn D-pad arm centers come from the cross subpaths, not the old guessed grid:
  - `Up (13.89,26.27)`
  - `Down (13.90,34.98)`
  - `Left (9.57,30.65)`
  - `Right (18.24,30.46)`

## General Rules

- Do not use the raw `M` coordinate of a circle path as the center. In these SVGs, `M` is often the arc start point.
- When the art is a single large path, split on `Z` / `z` and analyze subpaths independently.
- Arc-derived centers plus visual verification against the real controller art are more reliable than naive subpath bounding boxes alone.
