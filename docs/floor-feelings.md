# Floor feelings

`GeneratedWorld.feelings` stores the final feeling of each generated regular
floor, in ascending depth order. The regional, batch, and gated generators
carry this data alongside items and quests. Boss floors have no feeling entry;
prefix generation only includes floors within the requested effective depth.
There is no observer, callback, extra generation pass, or independent RNG roll.

All scout panes display the game sprite immediately after the floor label.
Normal floors and old packets without feeling metadata display no extra icon.
Feeling names are available to accessibility APIs without visible text.
The web UI reads the same world data through the WASM `feelings` array:
`[{ "depth": 2, "feeling": "chasm" }]`.

## Native wire format

The current encoder emits `SSC5`. Its prefix is the existing `SSC4` layout:
seed, ring gems, quests, items, and the 17-entry trinket deck. Immediately after
the deck it appends:

```text
feeling_count:u8
repeated feeling_count times { depth:u8, feeling:u8 }
```

| ID | Feeling |
| --- | --- |
| 0 | None |
| 1 | Chasm |
| 2 | Water |
| 3 | Grass |
| 4 | Dark |
| 5 | Large |
| 6 | Traps |
| 7 | Secrets |

Counts are 0..20. Depths must be unique and strictly ascending in 1..24,
excluding boss depths 5, 10, 15, and 20. Unknown feeling IDs, invalid counts,
depths or ordering, truncated fields, and trailing bytes are rejected. A full
canonical scout carries all 20 regular floors, including normal floors (ID 0).

Decoders continue to accept `SSC3` (no deck or feelings) and `SSC4` (deck, no
feelings), returning an empty feelings collection. Current producers always
emit `SSC5`; legacy formats are supported only for decoding.

The sprite atlas is Shattered Pixel Dungeon's `interfaces/icons.png`, pinned
with provenance in the asset attribution. The approved large feeling frames
are 15 by 16 pixels at y=64, x=16 times the feeling ID.
