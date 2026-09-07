# Java baseline seed finder (v4.0.0-BETA-4)

The Java side of the benchmark in the top-level `README.md`: a seed finder that
runs Shattered Pixel Dungeon's own generator on the JVM, so Seed Seeker's
throughput can be compared with the game's own code **at the version Seed
Seeker targets**.

The published throughput and match counts below were measured on BETA-3;
rerun the commands to obtain BETA-4 measurements.

It exists because the established Java finder cannot follow the engine to
v4.0.0. [Elektrochecker's shpd-seed-finder](https://github.com/Elektrochecker/shpd-seed-finder)
patches the game's *source* tree (26 files, mostly item-recording hooks) and
builds a modified desktop JAR; its newest release is `3.3.X`. Upstream has
published a v4.0.0-BETA-4 release JAR but no v4.0.0 source, so there is nothing
to apply that patch to. This finder takes the route `tooling/oracle-4.0` takes
instead: it drives the **unmodified official JAR** headlessly and recompiles
nothing of the game.

The pin is the same artifact the oracle uses:

- artifact: `ShatteredPD-v4.0.0-BETA-4-Java.jar`
- URL: `https://github.com/00-Evan/shattered-pixel-dungeon/releases/download/4.0.0-beta/ShatteredPD-v4.0.0-BETA-4-Java.jar`
- sha256: `76f6983e7b619267666621de9f1ecbbc3645d4925c2c446736987c3011b9dfd1`

`build.sh` reuses the oracle's download when there is one, verifies the sha256,
and compiles `src/**/*.java` plus the oracle's `TextureFilm` stand-in into
`.work/classes`. `run.sh` builds when needed and runs
`com.shatteredpixel.shatteredpixeldungeon.JarSeedFinder` with `classes` first on
the classpath, then the JAR. Both honour `JAVA_21_HOME`, then `JAVA_HOME`, then
`PATH`; the published numbers were measured with JDK 21.0.11.

## Build and run

```sh
tooling/java-finder/build.sh
tooling/java-finder/run.sh --seeds 2000
tooling/java-finder/run.sh --item WandOfFireblast --upgrade 3 --floors 24 --seeds 500
tooling/java-finder/run.sh --seeds 10000 --warmup 0 --print-matches > matches.txt
```

| Option | Meaning |
| --- | --- |
| `--item CLASS` | item class simple name, e.g. `RunicBlade` (default) |
| `--upgrade N` | required true upgrade; `-1` accepts any (default `5`) |
| `--floors N` | deepest floor generated (default `19`) |
| `--seeds N` | timed seeds (default `2000`) |
| `--start N` | first numeric seed (default `0`) |
| `--warmup N` | untimed seeds searched before timing starts (default `200`) |
| `--challenges N` | challenge bit mask (default `0`) |
| `--no-vault` | skip the Imp's Vault |
| `--skip-boss-floors` | step over the state-neutral boss depths 5, 10, 15 and 25 |
| `--print-matches` | print each matching seed code before the `BENCH` line |

The run ends with one line:

```
BENCH item=RunicBlade+5 floors=19 start=200 warmup=200 seeds=2000 matches=48 elapsed=57.412 seeds_per_s=34.8
```

## What it searches

Per seed it calls the game's own `Dungeon.initSeed()`, `Dungeon.init()` and
`Dungeon.newLevel()` for floors 1..N, then — when the Imp has spawned — builds
the Imp's Vault (branch 1 of the Imp's floor), and scans everything generated
for a searchable item (weapon, armor, wand or ring) of the wanted class at the
wanted `trueLevel()`:

- every heap and its containers, on main floors and in the vault;
- mimics' contents, statues' weapons and armor, the sacrificial-fire prize;
- the Ghost, Wandmaker, Blacksmith and Imp reward options;
- the Imp's depth-20 shop cache, when `--floors` reaches the boss level.

Reading `trueLevel()` (rather than `name()` or `identify()`) is what keeps the
scan from mutating a generated item, so the search cannot perturb the run it is
searching. A seed's floors stop being generated as soon as a match is found.

## Headless technique

Startup is the oracle's, and the oracle's README explains it in full:
`Game.version` is set to a `-INDEV` string so `DeviceCompat.isDebug()` marks the
journal read, `GameSettings.set(new MemoryPreferences())` provides in-memory
settings, `Badges.global` and the `Bones` fields are reset, and the geometry-only
`com.watabou.noosa.TextureFilm` stand-in keeps `ItemSpriteSheet.Icons.film` off
the GPU. That stand-in is compiled straight out of `tooling/oracle-4.0/src`
rather than copied, so the two tools cannot drift apart.

Two things are specific to searching many seeds in one process:

