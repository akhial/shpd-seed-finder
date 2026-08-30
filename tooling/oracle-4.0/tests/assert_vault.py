#!/usr/bin/env python3
"""Reduce official-oracle --vault documents to a stable Imp quest / Vault fixture."""

import json
import pathlib
import sys


FIXTURE_SCHEMA = "shpd-vault-parity-fixture/v1"
ORACLE_SCHEMA = "shpd-parity-oracle/v2"
IMP_DEPTHS = (17, 18, 19)

# Official v4.0.0-BETA-3 VaultLevel.setupConsumables() orders each tier's pool
# with the unseeded java.util.Collections.shuffle, so WHICH pool consumable ends
# up in a given heap is not a function of the seed (the heap cells themselves,
# every piece of equipment, and everything else on the level are).  These pool
# classes are therefore pinned by cell/room only, never by class.
CONSUMABLE_POOL = {
    "PotionOfHealing", "PotionOfFrost", "PotionOfLevitation", "PotionOfToxicGas",
    "PotionOfParalyticGas", "PotionOfMindVision", "PotionOfLiquidFlame",
    "PotionOfExperience", "PotionOfInvisibility",
    "Mageroyal$Seed", "Icecap$Seed", "Stormvine$Seed", "Firebloom$Seed",
    "Sorrowmoss$Seed", "Blindweed$Seed", "Swiftthistle$Seed", "Sungrass$Seed",
    "Earthroot$Seed", "Starflower$Seed",
    "ScrollOfMirrorImage", "ScrollOfTeleportation", "ScrollOfRecharging",
    "ScrollOfTerror", "ScrollOfLullaby", "ScrollOfMagicMapping",
    "ScrollOfRetribution", "ScrollOfTransmutation",
    "StoneOfFlock", "StoneOfShock", "StoneOfFear", "StoneOfDeepSleep",
    "StoneOfClairvoyance", "StoneOfAggression", "StoneOfBlast", "StoneOfBlink",
    "StoneOfEnchantment", "StoneOfAugmentation",
}


def simple_name(value):
    return None if value is None else value.rsplit(".", 1)[-1]


def effect(record):
    enchantment = simple_name(record["enchantment"])
    glyph = simple_name(record["glyph"])
    assert enchantment is None or glyph is None
    return enchantment if enchantment is not None else glyph


def load_document(path):
    document = json.loads(pathlib.Path(path).read_text(encoding="utf-8"))
    assert document["schema"] == ORACLE_SCHEMA
    return document


