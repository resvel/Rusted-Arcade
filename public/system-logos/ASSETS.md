## System Art Assets

This folder keeps original uploaded controller images and cleaned transparent versions.

### Original source images (kept as-is)
- `nescontroller.jpeg`
- `snescontroller.jpeg`
- `GenesisController.jpeg`

### Normalized source alias
- `genesiscontroller.jpeg` (lowercase alias of `GenesisController.jpeg`)

### Cleaned transparent controller assets (frontend-ready)
- `nescontroller.png`
- `snescontroller.png`
- `genesiscontroller.png`

All cleaned controller assets have transparent backgrounds (alpha channel) and are intended for placement over themed UI backgrounds.

## Runtime Background Assets

The frontend now loads runtime backgrounds directly from `public/system-logos/`.
