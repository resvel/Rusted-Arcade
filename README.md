# Rusted Arcade

Rusted Arcade is a native desktop frontend for a local retro game library. It
loads libretro cores, manages your library and save states, supports controller
mapping, and gives each supported system a consistent arcade-style launcher.

The current primary target is macOS, with stable runtime lanes for both native
Apple Silicon and whole-app x86_64 under Rosetta. The app can be built and run
from source today, and the macOS package script can create local `.app` bundles
for testing.

## What This App Does

- Presents a native `egui` desktop UI for Favorites, Library, Settings, and
  gameplay.
- Scans local ROM folders into a SQLite library.
- Loads libretro cores dynamically from your runtime core folder.
- Provides video, audio, input, save state, VFS, and core-option integration.
- Includes system-aware controller mapping with visual controller layouts.
- Includes an in-app Dependency Installer under Settings for runtime setup,
  repairs, imports, diagnostics, and per-system core browsing.
- Browses the live libretro buildbot for the active macOS architecture and uses
  libretro `.info` metadata to classify additional downloadable cores.
- Supports separate native ARM64 and Rosetta/x86_64 macOS core folders.
- Builds local macOS app bundles with bundled UI assets and ad-hoc signing.

## What You Must Provide

Rusted Arcade does not include games, BIOS files, or proprietary runtime
packages.

You are responsible for providing:

- ROMs and disc images you legally own.
- BIOS files required by systems such as PS2, Saturn, and PCE-CD.
- Compatibility cores and resource folders that are not available as standard
  libretro buildbot downloads.
- Any third-party runtime resources whose licenses require you to obtain them
  directly from their upstream source.

The Dependency Installer can download standard libretro cores from the upstream
libretro buildbot where configured, and it can import local files or folders
that you select. It never downloads BIOS files or ROMs.

## Runtime Folder

By default, Rusted Arcade uses:

```text
/Library/Application Support/RustedArcade
```

The app creates and uses paths like:

```text
/Library/Application Support/RustedArcade/config.toml
/Library/Application Support/RustedArcade/roms/
/Library/Application Support/RustedArcade/cores/
/Library/Application Support/RustedArcade/cores/metadata/
/Library/Application Support/RustedArcade/cores/x86_64/
/Library/Application Support/RustedArcade/bios/
/Library/Application Support/RustedArcade/data/arcade.db
/Library/Application Support/RustedArcade/data/save-states/
/Library/Application Support/RustedArcade/covers/
```

Native Apple Silicon cores live in `cores/`. Rosetta/x86_64 cores live in
`cores/x86_64/`. Runtime Setup caches buildbot listings and libretro `.info`
metadata under the active core root's `metadata/` folder.

## First Launch

On first launch, the app scans its runtime dependencies. If required pieces are
missing, it opens Settings to the Dependencies view.

Use that view to:

- Install standard libretro cores from the configured libretro buildbot.
- Browse downloadable cores per supported system.
- Open the ALL-page Advanced Buildbot Browser for ambiguous or unclassified
  live buildbot cores.
- Import local compatibility cores, such as PS2 or ARM64 N64 dynarec builds.
- Import required resource folders, such as Dolphin `Sys` or PS2 Metal
  resources.
- Open BIOS target folders and see accepted filenames.
- Re-scan after installing or importing files.

On macOS, imported or downloaded `.dylib` cores are ad-hoc codesigned by the
app after installation.

## Runtime Setup Core Browser

Settings -> Runtime Setup includes a `Cores` section for each supported system.
It combines Rusted Arcade's curated runtime catalog with the live upstream
libretro buildbot listing for the active architecture:

- Native Apple Silicon uses the ARM64 buildbot lane and `cores/`.
- Rosetta/x86_64 uses the x86_64 buildbot lane and `cores/x86_64/`.
- The app caches the buildbot directory listing and `info.zip` metadata under
  `cores/metadata/` so Runtime Setup can still show cached catalog information
  when the network or buildbot is unavailable.

Curated recommended/default cores appear first. Additional remote buildbot cores
can appear in a system's Advanced section when libretro `.info` metadata gives a
reliable single-system match. Ambiguous, unsupported, or unclassified remote
cores stay in the ALL-page Advanced Buildbot Browser with warnings.

Runtime Setup intentionally keeps automatic setup conservative:

- `Get What We Can` installs only standard required/recommended setup items; it
  does not download every browsable buildbot core.
