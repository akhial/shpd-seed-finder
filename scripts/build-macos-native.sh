#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROFILE="$ROOT/pgo/seed-seeker.profdata"

# The checked-in profile was recorded from `seed-seeker --benchmark` and
# improves the seed search on top of the source-level optimizations. Resolve it
# here because rustc evaluates profile-use paths from each dependency's source
# directory, not consistently from the Cargo workspace.
RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-Cprofile-use=$PROFILE" \
    cargo build --locked --release --target aarch64-apple-darwin \
        -p shpd-seedfinder-ffi --manifest-path "$ROOT/Cargo.toml"
