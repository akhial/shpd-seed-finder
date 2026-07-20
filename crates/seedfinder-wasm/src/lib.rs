//! Thin JSON and cooperative-search adapter for browser WebAssembly.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use shpd_seedfinder_core::catalog::{Effect, ItemId, ItemKind, item};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::feasibility::QueryPlan;
use shpd_seedfinder_core::json_query;
use shpd_seedfinder_core::main_world::{
    CanonicalMainWorldGenerator, ConfiguredMainWorldGenerator, generate_main_world_with_challenges,
};
use shpd_seedfinder_core::model::{Accessibility, GeneratedWorld, ItemSource, WorldItem};
use shpd_seedfinder_core::probability::estimate_match_probability;
use shpd_seedfinder_core::query::SearchQuery;
use shpd_seedfinder_core::search::WorldGenerator;
use shpd_seedfinder_core::seed::{DungeonSeed, TOTAL_SEEDS};
use shpd_seedfinder_core::{SHPD_COMMIT, SHPD_VERSION};
use wasm_bindgen::prelude::*;

/// Maximum number of matches retained by one browser search session.
pub const MAX_RESULTS: usize = 1_024;
const SEARCH_BATCH_SIZE: u64 = 256;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineInfo {
    shpd_version: &'static str,
    shpd_commit: &'static str,
    total_seeds: u64,
    max_results: usize,
}

#[derive(Serialize)]
struct SeedOutput {
    code: String,
    value: u64,
}

impl From<DungeonSeed> for SeedOutput {
    fn from(seed: DungeonSeed) -> Self {
        Self {
            code: seed.to_code(),
            value: seed.value(),
        }
    }
}

