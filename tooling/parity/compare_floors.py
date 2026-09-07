#!/usr/bin/env python3
"""Compare a parity-oracle JSON document with the engine's `dump_floors` text.

Usage:

    python tooling/parity/compare_floors.py ORACLE.json ENGINE.txt [--verbose]

The oracle document is the `--format json` output of `tooling/oracle*/run.sh`
(schema `shpd-parity-oracle/v1` or `v2`); the engine text is the output of
`cargo run --example dump_floors -- SEED [MAX_DEPTH]`. Both are reduced to the
same per-floor summary — size, map hash, entrance, exit, feeling, ordinary mob
multiset, and searchable item multiset (kind, item, upgrade, cursed, effect) —
and every difference is printed. Exit status is 1 when anything differs.

Only main-branch floors present on both sides are compared; the oracle's
vault records (`branch` 1) are reported separately when present.
"""

import json
import pathlib
import re
import sys
from collections import Counter

# Actors the engine does not list as ordinary mobs: quest givers, shopkeepers,
# painted hazards and containers. Their cells are covered by other checks
# (items for mimics/statues, map hashes for terrain).
NON_ORDINARY_MOBS = {
    "Ghost", "Wandmaker", "Blacksmith", "Imp", "Shopkeeper", "ImpShopkeeper",
    "RatKing", "Mimic", "GoldenMimic", "CrystalMimic", "EbonyMimic", "Statue",
    "ArmoredStatue", "Sentry", "Piranha", "PhantomPiranha", "DemonSpawner",
    "Sheep", "VaultSentry", "VaultLaser", "VaultMirror", "VaultTokenDoor",
    "WandOfRegrowth$Lotus", "CrystalSpire", "GnollGeomancer", "Bee", "RotHeart",
    "RotLasher",
}
# A Mass Grave quest room also paints one `Skeleton`, which this script cannot
# tell apart from the Prison's ordinary skeletons; expect one such report on
# the Corpse Dust floor.

# Upstream classes that are not catalog equipment and so never reach the
# engine's item list (the vault drops a plain Dart stack).
NON_CATALOG_ITEMS = {"dart"}

SEARCHABLE_KINDS = {"weapon", "armor", "wand", "ring", "missile", "melee", "artifact"}


def snake(name):
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def item_id(simple_class):
    """Map an upstream item class name to the engine's stable id."""
    name = snake(simple_class)
    name = name.replace("wand_of_", "wand_")
    name = name.replace("ring_of_", "ring_")
    return name


def load_oracle(path):
    document = json.loads(pathlib.Path(path).read_text(encoding="utf-8"))
    records = document["records"] if "records" in document else document
    floors = {}
    vault = {}
    for record in records:
        kind = record.get("record")
        branch = record.get("branch", 0) or 0
        target = vault if branch == 1 else floors
        if kind == "level":
            depth = record["depth"]
            entry = target.setdefault(depth, empty_floor())
            entry["class"] = simple(record.get("level_class"))
            entry["size"] = (record.get("width"), record.get("height"))
            entry["map_hash"] = record.get("map_hash")
            entry["entrance"] = record.get("entrance")
            entry["exit"] = record.get("exit")
            entry["feeling"] = (record.get("feeling") or "NONE").title().replace("_", "")
            mobs = Counter()
            for mob in record.get("mobs", []):
                name = mob_name(mob.get("class"))
                if name in {entry.lower() for entry in NON_ORDINARY_MOBS}:
                    continue
                mobs[(name, mob.get("cell"))] += 1
            entry["mobs"] = mobs
        elif kind == "item":
            if not record.get("searchable", True):
                continue
            depth = record["depth"]
            if branch == 1:
                # The engine reports vault treasure on the Imp's floor; the
                # final room only re-lists the reward options already recorded
                # there, so it is skipped.
                if record.get("room") == "VaultFinalRoom":
                    continue
                entry = floors.setdefault(depth, empty_floor())
            else:
                entry = target.setdefault(depth, empty_floor())
            effect = simple(record.get("enchantment")) or simple(record.get("glyph")) or "-"
            identity = item_id(record.get("simple_class") or simple(record.get("class")))
            if identity in NON_CATALOG_ITEMS:
                continue
            entry["items"][(
                identity,
                record.get("search_upgrade", record.get("true_level")),
                bool(record.get("cursed")),
                effect_name(effect),
            )] += 1
    return floors, vault


