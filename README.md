# Personal Arcade Native

macOS native desktop app for the Personal Arcade project, built as a Rust workspace around `egui`, SQLite, and libretro. This is a macOS-only implementation focused on Apple Silicon (M1/M2/M3) performance.

## Libretro Backend

Personal Arcade Native is a **libretro frontend**. Libretro is an open API standard that allows emulation cores to be developed independently and loaded at runtime. This architecture provides several benefits:

- **Modular design**: Cores are loaded dynamically as `.dylib` plugins, keeping the app binary small
- **Standardized interface**: All cores implement the same libretro API for video, audio, input, and state management
- **Wide emulation support**: Access to 15+ emulation cores covering 11 different gaming systems
- **User-provided cores**: Users can update or add cores without rebuilding the application

The `arcade-libretro` crate implements the libretro host, managing:
- Dynamic core loading and lifecycle
- Video output and frame rendering into the UI
- Audio callback routing via `cpal`
- Input event translation to controller callbacks
- Save state serialization/unserialize through libretro's API
- Environment variable handling for core configuration

**Core Distribution Policy:** This project ships app binaries only. Libretro cores must be obtained separately from official libretro repositories and placed in the configured `core_root` directory. See [CORES.md](CORES.md) for the complete list of supported cores and setup instructions.

## Workspace

- `arcade-app`: desktop entry point and feature wiring
- `arcade-ui`: native `egui` shell, views, rendering, and input handling
- `arcade-libretro`: dynamic libretro core host (implements libretro API integration)
- `arcade-domain`: config, models, core resolution, and arcade compatibility policy
- `arcade-data`: SQLite bootstrap, migrations, and repositories
- `arcade-services`: app-facing use cases for library, favorites, save states, and controller mappings

## Architecture Snapshot

### Runtime model

- App starts by loading or creating `config.toml`.
- Default config resolution prefers `config.toml` next to the executable.
- Config controls ROM, core, BIOS, DB, and save-state roots plus N64 defaults.
- SQLite is opened and bootstrapped from the native schema.
- UI runs through `eframe` with Metal rendering via `wgpu` or optional OpenGL rendering via `glow`.
- The libretro host loads the selected core dynamically and runs the frame loop.
- Save states are supported through libretro serialize/unserialize with native size limits.

### UI flows

- Top-level views are `Home`, `Library`, and `Settings`.
- `Library -> Manage` handles:
  - smart scan/import
  - DB-only ROM removal
  - cover scraping
  - managed inventory operations
- `Settings` owns editable app configuration and TheGamesDB settings.

### Input handling

- Native macOS gamepad input via IOKit
- Controller mappings support frontend actions:
  - `Exit`
  - `Reset`
  - `Quick Save`
  - `Quick Load`
  - `Next Save Slot`

## Current behavior

- Creates or loads `config.toml` next to the executable by default
- Creates required directories for DB, save states, cores, and BIOS assets
- Opens SQLite, bootstraps schema from `sql/bootstrap.sql`, and creates a one-time backup of an existing DB on first native open
- Uses a single implicit local profile for favorites, save states, and gamepad mappings
- Shows three top-level app views:
  - `Home`: favorites shelf
  - `Library`: searchable/filterable ROM grid
  - `Settings`: app configuration and TheGamesDB settings
- Includes a `Library -> Manage` flow for:
  - smart ROM scans
  - DB-only ROM removal
  - cover scraping runs
  - managed ROM inventory review
- Launches a separate `Play` session view when a ROM starts
- Includes a compact `Library` sidebar editor for system-specific controller mappings backed by `GamepadMapping`, including configurable in-play frontend shortcuts
- Loads libretro cores dynamically from the `core_root` directory as `.dylib` plugins
- Implements the full libretro API contract: video callbacks, audio output, input handling, environment variables, and VFS support
- Runs ROMs through the libretro host and renders emulation output into the UI in real-time
- Supports libretro save-state serialize/unserialize through the API with slots `0..=9`
- Enforces native save-state limits:
  - per-slot max: `5 MiB`
  - per-profile max: `25 MiB`
- Supports keyboard controls and native macOS gamepad input
- Starts native audio output via `cpal` when built with the audio feature

## Theme Asset Overrides

You can override themed header/background images per system by dropping files in `public/system-logos/`.

- Header override pattern: `<system>_header.(png|webp|jpg|jpeg)`
- Background override pattern: `<system>_background.(png|webp|jpg|jpeg)`
- `<system>` is the lowercase system key used by the UI (examples: `nes`, `snes`, `genesis`, `n64`, `arcade`, `psx`, `ps2`, `dreamcast`, `dos`, `all`)
- Example file: `public/system-logos/nes_header.png`
- Example file: `public/system-logos/nes_background.webp`

Header selection order:

- Try `public/system-logos/<system>_header.png`
- Then `public/system-logos/<system>_header.webp`
- Then `public/system-logos/<system>_header.jpg`
- Then `public/system-logos/<system>_header.jpeg`
- Fallback: `public/system-logos/headerbackground.png`

