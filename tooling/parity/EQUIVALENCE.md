# Exact RC1 item and cell equivalence

This harness generates every supported floor through 24 and the Imp vault for
consecutive seeds with zero challenges and a Warrior. It compares sorted
multisets of `(floor, source, item, upgrade, cursed, effect)` without hashes or
search early exits. Duplicate entries count separately. All catalog weapons,
armor, rings, and wands are included, including quest choices, shops, mimics,
and statues, plus the four initial catalyst offers. A separate sorted multiset
compares `(floor, branch, cell, item, upgrade, cursed, effect)` for physical
equipment in heaps, shops, mimics, and statues. Quest reward choices and the
unplaced floor-20 shop cache have no physical cell in this profile.

For all 20 regular floors and the vault, every integer terrain cell is compared
directly, along with width and height. No map hashes substitute for cell equality.
The production world generator supplies the item multiset; the regional floor
generators expose the terrain and placement event records used by the cell check.
Consumables, quantities, mob identities, custom visual tiles, secret flags, and
accessibility groups are outside this comparison. Boss terrain is not simulated
by the engine. These boundaries are recorded in each run's manifest.

The oracle loads the official 4.0.0-RC-1 JAR, build 907 (SHA-256
`43f881f0d6484faffea913f5563fd2c3277ed83159eda6e83efc55e586fbfdbf`).
Only the existing headless `TextureFilm` and `ItemSprite` stand-ins precede the
JAR; no generation classes are substituted. The one-seed finder startup is
outside the requested interval. Each subsequent seed resets the game and the
Imp's persistent reward fields. Boss floors 5, 10, and 15 contribute no catalog
equipment or persistent item-deck changes and are skipped; floor 20's shop is
included. The vault is generated after the main dungeon, as in the engine.

The [RC1 100,000-seed report](rc1-100000.json) records a successful run of seeds
0 through 99,999 and a second replay of the retained oracle streams. Both
completed with zero mismatches or errors: 9,357,260 searchable item entries,
7,017,389 equipment positions, 2,100,000 floor maps, and 4,035,195,654 terrain
cells. The report includes the profile, exclusions, executable hashes, archive
hashes, and per-shard counts. Local raw evidence is retained under
`tooling/oracle-4.0/.work/rc1-100000/`.

## Run

From the repository root, build the existing Java finder and Rust comparator:

```sh
bash tooling/java-finder/build.sh
cargo build --release -p shpd-seedfinder-core --example equivalence
```

Compile `tooling/parity/BatchEquipmentOracle.java` with the selected JDK into
`tooling/oracle-4.0/.work/batch-classes`, using these classpath entries:

- `tooling/java-finder/.work/classes`
- `tooling/oracle-4.0/.work/ShatteredPD-v4.0.0-RC-1-Java.jar`

Use `;` between classpath entries on Windows and `:` elsewhere. Run:

```sh
python tooling/parity/run_equivalence.py 100000 6 \
  --output tooling/oracle-4.0/.work/equivalence-new \
  --exe target/release/examples/equivalence
```

On Windows use the `.exe` suffix and the GNU Rust toolchain if MSVC is absent.
Set `JAVA_21_HOME`, `JAVA_HOME`, or pass `--java` with the Java executable.
The RC1 validation used Eclipse Adoptium JDK 25.0.4.1 on Windows x86-64.
The output directory must not exist. The runner freezes the comparator binary
and records its hash, the Git revision, oracle hash, and exact seed coverage.

Each shard retains compressed oracle records, JSON multiset differences,
stderr, and a completion record. `progress.json` is partial progress only.
A successful result requires every shard to finish, each comparator summary
to report the requested count with zero deviations and errors, and all process
exit codes to be zero. The comparator rejects missing or out-of-order seeds.
Generating full dungeons is much slower than the finder search benchmark;
measure sustained throughput before estimating a multi-million-seed run.

## Recheck a fix without regenerating the oracle

```sh
python tooling/parity/replay_equivalence.py \
  tooling/oracle-4.0/.work/equivalence-new \
  /absolute/path/to/frozen-fixed-equivalence
```

An optional third argument sets a new evidence label (default `fixed`) for
another replay of the same archives without overwriting prior results.

This follows an active archive or reads a completed one and writes separate
`fixed-*` evidence. Preserve the supplied binary while replay is running.
The gzip end marker and exact seed counts are checked before completion. Both
runner and replay return a nonzero exit code for any failed shard.

RC1 records contain four pipe-separated fields: numeric seed, item multiset,
full maps, and physical equipment multiset. This comparator rejects the older
equipment-only streams rather than treating missing cell coverage as a pass.

To exercise the comparator's negative controls against a completed shard:

```sh
python tooling/parity/test_equivalence.py --exe target/release/examples/equivalence \
  --oracle-archive tooling/oracle-4.0/.work/equivalence-new/shard-0.oracle.txt.gz
```

This checks an exact record, changed upgrades, duplicate items, changed terrain,
moved equipment, missing floors, missing/duplicate seeds, and oracle errors.

RC1 leaves the Halls King's `attrition` page unread in debug defaults. Its
floor-24 placement clears tall grass even though it is not searchable loot.
Seed 17 / `AAA-AAA-AAR`, cell 1191, is the engine regression for this case.
The Segmented Library's successful horizontal-split return and both no-trinket
feeling rolls were already present in the Rust port and match RC1.

The checked-in `beta4-mob-retry-equipment.txt` fixture records three retry failures
from the large run (667551, 1334551, 1334612). They exposed Sewer mob placement
accepting the 31st candidate after Java's retry budget expired, changing later
item draws. The quest-room fixture also checks Mass Grave loot's Skeleton source.

Seed 905410 additionally covers disconnected adjacent Platform Rooms in the
Caves. Their shared wall must preserve the requested solid decorative terrain;
only connected Platform Rooms substitute chasm and restore the doorway. The
wrong wall changes entrance-distance exclusions, mob placement, and later loot.
