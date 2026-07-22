// SPDX-License-Identifier: GPL-3.0-or-later
plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "dev.seedseeker.app"
    compileSdk = 36
    ndkVersion = "28.2.13676358"

    defaultConfig {
        applicationId = "dev.seedseeker.unofficial"
        // Compose 1.9+ (required for Material 3 Expressive) raised the floor to API 23.
        minSdk = 23
        targetSdk = 36
        versionCode = 8
        versionName = "0.5.3"

        ndk {
            // The Rust build produces exactly these ABIs. Without an explicit filter,
            // transitive AndroidX native libraries make the APK appear installable on
            // 32-bit devices where libshpd_seedfinder.so is unavailable.
            abiFilters += setOf("arm64-v8a", "x86_64")
        }

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        vectorDrawables.useSupportLibrary = true
    }

    buildTypes {
        debug {
            buildConfigField("boolean", "USE_DEMO_ENGINE", "true")
            applicationIdSuffix = ".debug"
            versionNameSuffix = "-demo"
        }
        release {
            buildConfigField("boolean", "USE_DEMO_ENGINE", "false")
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }

    packaging {
        resources.excludes += setOf(
            "/META-INF/{AL2.0,LGPL2.1}",
            "/META-INF/DEPENDENCIES",
        )
    }

    testOptions {
        unitTests.isIncludeAndroidResources = true
    }

    lint {
        // Gradle 9.4 is intentionally paired with the audited upstream v3.3.8 toolchain.
        disable += setOf("GradleDependency", "AndroidGradlePluginVersion")
    }

    // Debug builds use DemoNativeSeedFinder and must not pick up stale release JNI outputs.
    sourceSets.getByName("release").jniLibs.directories.add("build/generated/jniLibs")
}

val rustJniOutput = layout.buildDirectory.dir("generated/jniLibs")
val buildRustJni by tasks.registering(Exec::class) {
    group = "build"
    description = "Builds arm64-v8a and x86_64 Rust JNI libraries"
    workingDir(rootProject.projectDir.parentFile)
    commandLine(
        "sh",
        rootProject.projectDir.parentFile.resolve("scripts/build-android-native.sh").absolutePath,
        rustJniOutput.get().asFile.absolutePath,
    )
    inputs.files(
        fileTree(rootProject.projectDir.parentFile.resolve("crates")) {
            include("**/*.rs", "**/Cargo.toml")
        },
        rootProject.projectDir.parentFile.resolve("Cargo.toml"),
        rootProject.projectDir.parentFile.resolve("Cargo.lock"),
        rootProject.projectDir.parentFile.resolve("scripts/build-android-native.sh"),
    )
    outputs.dir(rustJniOutput)
}

tasks.matching { it.name == "mergeReleaseJniLibFolders" }.configureEach {
    dependsOn(buildRustJni)
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2025.12.01")

    implementation(composeBom)
    implementation("androidx.activity:activity-compose:1.11.0")
    implementation("androidx.compose.foundation:foundation")
    // Pinned past the BOM: the Material 3 Expressive APIs (MaterialExpressiveTheme,
    // ToggleButton, LoadingIndicator, flexible top app bars, …) are internal in the
    // 1.4.0 stable artifact and only public on the 1.5.0 pre-release line.
    // alpha18 is the newest alpha whose Compose dependencies still accept compileSdk 36
    // (alpha19+ pull Compose 1.12 alphas that demand compileSdk 37).
    implementation("androidx.compose.material3:material3:1.5.0-alpha18")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    // The icons artifacts are frozen at 1.7.8 and no longer ship in the BOM.
    implementation("androidx.compose.material:material-icons-core:1.7.8")
    debugImplementation("androidx.compose.ui:ui-tooling")

    testImplementation("junit:junit:4.13.2")
}