Background selection order:

- For normal system views (`NES`, `SNES`, `N64`, etc.):
  - Try `public/system-logos/<system>_background.png`
  - Then `.webp`, `.jpg`, `.jpeg`
  - Fallback to the built-in per-system background asset path in code
- For `ALL` view:
  - Use `public/system-logos/All-background.png`
  - `all_background.*` is not used for the `ALL` canvas path

Rendering behavior:

- Header and system backgrounds are drawn as a single full-bleed image and scaled to cover their target area.
- The `ALL` background is also rendered as a single fitted image (not tiled by default).

## Prerequisites

- Stable Rust toolchain
- macOS (Apple Silicon M1/M2/M3 or Intel with Rosetta 2)
- libretro cores in `core_root`:
  - `<core_name>_libretro.dylib`
- Optional Apple Silicon N64 dynarec lane binary:
  - `mupen64plus_next_dynarec_arm64_libretro.dylib` (native performance, no Rosetta 2)
- BIOS/ROM assets you legally own

## Run

Default build:

```bash
cd native
cargo run -p arcade-app
```

Default packaged layout:

- `./config.toml`
- `./roms`
- `./data/arcade.db`
- `./data/save-states`
- `./cores`
- `./bios`

Optional config path:

```bash
cargo run -p arcade-app -- /path/to/config.toml
```

Example:

```bash
cargo run -p arcade-app -- ~/Library/Application\ Support/personal-arcade-native/config.toml
```

## In-app configuration

- The `Settings` tab can edit and save these `config.toml` values at runtime:
  - `paths.rom_root`
  - `paths.db_path`
  - `paths.save_state_root`
  - `paths.core_root`
  - `paths.bios_root`
  - `emulation.n64.preferred_core`
  - `emulation.n64.parallel_rdp_upscaling`
  - `management.cover_scraping.tgdb_api_key`
  - `management.cover_scraping.platform_ids.*`
  - `management.cover_scraping.default_limit`
  - `management.cover_scraping.default_delay_ms`
- `Library -> Manage -> Core Settings` edits per-core libretro variables (including N64 options such as `Count Per Op` and `Count Per Op Denom Pot`).
- `Library -> Manage -> Core Settings -> N64` includes a CPU lane selector:
  - `Stable Cached` (`cached_interpreter`)
  - `Experimental Dynarec` (`dynamic_recompiler`)
- Changing DB, ROM, core, BIOS, or save-state paths is saved immediately, but those path changes are applied on the next app launch.

## macOS arcade quick setup

- Default `ARCADE` core is `fbneo`.
- Place arcade ROM archives under `<rom_root>/arcade-mame2003/` (for example, `mslugx.zip`).
- Shared arcade BIOS archives are discovered from these locations:
  - `<bios_root>/arcade-mame2003`
  - `<bios_root>`
  - `<bios_root>/roms/arcade-mame2003`
  - `<rom_root>/arcade-mame2003`
  - `<rom_root>/roms/arcade-mame2003`
- Common shared BIOS archives: `neogeo.zip`, `qsound.zip`, `pgm.zip`.
- For Neo Geo titles on FBNeo, make sure `neogeo.zip` is present in one of the paths above.

## Default controls

- Arrow keys: D-pad
- `Enter`: Start
- `Space`: Select
- `Z`: A
- `X`: B
- Hold `Escape`: leave immersive play mode

## Default controller frontend shortcuts

- Hold `Right Shoulder`: `Exit`
- Hold `Left Shoulder`: `Reset`
- `Quick Save`, `Quick Load`, and `Next Save Slot` can be assigned per system in the `Library` controller mapping panel
- Recognized PlayStation/Xbox-family controllers auto-fill:
  - `L3`: `Quick Save`
  - `R3`: `Quick Load`
  - `Guide / PS`: `Next Save Slot`

## Controller Mapping Precedence

- Exact device override for the active system
- System default for that system
- Built-in native fallback mapping

Native mappings store canonical controls (`South`, `LeftStickX +`, etc.) instead of raw platform button numbers, so saved mappings remain stable across more controllers.
Older saved mappings that do not include newer frontend actions are backfilled with the current built-in defaults when loaded, without overriding existing custom bindings.

## Supported core names

Core binaries use the `.dylib` extension on macOS.

See [CORES.md](CORES.md) for the authoritative list of supported cores and system-specific notes.

Current supported cores:
- `fceumm_libretro` (NES)
- `gambatte_libretro` (Game Boy)
- `mednafen_pce_fast_libretro` (PCE / TurboGrafx-16)
- `snes9x_libretro` (SNES)
- `genesis_plus_gx_libretro` (Genesis / Mega Drive)
- `mednafen_saturn_libretro` (Sega Saturn)
- `mupen64plus_next_libretro` (N64)
- `mgba_libretro` (Game Boy Advance)
- `mednafen_psx_hw_libretro` (PlayStation 1 with hardware rendering)
- `pcsx2_libretro` (PlayStation 2)
- `play_libretro` (PlayStation 2 - alternative core)
- `flycast_libretro` (Dreamcast)
- `fbneo_libretro` (Arcade — FBNeo)
- `mame2003_plus_libretro` (Arcade — MAME 2003 Plus)
- `dosbox_pure_libretro` (DOS)

