//! Checks [`estimate_match_probability`] against the generator it describes.
//!
//! The estimator is an analytical model of an empirical thing, so the only
//! honest test is to generate worlds and count. [`estimates_track_sampled_seeds`]
//! runs a small shared sample on every build; the ignored
//! [`fuzzed_queries_track_sampled_seeds`] sweeps randomly generated queries over
//! a much larger sample and is the one to run after touching the model:
//!
//! ```text
//! cargo test --release -p shpd-seedfinder-core --test probability_fuzz \
//!     -- --ignored --nocapture
//! ```

use std::fmt::Write as _;
use std::sync::OnceLock;

use shpd_seedfinder_core::catalog::{ArmorEffect, Effect, ItemId, ItemKind, WeaponEffect};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::main_world::CanonicalMainWorldGenerator;
use shpd_seedfinder_core::model::{GeneratedWorld, ItemSource};
use shpd_seedfinder_core::probability::estimate_match_probability;
use shpd_seedfinder_core::query::{Requirement, SearchQuery, TierRequirement, UpgradeRequirement};
use shpd_seedfinder_core::search::WorldGenerator;
use shpd_seedfinder_core::seed::{DungeonSeed, TOTAL_SEEDS};

/// Worlds sampled for the always-on test. Small enough to stay cheap in an
/// unoptimised build, which limits it to queries that hit often.
const SAMPLED_WORLDS: u64 = 512;

/// Worlds sampled by the ignored sweep.
const FUZZED_WORLDS: u64 = 30_000;

/// Randomly generated queries in the ignored sweep.
const FUZZED_QUERIES: usize = 200;

/// A query needs at least this many hits before its rate is worth comparing.
const MEANINGFUL_HITS: f64 = 12.0;

/// How far the estimate may sit from the sampled rate once sampling error is
/// accounted for. The model approximates competition between requirements and
/// ignores challenges, so it is not expected to be exact — only close.
const TOLERANCE: f64 = 2.0;

#[test]
fn estimates_track_sampled_seeds() {
    let worlds = sampled_worlds(SAMPLED_WORLDS);
    let mut checked = 0;
    for (name, query) in curated_queries() {
        assert!(query.validate().is_ok(), "{name} is not a valid query");
        if compare(&name, &query, worlds) {
            checked += 1;
        }
    }
    assert!(
        checked >= 8,
        "the sample was too small to check anything meaningful: {checked} queries"
    );
}

#[test]
#[ignore = "generates tens of thousands of worlds; run with --release"]
fn fuzzed_queries_track_sampled_seeds() {
    let worlds = sampled_worlds(FUZZED_WORLDS);
    let mut generator = QueryGenerator::new(0x5EED_5EEC);
    let mut checked = 0;
    let mut failures = Vec::new();
    for _ in 0..FUZZED_QUERIES {
        let query = generator.next_query();
        if query.validate().is_err() {
            continue;
        }
        let name = describe(&query);
        let (hits, estimate) = measure(&query, worlds);
        if f64::from(hits) < MEANINGFUL_HITS {
            continue;
        }
        checked += 1;
        let observed = f64::from(hits) / worlds_len(worlds);
        let ratio = estimate / observed;
        println!("{ratio:>8.3}  {observed:>10.3e}  {estimate:>10.3e}  {name}");
        if !within_tolerance(observed, estimate, f64::from(hits)) {
            failures.push(format!(
                "{name}: sampled {observed:.3e}, estimated {estimate:.3e}"
            ));
        }
    }
    assert!(checked > 40, "only {checked} queries produced enough hits");
    assert!(
        failures.is_empty(),
        "estimates drifted:\n{}",
        failures.join("\n")
    );
}

fn compare(name: &str, query: &SearchQuery, worlds: &[GeneratedWorld]) -> bool {
    let (hits, estimate) = measure(query, worlds);
    if f64::from(hits) < MEANINGFUL_HITS {
        return false;
    }
    let observed = f64::from(hits) / worlds_len(worlds);
    assert!(
        within_tolerance(observed, estimate, f64::from(hits)),
        "{name}: sampled {observed:.4e} over {} worlds, estimated {estimate:.4e}",
        worlds.len()
    );
    true
}

fn measure(query: &SearchQuery, worlds: &[GeneratedWorld]) -> (u32, f64) {
    let hits = worlds
        .iter()
        .filter(|world| query.matches(world))
        .count()
        .try_into()
        .unwrap_or(u32::MAX);
    (hits, estimate_match_probability(query))
}

