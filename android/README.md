# Seed Seeker Android

Seed Seeker is an independent, unofficial seed-search interface for Shattered Pixel Dungeon. It uses an original Jetpack Compose UI and does not include or reuse the game's UI components.

The debug build deliberately uses `DemoNativeSeedFinder` as its search engine, so UI states can be exercised with deterministic sample seeds. Interactive scouting uses the real engine in both build types, including trinket effects and match highlights. Release builds select `JniNativeSeedFinder`, whose compact wire contract is documented in `NativeSeedFinder.kt`. Both build types package `libshpd_seedfinder.so` for `arm64-v8a` and `x86_64` (built through `scripts/build-android-native.sh`): wire codecs such as the share-link format always run the canonical Rust implementation through `dev.seedseeker.app.engine.JniBindings`, even in debug APKs.

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

The requirement editor can choose matching trinkets at +3. The engine applies
that selection after the first brewing opportunity, with ambiguous offered OR
alternatives leaving no trinket selected. Presets, exports, and share links keep
this choice. Probability estimates use the selected generation effects.

Tap any of the four offered scout cards to apply it, or tap the selected card
again to deselect it. The selected card carries an �Applied +3� badge. Each
rescout keeps the query and explicit override together so match highlights use
the same generated world.

Scout requests use SSQ3 (little-endian challenge mask and length-prefixed seed
and override, followed by canonical query JSON). The native scout decoder reads
SSC5: SSC4's ordered 17-entry trinket deck followed by a UTF-8 selected ID with a
big-endian unsigned 16-bit length; an empty ID means no selection. SSC3 and SSC4
remain readable. The deck ordering is independent of item sorting.
