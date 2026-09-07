# Seed Seeker Android

Seed Seeker is an independent, unofficial seed-search interface for Shattered Pixel Dungeon. It uses an original Jetpack Compose UI and does not include or reuse the game's UI components.

The debug build deliberately uses `DemoNativeSeedFinder` as its search engine, so UI states can be exercised with deterministic sample seeds. Release builds select `JniNativeSeedFinder`, whose compact wire contract is documented in `NativeSeedFinder.kt`. Both build types package `libshpd_seedfinder.so` for `arm64-v8a` and `x86_64` (built through `scripts/build-android-native.sh`): wire codecs such as the share-link format always run the canonical Rust implementation through `dev.seedseeker.app.engine.JniBindings`, even in debug APKs.

Build with:

```shell
./gradlew :app:assembleDebug
./gradlew :app:assembleRelease
```

The build uses Gradle 9.4 and AGP 9.1. Run Gradle on JDK 21 with Android SDK 36
and NDK `28.2.13676358` installed; app bytecode remains Java 11 compatible. The
native build also needs these Rust targets:

```shell
rustup target add aarch64-linux-android x86_64-linux-android
```

If a newer JDK is your shell default, set `JAVA_HOME` to JDK 21 before invoking
the wrapper. `ANDROID_HOME` or `android/local.properties` must identify the SDK.

The app requests no Android permissions. It targets API 36, supports API 23+, opts into edge-to-edge drawing, and uses AndroidX's predictive-back handler for in-app navigation.

## Licensing

This project is licensed under GPL-3.0-or-later. The unmodified `items.png` atlas is redistributed from Shattered Pixel Dungeon v3.3.8 under that license; details and integrity metadata live under `app/src/main/assets/third_party/shattered-pixel-dungeon/`.

Shattered Pixel Dungeon is copyright © 2014–2026 Evan Debenham. Pixel Dungeon is copyright © 2012–2015 Oleg Dolya. Seed Seeker is not affiliated with or endorsed by Shattered Pixel Dungeon or its authors.

## Trinkets

The requirement picker has a named Trinket category, including either/or groups.
Trinkets save directly from the item picker without equipment details. Scout shows
four square choices beneath the catalyst on its actual floor, highlights matching
choices with a flat green fill, and keeps the remaining thirteen icons
in one row. Sprite drawing uses nearest-neighbor filtering.

The native scout decoder reads SSC4 (the SSC3 layout followed by a 17-entry
trinket deck: one-byte count and UTF-8 IDs with unsigned 16-bit lengths), while
retaining SSC3 compatibility. The deck ordering is independent of item sorting.
