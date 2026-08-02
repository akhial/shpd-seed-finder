// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import dev.seedseeker.app.engine.NativeSeedFinderFactory
import dev.seedseeker.app.ui.SeedFinderApp
import dev.seedseeker.app.ui.SharedLink
import dev.seedseeker.app.ui.theme.SeedSeekerTheme

class MainActivity : ComponentActivity() {
    private var sharedLink by mutableStateOf<SharedLink?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        // Debug-only test hook for the update dialog:
        //   adb shell am start -n dev.seedseeker.unofficial.debug/dev.seedseeker.app.MainActivity \
        //       --es fake_latest_version 9.9.9
        val fakeLatestVersion =
            if (BuildConfig.DEBUG) intent.getStringExtra("fake_latest_version") else null
        sharedLink = intent?.dataString?.let(::SharedLink)
        setContent {
            SeedSeekerTheme {
                val engine = remember { NativeSeedFinderFactory.create() }
                SeedFinderApp(engine, fakeLatestVersion, sharedLink)
            }
        }
    }

    // App Links tapped while the singleTask activity is already running
    // arrive here instead of onCreate.
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        sharedLink = intent.dataString?.let(::SharedLink)
    }
}
