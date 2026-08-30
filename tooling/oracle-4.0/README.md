# Shattered Pixel Dungeon v4.0.0-BETA-3 parity oracle

This directory builds an isolated, deterministic generation oracle for
Shattered Pixel Dungeon `v4.0.0-BETA-3`. Unlike `tooling/oracle/` (v3.3.8),
which patches and compiles the official source tree, no 4.0.0 source has been
published yet, so this oracle drives the **unmodified official desktop JAR**
headlessly. Nothing from the game is recompiled; the only things placed ahead
of the JAR on the classpath are the oracle itself and one small geometry-only
stand-in class (see "Headless technique").

The pin is:

- artifact: `ShatteredPD-v4.0.0-BETA-3-Java.jar`
- URL: `https://github.com/00-Evan/shattered-pixel-dungeon/releases/download/4.0.0-beta/ShatteredPD-v4.0.0-BETA-3-Java.jar`
- sha256: `f62f8ac2ef6d36c72223c1a4e78f18e98d0bb1282cd4f1fca123082d43edccc9`
- manifest: `Specification-Version: 4.0.0-BETA-3`, `Implementation-Version: 904`
  (used as `Game.versionCode`)

`build.sh` downloads the JAR into `.work/` when absent, verifies the sha256
(and fails loudly on mismatch), and compiles `src/**/*.java` into
`.work/classes` with `javac -nowarn` against the JAR. `run.sh` builds when
needed and runs `com.shatteredpixel.shatteredpixeldungeon.ParityOracle` with
the classpath `classes` first, then the JAR (`;` separated on Windows/MSYS,
`:` elsewhere). Both honour `JAVA_21_HOME`, then `JAVA_HOME`, then `PATH`; the
pinned fixtures were produced with JDK 21.0.11.

## Build and run

```sh
tooling/oracle-4.0/build.sh
tooling/oracle-4.0/run.sh --seed AAA-AAA-AAA --floors 1 --format ndjson
tooling/oracle-4.0/run.sh SEE-EEE-EED 1,3-5 --format json
tooling/oracle-4.0/run.sh AAA-AAA-AAA 6-9 --run-checkpoints
tooling/oracle-4.0/run.sh AAA-AAA-AAA 25 --boss-skip-checkpoints
tooling/oracle-4.0/run.sh AAA-AAA-AAA 17-19 --vault --run-checkpoints
```

`--floors` accepts comma-separated depths and inclusive ranges from 1 through
26. Earlier floors are still generated when a later floor is requested, so the
run-level decks and quest state are exact; only selected depths are emitted.
The default format is NDJSON; JSON format emits one document containing a
`records` array. `--run-checkpoints` emits a compact `generator_checkpoint`
after every generated floor (selected or not). `--boss-skip-checkpoints`
snapshots Generator, LimitedDrops, quest, room-queue and shop-dependent hero
state around depths 5, 10, 15, 20 and 25. `--challenges N` sets the challenge
mask.

`--vault` additionally builds the Imp's Vault (see below) after the main
floors. If the highest requested depth is below the Imp's floor, the oracle
keeps generating unselected floors (up to 19, where the Imp is guaranteed)
until the Imp has spawned; those extra floors only appear as
`generator_checkpoint` records when requested.

CLI compatibility with the v3.3.8 runner: `--seed`, `--floors`/`--depths`,
`--format`, `--challenges`, `--run-checkpoints`, `--boss-skip-checkpoints` and
the positional `SEED FLOORS` form are unchanged. `--no-phases` is accepted as a
no-op (schema v2 has no phase records). `--transmute-imp` is gone: the v4.0.0
Imp no longer hands out a single ring, so the diagnostic is obsolete.

## Headless technique

