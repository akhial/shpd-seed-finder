//! Versioned results-export documents: search results plus the query that
//! found them.
//!
//! This module is the canonical implementation of the format described in
//! `docs/results-export-format.md`. Frontends that cannot link this crate
//! re-implement the same schema and pin it with fixture tests.
//!
//! Compatibility contract:
//!
//! - The envelope carries `format` and an integer `format_version`.
//! - Readers ignore unknown envelope and per-result fields, so version 1
//!   readers keep working when a future release adds optional fields.
//! - Files declaring a `format_version` greater than [`FORMAT_VERSION`] are
//!   rejected with a clear "update the app" message rather than misread.
//! - The embedded query reuses the [`crate::json_query`] document format and
//!   is validated strictly: unknown query fields, items, effects, or
//!   challenges fail the import instead of silently changing its meaning.

use serde_json::{Map, Value, json};

use crate::json_query;
use crate::query::SearchQuery;
use crate::seed::DungeonSeed;

/// Identifies a Seed Seeker results file, whatever its version.
pub const FILE_FORMAT: &str = "seed-seeker-results";

/// The newest results-file version this build can read and the version it
/// writes.
pub const FORMAT_VERSION: u64 = 1;

/// One decoded results file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResultsFile {
    /// Version declared by the file, at most [`FORMAT_VERSION`].
    pub format_version: u64,
    /// App version that wrote the file, when declared. Informational.
    pub app_version: Option<String>,
    /// Upstream game version the exporting engine targeted. Informational.
    pub shpd_version: Option<String>,
    /// The validated query that produced the exported results.
    pub query: SearchQuery,
    /// The exported result seeds, in their exported order.
    pub seeds: Vec<DungeonSeed>,
}

/// Encodes a validated query and its result seeds as a pretty-printed
/// version-[`FORMAT_VERSION`] results document.
#[must_use]
pub fn encode(query: &SearchQuery, seeds: &[DungeonSeed], app_version: &str) -> String {
    let document = json!({
        "format": FILE_FORMAT,
        "format_version": FORMAT_VERSION,
        "app_version": app_version,
        "shpd_version": crate::SHPD_VERSION,
        "query": json_query::encode(query),
        "results": seeds
            .iter()
            .map(|seed| json!({ "seed": seed.to_code() }))
            .collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&document).unwrap_or_default()
}

/// Decodes and validates a results document.
///
/// # Errors
///
/// Returns a human-readable message for files that are not Seed Seeker
/// results documents, come from a newer format version, or contain an
/// invalid query or seed code.
pub fn decode(contents: &str) -> Result<ResultsFile, String> {
    let document: Value = serde_json::from_str(contents)
        .map_err(|error| format!("this is not a Seed Seeker results file: {error}"))?;
    let document = document
        .as_object()
        .ok_or("this is not a Seed Seeker results file: expected a JSON object")?;
    if document.get("format").and_then(Value::as_str) != Some(FILE_FORMAT) {
        return Err(format!(
            "this is not a Seed Seeker results file: missing \"format\": \"{FILE_FORMAT}\""
        ));
    }
    let format_version = document
        .get("format_version")
        .ok_or("this results file is missing its \"format_version\" number")?
        .as_u64()
        .filter(|version| *version >= 1)
        .ok_or("this results file does not declare a valid format version (a positive integer)")?;
    if format_version > FORMAT_VERSION {
        return Err(format!(
            "this results file uses format version {format_version}, but this app understands \
             up to version {FORMAT_VERSION}; update Seed Seeker to import it"
        ));
    }
    let query_value = document
        .get("query")
        .filter(|value| value.is_object())
        .ok_or("this results file is missing its \"query\" object")?;
    let query = json_query::decode(&query_value.to_string())
        .map_err(|error| format!("the query in this results file is not usable: {error}"))?;
    for (index, requirement) in query.requirements.iter().enumerate() {
        // The results format restricts same-item groups to what every app's
        // editor can express (A..D), even though the engine allows more.
        if requirement.identity_group.is_some_and(|group| group > 4) {
            return Err(format!(
                "requirement {}: same-item group must be between 1 and 4 (A..D)",
                index + 1
            ));
        }
    }
    let results = document
        .get("results")
        .and_then(Value::as_array)
        .ok_or("this results file is missing its \"results\" list")?;
    let seeds = results
        .iter()
        .enumerate()
        .map(|(index, entry)| decode_result_seed(index, entry))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ResultsFile {
        format_version,
        app_version: field_string(document, "app_version"),
        shpd_version: field_string(document, "shpd_version"),
        query,
        seeds,
    })
}

