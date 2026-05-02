# Commit Reference

This page is a durable reference for the repository commit history as of `61e6c25` on `2026-05-01`.

- Format: `short-hash | date | commit subject`
- Source command: `git log --reverse --date=short --pretty=format:'%h|%ad|%s'`
- Commit count at capture time: `83`

## History

```text
39933a5 | 2026-03-15 | Initial import: Personal Arcade Native
ed7d330 | 2026-03-15 | Vendor third_party/parallel-n64 instead of embedded git repo
a763a20 | 2026-03-15 | Ignore local CONTEXT.md development file
0ea1ae2 | 2026-03-15 | Update macOS core and arcade BIOS docs
2fa0cea | 2026-03-16 | macOS N64 Vulkan defaults, presets, and context update
c400888 | 2026-03-16 | stop tracking local CONTEXT.md
7d71666 | 2026-03-23 | macos-path: focus runtime on macOS and remove legacy parallel-n64 tree
3c6e25e | 2026-03-24 | Add N64 aspect ratio setting (4:3, 16:9, 16:9 adjusted) to settings UI
3643024 | 2026-03-24 | Fix Vulkan RSP plugin mismatch and enable live core variable updates
c780734 | 2026-03-25 | Sync LibretroHost emulation config on settings save
71f6d96 | 2026-03-26 | Replace hardcoded N64 settings UI with data-driven core settings system
8ba7d65 | 2026-03-26 | Add stylized header title, settings nav buttons, and move input config to Settings
2fad7c7 | 2026-03-26 | chore: commit all pending workspace changes
35b613c | 2026-03-28 | chore: update workspace manifests and arcade-ui manage view
d2edb28 | 2026-03-28 | chore: bump mupen64plus-libretro-nx pointer for macOS dynarec build
4f89bf6 | 2026-03-28 | chore: update mupen64plus-libretro-nx pointer after rebase
27be559 | 2026-04-15 | Fix external Vulkan z-order startup race and present diagnostics
631e003 | 2026-04-15 | Tune macOS N64 compatibility and reduce Vulkan present overhead
4274bbb | 2026-04-15 | Improve N64 2x frame pacing and reduce audio log churn
f500450 | 2026-04-15 | Fix N64 native analog Y axis orientation
464f89e | 2026-04-16 | Fix PS5 D-pad handling and axis polarity on macOS
53e0d13 | 2026-04-17 | Prevent gameplay directions from triggering frontend shortcuts
09c680c | 2026-04-17 | Align input debug with runtime profile and strict dpad ghost filtering
d639f25 | 2026-04-17 | Add explicit N64 stick mappings and wire runtime analog overrides
a40f89b | 2026-04-18 | Add device-based controller mapping tabs with clear/reset flow
f094a7f | 2026-04-18 | Clean repository artifacts and ignore local runtime data
2cffdec | 2026-04-18 | Expand core profiles and update frontend/libretro integration
e6b982a | 2026-04-18 | Add local cover relink workflow and update all-theme backgrounds
27feb7e | 2026-04-19 | Document per-system theme asset override naming and fallback
e24bf36 | 2026-04-19 | Update per-system theme visual assets
5f9a019 | 2026-04-19 | Support per-system header and background theme overrides
a488854 | 2026-04-19 | Improve PS2 play-core compatibility and GL frame source reliability
20be417 | 2026-04-19 | Expose Play core options in PS2 core settings tab
9b261e8 | 2026-04-19 | Improve Play timing: frame-time callback and tighter pacing
c8f0b7a | 2026-04-19 | Stabilize Play GL context handoff and frame warmup
ff96015 | 2026-04-19 | Fix Play frame alpha compositing in UI
c5257eb | 2026-04-19 | chore: format and style cleanup
c074ed2 | 2026-04-19 | Auto-select glow on macOS when Play core is present
f0fd9b9 | 2026-04-20 | Add safe N64 dynarec lane with cached fallback
f9d8c71 | 2026-04-21 | chore(submodule): update mupen64plus dynarec macOS ARM64 fix
c24de57 | 2026-04-21 | feat(n64): wire cpu lane into core settings and arm64 dynarec selection
ca7ae91 | 2026-04-21 | fix(n64): stop forcing dynarec CountPerOp on macOS arm64
71d5a60 | 2026-04-21 | docs(readme): sync macOS renderer and N64 dynarec lane guidance
419f9cc | 2026-04-21 | docs(readme): document UI header/background resolution order
c358dd7 | 2026-04-22 | Update dynarec core: ARM64 trampoline crash fix
b612c21 | 2026-04-22 | vulkan metrics: clear stale fail-fast state after frame recovery
5aa44a2 | 2026-04-23 | Fix Dreamcast/Flycast backend retry and GL frame stability
9349f41 | 2026-04-23 | Fix Flycast exit freeze and restore prior UI view
3cad19b | 2026-04-23 | Tune Flycast audio profile to prevent intro speed-up artifacts
6f51176 | 2026-04-23 | Expose Flycast core options in settings and remove forced runtime overrides
a92af4b | 2026-04-24 | Implement Flycast audio-master pacing and fixed frame-time sync
f7cb747 | 2026-04-24 | Apply audio-master pacing to N64, PSX, and PS2 cores
456f43f | 2026-04-25 | mupen64plus-next: silence verbose jit trace logging
71df790 | 2026-04-25 | audio: reset flow counters on callback video reset
487b487 | 2026-04-25 | input: add retro keyboard passthrough event sync
0db619f | 2026-04-25 | Fix DOS keyboard held-key stutter with polled RetroKeyboard state
3be6774 | 2026-04-25 | Add PCE-CD system integration with BIOS preflight and UI parity
465bdd5 | 2026-04-25 | Add Sega Saturn system integration
21e21d9 | 2026-04-25 | Support PCECD HuCard content and system logos
0b19560 | 2026-04-25 | Update arcade toolbar logo asset path
3f9a432 | 2026-04-25 | Add new system logo assets
e1e3ee4 | 2026-04-27 | Fix PS5 d-pad shortcut crosstalk
f46085d | 2026-04-27 | Format arcade-services lib.rs
5793969 | 2026-04-27 | Remove system-logos-web assets
37e35ff | 2026-04-27 | Remove legacy system logo backgrounds
1c2c8d0 | 2026-04-27 | Update CORES.md to reflect current system and core support
fcdac5e | 2026-04-27 | Update README.md to reflect current system and core support
e523c99 | 2026-04-27 | Add GPLv3 license
f05d050 | 2026-04-27 | Refocus README for macOS-only development
b323568 | 2026-04-27 | Document libretro architecture transparently in README
595a98d | 2026-04-27 | Add CLAUDE.md to .gitignore
206f007 | 2026-04-27 | Remove cover images from git tracking
17104e4 | 2026-04-27 | Optimize image assets for smaller repository footprint
a5db1ea | 2026-04-27 | Update ASSETS.md to document all current system logo assets
a6c6c59 | 2026-04-28 | Add visual controller mapper
fbe4d70 | 2026-04-28 | Update app icon and header title assets
64895e3 | 2026-04-29 | Add SVG hotspot overlays for system mapper
c094dd2 | 2026-04-29 | Tune NES and SNES hotspot overlays
72c4e20 | 2026-04-30 | Add AGENTS.md and LLM wiki knowledge base
c4c2d49 | 2026-04-30 | chore(submodule): bump mupen64plus-libretro-nx to fix register masks and RDRAM bounds
a8b7973 | 2026-05-01 | Refine SVG hotspot overlays for SNES, Genesis, N64, and PSX
cdda147 | 2026-05-01 | Refine controller hotspot overlays
61e6c25 | 2026-05-01 | Add direct press-to-bind controller mapping
```
