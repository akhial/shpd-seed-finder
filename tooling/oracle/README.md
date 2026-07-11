# Shattered Pixel Dungeon v3.3.8 parity oracle

This directory builds an isolated, deterministic generation oracle from the
official Shattered Pixel Dungeon `v3.3.8` source. It never patches the checkout
in `upstream/` and does not depend on the human-oriented reference oracle in
`tooling/java-oracle/`.

The source pin is:

- tag: `v3.3.8`
- commit: `7b8b845a76fe76c6b7c031ae9e570852411f56db`
- upstream: `https://github.com/00-Evan/shattered-pixel-dungeon.git`

The runner uses the local official checkout at
`upstream/shattered-pixel-dungeon` by default. If that checkout is absent it
clones the pinned tag into `tooling/oracle/.work/`. Set `SHPD_SOURCE` to use a
different official clone. The commit is verified before any build starts.

## Build and run

```sh
tooling/oracle/build.sh
tooling/oracle/run.sh --seed AAA-AAA-AAA --floors 1 --format ndjson
tooling/oracle/run.sh SEE-EEE-EED 1,3-5 --format json
tooling/oracle/run.sh AAA-AAA-AAA 6-9 --run-checkpoints
tooling/oracle/run.sh AAA-AAA-AAA 25 --boss-skip-checkpoints
tooling/oracle/run.sh AAA-AAA-AAF 17 --transmute-imp
```

`--floors` accepts comma-separated depths and inclusive ranges from 1 through
26. Earlier floors are still generated when a later floor is requested, so the
run-level decks and quest state are exact; only selected depths are emitted.
The default format is NDJSON. JSON format emits one document containing a
`records` array. Use `--no-phases` for a smaller final-snapshot-only stream.
Use `--run-checkpoints` to emit a compact `generator_checkpoint` record after
every generated floor, including unselected earlier floors. This makes the
preserved run-global deck state and boss-floor no-op cases directly testable.
Temurin 21 is the recommended oracle runtime and is used for the pinned smoke
fingerprint.

`--boss-skip-checkpoints` snapshots Generator, every LimitedDrops counter,
quest rewards/state, special/secret room queues, and shop-dependent hero state
immediately before and after depths 5, 10, 15, 20, and 25. The official
three-seed regression proves that 5, 10, 15, and 25 are state-neutral under the
canonical no-bones profile. Depth 20 is deliberately not neutral: its
`ImpShopRoom` eagerly generates cached stock during `CityBossLevel.build()`.

`--transmute-imp` uses the official `ScrollOfTransmutation.changeItem` path on
the first generated `+4` Imp ring. It clones and restores the active Java RNG
so the observation does not perturb the generated floor. `AAA-AAA-AAF` was the
first result of the seed finder's any-`+4`-Imp-ring query; its pinned regression
is available as `tests/imp-transmutation.sh`.

Set `ORACLE_OFFLINE=1` to make both scripts pass `--offline` to Gradle once the
official dependencies are cached.

## Stable record schema

Every NDJSON record has `schema: "shpd-parity-oracle/v1"` and a `record` type:

- `run_init` contains the canonical/numeric seed, JVM provenance, selected depth seeds,
  challenge mask, fixed Warrior initialization, limited-drop counters,
  potion/scroll/ring identity permutations, and the complete Generator deck
  state plus ordered special/secret room queues immediately after
  `Dungeon.init()`.
- `level_phase` records `prepared`, `built`, `flags`, `mobs`, and `items`
  checkpoints from the official `Level.create()` path. Each includes dimensions,
  `Arrays.hashCode(map)`, counts, a hash of the full Generator state, and the
  ordered special/secret room queue state at that phase.
- `level` is the final floor snapshot: class, feeling, dimensions, entrance,
  exit, full integer map plus its hash, sorted room descriptors/connections,
  sorted mob descriptors,
  limited drops, the complete Generator state and its compact hash, and the
  post-floor special/secret room queues and per-region secret counts.
- `generator_checkpoint`, when requested, records the generated depth and
  level class plus a hash of the complete post-floor Generator state. Records
  are emitted for every floor from depth 1 through the highest requested depth.
- `boss_transition`, when requested, records exact before/after hashes and
  equality flags for all persistent state components. It also records any
  searchable equipment generated during initial boss-level creation.
- `item` records every searchable weapon, armor, or wand in ordinary heaps,
  shops, mimics, statues, the sacrificial-fire prize, and generated Ghost,
  Wandmaker, Blacksmith, and Imp reward choices, plus the depth-20 Imp shop's
  eagerly generated cache. Fields include class, source,
  choice, cell/container/owner, `Item.trueLevel()`, cursed flag, enchantment
  class, glyph class, quantity, kind, and `searchable: true`. Heap sources are
  preserved (`heap`, `chest`, `locked_chest`, `crystal_chest`, `tomb`,
  `skeleton`, or `remains`), and `accessibility` identifies independent items
  and mutually exclusive quest or CrystalVault choices.
- `imp_transmutation`, when requested, records the original and resulting ring
  classes and true levels for one official Scroll of Transmutation call.

