#!/usr/bin/env python3
import json
import pathlib
import sys


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


def main():
    if len(sys.argv) != 4:
        raise SystemExit("usage: assert_smoke.py EXPECTED NDJSON JSON")

    expected = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
    records = load_ndjson(pathlib.Path(sys.argv[2]))
    document = json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))

    assert document["schema"] == "shpd-parity-oracle/v1"
    assert document["records"] == records

    run_init = one(records, "run_init")
    level = one(records, "level")
    assert run_init["game_version"] == "3.3.8"
    assert run_init["game_commit"] == "7b8b845a76fe76c6b7c031ae9e570852411f56db"
    assert run_init["seed_code"] == "AAA-AAA-AAA"
    assert run_init["seed"] == 0
    assert run_init["challenges"] == 0
    assert run_init["requested_depths"] == [1]
    assert level["depth"] == 1

    for key, value in expected["level"].items():
        assert level[key] == value, (key, level[key], value)

    item_records = [record for record in records if record.get("record") == "item"]
    assert len(item_records) == expected["item_record_count"]
    for item in item_records:
        for field in (
            "class", "true_level", "cursed", "enchantment", "glyph",
            "depth", "source", "choice", "searchable", "accessibility",
        ):
            assert field in item, (field, item)

    fingerprint = {
        "classes": sorted(item["class"] for item in item_records),
        "searchable": sorted([
            [item["source"], item["class"], item["true_level"], item["enchantment"], item["glyph"]]
            for item in item_records if item["searchable"]
        ]),
    }
    assert fingerprint == expected["items"]


if __name__ == "__main__":
    main()
