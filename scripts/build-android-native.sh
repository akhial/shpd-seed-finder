#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
OUTPUT=${1:-"$ROOT/android/app/build/generated/jniLibs"}
# Space-separated ABI list. Releases ship both; pull-request CI sets a single
# ABI to halve the native build.
ABIS=${ANDROID_ABIS:-"arm64-v8a x86_64"}

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
# Drop ABIs left over from a previous, wider run so the packaged set is
# exactly $ABIS.
rm -rf "$OUTPUT"

# rustc 1.94/LLVM 21.1.8 miscompiles the deterministic generator at O3 on
# Android AArch64 (the scalar seed-1 City prefix diverges). O2 is parity-clean
# on device, including with the workspace's fat LTO, and is used for both
# shipped ABIs so one Android build policy governs canonical results.
for ABI in $ABIS; do
    case "$ABI" in
        arm64-v8a)
            TRIPLE=aarch64-linux-android
            LINKER=CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER
            CLANG=aarch64-linux-android21-clang
            ;;
        x86_64)
            TRIPLE=x86_64-linux-android
            LINKER=CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER
            CLANG=x86_64-linux-android21-clang
            ;;
        *)
            echo "Unknown Android ABI: $ABI" >&2
            exit 1
            ;;
    esac
    env CARGO_PROFILE_RELEASE_OPT_LEVEL=2 \
        "$LINKER=$TOOLCHAIN/$CLANG" \
        cargo build --locked --release -p shpd-seedfinder-jni --target "$TRIPLE"
    mkdir -p "$OUTPUT/$ABI"
    cp "$ROOT/target/$TRIPLE/release/libshpd_seedfinder.so" \
        "$OUTPUT/$ABI/libshpd_seedfinder.so"
done
