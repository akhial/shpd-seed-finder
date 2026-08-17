# Results export format

Every Seed Seeker frontend can export the current search results — together
with the query that found them — to a JSON file, and import such a file to
restore both the results list and the query editor. All platforms read and
write the same schema. The canonical implementation and its compatibility
tests live in `crates/seedfinder-core/src/results_export.rs`, and every
platform links that engine: the codec is exposed over FFI
(`seedfinder_results_encode`/`seedfinder_results_decode`), WASM
(`encode_results_file`/`decode_results_file`) and JNI
(`resultsEncode`/`resultsDecode`). Frontends must delegate to those entry
points rather than re-implement the schema, so there is exactly one reader and
one writer of this format.

Exports always contain the query **that produced the listed results**: every
app snapshots the query when a search starts (or when a file is imported) and
exports that snapshot, not the live editor state, so a file never claims a
query that did not produce its seeds.

## Envelope

```json
{
  "format": "seed-seeker-results",
  "app_version": "0.6.1",
  "shpd_version": "3.3.8",
  "query": { "requirements": [ { "item": "ring_wealth", "upgrade": 4 } ] },
  "results": [
    { "seed": "AAA-AAA-BUH" },
    { "seed": "ABC-DEF-GHI" }
  ]
}
```

| Field            | Type    | Required | Meaning                                                                 |
| ---------------- | ------- | -------- | ----------------------------------------------------------------------- |
| `format`         | string  | yes      | Always `"seed-seeker-results"`. Distinguishes these files from other JSON. |
| `app_version`    | string  | no       | App version that wrote the file. Informational only.                    |
| `shpd_version`   | string  | no       | Upstream Shattered Pixel Dungeon version the exporting engine targeted. See *Cross-version imports* below. |
| `query`          | object  | yes      | The query that produced the results, in the shared JSON query-document format (see below). |
| `results`        | array   | yes      | The exported results, in display order. May be empty.                   |

Each entry of `results` is an object with one required field:

| Field  | Type   | Required | Meaning                                                     |
| ------ | ------ | -------- | ------------------------------------------------------------ |
| `seed` | string | yes      | Seed code in **strictly canonical** `XXX-XXX-XXX` form: nine uppercase `A–Z` digits with dashes after the third and sixth. Lowercase, undashed, or whitespace-padded codes are rejected, so a file that imports on one platform imports on all of them. |

Result entries are objects (not bare strings) so future releases can attach
per-result metadata without a format break.

## The `query` object

The query reuses the existing JSON query-document format shared by the CLI
(`seed-seeker --query`), the web frontend, and the presets on every platform.
It is decoded by `crates/seedfinder-core/src/json_query.rs`:

