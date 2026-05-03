---
name: Arcade Controller Art Evaluation
description: Evaluation of a candidate arcade-machine SVG as source material for the arcade system mapper
type: decision
---

# Arcade Controller Art Evaluation

## Status

The cabinet candidate at `/Users/jules/Downloads/arcade-machine-game-icon.svg` was rejected as a direct source. A custom bundled control-panel asset now exists at `public/gamepads/controllercons.2.1/svg/outline/arcade.svg`, and Arcade is now wired into the visual mapper with `public/gamepads/hotspots/outline/arcade.hotspots.svg`.

## Candidate Summary

What is good:

- It is a true vector SVG.
- It appears to contain a complete arcade-cabinet illustration with visible controls.
- The button clusters are visually rich and might be useful as inspiration or tracing material.

What is risky:

- The entire drawing is a single monolithic `<path>`, so individual control regions are not exposed as editable primitives.
- It depicts a full upright arcade cabinet, not a focused control panel layout.
- The control layout shown in the art does not line up cleanly with this repo's arcade action model of `Up`, `Down`, `Left`, `Right`, `A`, `B`, `C`, `D`, `Start`, and `Coin`.

## Fit For This Repo

For the current mapper, this is a poor match:

- The repo's arcade system expects a control panel style mapping surface, not a cabinet-front illustration.
- Because the SVG is a single path, hotspot extraction would be manual and brittle.
- Even if normalized into a bundled asset, the cabinet body would add a lot of visual noise around the actual bindable controls.

## Recommendation

Do not use this SVG directly as the arcade mapper asset.

Better options:

- find or build a clean arcade control-panel SVG
- create a purpose-built repo-style `arcade.svg` that shows only stick, buttons, `Start`, and `Coin`
- use this cabinet SVG only as loose visual reference if we want an arcade-inspired style

## Result

The repo now has a purpose-built arcade panel asset:

- `public/gamepads/controllercons.2.1/svg/outline/arcade.svg`

It is intentionally mapper-first rather than cabinet-illustration-first, and now follows the later labeled panel reference closely. It shows:

- a left joystick area
- a four-button `A/B/C/D` cluster
- centered `Start` and `Coin` buttons

This should be a much better base for future arcade hotspot placement than the monolithic cabinet SVG.

The current visual mapper covers:

- `Up`, `Down`, `Left`, `Right`
- `A`, `B`, `C`, `D`
- `Start`, `Coin`

## Follow-up Comparison

A later reference image (`bc442dd5-8085-412d-b62b-209bac86e148.png`) is an even stronger fit for the arcade mapper than the first custom draft because it:

- uses the exact action labels `A`, `B`, `C`, `D`, `Start`, and `Coin`
- presents a cleaner control-panel silhouette
- uses a clearer joystick assembly that is easier to hotspot directionally
- visually separates the action cluster from the utility buttons better than the first draft

If we refine the arcade asset further, this image should be the preferred visual target.
