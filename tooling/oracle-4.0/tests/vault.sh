#!/usr/bin/env bash
# Pins the v4.0.0 Imp quest reward options and the full Imp's Vault (branch 1)
# for one seed per possible Imp floor: 17 (AAA-AAA-AAC), 18 (AAA-AAA-AAB) and
# 19 (AAA-AAA-AAA).
set -euo pipefail

ORACLE_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PYTHON=${PYTHON:-python3}
EXPECTED="$ORACLE_DIR/tests/vault.expected.json"

if [[ -n "${JAVA_21_HOME:-}" ]]; then
    export JAVA_HOME="$JAVA_21_HOME"
elif [[ -x /usr/libexec/java_home ]]; then
    export JAVA_HOME=$(/usr/libexec/java_home -v 21)
elif [[ -z "${JAVA_HOME:-}" ]]; then
    echo "Vault fixture requires JDK 21; set JAVA_21_HOME" >&2
    exit 1
fi

D17=$(mktemp "${TMPDIR:-/tmp}/shpd-vault-17.XXXXXX")
D18=$(mktemp "${TMPDIR:-/tmp}/shpd-vault-18.XXXXXX")
D19=$(mktemp "${TMPDIR:-/tmp}/shpd-vault-19.XXXXXX")
trap 'rm -f "$D17" "$D18" "$D19"' EXIT

"$ORACLE_DIR/run.sh" --seed AAA-AAA-AAC --floors 17-19 \
    --format json --run-checkpoints --vault >"$D17"
"$ORACLE_DIR/run.sh" --seed AAA-AAA-AAB --floors 17-19 \
    --format json --run-checkpoints --vault >"$D18"
"$ORACLE_DIR/run.sh" --seed AAA-AAA-AAA --floors 17-19 \
    --format json --run-checkpoints --vault >"$D19"

MODE="$EXPECTED"
if [[ "${1:-}" == "--print" ]]; then
    MODE=--print
fi

"$PYTHON" "$ORACLE_DIR/tests/assert_vault.py" "$MODE" \
    "AAA-AAA-AAC/17=$D17" \
    "AAA-AAA-AAB/18=$D18" \
    "AAA-AAA-AAA/19=$D19"

if [[ "$MODE" != "--print" ]]; then
    echo "Imp quest and Vault official parity fixtures passed"
fi
