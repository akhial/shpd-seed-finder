# Results export format

Every Seed Seeker frontend can export the current search results — together
with the query that found them — to a JSON file, and import such a file to
restore both the results list and the query editor. All platforms read and
write the same schema. The canonical implementation and its compatibility
tests live in `crates/seedfinder-core/src/results_export.rs`; frontends that
cannot link the core crate (web, Android, macOS, Windows) re-implement the
schema and pin it against the shared frozen fixture with their own tests.

Exports always contain the query **that produced the listed results**: every
app snapshots the query when a search starts (or when a file is imported) and
exports that snapshot, not the live editor state, so a file never claims a
query that did not produce its seeds.

## Envelope

```json
{
  "format": "seed-seeker-results",
  "format_version": 1,
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
| `format_version` | integer | yes      | Schema version, strictly a positive integer. This document describes version `1`. |
| `app_version`    | string  | no       | App version that wrote the file. Informational only.                    |
| `shpd_version`   | string  | no       | Upstream Shattered Pixel Dungeon version the exporting engine targeted. See *Cross-version imports* below. |
| `query`          | object  | yes      | The query that produced the results, in the shared JSON query-document format (see below). |
| `results`        | array   | yes      | The exported results, in display order. May be empty.                   |

Each entry of `results` is an object with one required field:

| Field  | Type   | Required | Meaning                                                     |
| ------ | ------ | -------- | ------------------------------------------------------------ |
| `seed` | string | yes      | Seed code in **strictly canonical** `XXX-XXX-XXX` form: nine uppercase `A–Z` digits with dashes after the third and sixth. Lowercase, undashed, or whitespace-padded codes are rejected, so a file that imports on one platform imports on all of them. |

Result entries are objects (not bare strings) so future versions can attach
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
    melee/thrown search filters as an **additive enum value within format
    version 1** — a file that uses them simply fails to import on builds
    older than both features, with the codec's unknown-category message,
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

## Compatibility rules

Version 1 readers must follow these rules; they are what lets files exported
today stay importable forever, and files from slightly newer apps degrade
gracefully:

1. **Reject non-results files clearly.** If `format` is missing or not
   `"seed-seeker-results"`, report that the file is not a Seed Seeker results
   file.
2. **Check `format_version` first.** If it is missing, report that; if it is
   not a positive integer (booleans, strings, fractions, zero, and negatives
   all fail), report an invalid version; if it is *greater* than the newest
   version the reader understands, fail with a message telling the user to
   update the app. Never guess at a newer schema.
3. **Ignore unknown *envelope* fields and unknown *per-result* fields.** A
   future release may add optional fields there (for example an export
   timestamp or per-result annotations) without bumping `format_version`;
   version-1 readers must skip them silently. Note the flip side: an app that
   imports such a file and re-exports it writes only the fields it knows, so
   round-tripping through an older app drops newer optional fields.
4. **Be strict about the `query` contents.** Unknown query fields, item ids,
   effects, sources, or challenge names — and any field whose value has the
   wrong JSON type (for example `"max_depth": "12"`, `"item": 42`,
   `"upgrade": true`, or `"challenges": "barren_land"`) — must fail the
   import with a message naming the offender. Silently dropping or coercing a
   constraint would make the restored query mean something different from the
   one that produced the results. (This is also what happens when a file from
   a newer app references an item that this build's catalog does not know.)
   A JSON `null` for an optional string/integer field counts as absent.
5. **Validate seed codes strictly** (canonical form, rule table above) and
   report the index of the first invalid entry.
6. **Deduplicate, then cap.** After decoding, importers drop duplicate seed
   codes (keeping the first occurrence) and cap the restored list at the
   shared result limit (1,024 seeds), in that order, and must tell the user
   how many entries were dropped. This keeps a given file restoring the same
   list on every platform, keeps UI list keys unique, and bounds the work an
   adversarial file can cause.
7. **Bound resource use.** Apps refuse files larger than 2 MiB (a maximal
   legal file is far smaller) and parse imports off the UI thread. Parsers
   may also impose implementation nesting limits (serde_json caps recursion
   at 128 levels), so ignored unknown fields should stay shallow.

Writers must:

1. Write `format`, `format_version`, `query`, and `results` always, plus
   `app_version` and `shpd_version`.
2. Only emit the fields documented for the version they declare.
3. Bump `format_version` for **any** change to the `query` section, including
   additive optional fields — readers validate the query strictly (rule 4),
   so even an optional new query field would make every shipped app reject
   the whole file. Additive optional fields in the envelope or in result
   entries do **not** need a bump (rule 3). Renamed/removed fields or changed
   meanings anywhere need a bump.

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

The format is pinned by one shared, frozen version-1 fixture:
`crates/seedfinder-core/tests/fixtures/results-export-v1.json`. The core unit
tests, the web tests (via a raw import), the Android tests, and the macOS
tests all decode **that same file**, so a platform codec cannot silently
drift from the canonical schema. Windows has no test harness in this repo;
its codec must be kept in sync by review.

When evolving the format, never edit the version-1 fixture — add new fixtures
for the new version and keep the old ones passing. Additive changes inside a
version get their own fixture the same way:
`results-export-v1-weapon-categories.json` pins the narrowed
`melee_weapon`/`thrown_weapon` kinds on every tested platform.

## Import semantics

Importing a results file **replaces** the current results list and the query
editor state on every platform (after full validation, so a bad file never
half-applies), and records the imported query as the export snapshot so
re-exporting reproduces the file. Imports are refused while a search is
running — including when a search started while the file picker was open.
The informational `app_version` field is not compared against the running
app; a version-1 file is importable regardless of which app version wrote it.
