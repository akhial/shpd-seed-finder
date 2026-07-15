# Seed Seeker for Linux

This directory contains the native Linux application. It uses GTK 4 through gtk-rs and
libadwaita-rs, follows the GNOME Human Interface Guidelines and the GNOME application-ID and
resource conventions, and is part of the root Cargo workspace. The binary is named
`seed-seeker-gtk` because the CLI already claims `seed-seeker` in the shared workspace target
directory.

## Interface

The window is an adaptive triple-pane layout built from two nested `AdwNavigationSplitView`s,
the libadwaita pattern for triple-pane navigation. Breakpoints collapse the panes into
push/pop navigation as the window narrows, down to a fully single-pane phone-sized layout.

- **Query** (sidebar) builds the search declaratively: item requirements as editable rows,
  plus floor limit, blacksmith, and fast-search scope controls. Requirements open in a dialog
  with category, item, tier, upgrade, enchantment/glyph, source, same-item group, and
  per-item floor limit predicates. Challenges live in a preferences dialog behind the main
  menu, and the whole query persists across sessions in the user configuration directory.
- **Results** streams matching seed codes from a full-seed-space production session running
  on all cores with a rotated start, with live match probability, seeds-per-second,
  time-to-match, and progress. Impossible queries, empty completions, and worker failures
  each get a dedicated status page. Sessions are cancellable and cap accepted results at
  1,024, like the other app frontends.
- **Seed** scouts one seed — typed in, or selected from the results — and lists every
  searchable item through depth 24, grouped by floor with region names, upgrade and
  enchantment tags, cursed state, source, and choice constraints. Items that jointly
  satisfy the current requirements are highlighted.

The engine is linked in-process through `shpd-seedfinder-session`. The shell also provides
the application lifecycle, app actions and shortcuts (with a shortcuts dialog), an About
dialog, embedded resources including per-category symbolic icons, a desktop entry, and
AppStream metadata.

## Requirements

- Rust 1.97 or newer
- GTK 4.22 or newer
- libadwaita 1.9 or newer
- `pkg-config` and `glib-compile-resources`

On Fedora 44:

```sh
sudo dnf install gcc gtk4-devel libadwaita-devel pkgconf-pkg-config
```

## Run

From the repository root:

```sh
cargo run -p shpd-seedfinder-gtk
```

GTK Inspector can be enabled while developing with `GTK_DEBUG=interactive`.

## AppImage

The AppImage builder packages the release binary, GTK, libadwaita, the Adwaita icon theme,
GSettings schemas, desktop integration metadata, and license notices. It supports native x86_64
and arm64 builds on Fedora 44:

```sh
sudo dnf install gcc curl file gtk4-devel libadwaita-devel pkgconf-pkg-config
APPIMAGE_VERSION=dev bash scripts/build-linux-appimage.sh
./dist/seed-seeker-dev-"$(uname -m)".AppImage
```

The Release workflow runs the same builder for both architectures when a `v*` tag is pushed and
publishes the AppImages with the other GitHub Release assets. Release builds first compile the
pinned GTK and libadwaita stack against Ubuntu 24.04 so they retain an older glibc baseline;
`build-linux-appimage.sh` can use that stack locally through `APPIMAGE_GTK_PREFIX` as well.

## Validate

```sh
cargo check -p shpd-seedfinder-gtk
cargo clippy -p shpd-seedfinder-gtk --all-targets -- -D warnings
cargo test -p shpd-seedfinder-gtk
desktop-file-validate linux/data/dev.seedseeker.SeedSeeker.desktop
appstreamcli validate --no-net linux/data/dev.seedseeker.SeedSeeker.metainfo.xml
```
