# Seed Seeker

[![CI](https://github.com/akhial/shpd-seed-seeker/actions/workflows/ci.yml/badge.svg)](https://github.com/akhial/shpd-seed-seeker/actions/workflows/ci.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](COPYING)

An extremely fast, offline seed finder for [Shattered Pixel Dungeon](https://shatteredpixel.com/),
written in Rust — with native Android, Linux, macOS, and Windows apps.

<p align="center">
  <img alt="Bar chart of seed-search throughput. Seed Seeker tests 4,528 seeds per second on 12 cores and 604 on one core; the incumbent Java shpd-seed-finder tests 421 seeds per second across 6 processes and 93 in one process." src="assets/benchmark.svg">
</p>

<p align="center">
  <i>Scanning seeds for a +4 Ring of Tenacity in the first 24 floors, on an Apple M4 Pro (12 cores).
  Both finders were built from the same pinned v3.3.8 game source.
  See <a href="#benchmarks">Benchmarks</a> for the full methodology.</i>
</p>

- ⚡️ **6–10× faster** than the established Java seed finder — per core *and* at full machine width
- 🎯 **Exact**: a Rust reimplementation of the game's deterministic generation path, pinned to
  Shattered Pixel Dungeon **v3.3.8** and continuously cross-checked against the real game code
- 🔍 **Rich queries**: multiple AND requirements across melee and thrown weapons, armor, wands,
  and all twelve rings — exact or minimum upgrades, enchantments/glyphs/curses, loot sources,
  per-item floor limits, same-item groups, and blacksmith constraints
- 🔮 **Seed scouting**: paste a seed code, get every searchable item through depth 24 with floor,
  upgrade, enchantment, cursed state, source, and choice constraints
- 📱 **Android app** (Jetpack Compose) with streaming results and bounded memory
- 🐧 **Native Linux app** (GTK 4 and libadwaita through gtk-rs) sharing the same Rust engine
  in-process through the session crate
- 🍎 **Native macOS app** (SwiftUI, Apple Silicon) sharing the same Rust engine over a C ABI
- 🪟 **Native Windows app** (WinUI 3, x64 and ARM64) using Fluent Design 2 and the same Rust
  engine
- 🧵 **Multicore scheduler** with per-search cancellation, atomic progress, and NEON-batched RNG
  on ARM64
- 🧪 **Oracle-verified**: Java-parity tests and reproducible whole-floor snapshots generated from
  an isolated export of the exact pinned game revision

Seeds use the canonical `XXX-XXX-XXX` base-26 form. Searching and scouting share the same
version-pinned world generator, so a seed the search reports is exactly what the scout — and the
game — will generate.

## Table of contents

1. [Getting started](#getting-started)
1. [Search queries](#search-queries)
1. [Benchmarks](#benchmarks)
1. [Compatibility](#compatibility)
1. [Performance model](#performance-model)
1. [Project layout](#project-layout)
1. [Development](#development)
1. [Acknowledgements](#acknowledgements)
1. [License and identity](#license-and-identity)

## Getting started<a id="getting-started"></a>

### Download a release

Prebuilt binaries for every tagged version are published on the
[GitHub Releases page](https://github.com/akhial/shpd-seed-seeker/releases). Each release is
built by the Release workflow from a `v*` tag and includes a `SHA256SUMS.txt` covering every
asset.

| Asset | Platforms |
| --- | --- |
| `seed-seeker-cli-<tag>-<target>.tar.gz` / `.zip` | CLI for Linux (x86_64, arm64), macOS (Apple Silicon, Intel), and Windows (x86_64, arm64) |
| `seed-seeker-<tag>-<arch>.AppImage` | Native Linux app (x86_64, arm64) |
| `seed-seeker-<tag>-macos-arm64.app.zip` | Native macOS app (Apple Silicon, macOS 14+) |
| `seed-seeker-<tag>-windows-<arch>.zip` | Native Windows app (x64, ARM64) |
| `seed-seeker-<tag>-android.apk` | Android app (arm64-v8a and x86_64) |

Platform notes:

- The macOS app is signed with a Developer ID certificate and notarized by Apple, so it opens
  normally after download.
- The Android APK is signed with the project's release key and installs directly once
  installing from unknown sources is allowed. Locally built APKs are unsigned; see the
  [Android](#android) section below for an `apksigner` example.
- The Windows app requires the
  [Windows App SDK 1.8 runtime](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/downloads)
  to be installed.
- The Linux AppImage bundles GTK and libadwaita. Make it executable, then run it directly; it does
  not need to be extracted or installed.

### CLI

Build and run the canonical depth-24 benchmark (10,000 seeds on all available CPUs by default):

```sh
cargo run --release -p shpd-seedfinder-cli -- --benchmark
cargo run --release -p shpd-seedfinder-cli -- -b 1000 --workers 4
```

To search, put the requirements in a JSON file and pass it with `--items` (or `-i`). Matching
seed codes are written to standard output in ascending order, starting from `AAA-AAA-AAA`:

```sh
cargo run --release -p shpd-seedfinder-cli -- --items requirements.json
cargo run --release -p shpd-seedfinder-cli -- -i requirements.json -b 1000 --workers 4
```

### Android

```sh
cd android
JAVA_HOME=/path/to/temurin-21 ./gradlew :app:assembleRelease --offline
```

`assembleRelease` automatically cross-compiles the Rust JNI library for `arm64-v8a` and `x86_64`
with Android NDK 28.2. The resulting
`android/app/build/outputs/apk/release/app-release-unsigned.apk` must be signed before
installation. For local testing only, it can be signed with Android's standard debug key:

```sh
"$ANDROID_HOME/build-tools/36.1.0/apksigner" sign \
  --ks "$HOME/.android/debug.keystore" \
  --ks-pass pass:android --key-pass pass:android \
  --out seed-seeker-release-debug-signed.apk \
  android/app/build/outputs/apk/release/app-release-unsigned.apk
```

A published build should instead use a protected distribution key. Debug builds deliberately use
the deterministic demo adapter and do not package stale release JNI binaries.

### macOS

The native Apple Silicon app (macOS 14+) links the shared Rust engine as a static library. The
app-bundle script runs the release builds, assembles `dist/Seed Seeker.app`, and ad-hoc signs it:

```sh
bash scripts/build-macos-native.sh
bash scripts/build-macos-app.sh
```

Android, macOS, and Linux app searches rotate the full seed space: the first session start is
randomized, and later sessions use distinct, widely separated starts. CLI searches and benchmarks
remain deterministic for reproducibility.

### Linux

The native GTK 4 and libadwaita app links the shared Rust engine in-process through
`shpd-seedfinder-session`. The Search page takes the same JSON query format as the CLI and
streams matching seed codes with live progress; the Scout page lists every searchable item of a
seed through depth 24. It requires GTK 4.22, libadwaita 1.9, and `glib-compile-resources`;
[`linux/README.md`](linux/README.md) lists the development packages.

```sh
cargo run -p shpd-seedfinder-gtk
```

Tagged releases include x86_64 and arm64 AppImages. To build one locally on Fedora 44, install the
packages from [`linux/README.md`](linux/README.md), plus `curl` and `file`, then run:

```sh
APPIMAGE_VERSION=dev bash scripts/build-linux-appimage.sh
./dist/seed-seeker-dev-"$(uname -m)".AppImage
```

## Search queries<a id="search-queries"></a>

Each requirement can target a concrete item or any item in its category, use an exact, minimum,
or unrestricted upgrade predicate, constrain the loot source, set its own inclusive floor limit,
and join a same-item group shared by other requirements. Exact upgrades run through `+3` for
weapons, armor, and wands and through `+4` for rings; minimum predicates also support `+0`.
Weapon enchantment/curse and armor glyph/curse constraints are supported, and any requirement can
demand that its matching copy be uncursed. A concrete `+4` ring
requirement also accepts an Imp ring when one immediate Scroll of Transmutation roll produces the
requested ring. Queries can require an accessible blacksmith, prevent the Blacksmith's Smith
rewards from satisfying item requirements, and limit every item and facility to the first X
dungeon floors. Mutually exclusive rewards are represented explicitly so impossible reward
combinations cannot satisfy a query.

```json
{
  "max_depth": 24,
  "require_blacksmith": false,
  "exclude_blacksmith_rewards": false,
  "challenges": ["barren_land", "into_darkness", "forbidden_runes"],
  "requirements": [
    {
      "item": "ring_tenacity",
      "upgrade": 4,
      "source": "imp_reward",
      "max_depth": 19
    },
    {
      "kind": "wand",
      "upgrade": { "at_least": 2 },
      "uncursed": true
    }
  ]
}
```

Omitting `upgrade` means any upgrade; an integer means an exact upgrade. Item effects, `uncursed`, loot
`source`, `identity_group`, and per-item `max_depth` are optional. The `kind` field is only
required for wildcard requirements; concrete items use the stable IDs from
`crates/seedfinder-core/src/catalog.rs`. Set `exclude_blacksmith_rewards` when the Smith choice
must remain unused so the Blacksmith's favor can instead be spent on reforging.

The optional `challenges` array accepts `on_diet`, `faith_is_my_armor`, `pharmacophobia`,
`barren_land`, `swarm_intelligence`, `into_darkness`, `forbidden_runes`,
`hostile_champions`, and `badder_bosses`. An omitted or empty array uses the normal game rules.

Searches automatically exploit generation logic: queries that can only be satisfied by quest
rewards stop generating floors past the quest's depth window (+3 wands end at floor 9, +3/+4
rings at floor 19). Per-item floor limits reject a seed as soon as a missing item's deadline
passes, and resolved quests can also rule a seed out early. These shortcuts are exact. The
optional top-level `"fast_mode": true` adds one lossy shortcut: +3 weapon/armor requirements
consider only Ghost and Blacksmith rewards, skipping the far rarer Crypt and Sacrificial-fire
prizes, so those searches end at floor 14. Fast-mode matches are always genuine; some exotic
seeds are simply not reported.

## Benchmarks<a id="benchmarks"></a>

The chart above compares Seed Seeker with the established Java seed finder,
[Elektrochecker's shpd-seed-finder](https://github.com/Elektrochecker/shpd-seed-finder), which
patches the real game source and replays generation on the JVM.

| Configuration | Throughput | Relative |
| --- | ---: | ---: |
| Seed Seeker, 12 threads | 4,528 seeds/s | **10.8×** |
| Seed Seeker, 1 thread | 604 seeds/s | 6.5× (per core) |
| shpd-seed-finder, 6 processes (its best) | 421 seeds/s | 1× |
| shpd-seed-finder, 1 process | 93 seeds/s | — |

Methodology:

- **Machine:** Apple M4 Pro (12 cores), 48 GB, macOS 26.5. Both tools ran on the same machine
  on the same day.
- **Workload:** scan seeds sequentially from `AAA-AAA-AAA`, testing the first 24 floors of each
  seed for a +4 Ring of Tenacity — Seed Seeker's canonical `--benchmark` query, expressed for the
  Java finder as an `all`-mode item list containing `ring of tenacity +4` with 24 floors.
- **Incumbent build:** the reference finder was built from the *same pinned v3.3.8 game source*
  using its own `changes.patch` and run on OpenJDK 26 (Temurin) with its default configuration
  in sequential mode. Timing instrumentation was added only to its driver loop (a counter and a
  `System.nanoTime()` pair); the per-seed search code is unmodified, and JVM startup is excluded.
- **Single process:** 10,000 seeds per run for the Java finder (107.6 s); 30,000–100,000 seeds
  for Seed Seeker, whose timer also excludes process startup.
- **Multi-process:** the Java finder's documented "turbo mode" runs independent JVM processes.
  Concurrency 4, 6, 8, and 12 were measured and its best (6 processes, summed steady-state rate)
  is reported; at 12 processes its aggregate falls to 179 seeds/s.
- **Cross-check:** every one of the 58 seeds the Java finder reported in the first 10,000 was
  also reported by Seed Seeker. (Seed Seeker finds a documented superset: a concrete +4 ring
  requirement also accepts a transmutable Imp ring.)
- **Same question, different engines:** Seed Seeker's always-on exact planning shortcuts (boss
  floors whose state transitions are precomputed, quest-window deadlines) are part of the
  measurement; the lossy `fast_mode` is off. The Java finder generates every floor of every seed.

Reproduce the Seed Seeker side with `cargo run --release -p shpd-seedfinder-cli -- --benchmark`.

## Compatibility<a id="compatibility"></a>

The compatibility target is intentionally pinned:

- Shattered Pixel Dungeon **v3.3.8**
- upstream commit `7b8b845a76fe76c6b7c031ae9e570852411f56db`
- new custom-seed dungeon, main branch, Warrior, no challenges, no equipped trinket, no bones or
  profile-dependent bonus items
- tutorial/journal progression treated as complete (`intro = false`), matching established
  seed-finder behavior and excluding unseeded Guidebook placement

Scouting lists the searchable static equipment and deterministic quest rewards generated through
depth 24. Normal monster drops and other play-time loot remain outside the compatibility profile:
they are rolled during play from unseeded RNG. See the
[compatibility notes](docs/COMPATIBILITY.md) for the exact parity boundary, boss-floor handling,
and known upstream nondeterminism.

## Performance model<a id="performance-model"></a>

Candidate worlds are independent. The scheduler assigns chunks to all available cores with
per-search cancellation and atomic progress. ARM64 builds batch MX3 and Java-LCG depth-root work
in NEON lanes. The spatial generator remains scalar inside each candidate because its rejection
loops and room graph are highly divergent; thread-level parallelism is the effective acceleration
there. Android streams matches as they are found and stops after 1,024 results so an unattended
search cannot grow memory without bound.

Android builds retain fat LTO but use optimization level O2; this is a pinned correctness
requirement for the audited Rust/LLVM Android AArch64 toolchain. The host release profile remains
O3. See the compatibility notes for the on-device parity gate.

## Project layout<a id="project-layout"></a>

- `crates/seedfinder-core`: deterministic Rust engine, query model, matcher, multicore scheduler,
  and Java-parity tests.
- `crates/seedfinder-cli`: command-line entry point and canonical engine benchmark.
- `crates/seedfinder-session`: frontend-neutral native session lifecycle, registry, status
  packets, and panic-contained scouting.
- `crates/seedfinder-ffi`: thread-safe C ABI and public header used by Apple frontends.
- `linux`: native GTK 4 and libadwaita app (gtk-rs), linked to the shared Rust engine in-process
  through `seedfinder-session`.
- `android`: Jetpack Compose UI, coarse-grained search sessions, and the one-shot JNI seed-scout
  contract.
- `macos/SeedSeeker`: native arm64 macOS 14+ SwiftUI app and SwiftPM package, linked to the
  shared Rust engine through its C ABI.
- `windows/SeedSeeker`: native WinUI 3 desktop app (x64 and ARM64) using Fluent Design 2 and the
  shared Rust engine through its C ABI.
- `tooling/oracle`: reproducible, machine-readable whole-floor snapshots from an isolated export
  of the exact pinned game revision.
- `tooling/parity`: focused Java fixture generators for individual RNG and reward paths.
- `upstream` (ignored): local read-only clones of the official game and the established Java
  seed finder used as an independent oracle.

## Development<a id="development"></a>

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The workspace includes the GTK app, so the commands above need its system libraries (GTK 4.22
and libadwaita 1.9). On hosts without them — macOS, Windows, and older Linux distributions —
add `--exclude shpd-seedfinder-gtk`, exactly as the non-Linux CI jobs do.

```sh
cd android
JAVA_HOME=/path/to/temurin-21 ./gradlew \
  :app:testDebugUnitTest :app:lintDebug :app:assembleRelease --offline
```

For the native macOS app, build the Rust static library before running the Swift tests:

```sh
bash scripts/build-macos-native.sh
cd macos/SeedSeeker
swift test
```

The Swift package supports macOS 14 and newer and is arm64-only. Its manifest links
`target/aarch64-apple-darwin/release/libshpd_seedfinder_ffi.a`; there is no demo engine on macOS.

The Java RNG fixture is JDK-only:

```sh
javac -d /tmp tooling/parity/RngOracle.java
java -cp /tmp RngOracle
```

`EquipmentOracle.java` is compiled against the isolated v3.3.8 oracle JAR and calls the real game
equipment classes.

## Acknowledgements<a id="acknowledgements"></a>

Seed Seeker reimplements the deterministic generation path of
[Shattered Pixel Dungeon](https://github.com/00-Evan/shattered-pixel-dungeon) by Evan Debenham,
itself based on [Pixel Dungeon](https://github.com/watabou/pixel-dungeon) by Oleg Dolya.

[Elektrochecker's shpd-seed-finder](https://github.com/Elektrochecker/shpd-seed-finder) pioneered
seed finding for this game and serves as an independent oracle for this project's parity tests.
The benchmark above exists because that tool set the standard to beat.

## License and identity<a id="license-and-identity"></a>

This project is GPL-3.0-or-later. It contains a derived generation implementation and an
unchanged item sprite atlas from Shattered Pixel Dungeon, which is also GPL-licensed. Copyright
notices and the full license are included with the Android distribution.

Seed Seeker is unofficial and is not endorsed by Shattered Pixel Dungeon or its developers. It
uses a distinct package, name, icon, and UI; no game UI components are reused.

- Pixel Dungeon © 2012–2015 Oleg Dolya / Watabou
- Shattered Pixel Dungeon © 2014–2026 Evan Debenham