/// Whether an estimate is close enough, widening [`TOLERANCE`] by the sampling
/// error of the observed rate.
fn within_tolerance(observed: f64, estimate: f64, hits: f64) -> bool {
    let sampling = 1.0 + 3.0 / hits.sqrt();
    let ratio = estimate / observed;
    ratio <= TOLERANCE * sampling && ratio >= 1.0 / (TOLERANCE * sampling)
}

fn worlds_len(worlds: &[GeneratedWorld]) -> f64 {
    f64::from(u32::try_from(worlds.len()).unwrap_or(u32::MAX))
}

/// Worlds spread evenly over the seed space, generated once per process.
fn sampled_worlds(count: u64) -> &'static [GeneratedWorld] {
    static SMALL: OnceLock<Vec<GeneratedWorld>> = OnceLock::new();
    static LARGE: OnceLock<Vec<GeneratedWorld>> = OnceLock::new();
    let cell = if count == SAMPLED_WORLDS {
        &SMALL
    } else {
        &LARGE
    };
    cell.get_or_init(|| generate(count))
}

fn generate(count: u64) -> Vec<GeneratedWorld> {
    let generator = CanonicalMainWorldGenerator::with_challenges(Challenges::NONE);
    let workers = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);
    let stride = (TOTAL_SEEDS / count.max(1)).max(1);
    let mut worlds = Vec::new();
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for worker in 0..workers {
            let generator = &generator;
            handles.push(scope.spawn(move || {
                let mut local = Vec::new();
                let mut index = worker as u64;
                while index < count {
                    let value = index.wrapping_mul(stride) % TOTAL_SEEDS;
                    let seed = DungeonSeed::new(value).expect("stride stays in range");
                    local.push(generator.generate(seed, 24));
                    index += workers as u64;
                }
                local
            }));
        }
        for handle in handles {
            worlds.extend(handle.join().expect("no worker panics"));
        }
    });
    worlds
}

fn base(kind: ItemKind) -> Requirement {
    Requirement {
        kind,
        item: None,
        tier: TierRequirement::Any,
        upgrade: UpgradeRequirement::Any,
        effect: None,
        require_uncursed: false,
        source: None,
        identity_group: None,
        max_depth: None,
    }
}

fn query(requirements: Vec<Requirement>, max_depth: u8) -> SearchQuery {
    SearchQuery {
        requirements,
        max_depth,
        challenges: Challenges::NONE,
        require_blacksmith: false,
        exclude_blacksmith_rewards: false,
        fast_mode: false,
    }
}

/// Queries chosen to exercise one modelling decision each, all common enough to
/// be measurable in a small sample.
fn curated_queries() -> Vec<(String, SearchQuery)> {
    let mut queries = depth_queries();
    queries.extend(modifier_queries());
    queries.extend(competition_queries());
    queries
}

/// Floor limits, both the search-wide one and the per-item one.
fn depth_queries() -> Vec<(String, SearchQuery)> {
    let mut queries = Vec::new();
    for depth in [4_u8, 10, 24] {
        queries.push((
            format!("a +2 wand within depth {depth}"),
            query(
                vec![Requirement {
                    upgrade: UpgradeRequirement::Exact(2),
                    ..base(ItemKind::Wand)
                }],
                depth,
            ),
        ));
    }
    // A per-item floor limit has to bind independently of the search depth.
    queries.push((
        "a +2 wand by floor 4 while searching all 24".to_owned(),
        query(
            vec![Requirement {
                upgrade: UpgradeRequirement::Exact(2),
                max_depth: Some(4),
                ..base(ItemKind::Wand)
            }],
            24,
        ),
    ));
    queries
}

