#!/usr/bin/env bash
# Builds the Shattered Pixel Dungeon v4.0.0-BETA-4 parity oracle against the
# official desktop JAR.  No game source is compiled: the oracle plus one small
# headless shadow class are compiled into .work/classes and placed ahead of the
# JAR on the classpath by run.sh.
set -euo pipefail

ORACLE_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WORK="$ORACLE_DIR/.work"
JAR_NAME=ShatteredPD-v4.0.0-BETA-4-Java.jar
JAR_URL="https://github.com/00-Evan/shattered-pixel-dungeon/releases/download/4.0.0-beta/$JAR_NAME"
JAR_SHA256=76f6983e7b619267666621de9f1ecbbc3645d4925c2c446736987c3011b9dfd1
JAR="$WORK/$JAR_NAME"
CLASSES="$WORK/classes"

mkdir -p "$WORK"

if [[ ! -f "$JAR" ]]; then
    echo "parity-oracle: downloading $JAR_URL" >&2
    curl -fsSL --retry 3 -o "$JAR.part" "$JAR_URL"
    mv "$JAR.part" "$JAR"
fi

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    else
        openssl dgst -sha256 "$1" | sed 's/^.*= //'
    fi
}

ACTUAL_SHA256=$(sha256_of "$JAR")
if [[ "$ACTUAL_SHA256" != "$JAR_SHA256" ]]; then
    echo "parity-oracle: $JAR has sha256 $ACTUAL_SHA256, expected $JAR_SHA256" >&2
    echo "parity-oracle: delete the file and rerun build.sh to download the pinned JAR" >&2
    exit 1
fi

# JDK selection: JAVA_21_HOME, then JAVA_HOME, then PATH.  Fixtures are pinned
# with JDK 21.
if [[ -n "${JAVA_21_HOME:-}" ]]; then
    JAVAC="$JAVA_21_HOME/bin/javac"
elif [[ -n "${JAVA_HOME:-}" ]]; then
    JAVAC="$JAVA_HOME/bin/javac"
else
    JAVAC=javac
fi

SOURCES=()
while IFS= read -r -d '' source; do
    SOURCES+=("$source")
done < <(find "$ORACLE_DIR/src" -name '*.java' -print0 | sort -z)

# The compiled tree is keyed by the source and script contents so that stale
# classes are never reused after an edit.
REVISION=$({ "$JAVAC" -version 2>&1; cat "${SOURCES[@]}" "$ORACLE_DIR/build.sh"; } | sha256_of /dev/stdin)
STAMP="$CLASSES/.revision"
if [[ -f "$STAMP" && "$(cat "$STAMP")" == "$REVISION" ]]; then
    echo "$CLASSES"
    exit 0
fi

rm -rf "$CLASSES"
mkdir -p "$CLASSES"
"$JAVAC" -nowarn -encoding UTF-8 -d "$CLASSES" -cp "$JAR" "${SOURCES[@]}"
echo "$REVISION" >"$STAMP"

echo "$CLASSES"
