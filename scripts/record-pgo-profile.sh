#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Re-records pgo/seed-seeker.profdata, the profile scripts/build-macos-native.sh
# feeds to `-Cprofile-use`.
#
# The profile has to be re-recorded whenever the engine changes shape, and it
# rots *silently*: rustc keys profile entries on the mangled symbol name, which
# carries both the mangling version and the crate disambiguator cargo derives
# from the unit graph. A profile recorded from a differently shaped build (a
# host build instead of a `--target` one, an older toolchain with legacy
# mangling, a `-p shpd-seedfinder-cli`-only build) matches nothing at all and
# just gets dropped without an error. So the instrumented build here uses the
# same target and the same package set as the consuming build, and the last
# step re-runs that consuming build to prove the hot functions were found.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${PGO_TARGET:-aarch64-apple-darwin}"
PROFILE="$ROOT/pgo/seed-seeker.profdata"

LLVM_BIN="$(rustc --print target-libdir)/../bin"
PROFDATA="$LLVM_BIN/llvm-profdata"
if [ ! -x "$PROFDATA" ]; then
    echo "record-pgo-profile: $PROFDATA is missing." >&2
    echo "Install it with: rustup component add llvm-tools" >&2
    exit 1
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
RAW="$WORK/raw"
mkdir -p "$RAW"

# `-p shpd-seedfinder-ffi` is in the instrumented build only so that
# shpd-seedfinder-core is compiled as the same cargo unit the macOS build
# compiles it as; the workload below runs through the CLI binary.
echo "==> building the instrumented binary"
RUSTFLAGS="-Cprofile-generate=$RAW" \
    CARGO_TARGET_DIR="$WORK/target" \
    cargo build --locked --release --target "$TARGET" \
        -p shpd-seedfinder-cli -p shpd-seedfinder-ffi \
        --manifest-path "$ROOT/Cargo.toml"

SEEKER="$WORK/target/$TARGET/release/seed-seeker"

# The training set has to cover the shapes real searches take, because a
# function the workload never reaches gets no profile at all. The canonical
# benchmark only reaches depth 9 (the Wandmaker window), so on its own it never
# trains the caves, the city, or the Imp's Vault — the most expensive level in
# the game.
echo "==> recording the training workload"
cat > "$WORK/deep-wand.json" <<'JSON'
{"max_depth":19,"requirements":[{"kind":"wand","upgrade":2}]}
JSON
cat > "$WORK/imp-ring.json" <<'JSON'
{"max_depth":19,"requirements":[{"kind":"ring","upgrade":4}]}
JSON
cat > "$WORK/halls-armor.json" <<'JSON'
{"max_depth":24,"requirements":[{"kind":"armor","upgrade":3,"effect":"any_enchantment"}]}
JSON

# One worker: the IR counters are not atomic, so a multi-threaded run would
# record slightly different numbers on every pass for no extra coverage.
"$SEEKER" --benchmark 4000 --workers 1 > /dev/null
"$SEEKER" --items "$WORK/deep-wand.json" --benchmark 1600 --workers 1 > /dev/null
"$SEEKER" --items "$WORK/imp-ring.json" --benchmark 1600 --workers 1 > /dev/null
"$SEEKER" --items "$WORK/halls-armor.json" --benchmark 1000 --workers 1 > /dev/null

echo "==> merging into $PROFILE"
"$PROFDATA" merge -o "$PROFILE" "$RAW"/*.profraw

echo "==> verifying the profile against the shipped build"
bash "$ROOT/scripts/check-pgo-profile.sh"