Non-searchable tutorial/meta items are intentionally omitted. As in the
reference seed finder, debug journal defaults are enabled and `intro()` is
forced false; this prevents v3.3.8's unseeded early-Guidebook placement from
making a clean oracle depend on local tutorial progress. Collections whose game
representation is unordered are sorted before output.
Map fingerprints use Java's signed `Arrays.hashCode(int[])`, which is directly
reproducible in Rust with wrapping 32-bit arithmetic. The recorder never calls
`identify()`, `title()`, or any other item-mutating display path.

Byte determinism is defined for the same game commit, oracle options, and JVM
runtime. Official v3.3.8 has a runtime-dependent, RNG-free tie in
`ShopRoom.ChooseBag`: equal bag scores are resolved by iterating a
`HashMap<Bag, Integer>`. Different JVM implementations may therefore select a
different non-searchable bag and limited-drop flag even though room maps,
searchable item identities/levels/effects, and Generator state are unchanged.
The oracle preserves this official behavior and records the JVM in `run_init`;
it does not silently canonicalize the tie.

## Isolation and reference architecture

`build.sh` exports the exact official commit into a content-addressed directory
under `.work/`, applies `patches/v3.3.8-oracle.patch`, copies in
`ParityOracle.java`, and compiles only that staged tree. The patch adds a Gradle
`JavaExec` task and five observational checkpoints, enables the reference
finder's debug/meta-progression defaults, forces the intro setting off, and
replaces one eagerly loaded item-icon texture atlas with its existing 128x64
geometry-only constructor for headless startup. These observational and
headless changes do not consume or reseed RNG. Seed initialization and
sequential floor generation follow the reference seed finder architecture:

1. set the custom seed and challenge mask;
2. call `Dungeon.initSeed()` and `Dungeon.init()` with Warrior selected;
3. call `Dungeon.newLevel()` in order;
4. increment `Dungeon.depth` after each floor.

The first phase intentionally targets main-dungeon final-floor snapshots and
the listed searchable item sources. Branch floors and simulated post-generation
gameplay rewards are outside this oracle version.

## Smoke test

```sh
ORACLE_OFFLINE=1 tooling/oracle/tests/smoke.sh
JAVA_21_HOME=/path/to/temurin-21.0.11 \
  ORACLE_OFFLINE=1 tooling/oracle/tests/prison.sh
JAVA_21_HOME=/path/to/temurin-21.0.11 \
  ORACLE_OFFLINE=1 tooling/oracle/tests/boss-skips.sh
JAVA_21_HOME=/path/to/temurin-21.0.11 \
  ORACLE_OFFLINE=1 tooling/oracle/tests/city.sh
JAVA_21_HOME=/path/to/temurin-21.0.11 \
  ORACLE_OFFLINE=1 tooling/oracle/tests/halls.sh
```

The smoke test runs `AAA-AAA-AAA` floor 1 twice and requires byte-identical
NDJSON, validates JSON/NDJSON equivalence, and checks a pinned floor/item
fingerprint from official v3.3.8.

The Prison regression generates `AAA-AAA-AAA` sequentially through depths 1-9
and pins full floors 6-9, including phase/floor Generator hashes, dimensions,
map hashes, entrances/exits, feelings, sorted mob cells, Wandmaker rewards, and
all searchable equipment with accessibility. It also pins depth 6 for three
nonzero seeds (`AAA-AAA-AAB`, `ABC-DEF-GHI`, and `ZZZ-ZZZ-ZZZ`) under Temurin
21.0.11. The compact expected data lives in
`tests/prison-floors.expected.json` and is reduced from the full official
oracle documents by `tests/assert_prison.py`.

The boss-transition regression generates three seeds sequentially through
depth 25. It pins exact Generator hashes on both sides of every boss depth,
asserts equality of LimitedDrops, quest, room-queue, and shop state where
appropriate, and pins the shuffled searchable depth-20 Imp-shop cache. Its
compact fixture is `tests/boss-skips.expected.json`.

The City regression advances the same official run through all preceding
regions and state-neutral boss floors, then pins `AAA-AAA-AAA` floors 16-19.
It records phase and floor Generator hashes, full-map fingerprints, transitions,
sorted painted and ordinary actors, and all searchable equipment. Depth 16 is
also pinned for `AAA-AAA-AAB`, `ABC-DEF-GHI`, and `ZZZ-ZZZ-ZZZ`, including the
third shop's persistent bag state and rare Aquarium/PlantsRoom branches. The
compact fixture is `tests/city-floors.expected.json`, reduced by
`tests/assert_city.py`.

The Halls regression additionally advances through the stateful depth-20 City
boss Imp shop and pins `AAA-AAA-AAA` floors 21-24. It records the mandatory
Demon Spawner alongside ordinary actors, the Last Shop's persistent bag state,
phase and floor Generator hashes, full-map fingerprints, transitions, and all
searchable equipment. Depth 21 is also pinned for the same three nonzero seeds.
Seed `AAA-AAA-AIC` additionally pins the depth-22 unconnected Platform/Chasm
decoration merge and its Pit-room Skeleton reward source. The compact fixture
is `tests/halls-floors.expected.json`, reduced by `tests/assert_halls.py`.

Shattered Pixel Dungeon is GPL-3.0 software. The staged oracle source is
provided under the same license and is intended only as parity tooling.
