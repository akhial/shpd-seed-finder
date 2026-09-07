# Offered trinkets: web pilot

The 17 named trinket item IDs search the **first four catalyst offers**.
A trinket requirement must name an item; `kind: "trinket"` alone is rejected.
The web requirement editor labels this category "Trinket" and has no source or
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
floor from 1–3. Equipment requirements use the existing supply estimate,
approximating independence from the private trinket deck and catalyst
placement/accessibility. Mixed trinket/equipment OR groups conservatively use
the best feasible residual equipment query for each subset, rather than adding
overlapping ways to complete it. Explicit wire catalyst-source filters and
trinket combined-level groups remain unavailable because the estimator does
not model them.

The WASM scout JSON adds `trinketOrder`, an array of exactly 17 entries with
`id`, `name`, and `spriteIndex`. It follows private-deck draw order, independently
of the manifest's item sorting. Entries 0â€“3 are the initial choices. The other
13 entries are diagnostic deck order and never participate in offer matching;
they do not predict gameplay transmutations. `items` contains four records with
`category: "trinket"`, the catalyst's placement metadata and the normal `matched`
flag. The web scout groups these beneath a single Magical catalyst entry.

The engine reads a cloned category deck without consuming the world RNG or
changing generation. The existing canonical generation profile still applies;
this feature does not simulate choosing, upgrading, or equipping a trinket.

The pilot UI is web-only. Native production scout responses retain their
equipment-only manifest and corresponding match indices because the native
catalogs and scout layouts do not yet represent offer sets. The shared query
JSON and share-link codecs understand trinket requirements; category code 6
and appended item codes preserve existing links. The complete 17-entry order
is exposed through WASM JSON, without changing the legacy SSC3 packet layout.

`TrinketOracle` verifies the order against the pinned BETA-4 desktop JAR:

```sh
tooling/oracle-4.0/build.sh
java -cp "tooling/oracle-4.0/.work/classes:tooling/oracle-4.0/.work/ShatteredPD-v4.0.0-BETA-4-Java.jar" \
  com.shatteredpixel.shatteredpixeldungeon.TrinketOracle AAA-AAA-AAA
```

Use `;` instead of `:` as the classpath separator on Windows. Lines prefixed
`trinket_order` contain the 1-based draw position, Java class and sprite index.
The committed Rust fixture pins all 17 positions, and the WASM test verifies
the web pilot's catalog against the same output contract.
