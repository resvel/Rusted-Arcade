# Runtime Core Browser / On-Demand Core Downloads Roadmap

Last updated: 2026-06-28

This file is the working roadmap for adding a system-scoped core browser and on-demand core downloads to Rusted Arcade. We will use it as the source of truth for sequencing this feature, assign priorities before implementation, and mark tasks complete as we go.

Status legend:

- [ ] Not started
- [~] In progress
- [x] Completed
- [!] Blocked / needs decision

Priority legend:

- P0: Required foundation / must happen first
- P1: Core user-facing feature
- P2: Quality, resilience, polish
- P3: Later enhancement / optional
- TBD: Priority not assigned yet

## Product Direction

Runtime Setup should become the place where a user can answer: “What do I need for this system, and what cores can I install for it?”

Per-system Runtime Setup tabs should expose a new Cores section that shows the recommended/default core, installed compatible cores, missing compatible cores that can be downloaded, and an advanced browse path for other buildbot cores. Core Settings remains the place to tune installed core options. The selected-game launch/options UI remains the place for one-shot or per-game core choice.

Important guardrails:

- Keep “Get What We Can” conservative: install only known required standard cores and safe defaults.
- Do not automatically download every available core.
- Do not auto-switch a system or game to a newly downloaded core unless the user explicitly chooses that.
- Do not blur standard libretro buildbot cores with Rusted Arcade compatibility cores/resources.
- BIOS, ROMs, and user-owned material remain user-provided only.
- Option probing does not prove runtime compatibility; it only discovers configurable knobs.
- Arcade cores must clearly communicate ROM set compatibility differences.
- PS2 platform-specific lanes (`pcarmsx2` on native ARM64, PCSX2 Metal PoC on Rosetta) stay protected from random buildbot replacement unless explicitly advanced/experimental.
- First implementation should target the active runtime architecture only; alternate-architecture browsing can come later.

## Decisions Already Made For V1

These are locked for the first implementation unless Jules changes direction.

- [x] P0 Use a curated known-core catalog first; defer remote buildbot listing/cache to Phase 5.
- [x] P0 Keep the dependency manifest separate from the core catalog.
- [x] P0 Browse/install only for the active runtime architecture in v1.
- [x] P0 Keep `Get What We Can` conservative; it must not consume the full catalog.
- [x] P0 Treat PS2 buildbot browsing as protected/advanced only; do not replace platform-specific PS2 lanes automatically.
- [x] P1 Probe downloaded cores automatically as a best-effort post-install step; probe failures are warnings, not install failures.
- [x] P1 Hide or disable “Set Default” until Phase 4 default-storage behavior is explicitly implemented.

## Phase 0 — Planning and Scope Lock

Goal: agree on the shape before code changes.

- [x] P0 Define initial supported systems for the first implementation pass.
  - Use all currently supported systems in `SYSTEM_FILTERS` except `ALL`.
  - PS2 appears as a protected/platform lane; generic buildbot PS2 browsing is advanced/experimental only.

- [x] P0 Decide initial catalog strategy.
  - Start with curated known core list only.
  - Remote buildbot listing/cache is Phase 5.

- [x] P1 Decide where system-level default core selection belongs for v1.
  - Runtime Setup may show current/recommended core but should not enable “Set Default” until Phase 4.
  - Core Settings continues to tune per-core variables.
  - Selected-game Launch Options handles per-game override later.

- [x] P1 Decide exact first-pass UI wording.
  - “Recommended core”
  - “Other compatible cores”
  - “Advanced Core Browser” / “Advanced buildbot cores”
  - “Download”, “Repair”, “Probe Settings”
  - “Set Default” remains hidden/disabled until Phase 4.

- [x] P1 Decide whether downloaded cores should be probed immediately after install.
  - Yes, best effort.
  - Failure should not mark download failed if file install/codesign succeeded.

- [x] P0 Decide whether to support both active arch and alternate arch browsing.
  - First pass: active runtime architecture only.
  - Later: allow browsing Rosetta/x86_64 from ARM64 package build context if needed.

## Phase 1 — Domain Model: Core Catalog

Goal: separate downloadable/browsable cores from dependency readiness.

Recommended file: `crates/arcade-domain/src/core_catalog.rs`, exported from `crates/arcade-domain/src/lib.rs`.