/// Deduplicates seeds (keeping the first occurrence) and caps the list at
/// `limit`, returning the kept seeds and how many entries were dropped.
/// All importers apply this rule so a given file restores the same list
/// everywhere.
#[must_use]
pub fn dedupe_and_cap(seeds: &[DungeonSeed], limit: usize) -> (Vec<DungeonSeed>, usize) {
    let mut seen = std::collections::HashSet::new();
    let mut kept = Vec::new();
    for seed in seeds {
        if kept.len() == limit {
            break;
        }
        if seen.insert(*seed) {
            kept.push(*seed);
        }
    }
    let dropped = seeds.len() - kept.len();
    (kept, dropped)
}

fn decode_result_seed(index: usize, entry: &Value) -> Result<DungeonSeed, String> {
    let code = entry
        .get("seed")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("result {}: missing \"seed\" code", index + 1))?;
    // Stricter than the interactive parser on purpose: files must carry the
    // canonical form so every platform accepts exactly the same documents.
    if !is_canonical_code(code) {
        return Err(format!(
            "result {}: seed code must use the canonical XXX-XXX-XXX form",
            index + 1
        ));
    }
    DungeonSeed::from_code(code).map_err(|error| format!("result {}: {error}", index + 1))
}

fn is_canonical_code(code: &str) -> bool {
    let bytes = code.as_bytes();
    bytes.len() == 11
        && bytes.iter().enumerate().all(|(index, byte)| {
            if index == 3 || index == 7 {
                *byte == b'-'
            } else {
                byte.is_ascii_uppercase()
            }
        })
}

