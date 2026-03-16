# Personal Arcade Native

Native desktop app for the Personal Arcade project, built as a Rust workspace around `egui`, SQLite, and libretro.

## Workspace

- `arcade-app`: desktop entry point and feature wiring
- `arcade-ui`: native `egui` shell, views, rendering, and input handling
- `arcade-libretro`: dynamic libretro core host
- `arcade-domain`: config, models, core resolution, and arcade compatibility policy
- `arcade-data`: SQLite bootstrap, migrations, and repositories
- `arcade-services`: app-facing use cases for library, favorites, save states, and controller mappings

## Architecture Snapshot

### Runtime model

- App starts by loading or creating `config.toml`.
- Default config resolution prefers `config.toml` next to the executable.
- Config controls ROM, core, BIOS, DB, and save-state roots plus N64 defaults.
- SQLite is opened and bootstrapped from the native schema.
- UI runs through `eframe` and `glow`.
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

### Input backends

- Linux uses `gilrs`.
- Windows uses SDL `GameController`.
- Controller mappings can include frontend actions:
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
- Loads libretro cores dynamically and wires video, audio, input, environment, and VFS callbacks
- Runs ROMs through the embedded libretro host and renders frames into the UI
- Supports libretro save-state serialize/unserialize with slots `0..=9`
- Enforces native save-state limits:
  - per-slot max: `5 MiB`
  - per-profile max: `25 MiB`
- Supports keyboard controls and platform-specific gamepad backends by default:
  - Linux: `gilrs`
  - Windows: SDL `GameController`
- Starts native audio output via `cpal` when built with the audio feature
- Includes packaging helper scripts for Linux/Windows ZIP artifact builds

## Prerequisites

- Stable Rust toolchain
- Linux, Windows, or macOS desktop environment with graphics drivers
- libretro cores in `core_root`:
  - Linux: `<core_name>_libretro.so`
  - Windows: `<core_name>_libretro.dll`
  - macOS: `<core_name>_libretro.dylib`
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
cargo run -p arcade-app -- ~/.config/personal-arcade-native/config.toml
```

Windows PowerShell example:

```powershell
cargo run -p arcade-app -- C:\path\to\config.toml
```

macOS example:

```bash
cargo run -p arcade-app -- /Users/you/path/to/config.toml
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

## Native Gamepad + Audio

Install Linux development packages first:

```bash
sudo apt update
sudo apt install -y libudev-dev pkg-config
```

Default run includes native gamepad support:

```bash
cargo run -p arcade-app
```

Windows builds now use bundled SDL for controller input. Install CMake in addition to Rust + Visual Studio C++ build tools before building on Windows.

Release ZIP scripts build with `--features native-av`, so shipped Linux/Windows artifacts include both gamepad and audio support.

If you also want native audio output, install ALSA headers and enable the audio feature:

```bash
sudo apt install -y libasound2-dev
cargo run -p arcade-app --features native-av
```

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

Core binary extension is platform-specific (`.so` on Linux, `.dll` on Windows, `.dylib` on macOS).

- `fceumm_libretro`
- `snes9x_libretro`
- `genesis_plus_gx_libretro`
- `gambatte_libretro`
- `mgba_libretro`
- `mupen64plus_next_libretro`
- `parallel_n64_libretro`
- `fbneo_libretro`
- `mame2003_libretro`
- `mame2003_plus_libretro`

## Important notes

