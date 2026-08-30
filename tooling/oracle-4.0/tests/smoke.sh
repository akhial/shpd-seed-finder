#!/usr/bin/env bash
set -euo pipefail

ORACLE_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PYTHON=${PYTHON:-python3}
FIRST=$(mktemp "${TMPDIR:-/tmp}/shpd-oracle-first.XXXXXX")
SECOND=$(mktemp "${TMPDIR:-/tmp}/shpd-oracle-second.XXXXXX")
JSON_OUTPUT=$(mktemp "${TMPDIR:-/tmp}/shpd-oracle-json.XXXXXX")
trap 'rm -f "$FIRST" "$SECOND" "$JSON_OUTPUT"' EXIT

"$ORACLE_DIR/run.sh" --seed AAA-AAA-AAA --floors 1 --format ndjson >"$FIRST"
"$ORACLE_DIR/run.sh" --seed AAA-AAA-AAA --floors 1 --format ndjson >"$SECOND"
cmp "$FIRST" "$SECOND"

"$ORACLE_DIR/run.sh" --seed AAA-AAA-AAA --floors 1 --format json >"$JSON_OUTPUT"

MODE="$ORACLE_DIR/tests/aaa-aaa-aaa-floor1.expected.json"
if [[ "${1:-}" == "--print" ]]; then
    MODE=--print
fi
"$PYTHON" "$ORACLE_DIR/tests/assert_smoke.py" "$MODE" "$FIRST" "$JSON_OUTPUT"

if [[ "$MODE" != "--print" ]]; then
    echo "AAA-AAA-AAA floor 1 oracle smoke test passed"
fi
