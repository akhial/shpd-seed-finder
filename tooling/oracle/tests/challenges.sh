#!/usr/bin/env bash
set -euo pipefail

ORACLE_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
EXPECTED="$ORACLE_DIR/tests/challenges.expected.json"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/shpd-challenges.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

if [[ -n "${JAVA_21_HOME:-}" ]]; then
    export JAVA_HOME="$JAVA_21_HOME"
elif [[ -x /usr/libexec/java_home ]]; then
    export JAVA_HOME=$(/usr/libexec/java_home -v 21)
fi

SPECS=()
for seed in AAA-AAA-AAA AAA-AAA-AAF; do
    for mask in 0 8 32 64 104; do
        path="$TMP/$seed-$mask.json"
        "$ORACLE_DIR/run.sh" --seed "$seed" --floors 1-14 --format json \
            --challenges "$mask" >"$path"
        SPECS+=("$seed/$mask=$path")
    done
done

MODE="$EXPECTED"
if [[ "${1:-}" == "--print" ]]; then
    MODE=--print
fi
python3 "$ORACLE_DIR/tests/assert_challenges.py" "$MODE" "${SPECS[@]}"

if [[ "$MODE" != "--print" ]]; then
    echo "Challenge official parity fixtures passed"
fi
