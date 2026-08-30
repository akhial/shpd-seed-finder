#!/usr/bin/env python3
"""Reduce full official-oracle documents to stable Halls parity fixtures."""

import json
import pathlib
import sys


FIXTURE_SCHEMA = "shpd-halls-parity-fixture/v3"
ORACLE_SCHEMA = "shpd-parity-oracle/v2"


def simple_name(value):
    return None if value is None else value.rsplit(".", 1)[-1]


def load_document(path):
    document = json.loads(pathlib.Path(path).read_text(encoding="utf-8"))
    assert document["schema"] == ORACLE_SCHEMA
    return document


def summarize(document):
    records = document["records"]
    run = next(record for record in records if record["record"] == "run_init")
    assert not any(record["record"] == "level_phase" for record in records)

    levels = []
    for record in records:
        if record["record"] != "level":
            continue
        assert record["branch"] == 0
        levels.append({
            "depth": record["depth"],
            "class": simple_name(record["level_class"]),
            "feeling": record["feeling"],
            "size": [record["width"], record["height"]],
            "map_hash": record["map_hash"],
            "entrance": record["entrance"],
            "exit": record["exit"],
            "generator_state_hash": record["generator_state_hash"],
            "mobs": [[simple_name(mob["class"]), mob["cell"]]
                     for mob in record["mobs"]],
        })

    items = []
    for record in records:
        if record["record"] != "item":
            continue
        assert record["searchable"] is True
        assert record["branch"] == 0
        enchantment = simple_name(record["enchantment"])
        glyph = simple_name(record["glyph"])
        assert enchantment is None or glyph is None
        items.append([
            record["depth"],
            record["source"],
            record["choice"],
            record["cell"],
            record["container"],
            record["simple_class"],
            record["kind"],
            record["true_level"],
            record["cursed"],
            enchantment if enchantment is not None else glyph,
            record["accessibility"],
        ])

    return {
        "seed": run["seed"],
        "challenges": run["challenges"],
        "requested_depths": run["requested_depths"],
        "generator_checkpoints": [
            [record["depth"], simple_name(record["level_class"]),
             record["generator_state_hash"]]
            for record in records
            if record["record"] == "generator_checkpoint" and record["depth"] >= 15
        ],
        "levels": levels,
        "items": items,
    }


def fixture(documents):
    first_run = documents[0]["records"][0]
    result = {
        "schema": FIXTURE_SCHEMA,
        "game_version": first_run["game_version"],
        "game_jar_sha256": first_run["game_jar_sha256"],
        "runtime": first_run["runtime"],
        "generator_checkpoint_columns": ["depth", "level_class", "hash"],
        "mob_columns": ["class", "cell"],
        "item_columns": [
            "depth", "source", "choice", "cell", "container", "class",
            "kind", "upgrade", "cursed", "effect", "accessibility",
        ],
        "seeds": {},
    }
    for document in documents:
        run = document["records"][0]
        assert run["game_version"] == result["game_version"]
        assert run["game_jar_sha256"] == result["game_jar_sha256"]
        assert run["runtime"] == result["runtime"]
        result["seeds"][run["seed_code"]] = summarize(document)
    return result


def main():
    if len(sys.argv) < 3:
        raise SystemExit(
            "usage: assert_halls.py EXPECTED SEED=ORACLE_JSON...\n"
            "       assert_halls.py --print SEED=ORACLE_JSON..."
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