fn field_string(document: &Map<String, Value>, key: &str) -> Option<String> {
    document.get(key).and_then(Value::as_str).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use crate::catalog::{ItemId, ItemKind};
    use crate::challenges::Challenges;
    use crate::model::ItemSource;
    use crate::query::{Requirement, SearchQuery, TierRequirement, UpgradeRequirement};
    use crate::seed::DungeonSeed;

    use super::{FORMAT_VERSION, decode, dedupe_and_cap, encode};

    fn sample_query() -> SearchQuery {
        SearchQuery {
            requirements: vec![
                Requirement {
                    kind: ItemKind::Ring,
                    weapon_category: None,
                    item: Some(ItemId::RingWealth),
                    tier: TierRequirement::Any,
                    upgrade: UpgradeRequirement::Exact(4),
                    effect: None,
                    require_uncursed: false,
                    source: Some(ItemSource::ImpReward),
                    identity_group: None,
                    max_depth: None,
                },
                Requirement {
                    kind: ItemKind::Wand,
                    weapon_category: None,
                    item: None,
                    tier: TierRequirement::Any,
                    upgrade: UpgradeRequirement::AtLeast(2),
                    effect: None,
                    require_uncursed: true,
                    source: None,
                    identity_group: Some(1),
                    max_depth: Some(9),
                },
            ],
            max_depth: 21,
            challenges: Challenges::NO_HERBALISM | Challenges::DARKNESS,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: true,
        }
    }

    fn seeds(codes: &[&str]) -> Vec<DungeonSeed> {
        codes
            .iter()
            .map(|code| DungeonSeed::from_code(code).unwrap())
            .collect()
    }

    #[test]
    fn encode_then_decode_round_trips_query_seeds_and_versions() {
        let query = sample_query();
        let exported = seeds(&["AAA-AAA-AAB", "ZZZ-ZZZ-ZZZ", "SEE-DSE-EKR"]);
        let contents = encode(&query, &exported, "0.6.1");
        let decoded = decode(&contents).unwrap();
        assert_eq!(decoded.format_version, FORMAT_VERSION);
        assert_eq!(decoded.app_version.as_deref(), Some("0.6.1"));
        assert_eq!(decoded.shpd_version.as_deref(), Some(crate::SHPD_VERSION));
        assert_eq!(decoded.query, query);
        assert_eq!(decoded.seeds, exported);
    }

    /// A version-1 document written by hand and frozen: files exported today
    /// must always stay readable by future releases. Do not edit the fixture;
    /// add new fixtures for new versions instead.
    const VERSION_1_FIXTURE: &str = include_str!("../tests/fixtures/results-export-v1.json");

    #[test]
    fn version_one_fixture_always_decodes() {
        let decoded = decode(VERSION_1_FIXTURE).unwrap();
        assert_eq!(decoded.format_version, 1);
        assert_eq!(decoded.app_version.as_deref(), Some("0.6.1"));
        assert_eq!(decoded.shpd_version.as_deref(), Some("3.3.8"));
        assert_eq!(decoded.query.max_depth, 12);
        assert_eq!(decoded.query.challenges, Challenges::NO_HERBALISM);
        assert!(decoded.query.require_blacksmith);
        assert_eq!(decoded.query.requirements.len(), 2);
        assert_eq!(
            decoded.query.requirements[0].item,
            Some(ItemId::RingTenacity)
        );
        assert_eq!(decoded.query.requirements[1].kind, ItemKind::Wand);
        assert_eq!(decoded.seeds, seeds(&["AAA-AAA-BUH", "ABC-DEF-GHI"]));
    }

    /// Narrowed weapon kinds (`melee_weapon`/`thrown_weapon`) are an additive
    /// enum value within format version 1; every importer must accept them.
    const WEAPON_CATEGORIES_FIXTURE: &str =
        include_str!("../tests/fixtures/results-export-v1-weapon-categories.json");

    #[test]
    fn weapon_category_fixture_decodes_and_round_trips() {
        use crate::catalog::WeaponCategory;

        let decoded = decode(WEAPON_CATEGORIES_FIXTURE).unwrap();
        assert_eq!(decoded.format_version, 1);
        assert_eq!(decoded.query.requirements.len(), 3);
        assert_eq!(
            decoded.query.requirements[0].weapon_category,
            Some(WeaponCategory::Thrown)
        );
        assert_eq!(
            decoded.query.requirements[1].weapon_category,
            Some(WeaponCategory::Melee)
        );
        assert_eq!(decoded.query.requirements[1].item, Some(ItemId::Sword));
        assert_eq!(decoded.query.requirements[2].weapon_category, None);
        assert_eq!(decoded.seeds, seeds(&["AAA-AAA-ACO"]));

        // Re-encoding must keep the narrowing: widening "thrown_weapon" back
        // to "weapon" would silently change the query's meaning on import.
        let encoded = encode(&decoded.query, &decoded.seeds, "test");
        let round_tripped = decode(&encoded).unwrap();
        assert_eq!(round_tripped.query, decoded.query);
        assert_eq!(round_tripped.seeds, decoded.seeds);
    }

    #[test]
    fn unknown_envelope_and_result_fields_are_ignored() {
        let contents = r#"{
            "format": "seed-seeker-results",
            "format_version": 1,
            "exported_at": "2031-01-01T00:00:00Z",
            "future_minor_field": {"nested": true},
            "query": {"requirements": [{"item": "sword"}]},
            "results": [
                {"seed": "AAA-AAA-AAB", "future_note": "still fine"}
            ]
        }"#;
        let decoded = decode(contents).unwrap();
        assert_eq!(decoded.seeds, seeds(&["AAA-AAA-AAB"]));
        assert!(decoded.app_version.is_none());
    }

    #[test]
    fn future_format_versions_fail_with_an_update_message() {
        let contents = r#"{
            "format": "seed-seeker-results",
            "format_version": 2,
            "query": {"requirements": [{"item": "sword"}]},
            "results": []
        }"#;
        let error = decode(contents).unwrap_err();
        assert!(error.contains("format version 2"), "{error}");
        assert!(error.contains("update Seed Seeker"), "{error}");
    }

    #[test]
    fn foreign_and_malformed_files_are_rejected_clearly() {
        for contents in ["not json at all", "[]", "{}", r#"{"format":"other"}"#] {
            let error = decode(contents).unwrap_err();
            assert!(
                error.contains("not a Seed Seeker results file"),
                "{contents}: {error}"
            );
        }
        let missing_version =
            r#"{"format":"seed-seeker-results","query":{"requirements":[]},"results":[]}"#;
        assert!(
            decode(missing_version)
                .unwrap_err()
                .contains("format_version")
        );
    }

    #[test]
    fn unknown_query_content_fails_instead_of_changing_meaning() {
        let unknown_item = r#"{
            "format": "seed-seeker-results",
            "format_version": 1,
            "query": {"requirements": [{"item": "item_from_the_future"}]},
            "results": []
        }"#;
        let error = decode(unknown_item).unwrap_err();
        assert!(error.contains("query"), "{error}");
        assert!(error.contains("item_from_the_future"), "{error}");

        let unknown_field = r#"{
            "format": "seed-seeker-results",
            "format_version": 1,
            "query": {"requirements": [{"item": "sword"}], "wished_luck": 7},
            "results": []
        }"#;
        assert!(decode(unknown_field).is_err());
    }

    #[test]
    fn invalid_seed_codes_name_the_offending_result() {
        let contents = r#"{
            "format": "seed-seeker-results",
            "format_version": 1,
            "query": {"requirements": [{"item": "sword"}]},
            "results": [{"seed": "AAA-AAA-AAB"}, {"seed": "AAA-AAA-AA0"}]
        }"#;
        let error = decode(contents).unwrap_err();
        assert!(error.starts_with("result 2:"), "{error}");
    }

    #[test]
    fn only_canonical_seed_codes_are_accepted() {
        // The interactive parser tolerates these; the file format must not,
        // or files would import on some platforms and fail on others.
        for code in ["aaa-aaa-aab", "AAAAAAAAB", "AAA AAA AAB", " AAA-AAA-AAB"] {
            let contents = format!(
                r#"{{"format":"seed-seeker-results","format_version":1,
                    "query":{{"requirements":[{{"item":"sword"}}]}},
                    "results":[{{"seed":"{code}"}}]}}"#
            );
            let error = decode(&contents).unwrap_err();
            assert!(error.contains("canonical"), "{code}: {error}");
        }
    }

    #[test]
    fn format_version_must_be_a_positive_integer() {
        for version in ["0", "1.5", "true", "\"1\"", "-1"] {
            let contents = format!(
                r#"{{"format":"seed-seeker-results","format_version":{version},
                    "query":{{"requirements":[{{"item":"sword"}}]}},"results":[]}}"#
            );
            let error = decode(&contents).unwrap_err();
            assert!(error.contains("valid format version"), "{version}: {error}");
        }
    }

    #[test]
    fn wrong_typed_query_fields_are_rejected() {
        for query in [
            r#"{"requirements":[{"item":"sword"}],"max_depth":"12"}"#,
            r#"{"requirements":[{"item":42}]}"#,
            r#"{"requirements":[{"item":"sword"}],"challenges":"barren_land"}"#,
            r#"{"requirements":[{"item":"sword","upgrade":true}]}"#,
        ] {
            let contents = format!(
                r#"{{"format":"seed-seeker-results","format_version":1,
                    "query":{query},"results":[]}}"#
            );
            assert!(decode(&contents).is_err(), "{query}");
        }
    }

    #[test]
    fn same_item_groups_above_four_are_rejected() {
        let contents = r#"{
            "format": "seed-seeker-results",
            "format_version": 1,
            "query": {"requirements": [{"kind": "wand", "identity_group": 5}]},
            "results": []
        }"#;
        let error = decode(contents).unwrap_err();
        assert!(error.contains("1 and 4"), "{error}");
    }

    #[test]
    fn importers_dedupe_then_cap_preserving_first_occurrences() {
        let raw = seeds(&["AAA-AAA-AAC", "AAA-AAA-AAB", "AAA-AAA-AAC"]);
        let (kept, dropped) = dedupe_and_cap(&raw, 1_024);
        assert_eq!(kept, seeds(&["AAA-AAA-AAC", "AAA-AAA-AAB"]));
        assert_eq!(dropped, 1);

        let many: Vec<_> = (0..1_500)
            .map(|value| DungeonSeed::new(value).unwrap())
            .collect();
        let (kept, dropped) = dedupe_and_cap(&many, 1_024);
        assert_eq!(kept.len(), 1_024);
        assert_eq!(dropped, 476);
    }
}
