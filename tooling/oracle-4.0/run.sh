#!/usr/bin/env bash
# Runs the v4.0.0-RC-1 parity oracle.  The compiled oracle/shadow classes come
# FIRST on the classpath so that the headless TextureFilm stand-in shadows the
# JAR's copy; everything else is loaded from the unmodified official JAR.
set -euo pipefail

ORACLE_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WORK="$ORACLE_DIR/.work"
JAR="$WORK/ShatteredPD-v4.0.0-RC-1-Java.jar"
CLASSES=$("$ORACLE_DIR/build.sh")

if [[ -n "${JAVA_21_HOME:-}" ]]; then
    JAVA="$JAVA_21_HOME/bin/java"
elif [[ -n "${JAVA_HOME:-}" ]]; then
    JAVA="$JAVA_HOME/bin/java"
else
    JAVA=java
fi

case "$(uname -s 2>/dev/null || echo "${OSTYPE:-}")" in
    MINGW*|MSYS*|CYGWIN*|Windows*)
        SEP=';'
        # A Windows JVM needs native paths; MSYS does not rewrite ';'-joined lists.
        if command -v cygpath >/dev/null 2>&1; then
            CLASSES=$(cygpath -w "$CLASSES")
            JAR=$(cygpath -w "$JAR")
        fi
        ;;
    *) SEP=':' ;;
esac

exec "$JAVA" -cp "$CLASSES$SEP$JAR" \
    com.shatteredpixel.shatteredpixeldungeon.ParityOracle "$@"