- Curated and protected platform lanes remain authoritative.
- Generic buildbot PS2 cores do not replace the native ARM64 `pcarmsx2` lane or
  the Rosetta PCSX2 Metal PoC lane automatically.
- Hardware-rendered, experimental, ambiguous, and metadata-only candidates are
  treated as Advanced and should be considered unverified until tested.

When installing a catalog core, the app downloads the explicit buildbot archive,
extracts the expected `.dylib`, verifies that the installed file exists and is
non-empty, ad-hoc signs it on macOS, rescans catalog state, and runs best-effort
core-option probing when the helper is available. Probe failure is reported as a
warning, not as an install failure.

## Running From Source

Install a stable Rust toolchain. On macOS, Xcode Command Line Tools are also
recommended.

Clone the repository:

```bash
git clone --recurse-submodules <repo-url>
cd Rusted-Arcade
```

If you already cloned without submodules:

```bash
git submodule update --init
```

Run the native Apple Silicon build:

```bash
cargo run -p arcade-app
```

Or build and run the binary directly:

```bash
cargo build -p arcade-app
target/debug/arcade-app
```

Run with a log file:

```bash
ARCADE_LOG_FILE=run-arm64.log target/debug/arcade-app
```

## Running The Rosetta/x86_64 Lane

On Apple Silicon, the Rosetta build is useful for x86_64-only cores and for the
current Rosetta PS2 path.

```bash
cargo build --target x86_64-apple-darwin -p arcade-app
target/x86_64-apple-darwin/debug/arcade-app
```

With logs:

```bash
ARCADE_LOG_FILE=run-rosetta.log target/x86_64-apple-darwin/debug/arcade-app
```

The app should log `Rosetta: true` and use `/Library/Application Support/RustedArcade/cores/x86_64`
when that folder exists.

## Building macOS App Bundles

Native Apple Silicon bundle:

```bash
scripts/package-macos-app.sh
```

This creates:

```text
dist/RustedArcade.app
```

Rosetta/x86_64 bundle:

```bash
TARGET=x86_64-apple-darwin scripts/package-macos-app.sh
```

This creates:

```text
dist/RustedArcade_Universal_.app
```

The packaging script copies bundled UI assets, writes `Info.plist`, removes
macOS metadata, and ad-hoc signs the bundle. Runtime data remains in
`/Library/Application Support/RustedArcade`.

## Adding Games

Place your games under `/Library/Application Support/RustedArcade/roms/` using the system folders that
the Smart Scan understands:

| System | Folder | Common extensions |
| --- | --- | --- |
| NES | `nes` | `.nes` |
| SNES | `snes` | `.sfc`, `.smc` |
| Genesis / Mega Drive | `genesis` | `.gen`, `.smd`, `.md`, `.bin` |
| Game Boy / Game Boy Color | `gb` | `.gb`, `.gbc`, `.zip` |
| Game Boy Advance | `gba` | `.gba`, `.zip` |
| Nintendo 64 | `n64` | `.n64`, `.z64`, `.v64`, `.zip` |
| Arcade | `arcade` or `arcade-mame2003` | `.zip` |
| PlayStation | `psx` | `.cue`, `.img`, `.iso`, `.pbp`, `.chd` |
| PlayStation 2 | `ps2` | `.iso`, `.chd`, `.gz`, `.cso`, `.bin` |
| Dreamcast | `dreamcast` | `.cdi`, `.gdi`, `.chd` |
| GameCube | `gamecube` | `.iso`, `.gcm`, `.rvz`, `.gcz`, `.wbfs`, `.ciso`, `.tgc` |
| Saturn | `saturn` | `.chd`, `.cue`, `.ccd`, `.toc`, `.m3u` |
| PCE-CD / TurboGrafx | `pcecd` | `.chd`, `.cue`, `.ccd`, `.toc`, `.m3u`, `.pce`, `.sgx` |
| DOS | `dos` | `.zip`, `.exe`, `.com`, `.bat` |

Then open the app and use the Library management tools to run Smart Scan.

## Supported Systems And Core Paths

