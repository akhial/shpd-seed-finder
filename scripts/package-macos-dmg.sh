#!/usr/bin/env bash
# Packages dist/Seed Seeker.app (built by build-macos-app.sh) into a
# drag-to-install dmg. Usage: package-macos-dmg.sh [output.dmg]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/dist/Seed Seeker.app"
DMG="${1:-$ROOT/dist/SeedSeeker.dmg}"

if [ ! -d "$APP" ]; then
    echo "error: $APP not found; run scripts/build-macos-app.sh first" >&2
    exit 1
fi

STAGING="$(mktemp -d)"
trap 'rm -rf "$STAGING"' EXIT
# ditto preserves the code signature and notarization metadata that cp -R
# can mangle.
ditto "$APP" "$STAGING/Seed Seeker.app"
ln -s /Applications "$STAGING/Applications"

rm -f "$DMG"
hdiutil create -volname "Seed Seeker" -srcfolder "$STAGING" -format UDZO "$DMG"

# A signed dmg lets Gatekeeper attribute the download to the same developer
# as the app; skipped for ad-hoc local builds.
if [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
    codesign --force --timestamp --sign "$MACOS_SIGN_IDENTITY" "$DMG"
fi
