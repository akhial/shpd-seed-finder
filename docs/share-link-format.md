# Share-link format

Every Seed Seeker frontend can encode the current search query as a short
shareable link, and populate its query editor from such a link. The canonical
link form is

```
https://shpd-seed-seeker.web.app/#q=EAGWhMA
```

where the value after `#q=` is a base64url code carrying the whole query. The
web app reads the fragment on load; Android opens these links natively via
App Links; the desktop apps register the `seedseeker://` URI scheme
(`seedseeker://q/CODE`) and also accept pasted web links. Decoders take the
code from a `q=` parameter introduced by `#`, `?`, or `&` (ending at the next
`&` or `#`), from the last path segment of a `seedseeker://` URI, or from bare
code text.

The canonical implementation and its compatibility tests live in
`crates/seedfinder-core/src/deep_link.rs` — it is the only implementation.
The web app reaches it through WebAssembly, the macOS and Windows apps
through the C FFI (`seedfinder_share_encode` / `seedfinder_share_decode`),
and Android through JNI (`JniBindings.shareEncode` / `shareDecode` /
`shareExtract`); each platform converts between its own models and the
canonical JSON query document at that boundary.

## Payload

The code is base64url (`A–Z a–z 0–9 - _`, no padding) over a bit stream
written most-significant-bit first; unused bits in the final byte are zero.
Decoders reject codes with eight or more leftover bits, nonzero padding, or
values outside the ranges below, then validate the decoded query exactly like
a results-file import.

Optional fields are guarded by a presence bit: `1` means the field's value
bits follow immediately, `0` means the field takes its default and no value
bits are written.

### Header

| Field | Bits | Meaning |
| --- | --- | --- |
| `version` | 4 | Format version, `1`, `2` or `3`. Version 2 extends each requirement record (see below); version 3 is identical to version 2 except that an effect-set mask is 32 bits wide instead of 24, which is what the weapon effects Shattered Pixel Dungeon 4.0.0 added at codes 24–26 need. Encoders write the oldest version that can carry the query — version 1 whenever a query needs nothing version 2 adds, version 3 only when an effect set names an effect at code 24 or above — so plain links keep opening in older releases. Decoders must reject other versions (telling the user the link may come from a newer release), and encoders must never renumber or reorder anything within a version. |
| `require_blacksmith` | 1 | Query flag. |
| `exclude_blacksmith_rewards` | 1 | Query flag. |
| `fast_mode` | 1 | Query flag. |
| `max_depth` | 1 (+5) | Present only when not the default 24. Value is `max_depth − 1` (floors 1–24). |
| `challenges` | 1 (+9) | Present only when nonzero. The upstream challenge bitmask: bit 0 `on_diet` … bit 8 `badder_bosses`, in the order of `CHALLENGE_NAMES` in `json_query.rs`. |
| `wandmaker_quest` | 1 (+2) | Present only when the search filters on a Wandmaker variant. Value is the variant's one-based game value (`WandmakerQuestType` in `quests.rs`) minus one: `0` corpse dust · `1` elemental embers · `2` rotberry. `3` is invalid. |
| requirement count | 6 | Number of requirement records that follow (at most 63). |

### Requirement record

| Field | Bits | Meaning |
| --- | --- | --- |
| `kind` | 3 | `0` weapon · `1` melee_weapon · `2` thrown_weapon · `3` armor · `4` wand · `5` ring. |
| `item` | 1 (+7) | Item code: index into the frozen item table (88 entries today). |
| `tier` | 2 (+3) | Mode `0` any (no value bits) · `1` exact · `2` at_least · `3` at_most, then the tier value. |
| `upgrade` | 2 (+3) | Mode `0` any (no value bits) · `1` exact · `2` at_least, then the upgrade value (up to +5 for weapons, +4 otherwise; the three-bit field predates those ceilings). Mode `3` is invalid. |
| `effect` (v1) | 1 (+5) | Effect code: index into the weapon-effect table for weapon kinds, the armor-effect table for armor. Invalid for wands and rings. |
| `effect` (v2, v3) | 2 (+5, +24 or +32) | Mode `0` any (no value bits) · `1` one effect, then its 5-bit code · `2` any enchantment (every non-curse effect of the family, no value bits) · `3` a set, then a mask whose bit *n* is effect code *n* — 24 bits in version 2, 32 bits in version 3. Modes 1–3 are invalid for wands and rings; a mode-3 mask must be nonzero. Mode 2 carries no codes, so a link asking for "any enchantment" means the whole family of whichever release opens it. |
| `uncursed` | 1 | Requirement flag. |
| `source` | 1 (+5) | Source code: index into the frozen source table (18 entries). |
| `identity_group` | 1 (+8) | Same-item group. The field is eight bits wide, but like the results-file format only groups 1–4 (the editors' A–D) are accepted; 0 and 5–255 are invalid. |
| `max_depth` | 1 (+5) | Value is `depth − 1` (floors 1–24). |
| `alternative_group` (v2, v3) | 1 (+6) | Alternative-group label minus one. Records sharing a label form one "any of" slot; labels are renumbered in first-appearance order when encoding. |
| `level_sum` (v2, v3) | 1 (+10) | Combined-level group: two bits of group label minus one (groups 1–4, the editors' A–D), then the eight-bit minimum total in levels (1–255), where a matched item counts its upgrade plus one. |

### Code tables

The item, effect, and source tables are **append-only**: positions are part
of the persisted format and must never change, because links shared today
must keep decoding in every future release. The authoritative tables — and
the tests that freeze them — are `ALL_ITEM_IDS`, `SOURCE_CODES`, and the
`code_tables_are_frozen` test in `crates/seedfinder-core/src/deep_link.rs`.
Effects use the wire-name order of `ALL_WEAPON_EFFECTS` / `ALL_ARMOR_EFFECTS`
in `catalog.rs` (27 weapon entries — the 21 of v3.3.8 followed by the six
4.0.0 added, Venomous, Eldritch, Vorpal, Crystal, Pressurized and Wondrous,
at codes 21–26 — and 21 armor entries). If the game ever adds items, they get
fresh codes at the end of the table; existing codes stay put.

## Shared fixture

Every platform pins this vector:

- Query document: `{"requirements":[{"item":"wand_fireblast","kind":"wand","upgrade":{"at_least":3}}]}`
- Code: `EAGWhMA`
- Link: `https://shpd-seed-seeker.web.app/#q=EAGWhMA`

Encoding validates the query first, so a produced link always decodes;
decoding returns the canonical query document (defaults omitted, `kind`
spelled out).
