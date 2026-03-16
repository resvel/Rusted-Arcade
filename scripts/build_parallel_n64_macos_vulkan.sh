#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This script only supports macOS." >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CORE_DIR="$ROOT_DIR/third_party/parallel-n64"
OUTPUT_DIR="${1:-$ROOT_DIR/target/debug/cores}"
JOBS="${JOBS:-$(sysctl -n hw.logicalcpu 2>/dev/null || sysctl -n hw.ncpu)}"

if [[ ! -d "$CORE_DIR" ]]; then
  echo "parallel-n64 source not found at $CORE_DIR" >&2
  exit 1
fi

echo "Building parallel_n64 with macOS paraLLEl Vulkan enabled..."
make -C "$CORE_DIR" clean >/dev/null
make -C "$CORE_DIR" -j"$JOBS" platform=osx ALLOW_OSX_PARALLEL=1 HAVE_PARALLEL=1 HAVE_OPENGL=0

install -d "$OUTPUT_DIR"
install -m 0644 "$CORE_DIR/parallel_n64_libretro.dylib" "$OUTPUT_DIR/parallel_n64_libretro.dylib"

echo "Installed: $OUTPUT_DIR/parallel_n64_libretro.dylib"
if ! compgen -G "/opt/homebrew/lib/libvulkan*.dylib" >/dev/null &&
   ! compgen -G "/usr/local/lib/libvulkan*.dylib" >/dev/null &&
   ! compgen -G "/opt/homebrew/lib/libMoltenVK*.dylib" >/dev/null &&
   ! compgen -G "/usr/local/lib/libMoltenVK*.dylib" >/dev/null; then
  echo "Warning: no Vulkan loader/MoltenVK dylib found in /opt/homebrew/lib or /usr/local/lib."
  echo "Install with: brew install vulkan-loader molten-vk vulkan-tools"
fi
echo
echo "Core variable menu snippet (should include 'GFX Plugin; auto|angrylion|parallel'):"
strings "$OUTPUT_DIR/parallel_n64_libretro.dylib" | rg "GFX Plugin" -n -m 2 || true
strings "$OUTPUT_DIR/parallel_n64_libretro.dylib" | rg "parallel-n64-gfxplugin|parallel-rdp-upscaling" -n -m 4 || true
