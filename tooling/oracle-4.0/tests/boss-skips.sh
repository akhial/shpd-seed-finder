#!/usr/bin/env bash
set -euo pipefail

ORACLE_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PYTHON=${PYTHON:-python3}
EXPECTED="$ORACLE_DIR/tests/boss-skips.expected.json"

if [[ -n "${JAVA_21_HOME:-}" ]]; then
    export JAVA_HOME="$JAVA_21_HOME"
elif [[ -x /usr/libexec/java_home ]]; then
    export JAVA_HOME=$(/usr/libexec/java_home -v 21)
elif [[ -z "${JAVA_HOME:-}" ]]; then
    echo "boss-transition fixture requires JDK 21; set JAVA_21_HOME" >&2
    exit 1
fi

AAA=$(mktemp "${TMPDIR:-/tmp}/shpd-boss-aaa.XXXXXX")
ONE=$(mktemp "${TMPDIR:-/tmp}/shpd-boss-one.XXXXXX")
ABC=$(mktemp "${TMPDIR:-/tmp}/shpd-boss-abc.XXXXXX")
trap 'rm -f "$AAA" "$ONE" "$ABC"' EXIT

for spec in "AAA-AAA-AAA=$AAA" "AAA-AAA-AAB=$ONE" "ABC-DEF-GHI=$ABC"; do
    seed=${spec%%=*}
    path=${spec#*=}
    "$ORACLE_DIR/run.sh" --seed "$seed" --floors 25 --format json \
        --boss-skip-checkpoints >"$path"
done

MODE="$EXPECTED"
if [[ "${1:-}" == "--print" ]]; then
    MODE=--print
fi

"$PYTHON" "$ORACLE_DIR/tests/assert_boss_skips.py" "$MODE" \
    "AAA-AAA-AAA=$AAA" \
    "AAA-AAA-AAB=$ONE" \
    "ABC-DEF-GHI=$ABC"

if [[ "$MODE" != "--print" ]]; then
    echo "Boss transition official parity fixtures passed"
fi