/// Identity, upgrade, curse, and enchantment rolls.
fn modifier_queries() -> Vec<(String, SearchQuery)> {
    let mut queries = vec![(
        "one named wand".to_owned(),
        query(
            vec![Requirement {
                item: Some(ItemId::WandFireblast),
                ..base(ItemKind::Wand)
            }],
            24,
        ),
    )];
    // The Ghost always offers an armor, so this is a pure upgrade roll.
    queries.push((
        "the Ghost's armor at +2".to_owned(),
        query(
            vec![Requirement {
                upgrade: UpgradeRequirement::Exact(2),
                source: Some(ItemSource::GhostReward),
                ..base(ItemKind::Armor)
            }],
            24,
        ),
    ));
    queries.push((
        "a blazing weapon".to_owned(),
        query(
            vec![Requirement {
                effect: Some(Effect::Weapon(WeaponEffect::Blazing)),
                ..base(ItemKind::Weapon)
            }],
            24,
        ),
    ));
    queries.push((
        "a viscous armor".to_owned(),
        query(
            vec![Requirement {
                effect: Some(Effect::Armor(ArmorEffect::Viscosity)),
                ..base(ItemKind::Armor)
            }],
            24,
        ),
    ));
    queries.push((
        "an uncursed ring at +3 or better".to_owned(),
        query(
            vec![Requirement {
                upgrade: UpgradeRequirement::AtLeast(3),
                require_uncursed: true,
                ..base(ItemKind::Ring)
            }],
            24,
        ),
    ));
    queries
}

/// Requirements that have to be met by distinct items.
fn competition_queries() -> Vec<(String, SearchQuery)> {
    let mut queries = vec![(
        "a tier 4 weapon with a glyphed plate armor".to_owned(),
        query(
            vec![
                Requirement {
                    tier: TierRequirement::Exact(4),
                    ..base(ItemKind::Weapon)
                },
                Requirement {
                    item: Some(ItemId::PlateArmor),
                    ..base(ItemKind::Armor)
                },
            ],
            20,
        ),
    )];
    // Three wands that must all be the same wand: the linked-identity path.
    queries.push((
        "three wands of one kind".to_owned(),
        query(
            vec![
                Requirement {
                    identity_group: Some(1),
                    ..base(ItemKind::Wand)
                },
                Requirement {
                    identity_group: Some(1),
                    ..base(ItemKind::Wand)
                },
                Requirement {
                    identity_group: Some(1),
                    ..base(ItemKind::Wand)
                },
            ],
            24,
        ),
    ));
    // Four separate wands compete for one pool without being linked.
    queries.push((
        "four separate wands at +1 or better".to_owned(),
        query(
            (0..4)
                .map(|_| Requirement {
                    upgrade: UpgradeRequirement::AtLeast(1),
                    ..base(ItemKind::Wand)
                })
                .collect(),
            24,
        ),
    ));
    let mut with_blacksmith = query(
        vec![Requirement {
            upgrade: UpgradeRequirement::AtLeast(2),
            ..base(ItemKind::Armor)
        }],
        13,
    );
    with_blacksmith.require_blacksmith = true;
    queries.push((
        "a +2 armor in a run that reaches the Blacksmith by floor 13".to_owned(),
        with_blacksmith,
    ));
    queries
}

fn describe(query: &SearchQuery) -> String {
    let mut text = format!("depth<={}", query.max_depth);
    for requirement in &query.requirements {
        let _ = write!(
            text,
            " [{:?}{}{}{}{}{}{}{}]",
            requirement.kind,
            requirement
                .item
                .map(|value| format!(" {value:?}"))
                .unwrap_or_default(),
            match requirement.tier {
                TierRequirement::Any => String::new(),
                TierRequirement::Exact(tier) => format!(" t={tier}"),
                TierRequirement::AtLeast(tier) => format!(" t>={tier}"),
                TierRequirement::AtMost(tier) => format!(" t<={tier}"),
            },
            match requirement.upgrade {
                UpgradeRequirement::Any => String::new(),
                UpgradeRequirement::Exact(upgrade) => format!(" +{upgrade}"),
                UpgradeRequirement::AtLeast(upgrade) => format!(" >=+{upgrade}"),
            },
            requirement
                .effect
                .map(|value| format!(" {value:?}"))
                .unwrap_or_default(),
            if requirement.require_uncursed {
                " uncursed"
            } else {
                ""
            },
            requirement
                .source
                .map(|value| format!(" from {value:?}"))
                .unwrap_or_default(),
            requirement
                .max_depth
                .map(|value| format!(" by {value}"))
                .unwrap_or_default(),
        );
    }
    text
}

/// Deterministic random queries, biased towards ones a small sample can see.
struct QueryGenerator {
    state: u64,
}