1. **A second stand-in, `ItemSprite`.** Generation is otherwise sprite-free,
   but `Level.drop()` builds a throwaway `Heap` with a `new ItemSprite()` when
   the item it is given is null, and `RingRoom.placeCenterDetail` gives it
   `Level.findPrizeItem()`, which is null once a floor's prize items are spent.
   The upstream constructor needs a GL context, so roughly one seed in a hundred
   dies inside nineteen floors. `src/.../sprites/ItemSprite.java` replaces the
   class with a drawing-free one: it still extends `MovieClip`, but its
   constructors stop at `MovieClip()`, every method is a no-op, and it touches
   `com.watabou.utils.Random` nowhere, so the seeded stream is unchanged. The
   heap that path builds holds no item, so nothing searchable is lost.
   **`tooling/oracle-4.0` has the same gap** and crashes on those seeds
   (`AAA-AAA-ADU` is one); the stand-in is kept here rather than shared until
   the oracle's fixtures are re-run with it.
2. **Run statics the game never has to reset.** `Dungeon.init()` resets the
   Generator, the limited drops and each quest, but `Imp.Quest.reset()` leaves
   `rewardOptions`, `oldQuest` and `alternative` alone — upstream clears the
   reward options in `VaultFinalRoom.paint()` and never reuses a process. In a
   seed loop a run whose vault is never built would hand its reward options to
   the next seed, which then reports them as its own. `JarSeedFinder` restores
   all three to the values a fresh JVM holds before every seed.

Without the second fix the finder reports steadily more matches the longer it
runs; the cross-check below is what makes such a leak visible.

## Threads

The game's generator state is global (`Dungeon`, `Generator`, `Random`,
the quest classes), so one JVM searches one seed at a time. A multi-core figure
comes from several processes over disjoint ranges, the way the incumbent's own
`tools/turbo.js` drives it:

```sh
for i in $(seq 0 5); do
    tooling/java-finder/run.sh --start $((i * 100000)) --seeds 2000 &
done
wait
```

## A fair comparison

Seed Seeker plans a query before searching, and for the benchmark query the plan
takes two shortcuts that cost it no exactness. Both are available here, and the
published numbers use them, so that neither side is generating floors the other
one skips:

- `--no-vault`. A `+5` only ever appears on a tier-4 weapon in
  `Imp.Quest.rewardOptions`, which is rolled on the Imp's City floor; the
  vault's own treasure stops at `+4`. The engine builds the sub-level only when
  a requirement could be met by vault treasure, so for this query it does not,
  and neither should the baseline. The reward options are read on the Imp's
  floor either way.
- `--skip-boss-floors`. Depths 5, 10, 15 and 25 leave the Generator, the limited
  drops, the quests and the room queues untouched — `tooling/oracle-4.0`'s
  `boss-skips.sh` pins that for three seeds — and each floor is seeded
  independently by `Dungeon.seedForDepth`, so stepping over them changes no
  later floor. The engine never simulates them. Depth 20 is *not* neutral (it
  caches the Imp's shop) and is never skipped.

Both flags are verified, not assumed: with and without them the finder returns
the same seeds as the engine over the first 10,000.

## Cross-check

The finder and the engine must agree on which seeds match, or the throughput
comparison is meaningless. Over the first 10,000 seeds both report the same 251
seeds for the canonical query (`RunicBlade` at `+5` within 19 floors):

```sh
tooling/java-finder/run.sh --seeds 10000 --warmup 0 --no-vault --skip-boss-floors \
    --print-matches | grep -v '^BENCH' > java.txt

# the engine's count over the same range: "Matches: 251"
cargo run --release -p shpd-seedfinder-cli -- --benchmark 10000

# the engine's list: it scans in ascending order, so stop at the last match below 10,000
cargo run --release -p shpd-seedfinder-cli -- --items runic-blade.json \
    | sed "/^$(tail -1 java.txt)$/q" > engine.txt
diff java.txt engine.txt
```

where `runic-blade.json` is the canonical query:

```json
{"max_depth":19,"requirements":[{"kind":"weapon","item":"runic_blade","upgrade":5}]}
```

A second query shape checks the paths the canonical one never reaches — the
vault sub-level and the Wandmaker's rewards. `--item WandOfFireblast --upgrade 3
--floors 24` and
`{"max_depth":24,"requirements":[{"kind":"wand","item":"wand_fireblast","upgrade":3}]}`
return the same 195 seeds over the first 2,000.

## Isolation

No file outside this directory is written; `.work/` (the JAR and the compiled
classes) is gitignored. Shattered Pixel Dungeon is GPL-3.0 software, and the
finder and its stand-in are provided under the same license.
