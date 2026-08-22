// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app

import android.app.Application
import dev.seedseeker.app.catalog.ItemCatalog

class SeedSeekerApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        // The item catalog is parsed from the packaged asset. Binding it here,
        // rather than in an Activity, means every entry point into the process
        // (launcher, App Link, anything added later) finds it already in place.
        ItemCatalog.install { path -> assets.open(path) }
    }
}
