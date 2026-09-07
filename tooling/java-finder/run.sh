#!/usr/bin/env bash
# Runs the Java baseline seed finder.  The compiled classes come FIRST on the
# classpath so that the headless TextureFilm stand-in shadows the JAR's copy;
# everything else is loaded from the unmodified official JAR.
set -euo pipefail

FINDER_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
JAR="$FINDER_DIR/.work/ShatteredPD-v4.0.0-BETA-4-Java.jar"
CLASSES=$("$FINDER_DIR/build.sh")

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
        if command -v cygpath >/dev/null 2>&1; then
            CLASSES=$(cygpath -w "$CLASSES")
            JAR=$(cygpath -w "$JAR")
        fi
        ;;
    *) SEP=':' ;;
esac

exec "$JAVA" ${JAVA_FINDER_OPTS:-} -cp "$CLASSES$SEP$JAR" \
    com.shatteredpixel.shatteredpixeldungeon.JarSeedFinder "$@"
