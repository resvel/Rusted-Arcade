# Core Distribution Policy

This project ships app binaries only. Libretro cores are not bundled in release artifacts.

## What you need to do

1. Obtain core binaries that you are legally allowed to use.
2. Place them in your configured `core_root` directory.
3. Use platform-appropriate core files:
   - Linux: `<core_name>_libretro.so`
   - Windows: `<core_name>_libretro.dll`
   - macOS: `<core_name>_libretro.dylib`

## macOS core filename behavior

- On macOS, native core resolution expects the libretro suffix form only:
  - Example: `fbneo_libretro.dylib`
- Unlike Windows, macOS does not use a secondary fallback filename like `<core_name>.dylib`.

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

### Arcade
- `fbneo` (FBNeo — default arcade core)
- `mame2003_plus` (MAME 2003 Plus)

### Other
- `dosbox_pure` (DOS)

## Platform-specific core notes

### N64
- macOS uses `mupen64plus_next` as the embedded N64 core with support for both cached interpreter and dynarec CPU lanes.

### PlayStation 2
- Two cores available: `pcsx2` and `play`.

### Arcade
- Default `ARCADE` core is `fbneo`.
- `mame2003_plus` remains supported for title-specific compatibility.
- Shared arcade BIOS archives (`neogeo.zip`, `qsound.zip`, `pgm.zip`) are resolved from:
  - `bios_root/arcade-mame2003`
  - `bios_root`
  - `bios_root/roms/arcade-mame2003`
  - `rom_root/arcade-mame2003`
  - `rom_root/roms/arcade-mame2003`

### PSX with hardware rendering
- `mednafen_psx_hw` provides GPU-accelerated rendering for PlayStation 1 games.