def summarize(document, imp_depth):
    records = document["records"]
    run = next(record for record in records if record["record"] == "run_init")
    assert run["vault_requested"] is True
    assert run["requested_depths"] == [17, 18, 19]

    transitions = [record for record in records if record["record"] == "vault_transition"]
    assert len(transitions) == 1
    transition = transitions[0]
    assert transition["depth"] == imp_depth, (transition["depth"], imp_depth)
    assert transition["branch"] == 1
    assert simple_name(transition["level_class"]) == "VaultLevel"
    assert transition["restored_depth"] == 20
    assert transition["restored_branch"] == 0
    unchanged = {
        name: transition[f"{name}_unchanged"]
        for name in ("generator", "limited_drops", "quests", "room_queues", "shop_state")
    }
    assert all(unchanged.values()), unchanged
    assert transition["generator_hash_before"] == transition["generator_hash_after"]

    # The Imp's reward options are rolled on the Imp's floor (main branch) and
    # every searchable one is recorded there as an imp_quest choice.
    imp_items = [
        record for record in records
        if record["record"] == "item" and record["source"] == "imp_quest"
    ]
    assert imp_items, "no imp_quest records"
    for record in imp_items:
        assert record["depth"] == imp_depth and record["branch"] == 0
        assert record["searchable"] is True
        assert record["cursed"] is False
        assert simple_name(record["owner"]) == "Imp"
        assert record["accessibility"] == {
            "kind": "choice", "group": f"imp_quest@{imp_depth}", "option": record["choice"]
        }
        assert 0 <= record["choice"] <= 5
    imp_choices = sorted(record["choice"] for record in imp_items)
    assert imp_choices in ([1, 2, 3, 4, 5], [0, 1, 2, 3, 4, 5]), imp_choices
    imp_records = [
        [
            record["choice"], record["simple_class"], record["kind"],
            record["true_level"], record["cursed"], effect(record), record["quantity"],
        ]
        for record in sorted(imp_items, key=lambda record: record["choice"])
    ]

    # The Vault itself.
    vault_levels = [record for record in records if record["record"] == "level" and record["branch"] == 1]
    assert len(vault_levels) == 1
    vault = vault_levels[0]
    assert vault["depth"] == imp_depth
    assert simple_name(vault["level_class"]) == "VaultLevel"
    rooms = []
    for room in vault["rooms"]:
        name = simple_name(room["class"])
        assert name.startswith("Vault"), name
        rooms.append([name, room["left"], room["top"], room["right"], room["bottom"]])
    assert sum(1 for room in rooms if room[0] == "VaultFinalRoom") == 1
    assert sum(1 for room in rooms if room[0] == "VaultEntranceRoom") == 1
    mobs = []
    for mob in vault["mobs"]:
        name = simple_name(mob["class"])
        assert name.startswith("Vault"), name
        mobs.append([name, mob["cell"], mob["room"]])
    assert len(mobs) == vault["mob_count"]

    vault_items = [
        record for record in records
        if record["record"] == "item" and record["branch"] == 1
    ]
    assert len(vault_items) >= vault["heap_count"]
    item_records = []
    pool_records = []
    final_room_items = []
    for record in vault_items:
        assert record["source"] == "vault_heap"
        assert record["depth"] == imp_depth
        assert record["container"] in ("HEAP", "CHEST")
        assert record["room"] is not None and record["room"].startswith("Vault"), record["room"]
        assert record["accessibility"] == {"kind": "independent"}
        name = simple_name(record["class"])
        if name in CONSUMABLE_POOL:
            assert record["kind"] == "other" and record["searchable"] is False
            assert record["true_level"] == 0 and record["quantity"] == 1
            assert record["room"] != "VaultFinalRoom"
            pool_records.append([record["cell"], record["room"], record["container"]])
            continue
        row = [
            record["cell"], record["room"], record["container"], name,
            record["kind"], record["true_level"], record["cursed"], effect(record),
            record["quantity"], record["searchable"],
        ]
        item_records.append(row)
        if record["room"] == "VaultFinalRoom":
            final_room_items.append(record)

    # VaultFinalRoom holds the ImpStatue plus all six reward options (the
    # artifact-or-ring slot included), and its searchable ones are exactly the
    # imp_quest choices recorded on the main branch.
    final_classes = sorted(record["simple_class"] for record in final_room_items)
    assert "ImpStatue" in final_classes, final_classes
    assert len(final_room_items) == 7, final_classes
    final_searchable = sorted(
        [record["simple_class"], record["true_level"], record["cursed"], effect(record), record["quantity"]]
        for record in final_room_items if record["searchable"]
    )
    imp_searchable = sorted(
        [record["simple_class"], record["true_level"], record["cursed"], effect(record), record["quantity"]]
        for record in imp_items
    )
    assert final_searchable == imp_searchable, (final_searchable, imp_searchable)

    checkpoints = [
        [record["depth"], record["branch"], simple_name(record["level_class"]),
         record["generator_state_hash"]]
        for record in records
        if record["record"] == "generator_checkpoint" and record["depth"] >= 15
    ]
    # The vault checkpoint must equal the last main-branch checkpoint before it.
    vault_checkpoint = [value for value in checkpoints if value[1] == 1]
    assert len(vault_checkpoint) == 1
    main_before = [value for value in checkpoints if value[1] == 0][-1]
    assert main_before[3] == vault_checkpoint[0][3], (main_before, vault_checkpoint)

    return {
        "seed": run["seed"],
        "challenges": run["challenges"],
        "imp_depth": imp_depth,
        "imp_reward_options": imp_records,
        "vault": {
            "depth_seed": vault["depth_seed"],
            "feeling": vault["feeling"],
            "size": [vault["width"], vault["height"]],
            "map_hash": vault["map_hash"],
            "entrance": vault["entrance"],
            "exit": vault["exit"],
            "mob_count": vault["mob_count"],
            "heap_count": vault["heap_count"],
            "generator_state_hash": vault["generator_state_hash"],
            "rooms": rooms,
            "mobs": mobs,
            "items": item_records,
            "consumable_pool_heaps": pool_records,
        },
        "transition": {
            "unchanged": unchanged,
            "generator_hash": [transition["generator_hash_before"], transition["generator_hash_after"]],
        },
        "generator_checkpoints": checkpoints,
    }


def fixture(documents):
    first_run = documents[0][1]["records"][0]
    result = {
        "schema": FIXTURE_SCHEMA,
        "game_version": first_run["game_version"],
        "game_jar_sha256": first_run["game_jar_sha256"],
        "runtime": first_run["runtime"],
        "imp_reward_option_columns": [
            "choice", "class", "kind", "upgrade", "cursed", "effect", "quantity",
        ],
        "room_columns": ["class", "left", "top", "right", "bottom"],
        "mob_columns": ["class", "cell", "room"],
        "item_columns": [
            "cell", "room", "container", "class", "kind", "upgrade", "cursed",
            "effect", "quantity", "searchable",
        ],
        "consumable_pool_heap_columns": ["cell", "room", "container"],
        "generator_checkpoint_columns": ["depth", "branch", "level_class", "hash"],
        "seeds": {},
    }
    for imp_depth, document in documents:
        run = document["records"][0]
        assert run["game_version"] == result["game_version"]
        assert run["game_jar_sha256"] == result["game_jar_sha256"]
        assert run["runtime"] == result["runtime"]
        result["seeds"][run["seed_code"]] = summarize(document, imp_depth)
    assert sorted(value["imp_depth"] for value in result["seeds"].values()) == list(IMP_DEPTHS)
    return result


def main():
    if len(sys.argv) < 3:
        raise SystemExit(
            "usage: assert_vault.py EXPECTED SEED/IMP_DEPTH=ORACLE_JSON...\n"
            "       assert_vault.py --print SEED/IMP_DEPTH=ORACLE_JSON..."
        )

    printing = sys.argv[1] == "--print"
    expected_path = None if printing else sys.argv[1]
    documents = []
    for spec in sys.argv[2:]:
        label, separator, path = spec.partition("=")
        if not separator:
            raise AssertionError(f"missing seed prefix in {spec!r}")
        expected_seed, _, imp_depth = label.partition("/")
        document = load_document(path)
        actual_seed = document["records"][0]["seed_code"]
        assert actual_seed == expected_seed, (actual_seed, expected_seed)
        documents.append((int(imp_depth), document))

    actual = fixture(documents)
    if printing:
        json.dump(actual, sys.stdout, indent=2)
        print()
        return

    expected = json.loads(pathlib.Path(expected_path).read_text(encoding="utf-8"))
    assert actual == expected


if __name__ == "__main__":
    main()
