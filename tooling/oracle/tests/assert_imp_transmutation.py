#!/usr/bin/env python3
"""Check the official full-run Imp transmutation fixture."""

import json
import pathlib
import sys


def simple_name(value):
    return value.rsplit(".", 1)[-1]


def main():
    if len(sys.argv) != 3:
        raise SystemExit("usage: assert_imp_transmutation.py EXPECTED ACTUAL")

    printing = sys.argv[1] == "--print"
    expected = None if printing else json.loads(
        pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
    )
    document = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
    assert document["schema"] == "shpd-parity-oracle/v1"
    records = document["records"]
    run = next(record for record in records if record["record"] == "run_init")
    matches = [record for record in records
               if record["record"] == "imp_transmutation"]
    assert len(matches) == 1, len(matches)
    transmutation = matches[0]

    actual = {
        "schema": "shpd-imp-transmutation-fixture/v2",
        "game_commit": run["game_commit"],
        "seed_code": run["seed_code"],
        "seed": run["seed"],
        "challenges": run["challenges"],
        "depth": transmutation["depth"],
        "original_class": simple_name(transmutation["original_class"]),
        "original_true_level": transmutation["original_true_level"],
        "result_class": simple_name(transmutation["result_class"]),
        "result_true_level": transmutation["result_true_level"],
    }
    if printing:
        json.dump(actual, sys.stdout, indent=2)
        print()
    else:
        assert actual == expected, (actual, expected)


if __name__ == "__main__":
    main()
