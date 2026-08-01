#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Builds the JNI library for the *host* so the Android JVM unit tests can load
# the real engine. The refine continuation predicate is single-sourced in Rust
# (SearchQuery::continues, exported as JniBindings.queryContinues), so a
# Kotlin-only test of it would assert nothing about the shipped behaviour.
# Nothing here goes into an APK — scripts/build-android-native.sh does that.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
OUTPUT=${1:-"$ROOT/android/app/build/generated/hostJni"}

case "$(uname -s)" in
    Darwin) LIBRARY=libshpd_seedfinder.dylib ;;
    Linux) LIBRARY=libshpd_seedfinder.so ;;
    *) echo "Unsupported host for the JNI unit-test library" >&2; exit 1 ;;
esac

cd "$ROOT"
# The dev profile keeps this cheap: the tests decode two query packets and
# never search, so nothing here is performance sensitive.
cargo build --locked -p shpd-seedfinder-jni

TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target}
mkdir -p "$OUTPUT"
cp "$TARGET_DIR/debug/$LIBRARY" "$OUTPUT/$LIBRARY"
