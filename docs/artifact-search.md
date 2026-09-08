# Artifact search and scout

The engine searches the 11 artifacts in the deterministic spawn deck. A
requirement must name an artifact: `kind: "artifact"` alone is rejected, just
like a wildcard trinket. Cloak of Shadows and Holy Tome have zero spawn weight
and are not offered in the artifact picker.

```json
{
  "requirements": [
    { "item": "ethereal_chains", "max_depth": 9 },
    { "item": "sandals_of_nature", "source": "imp_reward", "upgrade": 5 }
  ],
  "max_depth": 19
}
```

Artifacts support named AND/OR queries, individual floor limits, source and
uncursed filters. Artifact editors use any upgrade and do not offer an upgrade
selector; scout results still show the generated upgrade. The generator's existing unique artifact deck is unchanged.
Only artifacts generated at deterministic locations participate: heaps,
containers, skeletons, shops, pre-generated mimic contents and the Imp reward.
Runtime drops, transmutations, purchases affecting later RNG, and other player
actions that could change the deck are not simulated.

Ordinary artifacts are +0. The Imp quest's Dwarven vault artifact receives
`transferUpgrade(5)`. Scout badges match the game's rounded displayed level:
**+7** for Sandals of Nature (internal level 2), **+6** for Ethereal Chains and
Timekeeper's Hourglass (internal level 3), and **+5** for the other artifacts.
Both the transfer and display conversions round to the nearest integer, with
halves rounding up. Raw wire/search upgrades retain the transfer amount for
compatibility with existing queries and probability tables. That reward
is uncursed and shares the vault's single-pick accessibility group with all
other Imp rewards and vault treasures. Matching cannot acquire it together
with another option from that group.

Artifact records carry the normal source, floor, curse, secret and accessibility
metadata. Core `SSC3`/`SSC4` wire records use stable artifact IDs with the existing
upgrade field. Query JSON and result files use `artifact`; share links append
category code 7 and artifact item codes, preserving previous codes. WASM scout
JSON reports `category: "artifact"` and the normal `matched` flag.

Web, Android, Windows, macOS, and Linux use the shared item catalog for artifact
pickers and scout results. Native scout packets and match indices include the
same full artifact stream as the core generator and WASM.

Artifact probability estimates use deterministic supply measured over 200,000
BETA-4 worlds, including floor, source, curse, and upgrade distributions.
Single-item estimates use the measured supply; joint artifact estimates average
all valid assignments of their unique identities over 4,096 anonymous generated
layouts. This respects the finite deck and each layout's accessibility choices.
The layouts are regenerated with `cargo run --release --example calibrate_artifacts`
into `src/probability_tables/artifact_worlds.bin` within the core crate.
Artifacts compete with equipment for the Imp vault prize. Like equipment estimates, mixed supply and alternative groups retain
the estimator's documented approximations.

Tests pin all 11 artifact identities across eight worlds against the official
BETA-4 JAR and cover
floor limits, gated/scalar search, vault exclusivity, codecs, and the web scout.
The parity oracle emits `search_upgrade` alongside `true_level` so transferred
artifact levels can be compared without losing the underlying Java level.
