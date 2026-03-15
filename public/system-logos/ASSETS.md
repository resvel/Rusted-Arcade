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

## Optimized Runtime Variants

Generated web-optimized variants used by the frontend live in `public/system-logos-web/`.

Generate/update them with:

```bash
npm run optimize-system-logos-web
```
