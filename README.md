# Rusted Arcade

Rusted Arcade is a Rust desktop libretro frontend for a local game library. It is built around `eframe`/`egui`, SQLite, native audio/input, and dynamically loaded libretro cores.

The current development target is macOS, especially Apple Silicon. Release readiness and runtime validation are focused on macOS.

## Repository Status

This repository is intended to publish source code and bundled UI assets only.

Included:

- Rust workspace source under `crates/`
- SQLite bootstrap schema under `sql/`
- Bundled UI art under `assets/`
- Example configuration in `config.example.toml`
- Top-level project docs and license
- A gitlink/submodule entry for `third_party/mupen64plus-libretro-nx`

Not included:

- ROMs or game archives
- BIOS files
- Libretro core binaries
- Local `config.toml`
- Local agent instructions and LLM wiki notes
- Local SQLite databases, save states, and generated cover art
- Build output such as `target/`, `dist/`, or `build/`

The `.gitignore` keeps those local/runtime paths out of Git. Release artifacts should ship app binaries separately from libretro cores.

## Features

- Native `egui` library shell with Home, Library, Settings, and Play views
- SQLite-backed ROM library, favorites, save states, and controller mappings
- Dynamic libretro core loading from a configured `core_root`
- Video, audio, input, environment variables, VFS, and save-state integration for libretro cores
- System-aware controller mapping with visual mapper assets and hotspot overlays
- Cover scraping and local cover relinking workflows
- macOS renderer selection between Metal-backed `wgpu` and OpenGL `glow`
- Apple Silicon N64 path with cached interpreter and experimental dynarec lanes

## Workspace

- `arcade-app`: desktop entry point and app wiring
- `arcade-ui`: native `egui` UI, rendering, views, assets, and input handling
- `arcade-libretro`: libretro host, callbacks, video/audio, VFS, and core lifecycle
- `arcade-domain`: configuration, system/core policy, platform helpers, and shared models
- `arcade-data`: SQLite schema/bootstrap and repositories
- `arcade-services`: library, launch, save-state, cover, and controller mapping services

## Requirements

- Stable Rust toolchain
- macOS for the primary supported runtime path
- Libretro core binaries you are legally allowed to use
- ROM and BIOS assets you legally own
- For Vulkan hardware-rendered cores on macOS, a working Vulkan/MoltenVK installation may be required depending on the core

## Quick Start

Clone with submodules:

```bash
git clone --recurse-submodules <repo-url>
cd native
```

Or initialize submodules after cloning:

```bash
git submodule update --init
```

Copy and edit the example config:

```bash
cp config.example.toml config.toml
```

Set these paths in `config.toml`:

- `paths.rom_root`
- `paths.db_path`
- `paths.save_state_root`
- `paths.core_root`
- `paths.bios_root`

Run the app:

```bash
cargo run -p arcade-app
```

Run with an explicit config path:

```bash
cargo run -p arcade-app -- /path/to/config.toml
```

Build a release binary:

```bash
cargo build -p arcade-app --release
```

The release executable is written to `target/release/arcade-app`.

## Runtime Layout

A typical local layout is:

```text
config.toml
roms/
cores/
bios/
data/arcade.db
data/save-states/
covers/
```

Those directories are local runtime content and should not be committed.

## Libretro Cores

This project ships app binaries only. Libretro cores are not bundled in source or release artifacts.

Core binaries should be placed in the configured `core_root` directory. On macOS, core files use the libretro suffix form:

```text
<core_name>_libretro.dylib
```

See [CORES.md](CORES.md) for core distribution policy and platform-specific notes.

## Supported Systems

| System | Default core |
| --- | --- |
| NES | `fceumm` |
| SNES | `snes9x` |
| Genesis / Mega Drive | `genesis_plus_gx` |
| Game Boy | `gambatte` |
| Game Boy Advance | `mgba` |
| Nintendo 64 | `mupen64plus_next` |
| Arcade | `fbneo` |
| PlayStation | `mednafen_psx_hw` |
| PlayStation 2 | `play` on Apple Silicon macOS |
| Dreamcast | `flycast` |
| GameCube | `dolphin` |
| Sega Saturn | `mednafen_saturn` |
| PC Engine / TurboGrafx-16 | `mednafen_pce_fast` |
| DOS | `dosbox_pure` |

Arcade also supports `mame2003` and `mame2003_plus` for title-specific compatibility.

## macOS Notes

- Default frontend rendering uses `wgpu`.
- Set `ARCADE_MACOS_RENDERER=glow` to force the OpenGL frontend path.
- If `ARCADE_MACOS_RENDERER` is unset and `play_libretro.dylib` is detected in `core_root`, the app auto-selects `glow` for Play! compatibility.
- The N64 `Stable Cached` lane uses `mupen64plus_next_libretro.dylib`.
- The N64 `Experimental Dynarec` lane prefers `mupen64plus_next_dynarec_arm64_libretro.dylib` on Apple Silicon.
- If the dynarec lane fails to launch, the host retries once with the cached lane.

## Arcade Setup

- Default `ARCADE` core: `fbneo`
- Place arcade ROM archives under `<rom_root>/arcade-mame2003/`
- Shared arcade BIOS archives are resolved from:
  - `<bios_root>/arcade-mame2003`
  - `<bios_root>`
  - `<bios_root>/roms/arcade-mame2003`
  - `<rom_root>/arcade-mame2003`
  - `<rom_root>/roms/arcade-mame2003`
- Common shared BIOS archives include `neogeo.zip`, `qsound.zip`, and `pgm.zip`

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
- Recognized PlayStation/Xbox-family controllers auto-fill common shortcut controls

## Theme Assets

The app loads bundled UI art from `assets/`. GitHub includes only the default header and background art: `assets/system-logos/all_header.jpg` and `assets/system-logos/All-background.jpg`. Per-system header and background art can be added locally by placing files in `assets/system-logos/`.

See [UI.md](UI.md) for the full header, background, logo, and controller art assignment map.

Header override pattern:

```text
<system>_header.(png|webp|jpg|jpeg)
```

Background override pattern:

```text
<system>_background.(png|webp|jpg|jpeg)
```

`<system>` is the lowercase system key used by the UI, such as `nes`, `snes`, `genesis`, `n64`, `arcade`, `psx`, `ps2`, `dreamcast`, or `dos`. If no override exists, the app uses the default bundled art.

## Development Checks

Useful validation commands:

```bash
cargo fmt --all
cargo check --workspace
cargo test -p arcade-domain
cargo test -p arcade-data
cargo test -p arcade-services
cargo test -p arcade-ui
cargo test -p arcade-libretro
cargo check -p arcade-app
```

Before publishing or cutting a release, also check:

```bash
git status --short --ignored
git submodule status
```

Confirm that ignored local content such as `roms/`, `bios/`, `cores/`, `data/`, `covers/`, and `target/` is not staged.

## Release Gates

Still required before final public release sign-off:

- Broader macOS dynarec stability validation across more N64 titles
- Longer gameplay testing across newly added systems
- Final clean-machine smoke test on target macOS versions
- Packaging, signing, and notarization decisions for macOS distribution

## License

This project is licensed under GPL-3.0-only. See [LICENSE](LICENSE).
