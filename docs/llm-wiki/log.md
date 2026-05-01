# Session Log

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
