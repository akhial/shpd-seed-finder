#!/usr/bin/env python3
"""Reduce official-oracle challenge runs to compact generation fixtures."""

import json
import pathlib
import sys


def simple_name(value):
    return None if value is None else value.rsplit(".", 1)[-1]


def summarize(path):
    document = json.loads(pathlib.Path(path).read_text(encoding="utf-8"))
    assert document["schema"] == "shpd-parity-oracle/v1"
    records = document["records"]
    run = next(record for record in records if record["record"] == "run_init")
    return run, {
        "seed": run["seed"],
        "challenges": run["challenges"],
        "requested_depths": run["requested_depths"],
        "item_phases": [
            [record["depth"], record["map_hash"], record["mob_count"],
             record["heap_count"], record["generator_state_hash"]]
            for record in records
            if record["record"] == "level_phase" and record["phase"] == "items"
        ],
        "items": [
            [record["depth"], record["source"], record["cell"],
             record["simple_class"], record["true_level"], record["cursed"],
             simple_name(record["enchantment"] or record["glyph"])]
            for record in records if record["record"] == "item"
        ],
    }


def main():
    if len(sys.argv) < 3:
        raise SystemExit("usage: assert_challenges.py EXPECTED LABEL=ORACLE_JSON...")
    printing = sys.argv[1] == "--print"
    result = {
        "schema": "shpd-challenge-parity-fixture/v1",
        "item_phase_columns": [
            "depth", "map_hash", "mob_count", "heap_count", "generator_hash"
        ],
        "item_columns": [
            "depth", "source", "cell", "class", "upgrade", "cursed", "effect"
        ],
        "runs": {},
    }
    provenance = None
    for spec in sys.argv[2:]:
        label, separator, path = spec.partition("=")
        assert separator, spec
        run, summary = summarize(path)
        current = [run["game_version"], run["game_commit"], run["runtime"]]
        provenance = current if provenance is None else provenance
        assert current == provenance
        result["runs"][label] = summary
    result["game_version"], result["game_commit"], result["runtime"] = provenance
    if printing:
        json.dump(result, sys.stdout, indent=2)
        print()
    else:
        expected = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
        assert result == expected


if __name__ == "__main__":
    main()
