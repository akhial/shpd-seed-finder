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

# The engine must be statically linked; a dyld reference here means the app
# only launches on the machine that built it.
if otool -L "$PACKAGE/.build/release/SeedSeeker" | grep -q shpd_seedfinder_ffi; then
    echo "error: SeedSeeker dynamically links the Rust engine" >&2
    exit 1
fi

rm -rf "$APP"
install -d "$APP/Contents/MacOS"
install -m 755 "$PACKAGE/.build/release/SeedSeeker" "$APP/Contents/MacOS/SeedSeeker"
install -m 644 "$PACKAGE/Info.plist" "$APP/Contents/Info.plist"
install -m 644 "$PACKAGE/PkgInfo" "$APP/Contents/PkgInfo"

# Embed Sparkle. SwiftPM links the framework from the resolved binary
# artifact but does not assemble bundles, so it is copied in by hand; the
# executable reaches it through the @executable_path/../Frameworks rpath
# set in Package.swift.
SPARKLE=$(find "$PACKAGE/.build/artifacts" -type d -name Sparkle.framework -path "*macos*" | head -n 1)
if [ -z "$SPARKLE" ]; then
    echo "error: Sparkle.framework not found under $PACKAGE/.build/artifacts" >&2
    exit 1
fi
install -d "$APP/Contents/Frameworks"
ditto "$SPARKLE" "$APP/Contents/Frameworks/Sparkle.framework"

# With MACOS_SIGN_IDENTITY set (a "Developer ID Application" identity),
# sign for notarized distribution; otherwise fall back to ad-hoc signing
# for local development builds. Notarization requires every nested Sparkle
# executable to carry the hardened runtime and a secure timestamp, signed
# inside-out before the framework and the app.
if [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
    FRAMEWORK="$APP/Contents/Frameworks/Sparkle.framework"
    VERSION="$FRAMEWORK/Versions/$(readlink "$FRAMEWORK/Versions/Current")"
    codesign --force --options runtime --timestamp \
        --sign "$MACOS_SIGN_IDENTITY" "$VERSION/XPCServices/Installer.xpc"
    # The downloader ships sandbox entitlements that must survive re-signing.
    codesign --force --options runtime --timestamp --preserve-metadata=entitlements \
        --sign "$MACOS_SIGN_IDENTITY" "$VERSION/XPCServices/Downloader.xpc"
    codesign --force --options runtime --timestamp \
        --sign "$MACOS_SIGN_IDENTITY" "$VERSION/Autoupdate"
    codesign --force --options runtime --timestamp \
        --sign "$MACOS_SIGN_IDENTITY" "$VERSION/Updater.app"
    codesign --force --options runtime --timestamp \
        --sign "$MACOS_SIGN_IDENTITY" "$FRAMEWORK"
    codesign --force --options runtime --timestamp \
        --sign "$MACOS_SIGN_IDENTITY" "$APP"
else
    codesign --force --deep --sign - "$APP"
fi
