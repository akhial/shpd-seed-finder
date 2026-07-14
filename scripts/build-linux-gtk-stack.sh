#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

# Build the GTK/libadwaita versions used by Seed Seeker against the release
# workflow's older glibc. This keeps the AppImage useful on more distributions
# than a package assembled from current Fedora libraries would be.

set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

gtk_version=4.22.4
gtk_sha256=51bd9f60c7d23a665a556c7364c21fb2e4e282566b3e7e092455e8f910330893
libadwaita_version=1.9.2
libadwaita_sha256=6920f813a76c4856591ca56ee842e94efbbe736e8ca2f445c9e9fc3b4e7076f0

prefix=${APPIMAGE_GTK_PREFIX:-"$repo_root/target/appimage-gtk"}
source_cache=${APPIMAGE_SOURCE_CACHE:-"$repo_root/target/appimage-sources"}
build_root=${APPIMAGE_GTK_BUILD_ROOT:-"$repo_root/target/appimage-gtk-build"}
marker="$prefix/.seed-seeker-gtk-stack"
expected_marker="gtk=$gtk_version libadwaita=$libadwaita_version"

for command in curl git meson ninja pkg-config sha256sum tar; do
    if ! command -v "$command" >/dev/null; then
        echo "Required command not found: $command" >&2
        exit 1
    fi
done

if [ -f "$marker" ] && [ "$(<"$marker")" = "$expected_marker" ]; then
    echo "Using cached GTK stack in $prefix"
    exit 0
fi

download_source() {
    local url=$1
    local checksum=$2
    local destination=$3

    if [ -f "$destination" ] && printf '%s  %s\n' "$checksum" "$destination" | sha256sum --check --status; then
        return
    fi

    rm -f "$destination" "$destination.tmp"
    curl --fail --location --retry 3 --retry-all-errors --connect-timeout 30 \
        --output "$destination.tmp" "$url"
    printf '%s  %s\n' "$checksum" "$destination.tmp" | sha256sum --check
    mv "$destination.tmp" "$destination"
}

mkdir -p "$source_cache"
gtk_archive="$source_cache/gtk-$gtk_version.tar.xz"
libadwaita_archive="$source_cache/libadwaita-$libadwaita_version.tar.xz"

download_source \
    "https://download.gnome.org/sources/gtk/4.22/gtk-$gtk_version.tar.xz" \
    "$gtk_sha256" \
    "$gtk_archive"
download_source \
    "https://download.gnome.org/sources/libadwaita/1.9/libadwaita-$libadwaita_version.tar.xz" \
    "$libadwaita_sha256" \
    "$libadwaita_archive"

rm -rf "$prefix" "$build_root"
mkdir -p "$prefix" "$build_root/gtk-source" "$build_root/libadwaita-source"
tar -xJf "$gtk_archive" -C "$build_root/gtk-source" --strip-components=1
tar -xJf "$libadwaita_archive" -C "$build_root/libadwaita-source" --strip-components=1

# GTK's release tarball points its Pango fallback at the moving main branch.
# Use the first stable Pango series satisfying GTK 4.22's requirement instead.
sed -i 's/^revision = main$/revision = 1.56.4/' \
    "$build_root/gtk-source/subprojects/pango.wrap"

meson setup "$build_root/gtk-build" "$build_root/gtk-source" \
    --prefix "$prefix" \
    --libdir lib \
    --buildtype release \
    --force-fallback-for=glib,pango,cairo,harfbuzz,wayland,wayland-protocols \
    -Dmedia-gstreamer=disabled \
    -Dprint-cups=disabled \
    -Dvulkan=disabled \
    -Dintrospection=disabled \
    -Dglib:sysprof=disabled \
    -Dglib:introspection=disabled \
    -Dglib:tests=false \
    -Ddocumentation=false \
    -Dman-pages=false \
    -Dbuild-demos=false \
    -Dbuild-examples=false \
    -Dbuild-tests=false \
    -Dbuild-testsuite=false
meson compile -C "$build_root/gtk-build"
meson install -C "$build_root/gtk-build"

export PKG_CONFIG_PATH="$prefix/lib/pkgconfig:$prefix/share/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export LD_LIBRARY_PATH="$prefix/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

meson setup "$build_root/libadwaita-build" "$build_root/libadwaita-source" \
    --prefix "$prefix" \
    --libdir lib \
    --buildtype release \
    -Dintrospection=disabled \
    -Dvapi=false \
    -Ddocumentation=false \
    -Dtests=false \
    -Dexamples=false
meson compile -C "$build_root/libadwaita-build"
meson install -C "$build_root/libadwaita-build"

license_root="$prefix/share/licenses"
for project in gtk glib pango cairo harfbuzz wayland wayland-protocols; do
    project_source="$build_root/gtk-source"
    if [ "$project" != gtk ]; then
        project_source="$build_root/gtk-source/subprojects/$project"
    fi
    mkdir -p "$license_root/$project"
    find "$project_source" -maxdepth 1 -type f \
        \( -iname 'copying*' -o -iname 'license*' \) \
        -exec cp -t "$license_root/$project" {} +
done
mkdir -p "$license_root/libadwaita"
find "$build_root/libadwaita-source" -maxdepth 1 -type f \
    \( -iname 'copying*' -o -iname 'license*' \) \
    -exec cp -t "$license_root/libadwaita" {} +

printf '%s\n' "$expected_marker" > "$marker"
echo "Built GTK $gtk_version and libadwaita $libadwaita_version in $prefix"
