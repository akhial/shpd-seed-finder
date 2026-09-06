# Exact BETA-4 equipment equivalence

This harness generates every supported floor through 24 and the Imp vault for
consecutive seeds with zero challenges and a Warrior. It compares sorted
multisets of `(floor, source, item, upgrade, cursed, effect)` without hashes or
search early exits. Duplicate entries count separately. All catalog weapons,
armor, rings, and wands are included, including quest choices, shops, mimics,
and statues. Consumables, quantities, tile coordinates, secret flags, and
accessibility groups are outside this equipment comparison.

The oracle loads the official BETA-4 JAR (SHA-256
`76f6983e7b619267666621de9f1ecbbc3645d4925c2c446736987c3011b9dfd1`).
Only the existing headless `TextureFilm` and `ItemSprite` stand-ins precede the
JAR; no generation classes are substituted. The one-seed finder startup is
outside the requested interval. Each subsequent seed resets the game and the
Imp's persistent reward fields. Boss floors 5, 10, and 15 contribute no catalog
equipment or persistent item-deck changes and are skipped; floor 20's shop is
included. The vault is generated after the main dungeon, as in the engine.

## Run

From the repository root, build the existing Java finder and Rust comparator:

```sh
bash tooling/java-finder/build.sh
cargo build --release -p shpd-seedfinder-core --example equivalence
```

Compile `tooling/parity/BatchEquipmentOracle.java` with JDK 21 into
`tooling/oracle-4.0/.work/batch-classes`, using these classpath entries:

- `tooling/java-finder/.work/classes`
- `tooling/oracle-4.0/.work/ShatteredPD-v4.0.0-BETA-4-Java.jar`

Use `;` between classpath entries on Windows and `:` elsewhere. Run:

```sh
python tooling/parity/run_equivalence.py 4000000 6 \
  --output tooling/oracle-4.0/.work/equivalence-new \
  --exe target/release/examples/equivalence
```

On Windows use the `.exe` suffix and the GNU Rust toolchain if MSVC is absent.
Set `JAVA_21_HOME`, `JAVA_HOME`, or pass `--java` with the Java executable.
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

This follows an active archive or reads a completed one and writes separate
`fixed-*` evidence. Preserve the supplied binary while replay is running.
The gzip end marker and exact seed counts are checked before completion.

The checked-in `beta4-mob-retry-equipment.txt` fixture records three failures
from the large run (667551, 1334551, 1334612). They exposed Sewer mob placement
accepting the 31st candidate after Java's retry budget expired, changing later
item draws. The quest-room fixture also checks Mass Grave loot's Skeleton source.
