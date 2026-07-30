# Results export format

Every Seed Seeker frontend can export the current search results — together
with the query that found them — to a JSON file, and import such a file to
restore both the results list and the query editor. All platforms read and
write the same schema. The canonical implementation and its compatibility
tests live in `crates/seedfinder-core/src/results_export.rs`; frontends that
cannot link the core crate (web, Android, macOS, Windows) re-implement the
schema and pin it with their own fixture tests.

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
| `format_version` | integer | yes      | Schema version. This document describes version `1`.                    |
| `app_version`    | string  | no       | App version that wrote the file. Informational only.                    |
| `shpd_version`   | string  | no       | Upstream Shattered Pixel Dungeon version the engine targeted. Informational only. |
| `query`          | object  | yes      | The query that produced the results, in the shared JSON query-document format (see below). |
| `results`        | array   | yes      | The exported results, in display order. May be empty.                   |

Each entry of `results` is an object with one required field:

| Field  | Type   | Required | Meaning                                              |
| ------ | ------ | -------- | ----------------------------------------------------- |
| `seed` | string | yes      | Canonical seed code, `XXX-XXX-XXX` with `A–Z` digits. |

Result entries are objects (not bare strings) so future versions can attach
per-result metadata without a format break.

## The `query` object

The query reuses the existing JSON query-document format shared by the CLI
(`seed-seeker --query`), the web frontend, and the presets on every platform.
It is decoded by `crates/seedfinder-core/src/json_query.rs`; in short:

- `requirements` — array of requirement objects with optional `kind`
  (`weapon` | `armor` | `wand` | `ring`), `item` (catalog stable id such as
  `ring_wealth`), `tier` (`{"exact": n}` | `{"at_least": n}` |
  `{"at_most": n}`), `upgrade` (number | `{"at_least": n}`), `effect`
  (enchantment/glyph wire name such as `Blazing`), `uncursed` (bool),
  `source` (snake_case source name such as `imp_reward`), `identity_group`
  (1–255), and `max_depth` (1–24).
- `max_depth` (default 24), `require_blacksmith`, `exclude_blacksmith_rewards`,
  `fast_mode` — top-level scope flags.
- `challenges` — array of snake_case challenge names (`on_diet`,
  `faith_is_my_armor`, `pharmacophobia`, `barren_land`, `swarm_intelligence`,
  `into_darkness`, `forbidden_runes`, `hostile_champions`, `badder_bosses`).

Writers omit defaults (`"tier": "any"`, `false` flags, `"max_depth": 24`, an
empty `challenges` list) so exported documents stay minimal and identical
across platforms.

## Compatibility rules

Version 1 readers must follow these rules; they are what lets files exported
today stay importable forever, and files from slightly newer apps degrade
gracefully:

1. **Reject non-results files clearly.** If `format` is missing or not
   `"seed-seeker-results"`, report that the file is not a Seed Seeker results
   file.
2. **Check `format_version` first.** If it is missing, non-numeric, zero, or
   *greater* than the newest version the reader understands, fail with a
   message telling the user to update the app. Never guess at a newer schema.
3. **Ignore unknown envelope fields and unknown per-result fields.** A future
   release may add optional fields (for example an export timestamp or
   per-result annotations) without bumping `format_version`; version-1
   readers must skip them silently.
4. **Be strict about the `query` contents.** Unknown query fields, item ids,
   effects, sources, or challenge names must fail the import with a message
   naming the offending value — silently dropping a requirement would make
   the restored query mean something different from the one that produced
   the results. (This is also what happens when a file from a newer app
   references an item that this build's catalog does not know.)
5. **Validate seed codes.** Every `results[i].seed` must parse as a canonical
   nine-letter seed code; report the index of the first invalid entry.

Writers must:

1. Write `format`, `format_version`, `query`, and `results` always, plus
   `app_version` and `shpd_version`.
2. Only emit the fields documented for the version they declare.
3. Bump `format_version` only for changes that version-1 readers cannot
   safely ignore (renamed/removed fields, changed meanings). Additive
   optional fields do not need a bump.

Fixture tests pin the format on every platform with a test harness:

- `crates/seedfinder-core/tests/fixtures/results-export-v1.json` (canonical,
  exercised by `results_export` unit tests),
- `web/src/lib/results-file.test.ts`,
- `android/app/src/test/java/dev/seedseeker/app/model/ResultsExportTest.kt`,
- `macos/SeedSeeker/Tests/SeedSeekerKitTests/ResultsExportTests.swift`.

When evolving the format, never edit the version-1 fixtures — add new
fixtures for the new version and keep the old ones passing.

## Import semantics

Importing a results file **replaces** the current results list and the query
editor state on every platform (after validation, so a bad file never
half-applies). Imports are refused while a search is running. The informational
`app_version`/`shpd_version` fields are not compared against the running app;
a version-1 file is importable regardless of which app version wrote it.
