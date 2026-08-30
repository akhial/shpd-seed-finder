#!/usr/bin/env python3
"""Byte-determinism, JSON/NDJSON equivalence and a pinned floor-1 fingerprint."""

import json
import pathlib
import sys


ORACLE_SCHEMA = "shpd-parity-oracle/v2"
GAME_VERSION = "4.0.0-BETA-3"
GAME_JAR_SHA256 = "f62f8ac2ef6d36c72223c1a4e78f18e98d0bb1282cd4f1fca123082d43edccc9"


def load_ndjson(path: pathlib.Path):
    records = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        try:
            records.append(json.loads(line))
        except json.JSONDecodeError as error:
            raise AssertionError(f"invalid JSON on line {line_number}: {error}") from error
    return records


def one(records, kind):
    matches = [record for record in records if record.get("record") == kind]
    assert len(matches) == 1, (kind, len(matches))
    return matches[0]


def fingerprint(records):
    level = one(records, "level")
    item_records = [record for record in records if record.get("record") == "item"]
    return {
        "level": {
            "level_class": level["level_class"],
            "branch": level["branch"],
            "width": level["width"],
            "height": level["height"],
            "map_hash": level["map_hash"],
        },
        "item_record_count": len(item_records),
        "items": {
            "classes": sorted(item["class"] for item in item_records),
            "searchable": sorted([
                [item["source"], item["class"], item["true_level"], item["enchantment"], item["glyph"]]
                for item in item_records if item["searchable"]
            ]),
        },
    }


def main():
    if len(sys.argv) != 4:
        raise SystemExit("usage: assert_smoke.py EXPECTED|--print NDJSON JSON")

    printing = sys.argv[1] == "--print"
    records = load_ndjson(pathlib.Path(sys.argv[2]))
    document = json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))

    assert document["schema"] == ORACLE_SCHEMA
    assert document["records"] == records
    assert all(record["schema"] == ORACLE_SCHEMA for record in records)
    assert not any(record["record"] == "level_phase" for record in records)

    run_init = one(records, "run_init")
    level = one(records, "level")
    assert run_init["game_version"] == GAME_VERSION
    assert run_init["game_jar_sha256"] == GAME_JAR_SHA256
    assert run_init["effective_game_version"] == GAME_VERSION + "-INDEV"
    assert run_init["seed_code"] == "AAA-AAA-AAA"
    assert run_init["seed"] == 0
    assert run_init["challenges"] == 0
    assert run_init["requested_depths"] == [1]
    assert run_init["vault_requested"] is False
    assert level["depth"] == 1
    assert level["branch"] == 0

    item_records = [record for record in records if record.get("record") == "item"]
    for item in item_records:
        for field in (
            "class", "true_level", "cursed", "enchantment", "glyph",
            "depth", "branch", "source", "choice", "cell", "room",
            "searchable", "accessibility",
        ):
            assert field in item, (field, item)

    actual = fingerprint(records)
    if printing:
        json.dump(actual, sys.stdout, indent=2)
        print()
        return

    expected = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
    for key, value in expected["level"].items():
        assert level[key] == value, (key, level[key], value)
    assert actual == expected, (actual, expected)


if __name__ == "__main__":
    main()
