# UI Asset Assignment

This document describes how the app assigns bundled background and header art across the native UI. The source of truth is the Rust UI code in `crates/arcade-ui/src/assets.rs`, `crates/arcade-ui/src/app/mod.rs`, and `crates/arcade-ui/src/app/top_nav.rs`.

## Asset Roots

At startup, the UI builds a list of existing `public/` asset roots from:

- The current working directory: `./public`
- The parent of the current working directory: `../public`
- The parent of the configured ROM root: `<rom_root_parent>/public`

Relative asset paths such as `/system-logos/all_header.jpg` are resolved against those roots. Absolute paths are used only when the file exists.

## App Sections

| Section | Header behavior | Background behavior |
| --- | --- | --- |
| Favorites / Home | Draws the top navigation header. Header art is based on the active system filter. | Draws the themed central background for the active system filter. |
| Library | Draws the same top navigation header. Header art is based on the active system filter. | Draws the themed central background for the active system filter. |
| Manage Library | Opened inside Library, so it keeps the same top navigation header. | Keeps the same themed Library background behind the manage panels. |
| Settings | Draws the same top navigation header. Header art is still based on the active system filter. | Draws the themed central background for the active system filter behind settings panels. |
| Play | The normal top navigation header is hidden while a game is running. | The central panel is painted black, then the active game frame is drawn full-screen. |

The active system comes from the library system filter. If no system filter is set, the active system is `ALL`.

## Header Assets

The top navigation tries a per-system header first:

```text
public/system-logos/<system>_header.png
public/system-logos/<system>_header.webp
public/system-logos/<system>_header.jpg
public/system-logos/<system>_header.jpeg
```

`<system>` is the lowercase system key, such as `nes`, `snes`, `genesis`, or `gb`. If no matching file exists, the header falls back to:

```text
public/system-logos/all_header.jpg
```

The title image is always:

```text
public/system-logos/headerTitle.png
```

If `headerTitle.png` cannot be loaded, the header renders the text fallback `Rusted Arcade`.

Bundled header default:

| Active system | Header asset |
| --- | --- |
| ALL | `all_header.jpg` |
| Every named system | `all_header.jpg`, unless a local per-system override exists |

Only `all_header.jpg` is bundled for GitHub publishing. Adding a correctly named local per-system header file changes that system without touching code.

## Background Assets

Every non-play section starts by painting the active system palette gradient. The UI then overlays the selected background image with transparency.

For regular systems, the app first checks for these per-system background files:

```text
public/system-logos/<system>_background.png
public/system-logos/<system>_background.webp
public/system-logos/<system>_background.jpg
public/system-logos/<system>_background.jpeg
```

If none exists, the code falls back to `All-background.jpg`.

Bundled background default:

| Active system | Background asset |
| --- | --- |
| ALL | `All-background.jpg` |
| Every named system | `All-background.jpg`, unless a local per-system override exists |

Only `All-background.jpg` is bundled for GitHub publishing.

## Related Mapper Art

The Library and Settings surfaces also use system logo assets in toolbars and SVG controller mapper assets in controller mapping panels. These are not the page background or top header, but they are resolved through the same `public/` asset-root system.

System logo assignments:

| System | Logo asset |
| --- | --- |
| NES | `nintendo.svg` |
| SNES | `snes.svg` |
| GENESIS | `genesis.svg` |
| GB | `Game_Boy_logo.png` |
| GBA | `Game_Boy_Advance_logo.png` |
| N64 | `n64logo.png` |
| ARCADE | `SNK_logo.png` |
| PSX | `Playstation_logo_colour.png` |
| PS2 | `PlayStation_2_logo.png` |
| DREAMCAST | `Dreamcast_logo_Japan.png` |
| SATURN | `SegaSaturn_logo.png` |
| PCECD | `pcecd_logo.png` |
| DOS | `Msdos.png` |

System controller mapper assignments:

| System | Mapper SVG | Hotspot SVG |
| --- | --- |
| NES | `gamepads/controllercons.2.1/svg/outline/nes.svg` | `gamepads/hotspots/outline/nes.hotspots.svg` |
| SNES | `gamepads/controllercons.2.1/svg/outline/snes.svg` | `gamepads/hotspots/outline/snes.hotspots.svg` |
| GENESIS | `gamepads/controllercons.2.1/svg/outline/mega-drive.svg` | `gamepads/hotspots/outline/genesis.hotspots.svg` |
| GB | `gamepads/controllercons.2.1/svg/outline/gb.svg` | `gamepads/hotspots/outline/gb.hotspots.svg` |
| GBA | `gamepads/controllercons.2.1/svg/outline/gba.svg` | `gamepads/hotspots/outline/gba.hotspots.svg` |
| N64 | `gamepads/controllercons.2.1/svg/outline/n64.svg` | `gamepads/hotspots/outline/n64.hotspots.svg` |
| ARCADE | `gamepads/controllercons.2.1/svg/outline/arcade.svg` | `gamepads/hotspots/outline/arcade.hotspots.svg` |
| PSX | `gamepads/controllercons.2.1/svg/outline/ps1.svg` | `gamepads/hotspots/outline/psx.hotspots.svg` |
| PS2 | `gamepads/controllercons.2.1/svg/outline/ps2.svg` | `gamepads/hotspots/outline/ps2.hotspots.svg` |
| DREAMCAST | `gamepads/controllercons.2.1/svg/outline/dreamcast.svg` | `gamepads/hotspots/outline/dreamcast.hotspots.svg` |
| SATURN | `gamepads/controllercons.2.1/svg/outline/sega-saturn.svg` | `gamepads/hotspots/outline/saturn.hotspots.svg` |
| PCECD | `gamepads/controllercons.2.1/svg/outline/pc-engine.svg` | `gamepads/hotspots/outline/pc-engine.hotspots.svg` |

DOS does not currently use the visual system controller mapper.

## Notes For Changing Art

- Put new UI art under `public/system-logos/`.
- Use lowercase system keys in override filenames.
- Prefer replacing or adding the documented asset path over changing Rust code.
- Keep large local-only generated art out of Git unless the app actually needs it at runtime.
