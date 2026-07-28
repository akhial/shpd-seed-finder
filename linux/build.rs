// SPDX-License-Identifier: GPL-3.0-or-later

// The canonical, checksum-verified copies of the Shattered Pixel Dungeon item
// atlases and their licence live under the Android assets tree. Passing that
// directory to glib-compile-resources as a second source directory lets the
// GResource XML reference them by name — aliased to their in-bundle path — so
// the binaries are never duplicated into linux/resources/ and the repository
// keeps exactly one copy of each to verify.
const THIRD_PARTY_ASSETS: &str =
    "../android/app/src/main/assets/third_party/shattered-pixel-dungeon";

fn main() {
    glib_build_tools::compile_resources(
        &["resources", THIRD_PARTY_ASSETS],
        "resources/resources.gresource.xml",
        "dev.seedseeker.SeedSeeker.gresource",
    );
}
