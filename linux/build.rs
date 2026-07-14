// SPDX-License-Identifier: GPL-3.0-or-later

fn main() {
    glib_build_tools::compile_resources(
        &["resources"],
        "resources/resources.gresource.xml",
        "dev.seedseeker.SeedSeeker.gresource",
    );
}
