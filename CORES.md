# Core Distribution Policy

This project ships app binaries only. Libretro cores are not bundled in release artifacts.

## What you need to do

1. Obtain core binaries that you are legally allowed to use.
2. Place them in your configured `core_root` directory.
3. Use macOS libretro dynamic library files:
   - `<core_name>_libretro.dylib`

## macOS core filename behavior

- On macOS, native core resolution expects the libretro suffix form only:
  - Example: `fbneo_libretro.dylib`
- This app does not use a secondary fallback filename like `<core_name>.dylib`.
- Compatibility filename fallbacks are currently recognized for:
  - `mednafen_pce_fast`: also tries `beetle_pce_fast_libretro.dylib`
  - `mednafen_saturn`: also tries `beetle_saturn_libretro.dylib`
- On Apple Silicon macOS, selecting the N64 dynarec lane makes `mupen64plus_next` prefer `mupen64plus_next_dynarec_arm64_libretro.dylib` before falling back to `mupen64plus_next_libretro.dylib`.

## Expected core names

### 8-bit systems
- `fceumm` (NES)
- `gambatte` (Game Boy)
- `mednafen_pce_fast` (PCE / TurboGrafx-16)

### 16-bit systems
- `snes9x` (SNES)
- `genesis_plus_gx` (Genesis / Mega Drive)
- `mednafen_saturn` (Sega Saturn)

### 32-bit and later
- `mupen64plus_next` (N64)
- `mgba` (Game Boy Advance)
- `mednafen_psx_hw` (PlayStation 1 with hardware rendering)
- `pcsx2` (PlayStation 2)
- `play` (PlayStation 2 - alternative core)
- `flycast` (Dreamcast)
- `dolphin` (GameCube)

### Arcade
- `fbneo` (FBNeo — default arcade core)
- `mame2003` (MAME 2003)
- `mame2003_plus` (MAME 2003 Plus)

### Other
- `dosbox_pure` (DOS)

## Platform-specific core notes

### N64
- N64 uses `mupen64plus_next` with support for both cached interpreter and Apple Silicon dynarec CPU lanes.
- The dynarec lane expects `mupen64plus_next_dynarec_arm64_libretro.dylib` on Apple Silicon macOS and falls back to `mupen64plus_next_libretro.dylib` if needed.

### PlayStation 2
- Two cores available: `pcsx2` and `play`.
- The default core is `play` on Apple Silicon macOS.

### GameCube
- GameCube uses the Libretro Dolphin core.
- Place `dolphin_libretro.dylib` in `core_root`.
- Place Dolphin's `Data/Sys` folder at `bios_root/dolphin-emu/Sys`; the app validates that `Sys/GC` and either `Sys/GameSettings` or `Sys/Resources` are present.
- Optional GameCube IPL BIOS files are not required by the app's launch validation.

### Arcade
- Default `ARCADE` core is `fbneo`.
- `mame2003` and `mame2003_plus` remain supported for title-specific compatibility.
- `mame2003_plus` has a core settings profile; plain `mame2003` is supported as a launch core but does not currently expose a separate settings profile.
- Shared arcade BIOS archives (`neogeo.zip`, `qsound.zip`, `pgm.zip`) are resolved from:
  - `bios_root/arcade-mame2003`
  - `bios_root`
  - `bios_root/roms/arcade-mame2003`
  - `rom_root/arcade-mame2003`
  - `rom_root/roms/arcade-mame2003`

### PSX with hardware rendering
- `mednafen_psx_hw` provides GPU-accelerated rendering for PlayStation 1 games.
