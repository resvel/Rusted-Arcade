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

- `fceumm`
- `snes9x`
- `genesis_plus_gx`
- `gambatte`
- `mgba`
- `parallel_n64`
- `mupen64plus_next`
- `fbneo`
- `mame2003`
- `mame2003_plus`

## N64 platform notes

- macOS uses `mupen64plus_next` as the supported embedded N64 core.
- Linux keeps `parallel_n64` available as the primary high-performance N64 path.

## Arcade notes (native/macOS included)

- Default `ARCADE` core is `fbneo`.
- `mame2003` and `mame2003_plus` remain supported for title-specific compatibility.
- Shared arcade BIOS archives (`neogeo.zip`, `qsound.zip`, `pgm.zip`) are resolved from:
  - `bios_root/arcade-mame2003`
  - `bios_root`
  - `bios_root/roms/arcade-mame2003`
  - `rom_root/arcade-mame2003`
  - `rom_root/roms/arcade-mame2003`
