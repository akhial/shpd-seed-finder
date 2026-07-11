# Seed Seeker for Shattered Pixel Dungeon

[![CI](https://github.com/akhial/shpd-seed-finder/actions/workflows/ci.yml/badge.svg)](https://github.com/akhial/shpd-seed-finder/actions/workflows/ci.yml)

An unofficial, offline seed finder for Shattered Pixel Dungeon. The engine is a
Rust reimplementation of the game's deterministic generation path and is called
from a standalone Android UI through JNI.

The compatibility target is intentionally pinned:

- Shattered Pixel Dungeon **v3.3.8**
- upstream commit `7b8b845a76fe76c6b7c031ae9e570852411f56db`
- new custom-seed dungeon, main branch, Warrior, no challenges, no equipped
  trinket, no bones or profile-dependent bonus items
- tutorial/journal progression treated as complete (`intro = false`), matching
  established seed-finder behavior and excluding unseeded Guidebook placement

Seeds use the canonical `XXX-XXX-XXX` base-26 form. Searches support multiple
AND requirements across melee and thrown weapons, armor, wands, and all twelve
rings. Each requirement can target a concrete item or any item in its category,
use an exact, minimum, or unrestricted upgrade predicate, constrain the loot
source, set its own inclusive floor limit, and join a same-item group shared by
other requirements. Exact upgrades
run through `+3` for weapons, armor, and wands and through `+4` for rings; minimum
predicates also support `+0`. Weapon enchantment/curse and armor glyph/curse
constraints are supported. A concrete `+4` ring requirement also accepts an Imp
ring when one immediate Scroll of Transmutation roll produces the requested ring.
Queries can require an accessible blacksmith, prevent
the Blacksmith's Smith rewards from satisfying item requirements, and limit every
item and facility to the first X dungeon floors.
Mutually exclusive rewards are represented explicitly so impossible reward
combinations cannot satisfy a query.

Seed scouting accepts one canonical seed code and lists the searchable static
equipment and deterministic quest rewards generated through depth 24, including
their floor, true upgrade, enchantment or glyph, cursed state, source, and choice constraints. Scouting and
searching use the same version-pinned world generator; normal monster drops and
other play-time loot remain outside the compatibility profile.

## Project layout

- `crates/seedfinder-core`: deterministic Rust engine, query model, matcher,
  multicore scheduler, and Java-parity tests.
- `crates/seedfinder-cli`: command-line entry point and canonical engine
  benchmark.
- `crates/seedfinder-session`: frontend-neutral native session lifecycle,
  registry, status packets, and panic-contained scouting.
- `crates/seedfinder-ffi`: thread-safe C ABI and public header used by Apple
  frontends.
- `android`: original Jetpack Compose UI, coarse-grained search sessions, and
  the one-shot JNI seed-scout contract.
- `macos/SeedSeeker`: native arm64 macOS 14+ SwiftUI app and SwiftPM package,
  linked to the shared Rust engine through its C ABI.
- `tooling/oracle`: reproducible, machine-readable whole-floor snapshots from
  an isolated export of the exact pinned game revision.
- `tooling/parity`: focused Java fixture generators for individual RNG and
  reward paths.
- `upstream` (ignored): local read-only clones of the official game and the
  established Java seed-finder used as an independent oracle.

## Local verification

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

cd android
JAVA_HOME=/path/to/temurin-21 ./gradlew \
  :app:testDebugUnitTest :app:lintDebug :app:assembleRelease --offline
```

For the native Apple Silicon macOS app, build the Rust static library before
running the Swift tests. The app-bundle script repeats the release builds,
assembles `dist/Seed Seeker.app`, and ad-hoc signs it:

```sh
bash scripts/build-macos-native.sh
cd macos/SeedSeeker
swift test
cd ../..
bash scripts/build-macos-app.sh
```

The Swift package supports macOS 14 and newer and is arm64-only. Its manifest
links `target/aarch64-apple-darwin/release/libshpd_seedfinder_ffi.a`; there is no
demo engine on macOS.

`assembleRelease` automatically cross-compiles the Rust JNI library for
`arm64-v8a` and `x86_64` with Android NDK 28.2. The resulting
`android/app/build/outputs/apk/release/app-release-unsigned.apk` must be signed
before installation. For local testing only, it can be signed with Android's
standard debug key:

```sh
"$ANDROID_HOME/build-tools/36.1.0/apksigner" sign \
  --ks "$HOME/.android/debug.keystore" \
  --ks-pass pass:android --key-pass pass:android \
  --out seed-seeker-release-debug-signed.apk \
  android/app/build/outputs/apk/release/app-release-unsigned.apk
```

A published build should instead use a protected distribution key. Debug builds
deliberately use the deterministic demo adapter and do not package stale release
JNI binaries.

The Java RNG fixture is JDK-only:

```sh
javac -d /tmp tooling/parity/RngOracle.java
java -cp /tmp RngOracle
```

`EquipmentOracle.java` is compiled against the isolated v3.3.8 oracle JAR and
calls the real game equipment classes. See [compatibility notes](docs/COMPATIBILITY.md)
for the parity boundary and known upstream nondeterminism.

## Performance model

Candidate worlds are independent. The scheduler assigns chunks to all available
cores with per-search cancellation and atomic progress. ARM64 builds batch MX3
and Java-LCG depth-root work in NEON lanes. The spatial generator remains scalar
inside each candidate because its rejection loops and room graph are highly
divergent; thread-level parallelism is the effective acceleration there. Android
streams matches as they are found and stops after 1,024 results so an unattended
search cannot grow memory without bound.

Android builds retain fat LTO but use optimization level O2; this is a pinned
correctness requirement for the audited Rust/LLVM Android AArch64 toolchain.
The host release profile remains O3. See the compatibility notes for the
on-device parity gate.

Run the canonical depth-24 search benchmark with a release build. It tests
10,000 seeds on all available CPUs by default; the seed and worker counts can
be overridden:

```sh
cargo run --release -p shpd-seedfinder-cli -- --benchmark
cargo run --release -p shpd-seedfinder-cli -- -b 1000 --workers 4
```

To search from `AAA-AAA-AAA`, put the requirements in a JSON file and pass it
with `--items` (or `-i`). Matching seed codes are written to standard output in
ascending order. The `kind` field is only required for wildcard requirements;
concrete items use the stable IDs from `crates/seedfinder-core/src/catalog.rs`.

Android and macOS app searches rotate the full seed space instead: the first
session start is randomized, and later sessions use distinct, widely separated
starts. Each session still increments contiguous seeds and wraps only once, so
this diversification adds no modular arithmetic to the per-seed hot path. CLI
searches and benchmarks remain deterministic for reproducibility.

```json
{
  "max_depth": 24,
  "require_blacksmith": false,
  "exclude_blacksmith_rewards": false,
  "requirements": [
    {
      "item": "ring_tenacity",
      "upgrade": 4,
      "source": "imp_reward",
      "max_depth": 19
    },
    {
      "kind": "wand",
      "upgrade": { "at_least": 2 }
    }
  ]
}
```

Omitting `upgrade` means any upgrade; an integer means an exact upgrade. Item
effects, loot `source`, `identity_group`, and per-item `max_depth` are optional.
Set `exclude_blacksmith_rewards` when the Smith choice must remain unused so the
Blacksmith's favor can instead be spent on reforging.

Searches automatically exploit generation logic: queries that can only be
satisfied by quest rewards stop generating floors past the quest's depth window
(+3 wands end at floor 9, +3/+4 rings at floor 19). Per-item floor limits reject
a seed as soon as a missing item's deadline passes, and resolved quests can also
rule a seed out early. These shortcuts are exact. The
optional top-level `"fast_mode": true` adds one lossy shortcut: +3 weapon/armor
requirements consider only Ghost and Blacksmith rewards, skipping the far rarer
Crypt and Sacrificial-fire prizes, so those searches end at floor 14. Fast-mode
matches are always genuine; some exotic seeds are simply not reported.

The same file can be used for a finite benchmark:

```sh
cargo run --release -p shpd-seedfinder-cli -- --items requirements.json --workers 4
cargo run --release -p shpd-seedfinder-cli -- -i requirements.json -b 1000 --workers 4
```

## Licensing and identity

This project is GPL-3.0-or-later. It contains a derived generation
implementation and an unchanged item sprite atlas from Shattered Pixel Dungeon,
which is also GPL-licensed. Copyright notices and the full license are included
with the Android distribution.

Seed Seeker is unofficial and is not endorsed by Shattered Pixel Dungeon or its
developers. It uses a distinct package, name, icon, and UI; no game UI components
are reused.

- Pixel Dungeon © 2012–2015 Oleg Dolya / Watabou
- Shattered Pixel Dungeon © 2014–2026 Evan Debenham