| System | Native Apple Silicon | Rosetta/x86_64 |
| --- | --- | --- |
| NES | `fceumm` | `fceumm` |
| SNES | `snes9x` | `snes9x` |
| Genesis / Mega Drive | `genesis_plus_gx` | `genesis_plus_gx` |
| Game Boy / Game Boy Color | `gambatte` | `gambatte` |
| Game Boy Advance | `mgba` | `mgba` |
| Nintendo 64 | `mupen64plus_next`, with optional ARM64 dynarec compatibility build | `mupen64plus_next` |
| Arcade | `fbneo`, optional `mame2003` / `mame2003_plus` | `fbneo`, optional `mame2003` / `mame2003_plus` |
| PlayStation | `mednafen_psx_hw` | `mednafen_psx_hw` |
| PlayStation 2 | `pcarmsx2` | `pcsx2_metal_poc_libretro.dylib` through `pcsx2` |
| Dreamcast | `flycast` | `flycast` |
| GameCube | `dolphin` | `dolphin` |
| Saturn | `mednafen_saturn` | `mednafen_saturn` |
| PCE-CD / TurboGrafx | `mednafen_pce_fast` | `mednafen_pce_fast` |
| DOS | `dosbox_pure` | `dosbox_pure` |

Standard libretro cores use this filename form on macOS:

```text
<core_name>_libretro.dylib
```

Compatibility builds are treated separately from stock buildbot cores. The app
does not silently substitute a standard upstream core when a compatibility
build is required.

## BIOS And Resources

BIOS files are user-owned files. Rusted Arcade does not download them.

Important locations:

- PS2 BIOS: `/Library/Application Support/RustedArcade/bios/pcsx2/bios/`
- PCE-CD BIOS: shown in the Dependencies view
- Saturn BIOS: shown in the Dependencies view
- Shared arcade BIOS archives: place with your arcade BIOS/ROM set as guided
  by the Dependencies view
- Dolphin Sys folder: import a valid Dolphin `Sys` folder through the
  Dependencies view
- PS2 Metal/resource folders: import the resources required by the selected
  PS2 compatibility core

## Controls

Default keyboard controls:

- Arrow keys: D-pad
- `Enter`: Start
- `Space`: Select
- `Z`: A
- `X`: B
- Hold `Escape`: leave play mode

Default controller frontend shortcuts:

- Hold `Right Shoulder`: Exit
- Hold `Left Shoulder`: Reset
- `Quick Save`, `Quick Load`, and `Next Save Slot` can be assigned per system
- Recognized PlayStation/Xbox-family controllers auto-fill common shortcut
  controls

Controller mappings can be edited and saved per system in Settings.

## Core Settings

Core Settings exposes per-core options such as N64 CPU mode, PS2 renderer/audio
choices, and PS2 recompiler toggles where supported. The native Apple Silicon
PS2 path uses `pcarmsx2`; Rosetta uses the PCSX2 Metal PoC path by default.
You should not need to launch with long environment-variable commands for
normal PS2 testing.

Installed libretro cores can also publish core options dynamically. Runtime
Setup and Core Settings treat that discovery as settings metadata, not as proof
that a core is compatible with a given system or platform lane.

## macOS Rendering Notes

- The default frontend renderer is Metal-backed `wgpu`.
- `ARCADE_MACOS_RENDERER=glow` forces the older OpenGL frontend path for
  diagnostics.
- Native Apple Silicon uses `pcarmsx2` for PS2 by default.
- Rosetta/x86_64 uses the PCSX2 Metal PoC path by default for PS2.
- `ARCADE_PCSX2_METAL_POC=0` is only for diagnosing the older Rosetta Vulkan
  path.

## Repository Contents

Included in Git:

- Rust workspace source under `crates/`
- macOS packaging script under `scripts/`
- SQLite bootstrap schema under `sql/`
- Bundled UI art under `assets/`
- Example configuration in `config.example.toml`
- Top-level docs and license
- A gitlink/submodule entry for `third_party/mupen64plus-libretro-nx`

Not included in Git:

- ROMs, game archives, or disc images
- BIOS files
- Libretro core binaries
- Local compatibility-core source snapshots
- Local `config.toml`
- Local SQLite databases, save states, and generated cover art
- Build output such as `target/`, `dist/`, or `build/`

## Theme Assets

The app loads bundled UI art from `assets/`. GitHub includes the default header
and background art:

```text
assets/system-logos/all_header.jpg
assets/system-logos/All-background.jpg
```

Per-system header and background art can be added locally:

```text
assets/system-logos/<system>_header.(png|webp|jpg|jpeg)
assets/system-logos/<system>_background.(png|webp|jpg|jpeg)
```

`<system>` is the lowercase system key used by the UI, such as `nes`, `snes`,
`genesis`, `n64`, `arcade`, `psx`, `ps2`, `dreamcast`, or `dos`.

See [UI.md](UI.md) for the full header, background, logo, and controller art
assignment map.

## License

This project is licensed under GPL-3.0-only. See [LICENSE](LICENSE).
