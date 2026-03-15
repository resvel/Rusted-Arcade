# Core Distribution Policy

This project ships app binaries only. Libretro cores are not bundled in release artifacts.

## What you need to do

1. Obtain core binaries that you are legally allowed to use.
2. Place them in your configured `core_root` directory.
3. Use platform-appropriate core files:
   - Linux: `<core_name>_libretro.so`
   - Windows: `<core_name>_libretro.dll`

## Expected core names

- `fceumm`
- `snes9x`
- `genesis_plus_gx`
- `gambatte`
- `mgba`
- `mupen64plus_next`
- `parallel_n64`
- `fbneo`
- `mame2003`
- `mame2003_plus`
