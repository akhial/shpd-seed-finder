// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.catalog

import java.io.File

/**
 * Points [ItemCatalog] at the module's `src/main/assets` for JVM unit tests,
 * standing in for the `AssetManager` the app installs in
 * `SeedSeekerApplication`. Gradle runs unit tests with the module directory as
 * the working directory. Idempotent, so every test that touches the catalog
 * can call it from its constructor.
 */
object PackagedCatalog {
    val directory = File("src/main/assets")

    fun install() {
        check(directory.isDirectory) { "packaged assets not found at ${directory.absolutePath}" }
        ItemCatalog.install { path -> File(directory, path).inputStream() }
    }
}