- Existing preview metadata is surfaced in the UI, but native preview capture/generation is still deferred.
- The native app does not implement account/admin flows; it uses one implicit local profile.
- Hardware-render cores are still limited in the embedded host.
- macOS is currently aimed at the safe boot path first:
  - Default renderer is `glow` (OpenGL) on macOS.
  - OpenGL-backed frontend integration and software frame delivery are the intended first working modes.
  - Vulkan/MoltenVK in the embedded host is experimental and requires Vulkan-capable core binaries.
  - `wgpu`/Metal is opt-in experimental only: set `ARCADE_MACOS_RENDERER=wgpu` (or `metal`) to run eframe through `wgpu` on macOS (`WGPU_BACKEND=metal` is auto-set if absent).
  - `parallel_n64` defaults to the Vulkan backend on macOS.
  - This Vulkan experiment requires a working Vulkan loader + MoltenVK installation on macOS.
  - Recommended install path: `brew install vulkan-loader molten-vk vulkan-tools` (loader path can be overridden with `ARCADE_VULKAN_LOADER=/absolute/path/to/libMoltenVK.dylib`).
  - Set `ARCADE_MACOS_EXPERIMENTAL_VULKAN=0` to force `parallel_n64` back to the OpenGL path.
  - macOS `parallel_n64` Vulkan defaults are tuned for headroom: `gfxplugin-accuracy=high` at `1x` upscaling, `medium` above `1x`, and ParaLLEl VI extras disabled (`vi-aa`, `vi-bilinear`, `dither-filter`, `divot-filter`, `gamma-dither`).
  - Settings now include `Parallel Preset` for N64 `parallel_n64`:
    - `balanced` (default): current headroom defaults.
    - `performance`: more aggressive quality reduction (`gfxplugin-accuracy=medium` at `1x`, `low` above `1x`) while keeping ParaLLEl VI extras disabled.
  - Fallback sync behavior: per-frame Vulkan idle waits are disabled by default for better performance/pacing in fallback readback mode.
  - Override fallback sync with `ARCADE_VULKAN_FORCE_FALLBACK_IDLE=1` (enable conservative waits) or `=0` (force disable).
  - Build and install a Vulkan-capable `parallel_n64` core with `./scripts/build_parallel_n64_macos_vulkan.sh`.
  - Current Apple Silicon caveat: this build path disables dynarec/NEON asm in `parallel_n64` to keep the paraLLEl Vulkan experiment linkable.
  - Current caveat: hardware-render cores in the host still rely on OpenGL frontend integration, so this mode can force software fallback for those cores.
- `parallel_n64` is the current exception:
  - Linux/X11 uses the high-performance external Vulkan presentation window path.
  - Windows defaults to the working software fallback with `parallel-n64-gfxplugin=angrylion`.
  - Setting `ARCADE_WINDOWS_EXTERNAL_VULKAN_PRESENT=1` switches Windows back to the experimental `parallel` Vulkan path with ParaLLEl-RDP upscaling.
  - `LIBRETRO_PARALLEL_N64_GL_FALLBACK=1` forces the software fallback explicitly.
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

- `parallel_n64` launches successfully in the native app.
- libretro Vulkan negotiation is implemented far enough to run the core.
- On Linux/X11 the host uses:
  - core-owned Vulkan device creation
  - external X11 Vulkan presentation window
  - direct external GPU presentation
- On Windows the host currently defaults to:
  - the software CPU-frame path with `parallel-n64-gfxplugin=angrylion`
  - no external Win32 Vulkan present window unless `ARCADE_WINDOWS_EXTERNAL_VULKAN_PRESENT=1` is set
  - an experimental opt-in `parallel` Vulkan path with ParaLLEl-RDP upscaling support

### What is in good shape

- Core selection and config wiring
- Executable-local config and asset path layout for packaged builds
- Manage view and runtime config persistence for cover scraping
- Native write-side ROM management flow
- `parallel_n64` Vulkan bring-up
- Linux external Vulkan presentation window
- Linux direct external presentation instead of UI texture readback
- Configurable ParaLLEl upscale

### Remaining work

- Windows `parallel_n64` Vulkan parity and stability in the opt-in external-present path
- Final gameplay-speed polish for `parallel_n64`
- Adaptive 60/30 presentation policy for heavier Linux scenes
- Runtime validation of `4x` and `8x` upscale modes
- Additional UX polish for the external Vulkan window lifecycle if needed
- Broader end-to-end validation of native cover scraping against real downloads

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

## Packaging helpers

- `native/scripts/build-linux-zip.sh`
- `native/scripts/build-windows-zip.ps1`
- `native/scripts/build-linux-tar.sh`
- `native/scripts/build-appimage.sh`
- `native/scripts/build-deb.sh`

ZIP artifacts are built per-OS and include:

- app executable
- `config.example.toml`
- `README-native.md`
- `CORES.md`
- `public/` theme and image assets
- empty `cores/` and `bios/` directories

Each ZIP has a matching `.sha256` checksum file.

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

Windows-target Rust compile has not been revalidated in this Linux environment for the current Vulkan work. Real Windows runtime QA is still required for final sign-off.

## Release Gates

Still required before final release sign-off:

1. Real Windows runtime QA on a clean machine.
2. `parallel_n64` Vulkan parity QA on Windows.
3. Longer stability pass for the required matrix.
4. Final clean-machine smoke for both Linux and Windows ZIP artifacts.