## Important notes

- Existing preview metadata is surfaced in the UI, but native preview capture/generation is still deferred.
- The native app does not implement account/admin flows; it uses one implicit local profile.
- Hardware-render cores are still limited in the embedded host.
- macOS renderer behavior:
  - Default is `wgpu` on Metal.
  - Set `ARCADE_MACOS_RENDERER=glow` to force OpenGL frontend rendering.
  - If `ARCADE_MACOS_RENDERER` is unset and `play_libretro.dylib` is detected in `core_root`, the app auto-selects `glow` for compatibility.
- Arcade launches are validated before start:
  - CPS3 titles are blocked until the required assets are installed.
  - Shared arcade BIOS files are resolved from `bios_root` and `rom_root` arcade directories (see macOS arcade quick setup above).
  - Default `ARCADE` core is `fbneo`.
  - Native arcade launches mirror the web app's title-specific arcade overrides before falling back to the ROM's configured core override or the default arcade core.
  - `mame2003` and `mame2003_plus` are treated as distinct native cores.
- On Linux, when the external Vulkan N64 window is active, the main app window is moved offscreen instead of minimized so frame pacing stays stable.

## Implementation Status

### Implemented

- Workspace/crate layout and app entrypoint
- Config loader with auto-create behavior and executable-local defaults
- SQLite bootstrap and data layer
- Domain/system/core mapping and launch planning
- Native `egui` UI shell and play flow
- Native `Library -> Manage` workflow
- Dynamic libretro host with serialize/unserialize support
- Linux packaging helper scripts

### Current N64 runtime path

The current development focus is macOS Apple Silicon. N64 support across platforms:

- **macOS (Apple Silicon)**:
  - Active N64 core: `mupen64plus_next`
  - Two CPU lanes available:
    - `Stable Cached`: loads `mupen64plus_next_libretro.dylib`
    - `Experimental Dynarec`: prefers `mupen64plus_next_dynarec_arm64_libretro.dylib`
  - If dynarec lane is selected and launch fails, native host retries once with cached lane
  - Graphics path: ParaLLEl (Vulkan in core)
  - `Count Per Op` default is `Auto (0)`; no forced override is applied by the frontend

### Apple Silicon dynarec core bring-up notes

The `mupen64plus_next_dynarec_arm64_libretro.dylib` lane is backed by our `third_party/mupen64plus-libretro-nx` core work for native arm64 macOS:

- JIT cache uses an Apple-compliant `MAP_JIT` mapping path in `new_dynarec`.
- Writes/exec transitions use per-thread W^X toggling via `pthread_jit_write_protect_np()`.
- Cache maintenance follows Apple ordering (`sys_icache_invalidate` with execute-mode transition).
- Dynarec ARM64 patch/flush sites were updated to respect that model during runtime code generation and link patching.

These changes are what enabled native M1/M2 dynarec boot and gameplay in current testing.

### What is in good shape

- Core selection and config wiring
- Executable-local config and asset path layout for packaged builds
- Manage view and runtime config persistence for cover scraping
- Native write-side ROM management flow
- macOS `mupen64plus_next` ParaLLEl/Vulkan path (cached + dynarec lane support)
- Configurable ParaLLEl upscale
- Support for multiple systems: NES, SNES, Genesis, N64, Game Boy, GBA, Arcade, PSX, PS2, Dreamcast, Saturn, PCE, DOS

### Remaining work

- Broader compatibility and long-session stability validation for macOS dynarec lane across more titles
- Runtime validation of `4x` and `8x` upscale modes
- Additional UX polish for system-specific features across newly supported platforms
- Broader end-to-end validation of native cover scraping against real downloads
- Cross-platform parity testing for newly added systems (Saturn, PCE, Dreamcast, PS2, PSX, DOS)

## N64 GoodName Renamer

Use the helper below to rename N64 ROM files from the local `mupen64plus.ini` catalog without launching each game.

Dry run first:

```bash
python3 scripts/n64_goodname_renamer.py --canonical
```

Apply the file renames and update the native SQLite library at the same time:

```bash
python3 scripts/n64_goodname_renamer.py --canonical --apply
```

## Building and Distribution

Build a release binary:

```bash
cargo build -p arcade-app --release
```

The executable is located at `target/release/arcade-app`.

## Validation

Recent validation in this environment has repeatedly passed:

```bash
cargo fmt --all
cargo test -p arcade-domain
cargo test -p arcade-data
cargo test -p arcade-services
cargo test -p arcade-ui
cargo test -p arcade-libretro
cargo check -p arcade-app
```

## Release Gates

Still required before final release sign-off:

1. macOS dynarec stability validation across broader game library.
2. Extended gameplay testing for newly added systems.
3. Longer stability pass across emulation scenarios.
4. Final clean-machine smoke test on target macOS versions.
