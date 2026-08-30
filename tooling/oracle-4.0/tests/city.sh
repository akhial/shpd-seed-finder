#!/usr/bin/env bash
set -euo pipefail

ORACLE_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PYTHON=${PYTHON:-python3}
EXPECTED="$ORACLE_DIR/tests/city-floors.expected.json"

if [[ -n "${JAVA_21_HOME:-}" ]]; then
    export JAVA_HOME="$JAVA_21_HOME"
elif [[ -x /usr/libexec/java_home ]]; then
    export JAVA_HOME=$(/usr/libexec/java_home -v 21)
elif [[ -z "${JAVA_HOME:-}" ]]; then
    echo "City fixture requires JDK 21; set JAVA_21_HOME" >&2
    exit 1
fi

AAA=$(mktemp "${TMPDIR:-/tmp}/shpd-city-aaa.XXXXXX")
ONE=$(mktemp "${TMPDIR:-/tmp}/shpd-city-one.XXXXXX")
ABC=$(mktemp "${TMPDIR:-/tmp}/shpd-city-abc.XXXXXX")
MAX=$(mktemp "${TMPDIR:-/tmp}/shpd-city-max.XXXXXX")
trap 'rm -f "$AAA" "$ONE" "$ABC" "$MAX"' EXIT

"$ORACLE_DIR/run.sh" --seed AAA-AAA-AAA --floors 16-19 \
    --format json --run-checkpoints >"$AAA"
"$ORACLE_DIR/run.sh" --seed AAA-AAA-AAB --floors 16 \
    --format json --run-checkpoints >"$ONE"
"$ORACLE_DIR/run.sh" --seed ABC-DEF-GHI --floors 16 \
    --format json --run-checkpoints >"$ABC"
"$ORACLE_DIR/run.sh" --seed ZZZ-ZZZ-ZZZ --floors 16 \
    --format json --run-checkpoints >"$MAX"

MODE="$EXPECTED"
if [[ "${1:-}" == "--print" ]]; then
    MODE=--print
fi

"$PYTHON" "$ORACLE_DIR/tests/assert_city.py" "$MODE" \
    "AAA-AAA-AAA=$AAA" \
    "AAA-AAA-AAB=$ONE" \
    "ABC-DEF-GHI=$ABC" \
    "ZZZ-ZZZ-ZZZ=$MAX"

if [[ "$MODE" != "--print" ]]; then
    echo "City floors 16-19 official parity fixtures passed"
fi
