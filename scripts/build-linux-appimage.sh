#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [ -n "${APPIMAGE_GTK_PREFIX:-}" ]; then
    export PKG_CONFIG_PATH="$APPIMAGE_GTK_PREFIX/lib/pkgconfig:$APPIMAGE_GTK_PREFIX/share/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
    export LD_LIBRARY_PATH="$APPIMAGE_GTK_PREFIX/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi

linuxdeploy_version=1-alpha-20251107-1
appimagetool_version=continuous

case "$(uname -m)" in
    x86_64)
        appimage_arch=x86_64
        linuxdeploy_sha256=c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d
        appimagetool_sha256=a6d71e2b6cd66f8e8d16c37ad164658985e0cf5fcaa950c90a482890cb9d13e0
        runtime_sha256=1cc49bcf1e2ccd593c379adb17c9f85a36d619088296504de95b1d06215aebbf
        ;;
    aarch64)
        appimage_arch=aarch64
        linuxdeploy_sha256=620095110d693282b8ebeb244a95b5e911cf8f65f76c88b4b47d16ae6346fcff
        appimagetool_sha256=1b00524ba8c6b678dc15ef88a5c25ec24def36cdfc7e3abb32ddcd068e8007fe
        runtime_sha256=7d5d772b7c32f0c84caf0a452a3072a5709027d7eac5856feb89a7a7a8881372
        ;;
    *)
        echo "Unsupported AppImage architecture: $(uname -m)" >&2
        exit 1
        ;;
esac

for command in cargo curl glib-compile-schemas ldconfig pkg-config sha256sum; do
    if ! command -v "$command" >/dev/null; then
        echo "Required command not found: $command" >&2
        exit 1
    fi
done

if ! pkg-config --atleast-version=4.22 gtk4; then
    echo "GTK 4.22 or newer is required to build the AppImage" >&2
    exit 1
fi
if ! pkg-config --atleast-version=1.9 libadwaita-1; then
    echo "libadwaita 1.9 or newer is required to build the AppImage" >&2
    exit 1
fi