def effect_name(effect):
    # The engine's wire names hyphenate two-word glyphs; upstream class names
    # do not.
    return {"AntiMagic": "Anti-Magic", "AntiEntropy": "Anti-Entropy"}.get(effect, effect)


def simple(value):
    return None if value is None else str(value).rsplit(".", 1)[-1]


def mob_name(value):
    """Upstream `Outer$Inner` class names versus the engine's enum variants,
    compared case-insensitively (`DM100` is `Dm100` in Rust)."""
    return simple(value).rsplit("$", 1)[-1].lower()


# Boss floors the engine never simulates (state-neutral under the canonical
# profile); their oracle records are informational only.
SKIPPED_DEPTHS = {5, 10, 15, 25}


def empty_floor():
    return {
        "class": None, "size": None, "map_hash": None, "entrance": None,
        "exit": None, "feeling": None, "mobs": Counter(), "items": Counter(),
    }


FLOOR_LINE = re.compile(
    r"^depth (?P<depth>\d+) (?P<class>\w+)"
    r"(?: size (?P<w>\d+)x(?P<h>\d+) map_hash (?P<hash>-?\d+))?"
    r"(?: entrance (?P<entrance>\d+))?(?: exit (?P<exit>\d+))?"
    r"(?: feeling (?P<feeling>\w+))?$"
)
MOB_LINE = re.compile(r"^  mob (?P<kind>\w+) (?P<cell>\d+)$")
ITEM_LINE = re.compile(
    r"^  item (?P<source>\w+) (?P<id>\w+) \+(?P<upgrade>\d+) cursed=(?P<cursed>true|false)"
    r" effect=(?P<effect>[^ ]+) secret=(?:true|false) (?P<access>\S+)$"
)


def load_engine(path):
    floors = {}
    current = None
    for line in pathlib.Path(path).read_text(encoding="utf-8").splitlines():
        match = FLOOR_LINE.match(line)
        if match:
            current = floors.setdefault(int(match["depth"]), empty_floor())
            current["class"] = match["class"]
            if match["w"]:
                current["size"] = (int(match["w"]), int(match["h"]))
                current["map_hash"] = int(match["hash"])
            if match["entrance"]:
                current["entrance"] = int(match["entrance"])
            if match["exit"]:
                current["exit"] = int(match["exit"])
            if match["feeling"]:
                current["feeling"] = match["feeling"]
            continue
        match = MOB_LINE.match(line)
        if match and current is not None:
            current["mobs"][(match["kind"].lower(), int(match["cell"]))] += 1
            continue
        match = ITEM_LINE.match(line)
        if match and current is not None:
            current["items"][(
                match["id"], int(match["upgrade"]), match["cursed"] == "true", match["effect"],
            )] += 1
    return floors


def compare(oracle, engine, verbose):
    differences = 0
    for depth in sorted(set(oracle) | set(engine)):
        if depth in SKIPPED_DEPTHS and depth not in engine:
            continue
        if depth not in oracle or depth not in engine:
            side = "oracle" if depth in oracle else "engine"
            print(f"depth {depth}: only in {side}")
            differences += 1
            continue
        left, right = oracle[depth], engine[depth]
        for field in ("size", "map_hash", "entrance", "exit", "feeling"):
            if left[field] is None or right[field] is None:
                continue
            if left[field] != right[field]:
                print(f"depth {depth}: {field} oracle={left[field]} engine={right[field]}")
                differences += 1
        for label in ("mobs", "items"):
            missing = left[label] - right[label]
            extra = right[label] - left[label]
            for key, count in sorted(missing.items(), key=str):
                print(f"depth {depth}: {label} missing in engine x{count}: {key}")
                differences += 1
            for key, count in sorted(extra.items(), key=str):
                print(f"depth {depth}: {label} extra in engine x{count}: {key}")
                differences += 1
        if verbose:
            print(f"depth {depth}: compared ({sum(left['items'].values())} items,"
                  f" {sum(left['mobs'].values())} mobs)")
    return differences


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 2
    verbose = "--verbose" in argv
    oracle, vault = load_oracle(argv[1])
    engine = load_engine(argv[2])
    differences = compare(oracle, engine, verbose)
    if vault:
        depth = next(iter(vault))
        entry = vault[depth]
        print(f"vault at depth {depth}: size {entry['size']} map_hash {entry['map_hash']}"
              f" mobs {sum(entry['mobs'].values())} items {sum(entry['items'].values())}")
    if differences:
        print(f"{differences} difference(s)")
        return 1
    print("floors match")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
