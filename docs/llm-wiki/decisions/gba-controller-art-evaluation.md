---
name: GBA Controller Art Evaluation
description: Evaluation of a candidate Game Boy Advance SVG as source material for a future visual mapper layout
type: decision
---

# GBA Controller Art Evaluation

## Status

Candidate reviewed at `/Users/jules/Downloads/gameboy_advance_exact_style_centered.svg`, normalized into `public/gamepads/controllercons.2.1/svg/outline/gba.svg`, and now wired into the visual mapper with `public/gamepads/hotspots/outline/gba.hotspots.svg`.

## Candidate Summary

What is good:

- It is a true vector SVG, not a raster image wrapped in SVG.
- The control geometry is already broken into useful primitives: shell paths, screen bezel, D-pad path, two small left-side circles, and separate `A/B` button circles.
- The overall proportions already match a classic wide GBA front view, which is much closer to repo-ready controller art than the monolithic original Game Boy source.

What is risky:

- It is presentation-style SVG rather than normalized bundled asset style: large `1412 x 854` canvas, explicit `stroke="#000"`, `stroke-width="12"`, accessibility tags, and comments.
- The current source only exposes the obvious control regions. If we want very exact `Select`/`Start` placement later, the two left-side circles may need verification against the intended reference because the file describes them generically as "small holes / buttons".
- It does not yet match the repo's compact `64x64` `controllercons.2.1` asset format.

## Fit For This Repo

This is a much better source than the Game Boy monolithic path because it already separates the controls we would need for hotspot placement:

- D-pad can be mapped directly from the plus-shaped path bounds.
- `A` and `B` already exist as explicit circles.
- The screen/bezel and shell shapes are clean enough to normalize into a compact bundled outline asset.

That means it is a good candidate both for:

- building `public/gamepads/controllercons.2.1/svg/outline/gba.svg`
- later deriving `public/gamepads/hotspots/outline/gba.hotspots.svg`

## Result

The normalized bundled asset now exists at:

- `public/gamepads/controllercons.2.1/svg/outline/gba.svg`

The conversion kept the source shell, bezel, D-pad, two small left-side circles, and `A/B` button circles, but stripped the presentation wrapper:

- removed `title`/`desc`
- removed comments
- switched to `viewBox="0 0 64 64"`
- switched to `currentColor`
- packed the original coordinates into a single transformed group

The visual mapper now treats `GBA` as a supported layout and uses:

- D-pad arms from the plus-shaped control
- `A/B` from the two explicit right-side button circles
- `Select/Start` from the two smaller buttons below the D-pad
- `L/R` as top-edge shoulder hotspots

The current overlay covers:

- `Up`, `Down`, `Left`, `Right`
- `A`, `B`
- `L`, `R`
- `Select`, `Start`

## Recommendation

Use the current `gba.svg` plus `gba.hotspots.svg` as the base for any future refinement. The most likely follow-up is only visual calibration in-app if any hotspot needs nudging.
