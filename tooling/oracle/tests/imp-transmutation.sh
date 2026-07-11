#!/usr/bin/env bash
set -euo pipefail

ORACLE_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
EXPECTED="$ORACLE_DIR/tests/imp-transmutation.expected.json"
ACTUAL=$(mktemp "${TMPDIR:-/tmp}/shpd-imp-transmutation.XXXXXX")
trap 'rm -f "$ACTUAL"' EXIT

"$ORACLE_DIR/run.sh" --seed AAA-AAA-AAF --floors 17 --format json \
    --no-phases --transmute-imp >"$ACTUAL"
python3 "$ORACLE_DIR/tests/assert_imp_transmutation.py" "$EXPECTED" "$ACTUAL"

echo "AAA-AAA-AAF +4 Imp ring transmutation oracle fixture passed"
