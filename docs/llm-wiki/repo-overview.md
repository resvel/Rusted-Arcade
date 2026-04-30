# Repo Overview

This project is a Rust desktop emulator frontend using eframe/egui and libretro cores.

The project currently works well on Linux. The long-term goal is to make as compatible as possible with ARM64 macOS platform.

## Project Structure

The project is organized as a Rust workspace with the following key crates:

- `crates/arcade-app`: The main entry point and application loop.
- `crates/arcade-ui`: The `egui` based user interface.
- `crates/arcade-libretro`: The core engine handling Libretro API integration, video (GL/Vulkan), and audio.
- `crates/arcade-domain`: Shared domain models and configuration types.
- `crates/arcade-data`: Data management and loading logic.
- `crates/arcade-services`: Backend services and utilities.

## Main Goals

- Run multiple libretro emulator cores.
- Support N64 through Mupen64Plus-based cores.
- Provide a native desktop UI using Rust.
- Debug low-level core issues, especially around JIT/dynarec behavior.

## Important Areas

- Rust frontend/app logic.
- Libretro core loading.
- Video/audio/input callbacks.
- Native graphics path.
- Platform-specific rendering integration.
- Third-party emulator cores under `third_party/`.

## Source of Truth

The source code is always authoritative. This wiki is only a compressed memory layer for AI assistants.