- `requirements` — non-empty array of requirement objects with the optional
  fields:
  - `kind` — `"weapon" | "melee_weapon" | "thrown_weapon" | "armor" | "wand"
    | "ring"` (required when `item` is absent). `"weapon"` matches melee and
    thrown weapons alike; the two narrowed kinds were added alongside the
    melee/thrown search filters — a file that uses them simply fails to
    import on builds older than that feature, with the codec's
    unknown-category message,
  - `item` — catalog stable id such as `"ring_wealth"`,
  - `tier` — `"any"` (the default) or exactly one of `{"exact": n}`,
    `{"at_least": n}`, `{"at_most": n}`,
  - `upgrade` — `"any"` (the default), a bare number `n` (shorthand for
    exact), or exactly one of `{"exact": n}`, `{"at_least": n}`,
  - `effect` — enchantment/glyph wire name such as `"Blazing"` or
    `"Anti-Magic"` (matched case-insensitively),
  - `uncursed` — boolean,
  - `source` — snake_case source name such as `"imp_reward"`,
  - `identity_group` — integer 1–4 (groups A–D; the engine allows more, but
    no app's editor can express them, so the file format caps at 4),
  - `max_depth` — integer 1–24.
- `max_depth` (integer 1–24, default 24), `require_blacksmith`,
  `exclude_blacksmith_rewards`, `fast_mode` (booleans) — top-level scope
  flags.
- `wandmaker_quest` — `"corpse_dust" | "elemental_embers" | "rotberry"`,
  restricting the search to runs whose Wandmaker asks for that item. Absent
  means any quest.
- `challenges` — array of snake_case challenge names (`on_diet`,
  `faith_is_my_armor`, `pharmacophobia`, `barren_land`, `swarm_intelligence`,
  `into_darkness`, `forbidden_runes`, `hostile_champions`, `badder_bosses`).

Enum names (`kind`, `source`, `challenges`) are matched **exactly** (lowercase
snake_case); only `effect` names and the `"any"` keyword are matched
case-insensitively, mirroring the core decoder.

Writers omit defaults (`"tier": "any"`, `"upgrade": "any"`, `false` flags,
`"max_depth": 24`, an empty `challenges` list) and write `upgrade` exact
filters as the bare-number shorthand, so exported documents stay minimal and
identical across platforms.

## Compatibility direction

The format guarantees exactly one direction: **whatever an app exported, every
later app still imports.** That is what the frozen fixtures below pin.

The reverse is not guaranteed. Readers validate the `query` strictly, so a
query field a build does not know fails the import by name — a file from a
newer app may well be unreadable by an older one. Documents used to carry a
`format_version` for that case, but a version number cannot make an old build
understand a new field; it only changes which error the user sees. So the
field is gone: releases up to 0.7.0 wrote it, later ones do not, and readers
ignore it wherever they find it.

## Compatibility rules

Readers must follow these rules; they are what lets files exported today stay
importable forever:

1. **Reject non-results files clearly.** If `format` is missing or not
   `"seed-seeker-results"`, report that the file is not a Seed Seeker results
   file.
2. **Ignore unknown *envelope* fields and unknown *per-result* fields.** That
   includes `format_version` in files written up to 0.7.0 — whatever number
   it carries, valid or not — and any optional field a future release adds
   (an export timestamp, per-result annotations); readers must skip them
   silently. Note the flip side: an app that imports such a file and
   re-exports it writes only the fields it knows, so round-tripping through
   an older app drops newer optional fields.
3. **Be strict about the `query` contents.** Unknown query fields, item ids,
   effects, sources, or challenge names — and any field whose value has the
   wrong JSON type (for example `"max_depth": "12"`, `"item": 42`,
   `"upgrade": true`, or `"challenges": "barren_land"`) — must fail the
   import with a message naming the offender. Silently dropping or coercing a
   constraint would make the restored query mean something different from the
   one that produced the results. (This is also what happens when a file from
   a newer app references an item that this build's catalog does not know.)
   A JSON `null` for an optional string/integer field counts as absent.
4. **Validate seed codes strictly** (canonical form, rule table above) and
   report the index of the first invalid entry.
5. **Deduplicate, then cap.** After decoding, duplicate seed codes are dropped
   (keeping the first occurrence) and the restored list is capped at the
   shared result limit (1,024 seeds), in that order; apps must tell the user
   how many entries were dropped. The decode entry points apply this rule
   themselves and report the count as `dropped` alongside the restored
   `seeds`, so a given file restores the same list on every platform, UI list
   keys stay unique, and an adversarial file's cost stays bounded.
6. **Bound resource use.** Files larger than 2 MiB are refused (a maximal
   legal file is far smaller). The cap is enforced by the engine —
   `results_export::MAX_FILE_BYTES`, applied by `decode` and therefore by
   every platform's decode entry point — so apps only need to parse imports
   off the UI thread. Parsers may also impose implementation nesting limits
   (serde_json caps recursion at 128 levels), so ignored unknown fields should
   stay shallow.

Writers must:

1. Write `format`, `query`, and `results` always, plus `app_version` and
   `shpd_version`. Never write `format_version`.
2. Only emit fields documented here, and only those the query actually needs
   (see the defaults rule above).
3. Keep every field's meaning stable. A field this document already describes
   may not be renamed, removed, or redefined: the next release must still read
   what this one wrote. New optional fields are fine, in the query as well —
   they cost older apps the import (rule 3), which is the accepted trade.

## Cross-version imports (`shpd_version`)

`shpd_version` records which upstream Shattered Pixel Dungeon generation the
exporting engine targeted. Importers do not reject on mismatch — the file is
still structurally valid — but they compare it against their own engine
version and warn the user that the listed seeds may generate different
dungeons under the importing app's engine. The constant lives in the Rust
core (`SHPD_VERSION` in `crates/seedfinder-core/src/lib.rs`); the Swift,
Kotlin, and C# codecs mirror it and must be updated together on an upstream
version bump.

## Fixtures

The format is pinned by shared, frozen fixtures under
`crates/seedfinder-core/tests/fixtures/`: `results-export-v1.json`,
`results-export-v1-weapon-categories.json` (the narrowed
`melee_weapon`/`thrown_weapon` kinds) and `results-export-wandmaker-quest.json`
(the quest filter). The core unit tests, the web tests (via a raw import), the
Android tests, and the macOS tests all decode **those same files**, so a
platform codec cannot silently drift from the canonical schema. Windows has no
test harness in this repo; its codec must be kept in sync by review.

The two `v1` fixtures keep the `"format_version": 1` their era's writers
emitted — they are exactly the documents already in users' hands, and decoding
them is the regression test for the one guarantee above.

When evolving the format, never edit an existing fixture: add one for the new
field and keep the old ones passing.

## Import semantics

Importing a results file **replaces** the current results list and the query
editor state on every platform (after full validation, so a bad file never
half-applies), and records the imported query as the export snapshot so
re-exporting reproduces the file. Imports are refused while a search is
running — including when a search started while the file picker was open.
The informational `app_version` field is not compared against the running
app; a file is importable regardless of which app version wrote it.
