#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PACKAGE="$ROOT/macos/SeedSeeker"
APP="$ROOT/dist/Seed Seeker.app"

bash "$ROOT/scripts/build-macos-native.sh"

cd "$PACKAGE"
CLANG_MODULE_CACHE_PATH="${CLANG_MODULE_CACHE_PATH:-$ROOT/target/swift-clang-cache}" \
SWIFTPM_MODULECACHE_OVERRIDE="${SWIFTPM_MODULECACHE_OVERRIDE:-$ROOT/target/swift-module-cache}" \
swift build -c release --disable-sandbox

rm -rf "$APP"
install -d "$APP/Contents/MacOS"
install -m 755 "$PACKAGE/.build/release/SeedSeeker" "$APP/Contents/MacOS/SeedSeeker"
install -m 644 "$PACKAGE/Info.plist" "$APP/Contents/Info.plist"
install -m 644 "$PACKAGE/PkgInfo" "$APP/Contents/PkgInfo"
codesign --force --deep --sign - "$APP"