package_version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
version=${APPIMAGE_VERSION:-v$package_version}
filename_version=${version//\//-}
tool_dir=${APPIMAGE_TOOL_DIR:-"$repo_root/target/appimage-tools"}
appdir=${APPIMAGE_APPDIR:-"$repo_root/target/SeedSeeker.AppDir"}
cargo_target_dir=${APPIMAGE_CARGO_TARGET_DIR:-"$repo_root/target/appimage-cargo"}
output_dir=${APPIMAGE_OUTPUT_DIR:-"$repo_root/dist"}
output=${APPIMAGE_OUTPUT:-"$output_dir/seed-seeker-$filename_version-$appimage_arch.AppImage"}

mkdir -p "$tool_dir" "$output_dir"
rm -rf "$appdir"

download_tool() {
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
    chmod +x "$destination"
}

linuxdeploy="$tool_dir/linuxdeploy-$appimage_arch.AppImage"
appimagetool="$tool_dir/appimagetool-$appimage_arch.AppImage"
runtime="$tool_dir/runtime-$appimage_arch"

download_tool \
    "https://github.com/linuxdeploy/linuxdeploy/releases/download/$linuxdeploy_version/linuxdeploy-$appimage_arch.AppImage" \
    "$linuxdeploy_sha256" \
    "$linuxdeploy"
download_tool \
    "https://github.com/AppImage/appimagetool/releases/download/$appimagetool_version/appimagetool-$appimage_arch.AppImage" \
    "$appimagetool_sha256" \
    "$appimagetool"
download_tool \
    "https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-$appimage_arch" \
    "$runtime_sha256" \
    "$runtime"

CARGO_TARGET_DIR="$cargo_target_dir" cargo build --release --locked -p shpd-seedfinder-gtk

# linuxdeploy's bundled strip predates modern DT_RELR sections emitted by some
# build hosts. Disable that extra pass; AppImage compression still keeps the
# portable source-built stack compact.
NO_STRIP=1 APPIMAGE_EXTRACT_AND_RUN=1 "$linuxdeploy" \
    --appdir "$appdir" \
    --executable "$cargo_target_dir/release/seed-seeker-gtk" \
    --desktop-file linux/data/dev.seedseeker.SeedSeeker.desktop \
    --icon-file linux/data/icons/hicolor/scalable/apps/dev.seedseeker.SeedSeeker.svg \
    --custom-apprun linux/AppRun

# linuxdeploy excludes these common libraries by default. The portable stack
# is newer than the versions on our baseline distributions, however, so keep
# the exact copies against which GTK and Pango were linked.
if [ -n "${APPIMAGE_GTK_PREFIX:-}" ]; then
    for soname in libharfbuzz.so.0 libwayland-client.so.0; do
        source_library=$(readlink -f "$APPIMAGE_GTK_PREFIX/lib/$soname")
        install -m755 "$source_library" "$appdir/usr/lib/$(basename -- "$source_library")"
        ln -sfn "$(basename -- "$source_library")" "$appdir/usr/lib/$soname"
    done
fi

install -Dm644 linux/data/dev.seedseeker.SeedSeeker.metainfo.xml \
    "$appdir/usr/share/metainfo/dev.seedseeker.SeedSeeker.appdata.xml"
install -Dm644 linux/data/icons/hicolor/symbolic/apps/dev.seedseeker.SeedSeeker-symbolic.svg \
    "$appdir/usr/share/icons/hicolor/symbolic/apps/dev.seedseeker.SeedSeeker-symbolic.svg"
install -Dm644 COPYING "$appdir/usr/share/licenses/seed-seeker/COPYING"
install -Dm644 NOTICE "$appdir/usr/share/licenses/seed-seeker/NOTICE"

# GTK's built-in fallback icons are deliberately small. Bundle Adwaita so all
# actions used by the application remain visible on non-GNOME desktops.
mkdir -p "$appdir/usr/share/icons"
cp -a /usr/share/icons/Adwaita "$appdir/usr/share/icons/"
install -Dm644 /usr/share/icons/hicolor/index.theme \
    "$appdir/usr/share/icons/hicolor/index.theme"

gtk_data_dir=/usr/share
if [ -n "${APPIMAGE_GTK_PREFIX:-}" ]; then
    gtk_data_dir=$APPIMAGE_GTK_PREFIX/share
fi

schema_dir="$appdir/usr/share/glib-2.0/schemas"
mkdir -p "$schema_dir"
cp "$gtk_data_dir"/glib-2.0/schemas/org.gtk.gtk4.* "$schema_dir/"
install -m644 /usr/share/glib-2.0/schemas/org.gnome.desktop.enums.xml "$schema_dir/"
install -m644 /usr/share/glib-2.0/schemas/org.gnome.desktop.interface.gschema.xml "$schema_dir/"
glib-compile-schemas "$schema_dir"

copy_package_licenses() {
    local package=$1
    local destination=$appdir/usr/share/licenses/$package

    if [ -d "/usr/share/licenses/$package" ] && [ ! -e "$destination" ]; then
        cp -a "/usr/share/licenses/$package" "$destination"
    elif [ -f "/usr/share/doc/$package/copyright" ] && [ ! -e "$destination" ]; then
        mkdir -p "$destination"
        cp "/usr/share/doc/$package/copyright" "$destination/"
    fi
}

copy_package_licenses adwaita-icon-theme
copy_package_licenses gtk4
copy_package_licenses libadwaita

if [ -n "${APPIMAGE_GTK_PREFIX:-}" ] && [ -d "$APPIMAGE_GTK_PREFIX/share/licenses" ]; then
    cp -a "$APPIMAGE_GTK_PREFIX/share/licenses/." "$appdir/usr/share/licenses/"
fi

# Preserve the notices for every system library linuxdeploy selected.
while IFS= read -r library; do
    library_name=$(basename -- "$library")
    system_library=$(ldconfig -p | awk -v name="$library_name" '$1 == name { print $NF; exit }')
    if [ -n "$system_library" ] && [ -e "$system_library" ]; then
        package=
        if command -v rpm >/dev/null; then
            package=$(rpm -qf --queryformat '%{NAME}' "$system_library" 2>/dev/null || true)
        elif command -v dpkg-query >/dev/null; then
            package=$(dpkg-query --search "$system_library" 2>/dev/null | head -n 1 | cut -d: -f1 || true)
        fi
        if [ -n "$package" ]; then
            copy_package_licenses "$package"
        fi
    fi
done < <(find "$appdir/usr/lib" -maxdepth 1 -type f -print)

rm -f "$output"
ARCH=$appimage_arch VERSION=$version APPIMAGE_EXTRACT_AND_RUN=1 "$appimagetool" \
    --runtime-file "$runtime" \
    "$appdir" "$output"
chmod +x "$output"

echo "Created $output"
