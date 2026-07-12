#!/usr/bin/env python3
"""Reduce official-oracle boss transitions to a stable skip-safety fixture."""

import json
import pathlib
import sys


FIXTURE_SCHEMA = "shpd-boss-transition-fixture/v2"
NEUTRAL_BOSSES = {5, 10, 15, 25}


def simple_name(value):
    return None if value is None else value.rsplit(".", 1)[-1]


def load_document(path):
    document = json.loads(pathlib.Path(path).read_text(encoding="utf-8"))
    assert document["schema"] == "shpd-parity-oracle/v1"
    return document


def summarize_transition(record):
    unchanged = {
        name: record[f"{name}_unchanged"]
        for name in ("generator", "limited_drops", "quests", "room_queues", "shop_state")
    }
    items = [
        [
            item["choice"], item["simple_class"], item["kind"], item["true_level"],
            item["cursed"], item["quantity"],
            simple_name(item["enchantment"] or item["glyph"]),
        ]
        for item in record["initial_searchable_items"]
    ]

    depth = record["depth"]
    if depth in NEUTRAL_BOSSES:
        assert all(unchanged.values()), (depth, unchanged)
        assert not items, (depth, items)
    elif depth == 20:
        assert unchanged["generator"] is False
        assert all(value for name, value in unchanged.items() if name != "generator")
        assert len(items) >= 4
    else:
        raise AssertionError(f"unexpected boss depth {depth}")

    return {
        "depth": depth,
        "class": simple_name(record["level_class"]),
        "generator_hash": [record["generator_hash_before"], record["generator_hash_after"]],
        "unchanged": unchanged,
        "items": items,
    }


def fixture(documents):
    first_run = documents[0]["records"][0]
    result = {
        "schema": FIXTURE_SCHEMA,
        "game_version": first_run["game_version"],
        "game_commit": first_run["game_commit"],
        "runtime": first_run["runtime"],
        "challenges": first_run["challenges"],
        "item_columns": [
            "choice", "class", "kind", "upgrade", "cursed", "quantity", "effect"
        ],
        "seeds": {},
    }
    for document in documents:
        run = document["records"][0]
        assert run["game_version"] == result["game_version"]
        assert run["game_commit"] == result["game_commit"]
        assert run["runtime"] == result["runtime"]
        assert run["challenges"] == result["challenges"]
        transitions = [
            summarize_transition(record)
            for record in document["records"]
            if record["record"] == "boss_transition"
        ]
        assert [value["depth"] for value in transitions] == [5, 10, 15, 20, 25]
        result["seeds"][run["seed_code"]] = transitions
    return result


def main():
    if len(sys.argv) < 3:
        raise SystemExit(
            "usage: assert_boss_skips.py EXPECTED SEED=ORACLE_JSON...\n"
            "       assert_boss_skips.py --print SEED=ORACLE_JSON..."
        )

    printing = sys.argv[1] == "--print"
    expected_path = None if printing else sys.argv[1]
    documents = []
    for spec in sys.argv[2:]:
        expected_seed, separator, path = spec.partition("=")
        if not separator:
            raise AssertionError(f"missing seed prefix in {spec!r}")
        document = load_document(path)
        actual_seed = document["records"][0]["seed_code"]
        assert actual_seed == expected_seed, (actual_seed, expected_seed)
        documents.append(document)

    actual = fixture(documents)
    if printing:
        json.dump(actual, sys.stdout, indent=2)
        print()
        return

    expected = json.loads(pathlib.Path(expected_path).read_text(encoding="utf-8"))
    assert actual == expected


if __name__ == "__main__":
    main()
