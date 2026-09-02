#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROFILE="$ROOT/pgo/seed-seeker-aarch64-apple-darwin.profdata"

# The checked-in profile is recorded by scripts/record-pgo-profile.sh and
# improves the seed search on top of the source-level optimizations. Resolve it
# here because rustc evaluates profile-use paths from each dependency's source
# directory, not consistently from the Cargo workspace.
#
# The profile only applies if its mangled symbol names match the ones this
# command produces, and rustc says nothing when they do not, so the shape of
# this invocation — the target, the profile, the package set — is load bearing.
RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-Cprofile-use=$PROFILE" \
    cargo build --locked --release --target aarch64-apple-darwin \
        -p shpd-seedfinder-ffi --manifest-path "$ROOT/Cargo.toml"