Startup follows the v3.3.8 oracle exactly: `Game.version`/`versionCode` are
set, `GameSettings.set(new MemoryPreferences())` provides in-memory settings,
`SPDSettings.intro(false)` is stored, `Badges.global` and
`Bones.{depth,branch,item,heroClass}` are reset reflectively (a clean install
without libGDX's absent file service), then `Dungeon.initSeed()`,
`Dungeon.init()` with the Warrior selected, and sequential
`Dungeon.newLevel()` / `Dungeon.depth++`.

Two things stood between the unmodified JAR and a headless run. The v3.3.8
oracle solved both with source patches; here they are solved without touching
any game class:

1. **Debug journal defaults** (`DeviceCompat.isDebug()`). In debug mode every
   journal page counts as read, which keeps the *unseeded* early-Guidebook
   heap off the first floors (without it, `AAA-AAA-AAA` floor 1 gains an
   eleventh heap and floor 2 changes). The JAR implements `isDebug()` as
   `Game.version.contains("INDEV")`, so the oracle simply sets
   `Game.version = "4.0.0-BETA-3-INDEV"`. Grepping the decompiled tree shows
   `Game.version` is otherwise read only by `DesktopLauncher`, the title/menu
   version labels, and `SPDSettings.betas()` (an update-checker default), none
   of which are on the generation path; the other `isDebug()` call sites are
   `HeroClass.isUnlocked` (Warrior is always unlocked anyway) and UI scenes.
   `run_init.game_version` records the true `4.0.0-BETA-3`;
   `run_init.effective_game_version` records the string actually installed.
2. **An eagerly loaded texture atlas.** `ItemSpriteSheet.Icons.film` is
   `new TextureFilm("sprites/item_icons.png", 8, 8)`, whose upstream
   constructor calls `TextureCache.get()` and needs a live GL context. Instead
   of shadowing the 1,000-line `ItemSpriteSheet`, the oracle ships a
   `com.watabou.noosa.TextureFilm` stand-in
   (`src/com/watabou/noosa/TextureFilm.java`, ~150 lines, marked as a headless
   stand-in for the GPL-3 upstream class). It reproduces the upstream frame
   geometry exactly, but texture-backed constructors read the texture size
   from the PNG's IHDR header on the classpath (128x64 for `item_icons.png`)
   rather than uploading a texture. It contains no random calls, keeps the
   JAR's `ItemSpriteSheet` (and every sprite index) intact, and is the only
   shadow class.

Both interventions are RNG-neutral by construction: neither touches
`com.watabou.utils.Random`, and the resulting floors are byte-identical to the
proof-of-concept that used full `DeviceCompat`/`ItemSpriteSheet` shadows.

There is no `Level.create()` patch, so **`level_phase` records are gone** in
v2. Every other checkpoint (`generator_checkpoint`, `boss_transition`, the
final `level` snapshot) is unchanged; the challenges regression now pins the
final-floor `map_hash`/`mob_count`/`heap_count` instead of the `items` phase.

## Stable record schema (v2)

Every record has `schema: "shpd-parity-oracle/v2"` and a `record` type. Where
nothing changed, the v1 vocabulary is kept verbatim.

- `run_init`: canonical/numeric seed, JVM provenance, `game_version`,
  `game_version_code`, `game_jar_url`, `game_jar_sha256` (replacing v1's
  `game_commit`), `effective_game_version`, `vault_requested`, selected depth
  seeds (each with `branch: 0`), challenge mask, fixed Warrior initialization,
  limited-drop counters, potion/scroll/ring identity permutations, the complete
  Generator deck state and ordered special/secret room queues immediately after
  `Dungeon.init()`.
- `level`: final floor snapshot: `depth`, **`branch`**, **`depth_seed`**
  (`Dungeon.seedForDepth(depth, branch)`), class, feeling, dimensions,
  entrance, exit, full integer map plus `Arrays.hashCode(int[])` fingerprint,
  sorted room descriptors/connections, sorted mob descriptors (now with the
  containing `room`), **`mob_count`/`heap_count`**, limited drops, the complete
  Generator state and its compact hash, and the post-floor room queues.
- `generator_checkpoint`, `boss_transition`: as in v1, plus `branch`.
- `item`: every searchable item in ordinary heaps, shops, mimics, statues, the
  sacrificial-fire prize, generated Ghost/Wandmaker/Blacksmith/Imp reward
  choices, and the depth-20 Imp shop cache. New/changed fields: **`branch`**,
  **`room`** (simple class name of the room containing `cell` on room-based
  levels, else null), and `kind` now distinguishes `ring` and `artifact`.
  **Rings are searchable in v2** (`Weapon`, `Armor`, `Wand`, `Ring`); v1 only
  recorded weapons/armor/wands, so ordinary floors now carry ring records too.
  Heap sources (`heap`, `chest`, `locked_chest`, `crystal_chest`, `tomb`,
  `skeleton`, `remains`, `shop`) and `accessibility` are unchanged.
- `item` with `source: "imp_quest"` (new): the six entries of
  `Imp.Quest.rewardOptions`, rolled by `Imp.Quest.spawn` on the City floor
  (17-19) whose `AmbitiousImpRoom` spawned: `[0]` artifact (+5 via
  `transferUpgrade`, or a +2..+4 ring when no artifact is left), `[1]` a
  +2..+4 ring of a different class, `[2]`/`[3]` a T5 weapon + T4 missile or a
  T5 missile + T4 weapon pair (enchanted, +2..+4 / +3..+5), `[4]` inscribed
  PlateArmor +2..+4, `[5]` a +2..+4 wand; all uncursed. `choice` is the index
  in `rewardOptions` and is preserved even though the artifact slot itself is
  non-searchable and therefore not emitted (so a run normally shows choices
  1-5, or 0-5 when slot 0 is a ring). `owner` is the `Imp` class,
  `accessibility` is `{"kind":"choice","group":"imp_quest@<depth>","option":<choice>}`,
  and the records are emitted on the floor where they appeared, like the other
  quest rewards. Enchantment/glyph/`true_level`/`cursed` are recorded exactly
  as stored. Note that `VaultFinalRoom.paint` clears `rewardOptions`, so they
  are captured before any vault is generated.
- `item` with `source: "vault_heap"` (new, `--vault` only): **every** heap item
  of the Vault, not only searchable ones (the `searchable` flag is still
  accurate), with `container` = heap type (`HEAP`/`CHEST`) and `room` set, so a
  reducer can separate `VaultFinalRoom` (ImpStatue + the six reward options)
  from treasure-room loot.
- `vault_transition` (new, `--vault` only): the Vault's depth/branch/class, the
  restored main-branch depth/branch, and the same before/after hashes and
  `*_unchanged` flags as `boss_transition` for Generator, LimitedDrops, quests,
  room queues and shop state.

The quest snapshot (`boss_transition`/`vault_transition` hashes) records the
v4.0.0 Imp fields `oldQuest`, `alternative`, `spawned`, `given`, `completed`,
`reward`, `hazardFreebies`, `mirrorUsed`, `score`; `rewardOptions` is
deliberately excluded because it is transient (rolled on the Imp's floor,
cleared by the Vault) and is already recorded as item records.
`Generator.Category` gained `defaultProbsTotal`; it is recorded as
`default_probabilities_total`.

Non-searchable tutorial/meta items are omitted on main floors as before.
Collections whose game representation is unordered are sorted before output.
The recorder never calls `identify()`, `title()`, or any other item-mutating
display path.

## Branch and Vault semantics

v4.0.0 replaces the old Imp token quest with the Imp's Vault: a sub-level
reached from the Imp's floor. `Dungeon.newLevel()` builds a `VaultLevel` when
`Dungeon.branch == 1` and `Dungeon.depth` is 16-19. It is seeded independently
by `Dungeon.seedForDepth(depth, 1)` (`depth + 30 * branch` look-ahead draws),
`Level.create()` skips the branch-0-only item/feeling rolls, `createMobs()` is
empty (rooms place their own `Vault*` mobs), and equipment/consumables come
from `Generator.randomUsingDefaults` (no deck mutation). `--vault` therefore
generates the Vault **after** all main floors, saving and restoring
`Dungeon.depth`/`Dungeon.branch`, and the emitted `vault_transition` record
plus the branch-1 `generator_checkpoint` (equal to the last main checkpoint)
prove that no run-persistent state changed. The only global side effects are
`Dungeon.generatedLevels` (gains `depth + 1000`) and `Imp.Quest.rewardOptions`
being emptied, neither of which influences later floors. Generating the Vault
mid-run, as the game does, would hence leave floors 18/19 unchanged.

Both `level` and `item` records carry `branch`; the Vault's `level` record has
`branch: 1`, `depth` equal to the Imp's floor and `depth_seed` equal to
`seedForDepth(depth, 1)`.

### Official non-determinism in the Vault

`VaultLevel.setupConsumables()` shuffles each consumable tier with the
**unseeded** `java.util.Collections.shuffle`. The seeded RNG stream is not
consumed by it, so the map, rooms, mobs, every heap cell, all equipment
(weapons, armor, wands, rings, the reward options) and the deterministic
non-pool items (ImpStatue, DwarfTokens, VaultBeacons, food, PotionOfPurity in
`VaultFlamesTreasureRoom`, ...) are identical run to run, but *which* pool
potion/seed/scroll/stone occupies a given consumable heap is not a function of
the seed, and partially consumed tiers can even change the multiset of pool
classes present. As with v3.3.8's `ShopRoom.ChooseBag` HashMap tie, the oracle
preserves the official behaviour rather than canonicalising it;
`tests/assert_vault.py` pins pool consumables by cell/room/container only.

## Tests

```sh
export JAVA_21_HOME=/path/to/jdk-21.0.11   # e.g. "C:/Program Files/Java/jdk-21.0.11"
export PYTHON=python                        # when python3 is not on PATH
tooling/oracle-4.0/tests/smoke.sh
tooling/oracle-4.0/tests/prison.sh
tooling/oracle-4.0/tests/caves.sh
tooling/oracle-4.0/tests/city.sh
tooling/oracle-4.0/tests/halls.sh
tooling/oracle-4.0/tests/boss-skips.sh
tooling/oracle-4.0/tests/challenges.sh
tooling/oracle-4.0/tests/vault.sh
```

Every script accepts `--print` to regenerate its `*.expected.json` fixture
from a fresh run. All fixtures were regenerated for 4.0.0 (the Builder change
alters every floor from depth 1: `AAA-AAA-AAA` floor 1 is now 37x43 with map
hash -72472821, versus 40x30 / -188128262 in 3.3.8) and use the same seeds and
floors as the v3.3.8 tests:

- `smoke.sh` runs `AAA-AAA-AAA` floor 1 twice, requires byte-identical NDJSON,
  validates JSON/NDJSON equivalence and pins the floor/item fingerprint.
- `prison.sh`, `caves.sh`, `city.sh`, `halls.sh` pin `AAA-AAA-AAA` floors 6-9,
  11-14, 16-19 and 21-24 (plus the first floor of each region for
  `AAA-AAA-AAB`, `ABC-DEF-GHI`, `ZZZ-ZZZ-ZZZ`, and depth 22 of `AAA-AAA-AIC`
  for the Halls) with generator checkpoints, dimensions, map hashes,
  transitions, feelings, sorted mobs and all searchable items with
  accessibility (`shpd-<region>-parity-fixture/v3`).
- `boss-skips.sh` proves depths 5, 10, 15 and 25 are state-neutral and pins the
  non-neutral depth-20 Imp shop cache for three seeds.
- `challenges.sh` pins `AAA-AAA-AAA` and `AAA-AAA-AAF` floors 1-14 under masks
  0, 8, 32, 64 and 104.
- `vault.sh` pins, for one seed per possible Imp floor (`AAA-AAA-AAC` at 17,
  `AAA-AAA-AAB` at 18, `AAA-AAA-AAA` at 19), the `imp_quest` reward options
  and the full Vault: `depth_seed`, size, map hash, entrance/exit, sorted rooms,
  sorted mobs with rooms, every deterministic heap item with room, upgrade,
  cursed and effect, the pool-consumable heap cells, the `vault_transition`
  neutrality flags and the generator checkpoints from depth 15 onwards. It
  also cross-checks that `VaultFinalRoom` holds exactly the ImpStatue plus the
  recorded reward options.

Byte determinism is defined for the same JAR, oracle options and JVM runtime;
the v3.3.8 caveat about `ShopRoom.ChooseBag` iterating a `HashMap` still
applies, and the Vault consumable caveat above is new.

## Isolation

No file outside this directory is touched. `.work/` (JAR, compiled classes,
scratch decompilations) is gitignored. The decompiled trees under `.work/` are
reading aids only; nothing from them is copied into the repository except the
minimal, clearly marked `TextureFilm` stand-in. Shattered Pixel Dungeon is
GPL-3.0 software; the oracle and the stand-in are provided under the same
license and are intended only as parity tooling.
