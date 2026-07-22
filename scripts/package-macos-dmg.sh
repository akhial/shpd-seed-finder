#!/usr/bin/env bash
# Packages dist/Seed Seeker.app (built by build-macos-app.sh) into a
# drag-to-install dmg with a designed background, 128px icons, and the app
# icon as the volume icon. Usage: package-macos-dmg.sh [output.dmg]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/dist/Seed Seeker.app"
DMG="${1:-$ROOT/dist/SeedSeeker.dmg}"

if [ ! -d "$APP" ]; then
    echo "error: $APP not found; run scripts/build-macos-app.sh first" >&2
    exit 1
fi

# dmgbuild (pure-python) lays out the Finder window deterministically —
# window geometry, icon positions, retina background, volume icon — which
# plain hdiutil cannot do. Installed into a cached venv so neither dev
# machines nor CI need it preinstalled. It copies the app with ditto, so
# code signatures and notarization metadata survive.
if command -v dmgbuild >/dev/null; then
    DMGBUILD=dmgbuild
else
    VENV="$ROOT/target/dmgbuild-venv"
    if [ ! -x "$VENV/bin/dmgbuild" ]; then
        python3 -m venv "$VENV"
        "$VENV/bin/pip" install --quiet dmgbuild
    fi
    DMGBUILD="$VENV/bin/dmgbuild"
fi

rm -f "$DMG"
"$DMGBUILD" -s "$ROOT/macos/dmg/settings.py" \
    -D app="$APP" \
    -D background="$ROOT/macos/dmg/background.tiff" \
    -D icon="$ROOT/macos/SeedSeeker/Resources/AppIcon.icns" \
    "Seed Seeker" "$DMG"

# A signed dmg lets Gatekeeper attribute the download to the same developer
# as the app; skipped for ad-hoc local builds.
if [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
    codesign --force --timestamp --sign "$MACOS_SIGN_IDENTITY" "$DMG"
fi
