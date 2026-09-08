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

The engine reads a cloned category deck without consuming the world RNG or
changing generation. The existing canonical generation profile still applies;
this feature does not simulate choosing, upgrading, or equipping a trinket.

Web, Windows, macOS, Linux, and Android share named trinket search and OR
semantics. Each scout uses four square initial-choice cards, a flat green
matched border/fill, single-line names that shrink to fit, and one row of 13
smaller nearest-neighbor icons below "Remaining deck order." The platform's
own controls, typography, and colors provide the surrounding UI.

Native production scout responses use `SSC5`: the existing `SSC3` layout,
with its magic changed, followed after the item records by `trinket_count:u8`
(equal to 17) and 17 `stable_item_id:utf8_u16` strings in draw order. The
four initial choices are ordinary item records carrying catalyst placement
metadata, and match indices address that full item stream. Native decoders
also accept legacy `SSC3` packets without deck metadata and `SSC4` packets
without feelings. In `SSC5`, a [floor-feeling block](floor-feelings.md) follows
the deck. Linux calls the same
typed generator and `trinket_order(seed)` directly. The shared Android catalog
asset supplies the same 17 title-case names and sprites to every platform.
Query JSON and share-link codecs use category code 6 and appended item codes,
preserving existing links.

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
