#!/usr/bin/env bash
set -euo pipefail

APP_NAME="${APP_NAME:-}"
BUNDLE_IDENTIFIER="${BUNDLE_IDENTIFIER:-}"
PROFILE="${PROFILE:-release}"
TARGET="${TARGET:-}"
CODESIGN="${CODESIGN:-1}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
APP_VERSION="$(
  sed -n 's/^version = "\(.*\)"/\1/p' "${REPO_ROOT}/crates/arcade-app/Cargo.toml" | head -n 1
)"

if [[ -z "${APP_VERSION}" ]]; then
  APP_VERSION="0.1.0"
fi

case "${TARGET}" in
  x86_64) TARGET="x86_64-apple-darwin" ;;
  arm64 | aarch64) TARGET="aarch64-apple-darwin" ;;
esac

EFFECTIVE_TARGET="${TARGET}"
if [[ -z "${EFFECTIVE_TARGET}" ]]; then
  case "$(uname -m)" in
    x86_64) EFFECTIVE_TARGET="x86_64-apple-darwin" ;;
    arm64) EFFECTIVE_TARGET="aarch64-apple-darwin" ;;
  esac
fi

if [[ -z "${APP_NAME}" ]]; then
  case "${EFFECTIVE_TARGET}" in
    x86_64-apple-darwin) APP_NAME="RustedArcade_Universal_" ;;
    *) APP_NAME="RustedArcade" ;;
  esac
fi

if [[ -z "${BUNDLE_IDENTIFIER}" ]]; then
  case "${EFFECTIVE_TARGET}" in
    x86_64-apple-darwin) BUNDLE_IDENTIFIER="com.jules.rusted-arcade.universal" ;;
    *) BUNDLE_IDENTIFIER="com.jules.rusted-arcade" ;;
  esac
fi

if [[ "${PROFILE}" == "release" ]]; then
  BUILD_ARGS=(build --release -p arcade-app)
  HELPER_BUILD_ARGS=(build --release -p arcade-libretro --bin arcade-core-probe)
  PROFILE_DIR="release"
elif [[ "${PROFILE}" == "dev" ]]; then
  BUILD_ARGS=(build -p arcade-app)
  HELPER_BUILD_ARGS=(build -p arcade-libretro --bin arcade-core-probe)
  PROFILE_DIR="debug"
else
  BUILD_ARGS=(build --profile "${PROFILE}" -p arcade-app)
  HELPER_BUILD_ARGS=(build --profile "${PROFILE}" -p arcade-libretro --bin arcade-core-probe)
  PROFILE_DIR="${PROFILE}"
fi

if [[ -n "${TARGET}" ]]; then
  BUILD_ARGS+=(--target "${TARGET}")
  HELPER_BUILD_ARGS+=(--target "${TARGET}")
  BINARY_PATH="${REPO_ROOT}/target/${TARGET}/${PROFILE_DIR}/arcade-app"
  HELPER_BINARY_PATH="${REPO_ROOT}/target/${TARGET}/${PROFILE_DIR}/arcade-core-probe"
else
  BINARY_PATH="${REPO_ROOT}/target/${PROFILE_DIR}/arcade-app"
  HELPER_BINARY_PATH="${REPO_ROOT}/target/${PROFILE_DIR}/arcade-core-probe"
fi

APP_BUNDLE="${REPO_ROOT}/dist/${APP_NAME}.app"
CONTENTS_DIR="${APP_BUNDLE}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"
ICON_FILE="${RESOURCES_DIR}/AppIcon.icns"

echo "Building arcade-app (${PROFILE}${TARGET:+, target ${TARGET}})..."
(cd "${REPO_ROOT}" && cargo "${BUILD_ARGS[@]}")
echo "Building arcade-core-probe helper (${PROFILE}${TARGET:+, target ${TARGET}})..."
(cd "${REPO_ROOT}" && cargo "${HELPER_BUILD_ARGS[@]}")

echo "Creating ${APP_BUNDLE}..."
rm -rf "${APP_BUNDLE}"
mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}"

cp "${BINARY_PATH}" "${MACOS_DIR}/arcade-app"
chmod 755 "${MACOS_DIR}/arcade-app"
cp "${HELPER_BINARY_PATH}" "${MACOS_DIR}/arcade-core-probe"
chmod 755 "${MACOS_DIR}/arcade-core-probe"
cp -R "${REPO_ROOT}/assets" "${RESOURCES_DIR}/assets"

if command -v sips >/dev/null 2>&1; then
  sips -s format icns "${REPO_ROOT}/assets/icon.png" --out "${ICON_FILE}" >/dev/null
else
  echo "warning: sips not found; copying PNG icon instead"
  cp "${REPO_ROOT}/assets/icon.png" "${RESOURCES_DIR}/AppIcon.png"
fi

cat > "${CONTENTS_DIR}/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>${APP_NAME}</string>
  <key>CFBundleExecutable</key>
  <string>arcade-app</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleIdentifier</key>
  <string>${BUNDLE_IDENTIFIER}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${APP_NAME}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${APP_VERSION}</string>
  <key>CFBundleVersion</key>
  <string>${APP_VERSION}</string>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

echo "APPL????" > "${CONTENTS_DIR}/PkgInfo"

find "${APP_BUNDLE}" -name .DS_Store -delete
if command -v xattr >/dev/null 2>&1; then
  xattr -cr "${APP_BUNDLE}" >/dev/null 2>&1 || true
fi

if [[ "${CODESIGN}" != "0" ]] && command -v codesign >/dev/null 2>&1; then
  echo "Ad-hoc signing ${APP_NAME}.app..."
  codesign --force --deep --sign - "${APP_BUNDLE}" >/dev/null
fi

echo "Created ${APP_BUNDLE}"
echo "Runtime folders stay in /Library/Application Support/RustedArcade."
