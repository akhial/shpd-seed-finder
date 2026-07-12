// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.seedseeker.app.BuildConfig

private const val LICENSE_PATH = "third_party/shattered-pixel-dungeon/LICENSE.txt"

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AboutScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    var showLicense by remember { mutableStateOf(false) }
    val licenseText = remember(context) {
        runCatching {
            context.assets.open(LICENSE_PATH).bufferedReader().use { it.readText() }
        }.getOrElse { "License text could not be loaded: ${it.message}" }
    }

    Scaffold(
        containerColor = MaterialTheme.colorScheme.background,
        topBar = {
            TopAppBar(
                title = { Text("About & licenses") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.background,
                ),
            )
        },
    ) { scaffoldPadding ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(scaffoldPadding),
            contentAlignment = Alignment.TopCenter,
        ) {
            LazyColumn(
                modifier = Modifier
                    .fillMaxWidth()
                    .widthIn(max = 680.dp)
                    .navigationBarsPadding(),
                contentPadding = PaddingValues(16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                item {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Surface(
                            shape = MaterialTheme.shapes.extraLarge,
                            color = MaterialTheme.colorScheme.primaryContainer,
                        ) {
                            Box(Modifier.size(68.dp), contentAlignment = Alignment.Center) {
                                CompassMark(Modifier.size(40.dp))
                            }
                        }
                        Spacer(Modifier.width(16.dp))
                        Column {
                            Text("Seed Seeker", style = MaterialTheme.typography.headlineSmall)
                            Text(
                                "Independent · unofficial · open source",
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }

                item {
                    AboutSection("Exact offline engine") {
                        Text(
                            "The native Rust engine recreates the pinned v3.3.8 main dungeon through depth 24. It uses all available CPU cores, reports matches live, and completes after the first 1,024 matches or the end of the seed space.",
                        )
                    }
                }

                item {
                    AboutSection("Not the game") {
                        Text(
                            "Seed Seeker is an independent utility and is not affiliated with or endorsed by Shattered Pixel Dungeon or its authors. Its interface and compass icon are original; no game UI components are used.",
                        )
                    }
                }

                item {
                    AboutSection("Artwork attribution") {
                        Text("The item sprites and type icons are unchanged copies of Shattered Pixel Dungeon's item atlases.")
                        Spacer(Modifier.height(10.dp))
                        AttributionLine("Upstream", "Shattered Pixel Dungeon v3.3.8")
                        AttributionLine("Commit", "7b8b845a76fe76c6b7c031ae9e570852411f56db")
                        AttributionLine("Pixel Dungeon", "© 2012–2015 Oleg Dolya")
                        AttributionLine("Shattered Pixel Dungeon", "© 2014–2026 Evan Debenham")
                        AttributionLine("Atlas SHA-256", "ce2496368660e9b2…a294caacaf")
                        AttributionLine("Icon SHA-256", "38df728d32842d9f…24d7eb9b72")
                    }
                }

                item {
                    AboutSection("GNU GPL v3 or later") {
                        Text(
                            "This program is free software. You may redistribute and modify it under GPL-3.0-or-later. It comes with no warranty. Source distributions must retain the license and copyright notices.",
                        )
                        Spacer(Modifier.height(8.dp))
                        TextButton(onClick = { showLicense = !showLicense }) {
                            Text(if (showLicense) "Hide full license" else "Read full license")
                        }
                    }
                }

                if (showLicense) {
                    item {
                        Card(
                            modifier = Modifier.fillMaxWidth(),
                            shape = MaterialTheme.shapes.large,
                            colors = CardDefaults.cardColors(
                                containerColor = MaterialTheme.colorScheme.surfaceContainerLowest,
                            ),
                        ) {
                            SelectionContainer {
                                Text(
                                    licenseText,
                                    modifier = Modifier.padding(16.dp),
                                    style = MaterialTheme.typography.bodySmall,
                                    fontFamily = FontFamily.Monospace,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        }
                    }
                }

                item {
                    Text(
                        "Seed Seeker ${BuildConfig.VERSION_NAME} · Shattered Pixel Dungeon v3.3.8 profile",
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(vertical = 14.dp),
                        textAlign = TextAlign.Center,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}

@Composable
private fun AboutSection(title: String, content: @Composable ColumnScope.() -> Unit) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
    ) {
        Column(Modifier.padding(18.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(8.dp))
            content()
        }
    }
}

@Composable
private fun AttributionLine(label: String, value: String) {
    Column(Modifier.padding(vertical = 4.dp)) {
        Text(label, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
        Text(value, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