- [x] P0 Keep dependency manifest separate from core catalog.
  - Dependency manifest answers “what is needed for readiness?”
  - Core catalog answers “what cores can I browse/download?”
  - Optional/advanced catalog entries must not affect missing-required dependency counts.

- [x] P0 Add a core catalog domain model.
  - `CoreCatalog`
  - `CoreCatalogEntry`
  - `CoreCatalogSource`
  - `CoreCatalogCompatibility`
  - `CoreCatalogInstallState`
  - `CoreCatalogSystemGroup`

- [x] P0 Include fields needed by UI and services.
  - stable catalog id
  - core name
  - display name
  - system(s)
  - buildbot file name / expected dylib member
  - source type and source identity/base URL
  - target path
  - installed state
  - recommended/default marker
  - compatibility label
  - warnings/notes

- [x] P1 Add a curated first-pass catalog.
  - Include existing manifest standard cores:
    - NES `fceumm`
    - SNES `snes9x`
    - GENESIS `genesis_plus_gx`
    - GB `gambatte`
    - GBA `mgba`
    - N64 `mupen64plus_next`
    - ARCADE `fbneo`, optional `mame2003`, optional `mame2003_plus`
    - PSX `mednafen_psx_hw`
    - DREAMCAST `flycast`
    - GAMECUBE `dolphin`
    - SATURN `mednafen_saturn`
    - PCECD `mednafen_pce_fast`
    - DOS `dosbox_pure`
  - Add known alternatives per system only where safe/confident.
  - Mark unknown/experimental entries conservatively.
  - Include protected compatibility lanes as non-buildbot catalog entries where useful:
    - native ARM64 PS2 `pcarmsx2`
    - Rosetta/x86_64 PS2 `pcsx2_metal_poc`
    - native ARM64 N64 dynarec compatibility core

- [x] P1 Add catalog grouping helpers.
  - `system_group(system)`
  - `system_groups()`
  - `entry(id)`
  - Group recommended, compatible/optional, advanced, and protected entries separately.

- [x] P1 Add tests for catalog grouping and install-state detection.
  - Recommended cores are visible for their system.
  - Installed/missing state reflects files on disk.
  - Active architecture target root is respected.
  - Unknown/advanced cores do not become recommended by default.
  - PS2 platform-specific lane remains protected.
  - Catalog creation does not change dependency report missing-required counts.
  - Grouping sort is stable.

## Phase 2 — Service Layer: Browse, Download, Verify, Probe

Goal: expose safe service APIs for Runtime Setup to list and install catalog cores.

- [x] P0 Add service method to load core catalog for the active runtime architecture.
  - Suggested API: `NativeServices::core_catalog() -> Result<CoreCatalogReport>`.
  - Include installed state from current `PathsConfig`.
  - Resolve ARM64 vs x86_64 target core folder using existing `dependency_core_root(&config.paths)` behavior.

- [x] P0 Reuse/refactor existing buildbot installer logic.
  - Extract shared helper from current `install_buildbot_core(file_name, target_path)`.
  - Avoid duplicating zip download/extract/codesign behavior.
  - Make base URL, zip file name, expected archive member, and target path explicit.
  - Keep `install_dependency()` behavior intact by making it call the shared helper.

- [x] P0 Add service method to install a catalog core by catalog id/core id.
  - Suggested API: `NativeServices::install_catalog_core(request, progress)`.
  - Validate source is installable for active architecture.
  - Validate target path is under active core root.
  - Download buildbot zip.
  - Extract expected dylib.
  - Write to target core path.
  - Verify file exists on disk and is non-empty after write.
  - Ad-hoc codesign on macOS.
  - Rescan install state.

- [x] P0 Add install verification.
  - File exists.
  - File is non-empty.
  - Target path is valid.
  - Catalog state reports installed after install.
  - Download/extract/write/sign/installed-state failures fail the install.

- [x] P1 Add progress events for catalog-core downloads.
  - Add a distinct operation kind if needed, e.g. `InstallCatalogCore`.
  - Progress stages:
    - preparing
    - downloading
    - extracting
    - verifying file
    - codesigning
    - rescanning
    - probing settings
    - complete / warning

- [x] P1 Add best-effort post-install probe.
  - Run `arcade-core-probe` when available.
  - Prefer a targeted single-core probe helper over bulk `discover_installed_core_options()`.
  - Persist dynamic profile if variables are found.
  - Treat helper missing, probe failure, or zero variables as warning/no-op, not install failure.

