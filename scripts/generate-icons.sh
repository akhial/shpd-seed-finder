#!/usr/bin/env bash
# Regenerates every checked-in raster icon from the master SVGs in
# assets/icon/. Run after editing the masters; the outputs are committed so
# CI never needs an SVG rasterizer.
#
# Requires: rsvg-convert (brew install librsvg), ImageMagick (magick),
# iconutil and tiffutil (macOS).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/assets/icon"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

render() { # render <svg> <size> <out.png>
    rsvg-convert -w "$2" -h "$2" "$1" -o "$3"
}

# --- macOS AppIcon.icns -----------------------------------------------------
ICONSET="$TMP/AppIcon.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
    render "$SRC/seed-seeker-macos.svg" "$size" "$ICONSET/icon_${size}x${size}.png"
    render "$SRC/seed-seeker-macos.svg" $((size * 2)) "$ICONSET/icon_${size}x${size}@2x.png"
done
mkdir -p "$ROOT/macos/SeedSeeker/Resources"
iconutil -c icns "$ICONSET" -o "$ROOT/macos/SeedSeeker/Resources/AppIcon.icns"

# --- Windows SeedSeeker.ico -------------------------------------------------
WINPNGS=()
for size in 16 24 32 48 64 128 256; do
    render "$SRC/seed-seeker-square.svg" "$size" "$TMP/win_$size.png"
    WINPNGS+=("$TMP/win_$size.png")
done
mkdir -p "$ROOT/windows/SeedSeeker/Assets"
magick "${WINPNGS[@]}" "$ROOT/windows/SeedSeeker/Assets/SeedSeeker.ico"

# --- Android legacy launcher mipmaps (API < 26) -----------------------------
RES="$ROOT/android/app/src/main/res"
for entry in mdpi:48 hdpi:72 xhdpi:96 xxhdpi:144 xxxhdpi:192; do
    density="${entry%%:*}"; px="${entry##*:}"
    mkdir -p "$RES/mipmap-$density"
    render "$SRC/seed-seeker-square.svg" "$px" "$RES/mipmap-$density/ic_launcher.png"
    render "$SRC/seed-seeker-round.svg" "$px" "$RES/mipmap-$density/ic_launcher_round.png"
done

# --- DMG background (1x + 2x combined into a retina tiff) -------------------
rsvg-convert -w 660 -h 420 "$SRC/dmg-background.svg" -o "$TMP/dmg_1x.png"
rsvg-convert -w 1320 -h 840 "$SRC/dmg-background.svg" -o "$TMP/dmg_2x.png"
mkdir -p "$ROOT/macos/dmg"
tiffutil -cathidpicheck "$TMP/dmg_1x.png" "$TMP/dmg_2x.png" \
    -out "$ROOT/macos/dmg/background.tiff"

echo "regenerated: macos icns, windows ico, android mipmaps, dmg background"