impl QueryGenerator {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_value(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let mut value = self.state;
        value ^= value >> 33;
        value = value.wrapping_mul(0xff51_afd7_ed55_8ccd);
        value ^ (value >> 29)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next_value() % bound.max(1)
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn next_query(&mut self) -> SearchQuery {
        let count = 1 + self.pick(3);
        let linked = self.chance(15);
        let kind = self.next_kind();
        let requirements = (0..count)
            .map(|_| {
                let mut requirement = self.next_requirement(if linked { Some(kind) } else { None });
                if linked {
                    requirement.identity_group = Some(1);
                    requirement.item = None;
                }
                requirement
            })
            .collect();
        let mut query = query(requirements, 1 + self.small(24));
        query.require_blacksmith = self.chance(8);
        query.exclude_blacksmith_rewards = self.chance(8);
        query
    }

    fn next_kind(&mut self) -> ItemKind {
        match self.below(4) {
            0 => ItemKind::Weapon,
            1 => ItemKind::Armor,
            2 => ItemKind::Wand,
            _ => ItemKind::Ring,
        }
    }

    fn next_requirement(&mut self, forced: Option<ItemKind>) -> Requirement {
        let kind = forced.unwrap_or_else(|| self.next_kind());
        let mut requirement = base(kind);
        if self.chance(35) {
            requirement.item = self.next_item(kind);
        }
        if requirement.item.is_none() && matches!(kind, ItemKind::Weapon | ItemKind::Armor) {
            match self.below(8) {
                0 => requirement.tier = TierRequirement::Exact(2 + self.small(4)),
                1 => requirement.tier = TierRequirement::AtLeast(3 + self.small(2)),
                2 => requirement.tier = TierRequirement::AtMost(3 + self.small(2)),
                _ => {}
            }
        }
        match self.below(6) {
            0 => {
                let maximum = u64::from(kind.maximum_search_upgrade());
                requirement.upgrade = UpgradeRequirement::Exact(1 + self.small(maximum));
            }
            1 | 2 => {
                let maximum = u64::from(kind.maximum_search_upgrade());
                requirement.upgrade = UpgradeRequirement::AtLeast(self.small(maximum + 1));
            }
            _ => {}
        }
        if self.chance(15) {
            requirement.effect = match kind {
                ItemKind::Weapon => Some(Effect::Weapon(
                    WEAPON_EFFECTS[self.pick(WEAPON_EFFECTS.len())],
                )),
                ItemKind::Armor => {
                    Some(Effect::Armor(ARMOR_EFFECTS[self.pick(ARMOR_EFFECTS.len())]))
                }
                ItemKind::Wand | ItemKind::Ring => None,
            };
        }
        if self.chance(20) && !requirement.effect.is_some_and(Effect::is_curse) {
            requirement.require_uncursed = true;
        }
        if self.chance(10) {
            requirement.source = Some(SOURCES[self.pick(SOURCES.len())]);
        }
        if self.chance(20) {
            requirement.max_depth = Some(1 + self.small(24));
        }
        requirement
    }

    fn next_item(&mut self, kind: ItemKind) -> Option<ItemId> {
        let candidates: Vec<ItemId> = shpd_seedfinder_core::catalog::ITEMS
            .iter()
            .filter(|definition| definition.kind == kind)
            .map(|definition| definition.id)
            .collect();
        let index = self.pick(candidates.len());
        candidates.get(index).copied()
    }

    fn pick(&mut self, len: usize) -> usize {
        let bound = u64::try_from(len).unwrap_or(1).max(1);
        usize::try_from(self.below(bound)).unwrap_or(0)
    }

    fn small(&mut self, bound: u64) -> u8 {
        u8::try_from(self.below(bound)).unwrap_or(0)
    }
}

const WEAPON_EFFECTS: [WeaponEffect; 6] = [
    WeaponEffect::Blazing,
    WeaponEffect::Chilling,
    WeaponEffect::Lucky,
    WeaponEffect::Projecting,
    WeaponEffect::Grim,
    WeaponEffect::Annoying,
];

const ARMOR_EFFECTS: [ArmorEffect; 6] = [
    ArmorEffect::Obfuscation,
    ArmorEffect::Viscosity,
    ArmorEffect::Brimstone,
    ArmorEffect::Flow,
    ArmorEffect::Thorns,
    ArmorEffect::Corrosion,
];

const SOURCES: [ItemSource; 6] = [
    ItemSource::Heap,
    ItemSource::Chest,
    ItemSource::Shop,
    ItemSource::GhostReward,
    ItemSource::WandmakerReward,
    ItemSource::BlacksmithReward,
];