- [x] P1 Persist dynamic profile if variables are found.
  - Use existing `update_discovered_core_profile()`.
  - Populate identity fields from installed file metadata:
    - `source_path`
    - `source_modified_unix`
    - `source_len`

- [x] P2 Add tests for install workflow boundaries.
  - [x] Correct URL construction.
  - [x] Correct extraction target.
  - [x] Nested archive paths still extract by file name.
  - [x] Missing expected dylib in zip is an error.
  - [x] Target parent directory is created.
  - [x] Invalid/unsafe target paths are rejected.
  - [x] Existing symlink targets under the core root are rejected before write.
  - [x] Existing installed core can be repaired/re-downloaded through the install API path.
  - [x] Probe failure does not erase successful install through the install API path.
  - [x] Full catalog installed-state rescan is covered through the install API path.

- [x] P2 Add local/fake zip install test path.
  - Avoid depending only on live network for automated tests.
  - Use temp directories and in-memory zip bytes through injected download/codesign/probe hooks.
  - Verify requested URL, extracted bytes, and target file existence.

## Phase 3 — Runtime Setup UI: Per-System Cores Section

Goal: add the user-facing core browser/download entry point without clutter.

- [x] P0 Add catalog state and refresh path to Manage state/UI.
  - Add `core_catalog_report` or equivalent service-provided catalog state.
  - `Rescan` should refresh dependency report and core catalog/install state.
  - Job completion should refresh catalog state after core installs.

- [x] P0 Add a Cores section to per-system Runtime Setup pages.
  - Place after primary action buttons and before “Rusted Arcade Can”.
  - Per-system pages only.
  - Show the recommended/default core first.

- [x] P0 Keep `ALL` Runtime Setup page conservative.
  - It can summarize missing recommended cores.
  - It should not show the full browse catalog by default.
  - It must not add a “download all available cores” action.

- [x] P0 Ensure controller/keyboard focus navigation includes new buttons.
  - Runtime Setup focus count must account for Cores section actions.
  - Activation order must match visual draw order.
  - Existing Settings system toolbar behavior should remain intact.
  - Use a separate core-browser drawer state; do not reuse dependency `runtime_setup_advanced_open`.

- [x] P1 Render recommended core row.
  - Display name.
  - Core id / filename.
  - Installed/missing/unavailable/warning status.
  - Source label.
  - Compatibility/default label.
  - Download/Repair action.
  - Hide/disable Set Default until Phase 4.

- [x] P1 Wire catalog core install job start/completion.
  - Use existing manage job machinery.
  - Show progress from service events.
  - Refresh dependency report, catalog state, and dynamic core settings after completion.

- [x] P1 Render other compatible cores.
  - Hide if none are known.
  - Show “Download” or “Repair”.
  - Mark optional/alternative clearly.
  - Add arcade ROM-set compatibility warning where relevant.

- [x] P1 Show clear outcome messages.
  - Download succeeded.
  - Repair succeeded.
  - Download succeeded but probe failed.
  - Download failed.
  - Archive missing expected dylib.
  - Installed but validation failed.
  - Codesign failed.
  - Source unavailable for this architecture.

- [x] P2 Add Advanced buildbot drawer.
  - Collapsed by default.
  - Use separate label, e.g. “Show Advanced Core Browser”.
  - Warn that advanced cores may not be tested or compatible.
  - Show unknown/imported entries separately from recommended ones.

- [ ] P2 Add UI/focus helper tests where practical.
  - Focus count includes primary actions, core actions, advanced core drawer, dependency actions.
  - Activation order matches draw order.
  - `ALL` excludes full catalog browse actions.
  - System switch resets core browser focus/drawer state.
  - Advanced dependency inventory and advanced core browser do not share state.

## Phase 4 — Core Defaults and Launch Integration

Goal: connect downloaded cores to actual launch choices without surprising the user.

- [ ] P2 Decide and implement system-level default storage if approved.
  - Avoid overloading per-game overrides.
  - Preserve current default resolver behavior unless user explicitly sets a default.

- [ ] P2 Add “Set as system default” action if approved.
  - Only enabled for installed compatible/recommended cores.
  - Confirm or clearly message when changing defaults.

