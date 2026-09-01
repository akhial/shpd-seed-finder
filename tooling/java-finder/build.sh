#!/usr/bin/env bash
# Builds the Java baseline seed finder against the official Shattered Pixel
# Dungeon v4.0.0-BETA-3 desktop JAR.  No game source is compiled: the finder
# and the parity oracle's headless TextureFilm stand-in are compiled into
# .work/classes and placed ahead of the JAR on the classpath by run.sh.
set -euo pipefail

FINDER_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ORACLE_DIR="$FINDER_DIR/../oracle-4.0"
WORK="$FINDER_DIR/.work"
JAR_NAME=ShatteredPD-v4.0.0-BETA-3-Java.jar
JAR_URL="https://github.com/00-Evan/shattered-pixel-dungeon/releases/download/4.0.0-beta/$JAR_NAME"
JAR_SHA256=f62f8ac2ef6d36c72223c1a4e78f18e98d0bb1282cd4f1fca123082d43edccc9
JAR="$WORK/$JAR_NAME"
CLASSES="$WORK/classes"
# The stand-in is shared with the oracle rather than copied: the two have to
# agree, and one marked copy of an upstream-derived class is enough.
SHADOW="$ORACLE_DIR/src/com/watabou/noosa/TextureFilm.java"

mkdir -p "$WORK"

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    else
        openssl dgst -sha256 "$1" | sed 's/^.*= //'
    fi
}

if [[ ! -f "$JAR" ]]; then
    # The oracle pins the same artifact; reuse its download when there is one.
    if [[ -f "$ORACLE_DIR/.work/$JAR_NAME" ]]; then
        cp "$ORACLE_DIR/.work/$JAR_NAME" "$JAR"
    else
        echo "java-finder: downloading $JAR_URL" >&2
        curl -fsSL --retry 3 -o "$JAR.part" "$JAR_URL"
        mv "$JAR.part" "$JAR"
    fi
fi

ACTUAL_SHA256=$(sha256_of "$JAR")
if [[ "$ACTUAL_SHA256" != "$JAR_SHA256" ]]; then
    echo "java-finder: $JAR has sha256 $ACTUAL_SHA256, expected $JAR_SHA256" >&2
    echo "java-finder: delete the file and rerun build.sh to download the pinned JAR" >&2
    exit 1
fi

if [[ ! -f "$SHADOW" ]]; then
    echo "java-finder: missing the headless stand-in $SHADOW" >&2
    exit 1
fi

# JDK selection: JAVA_21_HOME, then JAVA_HOME, then PATH.  The published
# numbers were measured with JDK 21.
if [[ -n "${JAVA_21_HOME:-}" ]]; then
    JAVAC="$JAVA_21_HOME/bin/javac"
elif [[ -n "${JAVA_HOME:-}" ]]; then
    JAVAC="$JAVA_HOME/bin/javac"
else
    JAVAC=javac
fi

SOURCES=("$SHADOW")
while IFS= read -r -d '' source; do
    SOURCES+=("$source")
done < <(find "$FINDER_DIR/src" -name '*.java' -print0 | sort -z)

# The compiled tree is keyed by the source and script contents so that stale
# classes are never reused after an edit.
REVISION=$({ "$JAVAC" -version 2>&1; cat "${SOURCES[@]}" "$FINDER_DIR/build.sh"; } | sha256_of /dev/stdin)
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
