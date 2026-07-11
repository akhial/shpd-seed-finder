# Compatibility boundary

Shattered Pixel Dungeon generation changes between versions. Every engine result
must therefore carry the target version, commit, and run profile. This project
targets v3.3.8 at commit
`7b8b845a76fe76c6b7c031ae9e570852411f56db`.

## What “exact” means

The Rust engine reproduces the seeded, static world loot visible in a fresh
custom-seed run under the canonical profile:

- main branch floors generated in ascending order;
- Warrior, no challenges, no equipped trinket;
- no bones, documents, or other profile-dependent bonus items;
- tutorial/journal progression complete (`SPDSettings.intro() == false`);
- no player-caused drops or inventory-dependent mutations;
- weapons, armor, wands, rings, their true upgrade, cursed flag, enchantment/glyph,
  floor, source, container, and mutually exclusive choice group.

Normal monster death loot is excluded. It is rolled during play after the level
generator has been popped and is generally based on the unseeded base RNG.
The seed scout reports this same static, searchable set rather than claiming to
be a complete inventory of every consumable or future drop in a run.

Search source constraints refer to the source stored on these static records.
Same-item groups require distinct obtainable copies with the same concrete item
ID, while normal AND requirements may match different types. The blacksmith
condition is satisfied only when the generated Blacksmith quest room is
accessible within the selected maximum floor; a failed room build does not
count. The Smith-reward exclusion independently prevents those choice items from
satisfying requirements while still allowing the room to meet the blacksmith
condition. The overall maximum floor and each item's optional floor limit are
passed into planning, so later regions are not simulated once every remaining
requirement has reached its deadline.

## Boss-floor transitions

Under the canonical fresh custom-seed profile, depths 5, 10, 15, and 25 can be
skipped by the search engine after their independent depth roots are computed.
Official v3.3.8 checkpoints across three seeds show identical before/after
Generator, LimitedDrops, quest, special/secret-room queue, and shop-dependent
state. Their initial level creation contains no searchable weapon, armor, wand,
or ring. Depth 10 does directly place an Iron Key, which is outside the searchable
catalog. The boss levels also seed a child generator for `Bones.get()`; with
bones disabled it produces no items, and both the child and all map RNG are
confined to that boss depth.

Depth 20 must not be skipped. `CityBossLevel.build()` creates an `ImpShopRoom`,
and `ImpShopRoom.paint()` eagerly calls `ShopRoom.generateItems()` before the
shop is visible. The resulting cached tier-five weapon, Plate Armor, missile,
tipped darts, and possible rare wand or ring are searchable initial-world content, and
the call mutates the live Generator state consumed by Halls. The Rust prefix
therefore executes this transition between depths 19 and 21 and includes its
shop records. Rewards generated only after fighting a boss remain excluded as
later gameplay loot.

The Imp quest ring is generated when its accessible City quest room is created,
including the two deterministic reward upgrades. It is therefore searchable and
reported with source `ImpReward`, even though collecting it requires completing
the quest. This is the canonical source of searchable `+4` rings.

## Why floors are simulated sequentially

Each floor has an independent depth root, but the world is not floor-independent.
Generator card decks, special/secret-room schedules, limited drops, and quest
rewards persist across the run. Spatial generation must also be reproduced:
room placement, painting, mob placement, and random-cell rejection loops consume
the same floor stream before and between item rolls.

## Upstream nondeterministic edges

On a first installation, v3.3.8 places the early Guidebook with an unseeded
auxiliary RNG. That heap can change how many isolated grass-painter draws are
skipped and can therefore change the decorative map. Like established seed
finders, this project uses completed tutorial/journal state and does not place
that Guidebook. No seeded RNG stream is replaced or canonicalized.

Two secret rooms select consumables from a real Java `HashMap<Class, …>`.
`Class.hashCode()` identity and map iteration order are not specified across JVM
implementations. Their consumable-only output cannot be intrinsically identical
between desktop OpenJDK and Android ART. These rooms do not generate the target
weapon/armor/wand/ring identity, but the oracle records the runtime/order policy when
their output is compared.

Entrance guide pages and missile set IDs also use unseeded randomness. Neither
changes searchable equipment type, level, enchantment, or glyph and both are
normalized out of parity snapshots.

`ShopRoom.ChooseBag` has an RNG-free equal-score tie whose result depends on
`HashMap` iteration (observed as Magical Holster versus Potion Bandolier across
JVM versions). It changes only non-searchable bag/limited-drop metadata. The
official oracle preserves the runtime result and records JVM provenance.

## Parity gates

1. Seed text/code conversion, including Java UTF-16 hashing.
2. MX3, Java's 48-bit LCG, bounded rejection, signed `nextLong`, floats,
   distributions, and both shuffle overloads.
3. Run initialization and every persistent deck/queue seed.
4. Phase snapshots for rooms, map, mobs, items, and final generator state.
5. Whole-floor item/event snapshots across randomized and targeted seeds.
6. An Android/ART oracle pass for runtime-sensitive collection ordering.

The pinned desktop fixtures use Temurin 21. `tooling/oracle` retains official
Java behavior and has no diagnostic RNG canonicalization.

Android native builds use Rust `opt-level=2` with fat LTO. The workspace's
host release profile remains O3. With rustc 1.94/LLVM 21.1.8, Android AArch64
O3 miscompiles the otherwise scalar seed-1 City prefix even though all SIMD
depth roots still match their scalar values. O2 is exact in on-device scalar
and batch fixtures and in a 4,096-seed depth-24 device scan. This target build
override is therefore part of the pinned compatibility profile, not a tuning
preference.