- [ ] P2 Ensure downloaded compatible cores appear in selected-game launch options.
  - Auto/default remains simple.
  - Compatible installed cores appear first.
  - Advanced/unknown installed cores appear behind an advanced affordance or warning.

- [ ] P2 Ensure per-game override still wins over system default.
  - Auto -> system default/resolver.
  - Per-game saved override -> chosen core.
  - One-shot launch override -> current launch only.

- [ ] P2 Add tests for resolver/default interactions.
  - Current defaults remain unchanged with no user default.
  - System default applies to matching system.
  - Per-game override wins.
  - Invalid/uninstalled default falls back safely.

## Phase 5 — Remote Buildbot Listing and Cache

Goal: live remote buildbot directory browsing is part of this implementation,
not a distant optional enhancement. Runtime Setup should use the active
architecture buildbot listing to decide which curated entries are currently
available, cache the listing for offline fallback, and expose unclassified live
buildbot cores only behind Advanced Core Browser warnings.

- [x] P1 Fetch active-architecture libretro buildbot directory listing.
  - ARM64: `https://buildbot.libretro.com/nightly/apple/osx/arm64/latest/`
  - x86_64: `https://buildbot.libretro.com/nightly/apple/osx/x86_64/latest/`

- [x] P1 Parse available `*_libretro.dylib.zip` entries.
  - Keep parser tolerant of simple HTML directory listing changes.
  - Ignore non-core files.

- [x] P1 Cache buildbot listings.
  - Store source URL, architecture, fetched timestamp, and entries.
  - Use cache when offline or fetch fails.
  - Existing Rescan action refreshes dependency status, catalog state, and live
    buildbot cache.

- [x] P1 Merge remote listing with curated catalog.
  - Curated metadata supplies system classification, display name, recommendation, and warnings.
  - Remote listing supplies availability.
  - Unknown remote entries go to the explicit `ALL` Advanced Buildbot Browser
    only until metadata classification can place them in per-system groups.

- [x] P1 Add tests for listing parse and merge behavior.
  - Known remote entries match curated catalog.
  - Unknown remote entries remain advanced/unknown.
  - Missing remote recommended core reports unavailable rather than installable.
  - Offline cache fallback is implemented in services; direct unit injection for
    downloader failure remains future polish.

## Phase 6 — Metadata-Driven Compatibility Classification

Goal: reduce manual catalog maintenance over time.

- [ ] P3 Investigate libretro `.info` metadata availability for macOS buildbot cores.
  - Determine whether to download `.info`, bundle a snapshot, or use a separate source.

- [x] P3 Add compatibility inference from metadata where reliable.
  - Supported extensions.
  - System/platform tags.
  - Required firmware notes if available.

- [x] P3 Keep curated overrides on top of inferred metadata.
  - Host policy requirements.
  - Known-bad cores.
  - Special resources.
  - Compatibility-core separation.

- [x] P3 Add tests for metadata classification.
  - Metadata can classify simple systems.
  - Curated override can hide or warn on a core.
  - Metadata does not override PS2/compatibility guardrails.

## Phase 7 — Packaging and Distribution

Goal: ensure packaged apps support browsing, downloading, and probing cores.

- [ ] P1 Verify `arcade-core-probe` is included in packaged app bundles.
  - Native ARM64 package.
  - Rosetta/x86_64 package.
  - `PROFILE=dev` package path behavior.

- [ ] P1 Verify downloaded dylibs are ad-hoc signed after install.
  - Codesign success.
  - Launch can load the installed core.

- [ ] P2 Verify Runtime Setup works from `dist/RustedArcade.app`.
  - Core download.
  - Status refresh.
  - Core Settings dynamic profile population.

- [ ] P2 Update packaging docs/wiki if behavior changes.
  - Build and Run.
  - Dependency Installer.
  - macOS Platform Core Matrix if needed.

## Phase 8 — Regression and End-to-End Validation

Goal: prove the feature works at runtime, not just by code inspection.

- [ ] P0 Unit tests.
  - Domain catalog grouping.
  - Install-state detection.
  - Service install flow with mocked/local zip where possible.
  - UI state/focus helper tests where practical.

- [ ] P1 Integration-style local test with a small/local fake buildbot zip.
  - Avoid depending only on live network for automated tests.
  - Verify extraction and file-existence validation.