#[derive(Serialize)]
#[serde(untagged)]
enum AnalysisOutput {
    Invalid {
        valid: bool,
        error: String,
    },
    Valid {
        valid: bool,
        probability: Option<f64>,
        impossible: bool,
        notes: Vec<String>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScoutRequest {
    seed: String,
    #[serde(default)]
    challenges: Vec<FileChallenge>,
    #[serde(default)]
    query: Option<Value>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileChallenge {
    OnDiet,
    FaithIsMyArmor,
    Pharmacophobia,
    BarrenLand,
    SwarmIntelligence,
    IntoDarkness,
    ForbiddenRunes,
    HostileChampions,
    BadderBosses,
}

impl From<FileChallenge> for Challenges {
    fn from(value: FileChallenge) -> Self {
        match value {
            FileChallenge::OnDiet => Self::NO_FOOD,
            FileChallenge::FaithIsMyArmor => Self::NO_ARMOR,
            FileChallenge::Pharmacophobia => Self::NO_HEALING,
            FileChallenge::BarrenLand => Self::NO_HERBALISM,
            FileChallenge::SwarmIntelligence => Self::SWARM_INTELLIGENCE,
            FileChallenge::IntoDarkness => Self::DARKNESS,
            FileChallenge::ForbiddenRunes => Self::NO_SCROLLS,
            FileChallenge::HostileChampions => Self::CHAMPION_ENEMIES,
            FileChallenge::BadderBosses => Self::STRONGER_BOSSES,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScoutOutput {
    seed: SeedOutput,
    items: Vec<ScoutItemOutput>,
    matched_requirements: usize,
    total_requirements: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScoutItemOutput {
    id: &'static str,
    name: &'static str,
    category: &'static str,
    sprite_index: u16,
    upgrade: u8,
    effect: Option<EffectOutput>,
    cursed: bool,
    depth: u8,
    source: &'static str,
    accessibility: AccessibilityOutput,
    matched: bool,
}

#[derive(Serialize)]
struct EffectOutput {
    name: &'static str,
    kind: &'static str,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AccessibilityOutput {
    Independent,
    Choice { group: u16, option: u8 },
    Scenarios { group: u16, mask: String },
}

#[derive(Serialize)]
struct AdvanceOutput {
    state: &'static str,
    tested: u64,
    matches: Vec<SeedOutput>,
}

/// Returns the pinned engine version and browser-session limits as JSON.
#[wasm_bindgen]
#[must_use]
pub fn engine_info() -> String {
    to_json(&EngineInfo {
        shpd_version: SHPD_VERSION,
        shpd_commit: SHPD_COMMIT,
        total_seeds: TOTAL_SEEDS,
        max_results: MAX_RESULTS,
    })
}

/// Formats partial interactive seed input as uppercase groups of three.
#[wasm_bindgen]
#[must_use]
pub fn format_seed_code(input: &str) -> String {
    let mut output = String::with_capacity(11);
    for (index, byte) in input
        .bytes()
        .filter(u8::is_ascii_alphabetic)
        .take(9)
        .enumerate()
    {
        if index == 3 || index == 6 {
            output.push('-');
        }
        output.push(char::from(byte.to_ascii_uppercase()));
    }
    output
}

/// Parses a seed using the core game's seed-code semantics and returns JSON.
///
/// # Errors
///
/// Returns a JavaScript error when the input is not a valid seed code.
#[wasm_bindgen]
pub fn parse_seed_code(input: &str) -> Result<String, JsError> {
    parse_seed_code_impl(input).map_err(|error| JsError::new(&error))
}

/// Decodes and analyzes a query without throwing or panicking on bad input.
#[wasm_bindgen]
#[must_use]
pub fn analyze_query(query_json: &str) -> String {
    let query = match json_query::decode(query_json) {
        Ok(query) => query,
        Err(error) => {
            return to_json(&AnalysisOutput::Invalid {
                valid: false,
                error,
            });
        }
    };
    let plan = QueryPlan::analyze(&query);
    let impossible = plan.is_unsatisfiable();
    let probability = (!impossible)
        .then(|| estimate_match_probability(&query))
        .filter(|value| value.is_finite());
    let notes = if impossible {
        vec!["No seed can satisfy this combination of requirements.".to_owned()]
    } else {
        Vec::new()
    };
    to_json(&AnalysisOutput::Valid {
        valid: true,
        probability,
        impossible,
        notes,
    })
}

/// Generates and describes one complete depth-24 world as JSON.
///
/// # Errors
///
/// Returns a JavaScript error for malformed requests, invalid seeds or
/// queries, and world-generation failures.
#[wasm_bindgen]
pub fn scout(request_json: &str) -> Result<String, JsError> {
    scout_impl(request_json).map_err(|error| JsError::new(&error))
}

/// Cooperative, single-threaded browser search state.
#[wasm_bindgen]
pub struct SearchSession {
    query: SearchQuery,
    plan: QueryPlan,
    generator: ConfiguredMainWorldGenerator,
    cursor: u64,
    end_seed_exclusive: u64,
    tested: u64,
    accepted: usize,
    completed: bool,
}

#[wasm_bindgen]
impl SearchSession {
    /// Creates a validated cooperative search over a half-open numeric range.
    ///
    /// # Errors
    ///
    /// Returns a JavaScript error for an invalid query, fractional or
    /// non-finite bounds, or a range outside the core seed space.
    #[wasm_bindgen(constructor)]
    pub fn new(
        query_json: &str,
        start_seed: f64,
        end_seed_exclusive: f64,
    ) -> Result<SearchSession, JsError> {
        Self::new_impl(query_json, start_seed, end_seed_exclusive)
            .map_err(|error| JsError::new(&error))
    }

    /// Tests at most `max_seeds` more seeds and returns only newly found matches.
    #[must_use]
    pub fn advance(&mut self, max_seeds: u32) -> String {
        let mut matches = Vec::new();
        let mut remaining =
            u64::from(max_seeds).min(self.end_seed_exclusive.saturating_sub(self.cursor));

        while remaining > 0 && !self.completed {
            let batch_len = remaining.min(SEARCH_BATCH_SIZE);
            let batch_end = self.cursor + batch_len;
            let seeds = (self.cursor..batch_end)
                .filter_map(|value| DungeonSeed::new(value).ok())
                .collect::<Vec<_>>();
            let worlds = self.generator.generate_batch_gated(
                &seeds,
                self.plan.generation_depth(),
                &self.plan,
            );
            for world in worlds {
                self.cursor += 1;
                self.tested += 1;
                if let Some(world) = world
                    && self.query.matches(&world)
                {
                    matches.push(world.seed.into());
                    self.accepted += 1;
                    if self.accepted == MAX_RESULTS {
                        self.completed = true;
                        break;
                    }
                }
            }
            remaining = remaining.saturating_sub(batch_len);
            if self.cursor == self.end_seed_exclusive {
                self.completed = true;
            }
        }

        to_json(&AdvanceOutput {
            state: if self.completed {
                "completed"
            } else {
                "running"
            },
            tested: self.tested,
            matches,
        })
    }
}

impl SearchSession {
    fn new_impl(
        query_json: &str,
        start_seed: f64,
        end_seed_exclusive: f64,
    ) -> Result<Self, String> {
        let query = json_query::decode(query_json)?;
        let start_seed = seed_bound(start_seed, false)?;
        let end_seed_exclusive = seed_bound(end_seed_exclusive, true)?;
        if start_seed >= end_seed_exclusive {
            return Err("start_seed must be less than end_seed_exclusive".to_owned());
        }
        let plan = QueryPlan::analyze(&query);
        let completed = plan.is_unsatisfiable();
        Ok(Self {
            generator: CanonicalMainWorldGenerator::with_challenges(query.challenges),
            query,
            plan,
            cursor: if completed {
                end_seed_exclusive
            } else {
                start_seed
            },
            end_seed_exclusive,
            tested: 0,
            accepted: 0,
            completed,
        })
    }
}

fn parse_seed_code_impl(input: &str) -> Result<String, String> {
    let seed = DungeonSeed::from_code(input).map_err(|error| error.to_string())?;
    Ok(to_json(&SeedOutput::from(seed)))
}

fn scout_impl(request_json: &str) -> Result<String, String> {
    let request: ScoutRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid scout request JSON: {error}"))?;
    let formatted_seed = format_seed_code(&request.seed);
    let seed = DungeonSeed::from_code(&formatted_seed).map_err(|error| error.to_string())?;
    let challenges = request
        .challenges
        .into_iter()
        .fold(Challenges::NONE, |mask, challenge| mask | challenge.into());
    let query = request
        .query
        .map(|value| json_query::decode(&value.to_string()))
        .transpose()?;
    let world = generate_main_world_with_challenges(seed, 24, challenges)
        .map_err(|error| format!("world generation failed: {error}"))?;
    let matched = query.as_ref().map_or_else(
        || vec![false; world.items.len()],
        |query| scout_matches(&world, query),
    );
    let matched_requirements = matched.iter().filter(|value| **value).count();
    let total_requirements = query.as_ref().map_or(0, |query| query.requirements.len());
    let items = world
        .items
        .iter()
        .zip(matched)
        .map(|(world_item, matched)| scout_item_output(world_item, matched))
        .collect();
    Ok(to_json(&ScoutOutput {
        seed: seed.into(),
        items,
        matched_requirements,
        total_requirements,
    }))
}

fn scout_item_output(world_item: &WorldItem, matched: bool) -> ScoutItemOutput {
    let definition = item(world_item.item);
    ScoutItemOutput {
        id: definition.stable_id,
        name: definition.name,
        category: item_kind_name(definition.kind),
        sprite_index: definition.sprite_index,
        upgrade: world_item.upgrade,
        effect: world_item.effect.map(effect_output),
        cursed: world_item.cursed,
        depth: world_item.depth,
        source: item_source_name(world_item.source),
        accessibility: accessibility_output(world_item.accessibility),
        matched,
    }
}

const fn item_kind_name(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Weapon => "weapon",
        ItemKind::Armor => "armor",
        ItemKind::Wand => "wand",
        ItemKind::Ring => "ring",
    }
}

const fn item_source_name(source: ItemSource) -> &'static str {
    match source {
        ItemSource::Heap => "heap",
        ItemSource::Chest => "chest",
        ItemSource::LockedChest => "locked_chest",
        ItemSource::CrystalChest => "crystal_chest",
        ItemSource::Tomb => "tomb",
        ItemSource::Skeleton => "skeleton",
        ItemSource::SacrificialFire => "sacrificial_fire",
        ItemSource::Mimic => "mimic",
        ItemSource::GoldenMimic => "golden_mimic",
        ItemSource::CrystalMimic => "crystal_mimic",
        ItemSource::Statue => "statue",
        ItemSource::ArmoredStatue => "armored_statue",
        ItemSource::Shop => "shop",
        ItemSource::GhostReward => "ghost_reward",
        ItemSource::WandmakerReward => "wandmaker_reward",
        ItemSource::BlacksmithReward => "blacksmith_reward",
        ItemSource::ImpReward => "imp_reward",
    }
}

const fn effect_output(effect: Effect) -> EffectOutput {
    EffectOutput {
        name: effect.wire_name(),
        kind: if effect.is_curse() {
            "curse"
        } else {
            "enchantment"
        },
    }
}

fn accessibility_output(accessibility: Accessibility) -> AccessibilityOutput {
    match accessibility {
        Accessibility::Independent => AccessibilityOutput::Independent,
        Accessibility::Choice { group, option } => AccessibilityOutput::Choice { group, option },
        Accessibility::Scenarios { group, mask } => AccessibilityOutput::Scenarios {
            group,
            mask: format!("0x{mask:x}"),
        },
    }
}

type RequirementCandidates = (Option<u8>, Vec<(usize, ItemId)>);

fn scout_matches(world: &GeneratedWorld, query: &SearchQuery) -> Vec<bool> {
    let mut candidates: Vec<RequirementCandidates> = query
        .requirements
        .iter()
        .map(|requirement| {
            (
                requirement.identity_group,
                world
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, candidate)| {
                        candidate.depth <= query.max_depth
                            && candidate.depth <= requirement.max_depth.unwrap_or(query.max_depth)
                            && (!query.exclude_blacksmith_rewards
                                || candidate.source != ItemSource::BlacksmithReward)
                            && requirement.matches(candidate)
                    })
                    .map(|(index, candidate)| (index, candidate.item))
                    .collect(),
            )
        })
        .collect();
    candidates.sort_by_key(|(_, values)| values.len());
    let mut search = BestSubset {
        candidates: &candidates,
        items: &world.items,
        used: vec![false; world.items.len()],
        selected: Vec::new(),
        best: Vec::new(),
        scenarios: BTreeMap::new(),
        identities: BTreeMap::new(),
    };
    search.visit(0);
    let mut matched = vec![false; world.items.len()];
    for index in search.best {
        matched[index] = true;
    }
    matched
}

struct BestSubset<'a> {
    candidates: &'a [RequirementCandidates],
    items: &'a [WorldItem],
    used: Vec<bool>,
    selected: Vec<usize>,
    best: Vec<usize>,
    scenarios: BTreeMap<u16, u64>,
    identities: BTreeMap<u8, ItemId>,
}

impl BestSubset<'_> {
    fn visit(&mut self, position: usize) {
        if position == self.candidates.len() {
            if self.selected.len() > self.best.len() {
                self.best.clone_from(&self.selected);
            }
            return;
        }
        if self.selected.len() + (self.candidates.len() - position) <= self.best.len() {
            return;
        }

        let (identity_group, candidates) = &self.candidates[position];
        for &(index, identity) in candidates {
            if self.used[index] {
                continue;
            }
            let mut previous_identity = None;
            if let Some(group) = identity_group {
                if self
                    .identities
                    .get(group)
                    .is_some_and(|wanted| *wanted != identity)
                {
                    continue;
                }
                previous_identity = Some((*group, self.identities.insert(*group, identity)));
            }
            let mut previous_scenarios = None;
            if let Some((group, mask)) = self.items[index].accessibility.scenario_constraint() {
                let compatible = self.scenarios.get(&group).copied().unwrap_or(u64::MAX) & mask;
                if compatible == 0 {
                    Self::rewind(&mut self.identities, previous_identity);
                    continue;
                }
                previous_scenarios = Some((group, self.scenarios.insert(group, compatible)));
            }

            self.used[index] = true;
            self.selected.push(index);
            self.visit(position + 1);
            self.selected.pop();
            self.used[index] = false;
            Self::rewind(&mut self.scenarios, previous_scenarios);
            Self::rewind(&mut self.identities, previous_identity);
        }
        self.visit(position + 1);
    }

    fn rewind<K: Ord, V>(map: &mut BTreeMap<K, V>, previous: Option<(K, Option<V>)>) {
        if let Some((key, previous)) = previous {
            if let Some(previous) = previous {
                map.insert(key, previous);
            } else {
                map.remove(&key);
            }
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn seed_bound(value: f64, allow_total: bool) -> Result<u64, String> {
    // Every permitted value is below 2^53 and therefore exactly representable.
    let upper = if allow_total {
        TOTAL_SEEDS as f64
    } else {
        (TOTAL_SEEDS - 1) as f64
    };
    if !value.is_finite() || value.fract() != 0.0 || value < 0.0 || value > upper {
        return Err(if allow_total {
            format!("seed bound must be an integer in 0..={TOTAL_SEEDS}")
        } else {
            format!("seed bound must be an integer in 0..{}", TOTAL_SEEDS - 1)
        });
    }
    Ok(value as u64)
}

fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| r#"{"error":"JSON serialization failed"}"#.to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::num::NonZeroUsize;

    use serde::Deserialize;
    use serde_json::{Value, json};
    use shpd_seedfinder_core::catalog::item;
    use shpd_seedfinder_core::json_query;
    use shpd_seedfinder_core::main_world::{CanonicalMainWorldGenerator, generate_main_world};
    use shpd_seedfinder_core::search::{SearchOptions, SearchProgress, search_parallel};
    use shpd_seedfinder_core::seed::{DungeonSeed, TOTAL_SEEDS};

    use super::{
        MAX_RESULTS, SearchSession, analyze_query, engine_info, format_seed_code,
        parse_seed_code_impl, scout_impl,
    };

    #[test]
    fn engine_info_reports_core_constants() {
        let info: Value = serde_json::from_str(&engine_info()).unwrap();
        assert_eq!(info["shpdVersion"], shpd_seedfinder_core::SHPD_VERSION);
        assert_eq!(info["shpdCommit"], shpd_seedfinder_core::SHPD_COMMIT);
        assert_eq!(info["totalSeeds"], TOTAL_SEEDS);
        assert_eq!(info["maxResults"], MAX_RESULTS);
    }

    #[test]
    fn interactive_seed_formatting_handles_partial_and_garbage_input() {
        assert_eq!(format_seed_code("a"), "A");
        assert_eq!(format_seed_code("abcD"), "ABC-D");
        assert_eq!(format_seed_code("abc-def-ghi"), "ABC-DEF-GHI");
        assert_eq!(format_seed_code(" 1a!b@c#d$e%f^g&h*i extra"), "ABC-DEF-GHI");
        assert_eq!(format_seed_code("åa😀b"), "AB");
    }

    #[test]
    fn seed_parsing_accepts_core_forms_and_rejects_invalid_codes() {
        let parsed: Value =
            serde_json::from_str(&parse_seed_code_impl("aaa-aaa-aab").unwrap()).unwrap();
        assert_eq!(parsed, json!({"code":"AAA-AAA-AAB","value":1}));
        assert!(parse_seed_code_impl("aaaaaaaaa").is_err());
        assert!(parse_seed_code_impl("AAA-AAA-AA0").is_err());
    }

    #[test]
    fn query_analysis_covers_valid_invalid_and_impossible_inputs() {
        let valid: Value = serde_json::from_str(&analyze_query(
            r#"{"requirements":[{"item":"wand_fireblast","upgrade":{"at_least":2}}]}"#,
        ))
        .unwrap();
        assert_eq!(valid["valid"], true);
        assert_eq!(valid["impossible"], false);
        assert!(valid["probability"].as_f64().is_some());

        let invalid: Value = serde_json::from_str(&analyze_query("not json")).unwrap();
        assert_eq!(invalid["valid"], false);
        assert!(invalid["error"].as_str().unwrap().contains("invalid JSON"));

        let impossible: Value = serde_json::from_str(&analyze_query(
            r#"{"requirements":[{"kind":"ring","upgrade":4,"uncursed":true}]}"#,
        ))
        .unwrap();
        assert_eq!(impossible["valid"], true);
        assert_eq!(impossible["impossible"], true);
        assert!(impossible["probability"].is_null());
        assert!(!impossible["notes"].as_array().unwrap().is_empty());
    }

    #[derive(Deserialize)]
    struct AndroidCatalog {
        entries: Vec<AndroidEntry>,
    }

    #[derive(Deserialize)]
    struct AndroidEntry {
        id: String,
        sprite: u16,
    }

    #[test]
    fn scout_matches_canonical_world_and_android_catalog() {
        let output: Value =
            serde_json::from_str(&scout_impl(r#"{"seed":"AAA-AAA-AAA"}"#).unwrap()).unwrap();
        let world = generate_main_world(DungeonSeed::MIN, 24).unwrap();
        let output_items = output["items"].as_array().unwrap();
        assert_eq!(output_items.len(), world.items.len());

        let catalog: AndroidCatalog = serde_json::from_str(include_str!(
            "../../../android/app/src/main/assets/third_party/shattered-pixel-dungeon/catalog-v3.3.8.json"
        ))
        .unwrap();
        let sprites = catalog
            .entries
            .into_iter()
            .map(|entry| (entry.id, entry.sprite))
            .collect::<BTreeMap<_, _>>();
        for (output_item, world_item) in output_items.iter().zip(&world.items) {
            let definition = item(world_item.item);
            assert_eq!(output_item["id"], definition.stable_id);
            assert_eq!(output_item["depth"], world_item.depth);
            assert_eq!(
                output_item["spriteIndex"],
                sprites.get(definition.stable_id).copied().unwrap()
            );
        }
    }

    #[test]
    fn scout_query_marks_a_matching_known_item() {
        let world = generate_main_world(DungeonSeed::MIN, 24).unwrap();
        let known = &world.items[0];
        let definition = item(known.item);
        let request = json!({
            "seed": "AAAAAAAAA",
            "query": {
                "requirements": [{
                    "item": definition.stable_id,
                    "max_depth": known.depth
                }]
            }
        });
        let output: Value =
            serde_json::from_str(&scout_impl(&request.to_string()).unwrap()).unwrap();
        assert_eq!(output["totalRequirements"], 1);
        assert_eq!(output["matchedRequirements"], 1);
        let matched = output["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["matched"] == true)
            .collect::<Vec<_>>();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0]["id"], definition.stable_id);
    }

    #[test]
    fn cooperative_search_matches_one_worker_parallel_search() {
        let query_json = r#"{
            "max_depth": 9,
            "requirements":[{
                "item":"wand_fireblast",
                "upgrade":{"exact":3},
                "source":"wandmaker_reward"
            }]
        }"#;
        let query = json_query::decode(query_json).unwrap();
        let parallel = search_parallel(
            &CanonicalMainWorldGenerator,
            &query,
            SearchOptions {
                start_seed: 0,
                end_seed_exclusive: 20_000,
                workers: NonZeroUsize::MIN,
                chunk_size: NonZeroUsize::new(137).unwrap(),
                max_results: NonZeroUsize::new(MAX_RESULTS).unwrap(),
            },
            &SearchProgress::default(),
        )
        .unwrap()
        .worlds
        .into_iter()
        .map(|world| world.seed.value())
        .collect::<Vec<_>>();

        let mut session = SearchSession::new_impl(query_json, 0.0, 20_000.0).unwrap();
        let mut cooperative = Vec::new();
        loop {
            let output: Value = serde_json::from_str(&session.advance(113)).unwrap();
            cooperative.extend(
                output["matches"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|seed| seed["value"].as_u64().unwrap()),
            );
            if output["state"] == "completed" {
                break;
            }
        }
        assert_eq!(cooperative, parallel);
    }
}
