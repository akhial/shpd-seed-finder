#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Fails when pgo/seed-seeker-$TARGET.profdata no longer matches the build that
# consumes it. rustc drops profile entries whose mangled name it cannot find
# and says nothing, so a stale profile costs throughput invisibly: before this
# check existed the checked-in profile had been recorded under legacy symbol
# mangling and matched not one of the engine's hot functions.
#
# Set PGO_TARGET to check another target's profile. Re-record with
# scripts/record-pgo-profile.sh when this fails.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${PGO_TARGET:-aarch64-apple-darwin}"
PROFILE="$ROOT/pgo/seed-seeker-$TARGET.profdata"

# The self time leaders of a depth-19 search, which is the workload the app
# spends its life on. Every one of them must carry profile data.
HOT_FUNCTIONS='grid_builder10build_grid
builder15place_room_impl
painter14generate_patch
maze9grow_maze
caves_floor17paint_caves_floor
city_floor16paint_city_floor'

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

RUSTFLAGS="-Cprofile-use=$PROFILE -Cllvm-args=-pgo-warn-missing-function" \
    CARGO_TARGET_DIR="$WORK/target" \
    cargo build --locked --release --target "$TARGET" \
        -p shpd-seedfinder-ffi --manifest-path "$ROOT/Cargo.toml" \
        2> "$WORK/warnings.txt" > /dev/null

missing=0
while IFS= read -r function; do
    if grep -q "no profile data available for function .*$function\b" "$WORK/warnings.txt"; then
        echo "check-pgo-profile: no profile data for $function" >&2
        missing=$((missing + 1))
    fi
done <<< "$HOT_FUNCTIONS"

if [ "$missing" -ne 0 ]; then
    echo >&2
    echo "check-pgo-profile: $missing hot function(s) missing from $PROFILE." >&2
    echo "The profile is stale or was recorded from a differently shaped build." >&2
    echo "Re-record it with: bash scripts/record-pgo-profile.sh" >&2
    exit 1
fi

echo "check-pgo-profile: all $(echo "$HOT_FUNCTIONS" | wc -l | tr -d ' ') hot functions carry profile data."
