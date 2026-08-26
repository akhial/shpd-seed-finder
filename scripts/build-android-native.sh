#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
OUTPUT=${1:-"$ROOT/android/app/build/generated/jniLibs"}

if [ -n "${ANDROID_NDK_HOME:-}" ]; then
    NDK=$ANDROID_NDK_HOME
elif [ -n "${ANDROID_HOME:-}" ]; then
    NDK=$ANDROID_HOME/ndk/28.2.13676358
elif [ -d "$HOME/Library/Android/sdk/ndk/28.2.13676358" ]; then
    NDK=$HOME/Library/Android/sdk/ndk/28.2.13676358
else
    echo "Set ANDROID_NDK_HOME to Android NDK 28.2.13676358" >&2
    exit 1
fi

case "$(uname -s)" in
    Darwin) HOST=darwin-x86_64 ;;
    Linux) HOST=linux-x86_64 ;;
    *) echo "Unsupported NDK host" >&2; exit 1 ;;
esac

TOOLCHAIN=$NDK/toolchains/llvm/prebuilt/$HOST/bin

cd "$ROOT"
# rustc 1.94/LLVM 21.1.8 miscompiles the deterministic generator at O3 on
# Android AArch64 (the scalar seed-1 City prefix diverges). O2 is parity-clean
# on device, including with the workspace's fat LTO, and is used for both
# shipped ABIs so one Android build policy governs canonical results.
CARGO_PROFILE_RELEASE_OPT_LEVEL=2 \
    CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TOOLCHAIN/aarch64-linux-android21-clang" \
    cargo build --locked --release -p shpd-seedfinder-jni --target aarch64-linux-android
CARGO_PROFILE_RELEASE_OPT_LEVEL=2 \
    CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$TOOLCHAIN/x86_64-linux-android21-clang" \
    cargo build --locked --release -p shpd-seedfinder-jni --target x86_64-linux-android

mkdir -p "$OUTPUT/arm64-v8a" "$OUTPUT/x86_64"
cp "$ROOT/target/aarch64-linux-android/release/libshpd_seedfinder.so" \
    "$OUTPUT/arm64-v8a/libshpd_seedfinder.so"
cp "$ROOT/target/x86_64-linux-android/release/libshpd_seedfinder.so" \
    "$OUTPUT/x86_64/libshpd_seedfinder.so"
