#!/usr/bin/env bash
set -euo pipefail

ORACLE_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PYTHON=${PYTHON:-python3}
EXPECTED="$ORACLE_DIR/tests/halls-floors.expected.json"

if [[ -n "${JAVA_21_HOME:-}" ]]; then
    export JAVA_HOME="$JAVA_21_HOME"
elif [[ -x /usr/libexec/java_home ]]; then
    export JAVA_HOME=$(/usr/libexec/java_home -v 21)
elif [[ -z "${JAVA_HOME:-}" ]]; then
    echo "Halls fixture requires JDK 21; set JAVA_21_HOME" >&2
    exit 1
fi

AAA=$(mktemp "${TMPDIR:-/tmp}/shpd-halls-aaa.XXXXXX")
ONE=$(mktemp "${TMPDIR:-/tmp}/shpd-halls-one.XXXXXX")
ABC=$(mktemp "${TMPDIR:-/tmp}/shpd-halls-abc.XXXXXX")
MAX=$(mktemp "${TMPDIR:-/tmp}/shpd-halls-max.XXXXXX")
AIC=$(mktemp "${TMPDIR:-/tmp}/shpd-halls-aic.XXXXXX")
trap 'rm -f "$AAA" "$ONE" "$ABC" "$MAX" "$AIC"' EXIT

"$ORACLE_DIR/run.sh" --seed AAA-AAA-AAA --floors 21-24 \
    --format json --run-checkpoints >"$AAA"
"$ORACLE_DIR/run.sh" --seed AAA-AAA-AAB --floors 21 \
    --format json --run-checkpoints >"$ONE"
"$ORACLE_DIR/run.sh" --seed ABC-DEF-GHI --floors 21 \
    --format json --run-checkpoints >"$ABC"
"$ORACLE_DIR/run.sh" --seed ZZZ-ZZZ-ZZZ --floors 21 \
    --format json --run-checkpoints >"$MAX"
"$ORACLE_DIR/run.sh" --seed AAA-AAA-AIC --floors 22 \
    --format json --run-checkpoints >"$AIC"

MODE="$EXPECTED"
if [[ "${1:-}" == "--print" ]]; then
    MODE=--print
fi

"$PYTHON" "$ORACLE_DIR/tests/assert_halls.py" "$MODE" \
    "AAA-AAA-AAA=$AAA" \
    "AAA-AAA-AAB=$ONE" \
    "ABC-DEF-GHI=$ABC" \
    "ZZZ-ZZZ-ZZZ=$MAX" \
    "AAA-AAA-AIC=$AIC"

if [[ "$MODE" != "--print" ]]; then
    echo "Halls floors 21-24 official parity fixtures passed"
fi
