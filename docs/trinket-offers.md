# Trinket search and scout

The 17 named trinket item IDs search the **first four catalyst offers**.
A trinket requirement must name an item; `kind: "trinket"` alone is rejected.
Every platform requirement editor labels this category "Trinket" and has no source or
floor Details controls for it. For example:

```json
{
  "requirements": [
    { "any_of": [{ "item": "mimic_tooth" }, { "item": "rat_skull" }] }
  ],
  "max_depth": 3
}
```

Offers inherit the generated catalyst's floor, source, accessibility and
secret status. A floor limit before its placement cannot match. Multiple
different offers may satisfy an AND query: this means they are offered
together, not that the player can keep multiple trinkets. Upgrade and effect
predicates do not describe offers.

Probability estimates enumerate all 2,380 equally likely four-trinket subsets
and match distinct identities to the query's AND/OR slots. A named trinket has
probability 4/17; two named trinkets joined by OR have probability 29/68, and
joined by AND have probability 3/68. Duplicate requirements cannot reuse one
offer. Explicit wire floor limits share the catalyst's single, uniformly drawn
floor from 1-3. Equipment requirements use the existing supply estimate,
approximating independence from the private trinket deck and catalyst
placement/accessibility. Mixed trinket/equipment OR groups conservatively use
the best feasible residual equipment query for each subset, rather than adding
overlapping ways to complete it. Explicit wire catalyst-source filters and
trinket combined-level groups remain unavailable because the estimator does
not model them.

The WASM scout JSON adds `trinketOrder`, an array of exactly 17 entries with
`id`, `name`, and `spriteIndex`. It follows private-deck draw order, independently
of the manifest's item sorting. Entries 0–3 are the initial choices. The other
13 entries are diagnostic deck order and never participate in offer matching;
they do not predict gameplay transmutations. `items` contains four records with
`category: "trinket"`, the catalyst's placement metadata and the normal `matched`
flag. The scout views group these beneath a single Magical catalyst entry.

The engine reads a cloned category deck without consuming the world RNG.
Requirements default to searching offers without equipping a trinket. In the
web requirement editor, **Choose matching trinket at +3** opts into selection
(`select_trinket: true` in query JSON). Effects begin on the next generated
floor after both the catalyst and the first alchemy pot have become available.
An earlier visited pot can be revisited when the catalyst is found. The pot's
own floor is generated before brewing and stays unchanged.

For a chosen OR slot, all its alternatives count: exactly one distinct initial
offer must match. Two or more matches mean **No Trinket**, even when only one
alternative carries the selection flag. Ambiguity across multiple chosen slots
also means No Trinket. The remaining 13 deck entries never affect selection.

The engine applies the +3 generation effects of Mimic Tooth, Parchment Scrap,
Rat Skull, Exotic Crystals, Mossy Clump, Trap Mechanism, and Cracked Spyglass.
Other trinkets can be chosen but their gameplay effects do not alter the
canonical generated world. Equipment probability estimates use a separate
measured profile for each of these seven trinkets (32,768 worlds each).
Per-floor supply, upgrades, curses, enchantments and tier shares include the
actual first brewing opportunity; the unchanged first two floors retain the
canonical tables. Duplicate scarcity and item-count variation also use the
selected profile. Trinkets without generation effects reuse No Trinket.

The estimator enumerates the 2,380 initial four-offer subsets. Each subset uses
the uniquely selected trinket's equipment profile, or No Trinket for ambiguous
matches, before averaging its chance of satisfying the entire query. This
preserves exact offer probabilities, including OR groups; equipment results
remain statistical estimates with the existing matching approximations and
challenge-effects limitation. Calibration averages brewing timing across
catalyst placements rather than conditioning equipment supply on an explicit
catalyst floor filter.

Click any of the four initial-choice cards in the scout to apply it, or
click the applied card again to clear the selection. An override
regenerates that seed with the same brewing timing and can be changed back to
the original match. A new scout request starts from the query's selection.
WASM scout requests accept `trinket: "none"` or an initial offer's stable ID;
omitting it resolves the query automatically. Responses include
`selectedTrinket` (a stable ID or null). Overrides do not edit the search query.

Queries without selection keep their version-4 share codes. Selected queries
use version 5; both versions decode in this release. The selection controls
and scout overrides are integrated in web, Windows, macOS, Linux, and Android.

Web, Windows, macOS, Linux, and Android share named trinket search and OR
semantics. Each scout uses four square initial-choice cards, a flat green
matched border/fill, single-line names that shrink to fit, and one row of 13
smaller nearest-neighbor icons below "Remaining deck order." The platform's
own controls, typography, and colors provide the surrounding UI.

Legacy native scout responses use `SSC4`: the existing `SSC3` layout,
with its magic changed, followed after the item records by `trinket_count:u8`
(equal to 17) and 17 `stable_item_id:utf8_u16` strings in draw order. The
four initial choices are ordinary item records carrying catalyst placement
metadata, and match indices address that full item stream. Native decoders
also accept legacy `SSC3` packets without deck metadata. Linux calls the same
typed generator and `trinket_order(seed)` directly. The shared Android catalog
asset supplies the same 17 title-case names and sprites to every platform.
Query JSON and share-link codecs use category code 6 and appended item codes,
preserving existing links.

Selected native scouting uses `SSQ3`: four magic bytes, a little-endian u16
challenge mask, a little-endian u16-length UTF-8 seed, a little-endian
u16-length UTF-8 override, and the canonical query JSON in all remaining
bytes (empty is allowed). An empty override resolves the query, `none`
deselects, and a stable ID must name one of the four initial offers.
The response uses `SSC5`, extending `SSC4` with one big-endian u16-length
selected stable ID; an empty string means no selection. Scout matches must
use the same request bytes so item indices refer to the selected world.
Linux uses `production_scout_world_selected` directly with equivalent semantics.

`TrinketOracle` verifies the order against the pinned BETA-4 desktop JAR:

```sh
tooling/oracle-4.0/build.sh
java -cp "tooling/oracle-4.0/.work/classes:tooling/oracle-4.0/.work/ShatteredPD-v4.0.0-BETA-4-Java.jar" \
  com.shatteredpixel.shatteredpixeldungeon.TrinketOracle AAA-AAA-AAA
```

Use `;` instead of `:` as the classpath separator on Windows. Lines prefixed
`trinket_order` contain the 1-based draw position, Java class and sprite index.
The committed Rust fixture pins all 17 positions, and the WASM test verifies
the shared catalog against the same output contract.

For full generation parity, `ParityOracle` accepts the JVM property
`-Dseedfinder.trinket=MimicTooth` (or another upstream trinket class). It equips
that trinket at +3 after the same brewing opportunity and explicitly marks all
journal pages read, including the final Halls lore page. `dump_floors` accepts
the stable ID after the challenges argument, e.g. `AAA-AAA-AAA 24 0 mimic_tooth`.