- [ ] P1 Manual live buildbot smoke test.
  - Pick one safe small core or repair an existing standard core.
  - Verify download URL.
  - Verify dylib exists at target path.
  - Verify codesign.
  - Verify dependency/core catalog status updates.

- [ ] P1 App runtime smoke test.
  - Launch app.
  - Open Settings -> Runtime Setup.
  - Select a system tab.
  - Browse/download a core.
  - Confirm Settings -> Core Settings sees dynamic options if probe succeeds.

- [ ] P2 Launch smoke test.
  - Launch a representative game using the default core path.
  - If system default/launch override work is implemented, launch with downloaded alternative core.

## Phase 9 — Documentation and Wiki Updates

Goal: keep the durable project memory accurate.

- [ ] P1 Update `docs/llm-wiki/architecture/dependency-installer.md`.
  - Explain Runtime Setup Cores section.
  - Explain catalog vs dependency manifest split.
  - Document safe setup behavior.

- [ ] P2 Update `docs/llm-wiki/current-state.md` if architecture/status changed.

- [ ] P2 Update relevant core matrix docs if new supported/recommended cores are added.

- [ ] P1 Append to `docs/llm-wiki/log.md` after implementation work sessions.

## Open Questions

- [x] P0 Should the first implementation include remote buildbot browsing, or should we ship curated-only browse first?
  - Updated answer: live remote buildbot browsing is a goal of this implementation.
    The app now fetches/parses/caches the active-architecture listing, uses it
    for catalog availability, and exposes remote-only cores as Advanced entries.

- [x] P1 Should Runtime Setup support setting a system-level default core in v1?
  - Answer: not in v1. Hide/disable Set Default until Phase 4.

- [x] P0 Should PS2 show any buildbot cores in the normal browse UI, or only under Advanced?
  - Answer: only protected/advanced. Do not present generic buildbot cores as normal PS2 replacements.

- [ ] P1 Should Arcade browse group cores by romset family/expected set format?
  - Recommendation: yes for warnings/labels at minimum; deeper grouping can be P2.

- [x] P1 Should post-install probing run automatically, or should there also be a visible “Probe Settings” button?
  - Answer: run automatically best-effort after install. A visible “Probe Settings” button can be added later if useful.

- [x] P3 Where should buildbot cache data live under `/Library/Application Support/RustedArcade`?
  - Buildbot listing cache lives under the active core root's `metadata/`
    folder, e.g. `/Library/Application Support/RustedArcade/cores/metadata/`.

- [ ] P2 How much remote listing/cache behavior should be tested without network?
  - Deferred until Phase 5; use fake/local listing fixtures.

## First Implementation Work Order

1. Phase 1 P0: create `core_catalog.rs` with model types and keep it separate from dependencies.
2. Phase 1 P0/P1: add curated catalog entries, grouping helpers, install-state detection, and tests.
3. Phase 2 P0: add service method to load catalog for active architecture.
4. Phase 2 P0: refactor buildbot install helper for reuse.
5. Phase 2 P0: add catalog-core install service with file verification and codesign.
6. Phase 2 P1: add best-effort targeted post-install probe and progress events.
7. Phase 3 P0: add catalog state/refresh path and Cores section skeleton to per-system Runtime Setup pages.
8. Phase 3 P1: render recommended core row and wire Download/Repair job flow.
9. Phase 3 P1: render other curated compatible cores and outcome messages.
10. Phase 8 P0/P1: run unit tests, fake-zip install tests, and one live/manual smoke test.
11. Phase 9 P1: update wiki docs/log.
12. Phase 5: add live remote buildbot listing/cache before considering the core browser complete.
13. Phase 6+: add metadata-driven compatibility so remote-only cores can move from unclassified Advanced entries into accurate per-system groups.

## Subagent Planning Notes Integrated

On 2026-06-28, three planning subagents reviewed the roadmap and source. Their key conclusions are now reflected above:

- Phase 1 must create a separate core catalog domain instead of overloading `DependencyComponent`.
- Phase 2 should reuse/refactor existing buildbot zip extraction and add a catalog-specific install path with verification and best-effort targeted probing.
- Phase 3 should insert a Cores section into per-system Runtime Setup pages after primary actions, keep `ALL` conservative, and update focus/navigation in visual order.
