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
//!
//! The sweep takes `FUZZ_WORLDS`, `FUZZ_QUERIES`, and `FUZZ_SEED` from the
//! environment, so the same corpus can be rerun over a deeper sample while
//! chasing a specific bias.

use std::fmt::Write as _;
use std::sync::OnceLock;
use std::time::Instant;

use shpd_seedfinder_core::catalog::{
    ArmorEffect, Effect, ItemId, ItemKind, WeaponCategory, WeaponEffect,
};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::main_world::CanonicalMainWorldGenerator;
use shpd_seedfinder_core::model::{GeneratedWorld, ItemSource};
use shpd_seedfinder_core::probability::estimate_match_probability;
use shpd_seedfinder_core::probability_tables::is_missile;
use shpd_seedfinder_core::query::{
    EffectRequirement, Requirement, SearchQuery, TierRequirement, UpgradeRequirement,
};
use shpd_seedfinder_core::search::WorldGenerator;
use shpd_seedfinder_core::seed::{DungeonSeed, TOTAL_SEEDS};

/// Worlds sampled for the always-on test. Small enough to stay cheap in an
/// unoptimised build, which limits it to queries that hit often.
const SAMPLED_WORLDS: u64 = 512;

/// Worlds sampled by the ignored sweep, unless `FUZZ_WORLDS` says otherwise.
const FUZZED_WORLDS: u64 = 60_000;

/// Randomly generated queries in the ignored sweep, unless `FUZZ_QUERIES` says
/// otherwise.
const FUZZED_QUERIES: usize = 400;

/// A query needs at least this many hits before its rate is worth comparing.
const MEANINGFUL_HITS: f64 = 12.0;

/// How far the estimate may sit from the sampled rate once sampling error is
/// accounted for. The model approximates competition between requirements and
/// ignores challenges, so it is not expected to be exact — only close.
const TOLERANCE: f64 = 2.0;

/// A search shows its estimate while the user types, so it has to land well
/// inside a frame even for the widest query.
const BUDGET: std::time::Duration = std::time::Duration::from_millis(50);

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

/// The estimate is computed on the interface thread before a search starts, so
/// the widest query it accepts still has to resolve well inside a frame.
#[test]
fn estimates_stay_fast() {
    let mut generator = QueryGenerator::new(0xC0FF_EE00);
    let mut widest = std::time::Duration::ZERO;
    let mut slowest = String::new();
    // Linked groups are the costly shape: every candidate identity re-runs the
    // whole matching, so they set the ceiling.
    for _ in 0..200 {
        let query = generator.next_query();
        if query.validate().is_err() {
            continue;
        }
        let started = Instant::now();
        let estimate = estimate_match_probability(&query);
        let elapsed = started.elapsed();
        assert!(
            estimate.is_finite(),
            "{} produced {estimate}",
            describe(&query)
        );
        if elapsed > widest {
            widest = elapsed;
            slowest = describe(&query);
        }
    }
    // Debug builds run this an order of magnitude slower than the release build
    // the budget describes, so only the optimised build is held to it.
    let budget = if cfg!(debug_assertions) {
        BUDGET * 20
    } else {
        BUDGET
    };
    println!("slowest estimate: {widest:?} for {slowest}");
    assert!(
        widest < budget,
        "slowest estimate took {widest:?}, over the {budget:?} budget: {slowest}"
    );
}

#[test]
#[ignore = "generates tens of thousands of worlds; run with --release"]
fn fuzzed_queries_track_sampled_seeds() {
    let sample = from_environment("FUZZ_WORLDS").unwrap_or(FUZZED_WORLDS);
    let queries = from_environment("FUZZ_QUERIES")
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(FUZZED_QUERIES);
    let seed = from_environment("FUZZ_SEED").unwrap_or(0x5EED_5EEC);
    let worlds = sampled_worlds(sample);
    let mut generator = QueryGenerator::new(seed);
    let mut ratios: Vec<(f64, String, f64, f64)> = Vec::new();
    let mut failures = Vec::new();
    for _ in 0..queries {
        let query = generator.next_query();
        if query.validate().is_err() {
            continue;
        }
        let name = describe(&query);
        let (hits, estimate) = measure(&query, worlds);
        if f64::from(hits) < MEANINGFUL_HITS {
            continue;
        }
        let observed = f64::from(hits) / count(worlds.len());
        ratios.push((estimate / observed, name.clone(), observed, estimate));
        if !within_tolerance(observed, estimate, f64::from(hits)) {
            failures.push(format!(
                "{name}: sampled {observed:.3e}, estimated {estimate:.3e}"
            ));
        }
    }
    report(&mut ratios, worlds.len());
    assert!(
        ratios.len() > 40,
        "only {} queries produced enough hits",
        ratios.len()
    );
    assert!(
        failures.is_empty(),
        "estimates drifted:\n{}",
        failures.join("\n")
    );
}

/// Prints every comparison worst-first, then the shape of the whole sweep.
fn report(ratios: &mut [(f64, String, f64, f64)], worlds: usize) {
    ratios.sort_by(|left, right| {
        right
            .0
            .ln()
            .abs()
            .partial_cmp(&left.0.ln().abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (ratio, name, observed, estimate) in ratios.iter() {
        println!("{ratio:>8.3}  {observed:>10.3e}  {estimate:>10.3e}  {name}");
    }
    let mut sorted: Vec<f64> = ratios.iter().map(|entry| entry.0).collect();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let at = |fraction: f64| {
        let index = ((count(sorted.len()) - 1.0) * fraction).round().max(0.0);
        sorted.get(position(index)).copied().unwrap_or(f64::NAN)
    };
    let logs: f64 = sorted.iter().map(|ratio| ratio.ln().abs()).sum();
    println!(
        "\n{} queries over {worlds} worlds: median {:.3}, p10 {:.3}, p90 {:.3}, \
         range {:.3}-{:.3}, mean |log| {:.4}",
        sorted.len(),
        at(0.5),
        at(0.1),
        at(0.9),
        sorted.first().copied().unwrap_or(f64::NAN),
        sorted.last().copied().unwrap_or(f64::NAN),
        logs / count(sorted.len().max(1)),
    );
}

/// A rounded, non-negative percentile position as an index.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn position(index: f64) -> usize {
    index as usize
}

/// Sweep sizes stay far inside what an `f64` counts exactly, and percentile
/// positions are already rounded and clamped before they are turned back.
#[allow(clippy::cast_precision_loss)]
fn count(value: usize) -> f64 {
    value as f64
}

fn from_environment(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.parse().ok()
}

fn compare(name: &str, query: &SearchQuery, worlds: &[GeneratedWorld]) -> bool {
    let (hits, estimate) = measure(query, worlds);
    if f64::from(hits) < MEANINGFUL_HITS {
        return false;
    }
    let observed = f64::from(hits) / count(worlds.len());
    assert!(
        within_tolerance(observed, estimate, f64::from(hits)),
        "{name}: sampled {observed:.4e} over {} worlds, estimated {estimate:.4e}",
        worlds.len()
    );
    true
}

fn measure(query: &SearchQuery, worlds: &[GeneratedWorld]) -> (u32, f64) {
    let workers = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);
    let chunk = worlds.len().div_ceil(workers.max(1)).max(1);
    let hits: usize = std::thread::scope(|scope| {
        let handles: Vec<_> = worlds
            .chunks(chunk)
            .map(|slice| scope.spawn(|| slice.iter().filter(|world| query.matches(world)).count()))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("no worker panics"))
            .sum()
    });
    (
        hits.try_into().unwrap_or(u32::MAX),
        estimate_match_probability(query),
    )
}

/// Whether an estimate is close enough, widening [`TOLERANCE`] by the sampling
/// error of the observed rate.
fn within_tolerance(observed: f64, estimate: f64, hits: f64) -> bool {
    let sampling = 1.0 + 3.0 / hits.sqrt();
    let ratio = estimate / observed;
    ratio <= TOLERANCE * sampling && ratio >= 1.0 / (TOLERANCE * sampling)
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
        weapon_category: None,
        item: None,
        tier: TierRequirement::Any,
        upgrade: UpgradeRequirement::Any,
        effect: EffectRequirement::Any,
        require_uncursed: false,
        source: None,
        identity_group: None,
        max_depth: None,
        alternative_group: None,
        level_sum: None,
    }
}

fn query(requirements: Vec<Requirement>, max_depth: u8) -> SearchQuery {
    SearchQuery {
        requirements,
        max_depth,
        challenges: Challenges::NONE,
        require_blacksmith: false,
        exclude_blacksmith_rewards: false,
        wandmaker_quest: None,
    }
}

/// Queries chosen to exercise one modelling decision each, all common enough to
/// be measurable in a small sample.
fn curated_queries() -> Vec<(String, SearchQuery)> {
    let mut queries = depth_queries();
    queries.extend(modifier_queries());
    queries.extend(vault_upgrade_queries());
    queries.extend(locked_level_queries());
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
                effect: EffectRequirement::exactly(Effect::Weapon(WeaponEffect::Blazing)),
                ..base(ItemKind::Weapon)
            }],
            24,
        ),
    ));
    queries.push((
        "a viscous armor".to_owned(),
        query(
            vec![Requirement {
                effect: EffectRequirement::exactly(Effect::Armor(ArmorEffect::Viscosity)),
                ..base(ItemKind::Armor)
            }],
            24,
        ),
    ));
    queries.push((
        "a cursed weapon".to_owned(),
        query(
            vec![Requirement {
                effect: EffectRequirement::exactly(Effect::Weapon(WeaponEffect::Annoying)),
                ..base(ItemKind::Weapon)
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
    queries.push((
        "a thrown weapon at +1 or better".to_owned(),
        query(
            vec![Requirement {
                item: Some(ItemId::ThrowingSpear),
                upgrade: UpgradeRequirement::AtLeast(1),
                ..base(ItemKind::Weapon)
            }],
            24,
        ),
    ));
    // Narrowed wildcards exercise the estimator's per-line category filter.
    queries.push((
        "any thrown weapon at +1 or better".to_owned(),
        query(
            vec![Requirement {
                weapon_category: Some(WeaponCategory::Thrown),
                upgrade: UpgradeRequirement::AtLeast(1),
                ..base(ItemKind::Weapon)
            }],
            24,
        ),
    ));
    queries.push((
        "any melee weapon at exactly +2".to_owned(),
        query(
            vec![Requirement {
                weapon_category: Some(WeaponCategory::Melee),
                upgrade: UpgradeRequirement::Exact(2),
                ..base(ItemKind::Weapon)
            }],
            24,
        ),
    ));
    queries
}

/// Tiers and levels that only ever arrive together.
///
/// The Imp's vault stocks four fixed shelves, so a tier there names the level
/// it carries. Scoring the two apart invents items the vault never stocks, and
/// since it is the deepest and best-stocked source in the dungeon, the
/// invented ones swamp the real answer. The Imp's own reward locks the two the
/// same way; its impossible pairs are the ones
/// [`Requirement::upgrade_ceiling`] rejects outright, so they cannot be
/// sampled here.
fn locked_level_queries() -> Vec<(String, SearchQuery)> {
    vec![
        (
            // The vault's armor is +0 at tier 2 and climbs one level a tier, so
            // no vault armor below tier 5 is ever +3.
            "a +3 armor of tier 3 or lower".to_owned(),
            query(
                vec![Requirement {
                    tier: TierRequirement::AtMost(3),
                    upgrade: UpgradeRequirement::Exact(3),
                    ..base(ItemKind::Armor)
                }],
                22,
            ),
        ),
        (
            // The vault's thrown weapons follow the same shelf rule, so a +3
            // Kunai — tier 4 — can only come from elsewhere.
            "a +3 uncursed Kunai".to_owned(),
            query(
                vec![Requirement {
                    item: Some(ItemId::Kunai),
                    upgrade: UpgradeRequirement::Exact(3),
                    require_uncursed: true,
                    ..base(ItemKind::Weapon)
                }],
                24,
            ),
        ),
    ]
}

/// The top of the upgrade range, which only the Imp's vault reaches — and
/// whose highest level only its tier-4 weapon carries, so the estimator has
/// to score that level against tier 4 alone.
fn vault_upgrade_queries() -> Vec<(String, SearchQuery)> {
    vec![
        (
            "any weapon at exactly +5".to_owned(),
            query(
                vec![Requirement {
                    upgrade: UpgradeRequirement::Exact(5),
                    ..base(ItemKind::Weapon)
                }],
                24,
            ),
        ),
        (
            "any tier 4 weapon at exactly +5".to_owned(),
            query(
                vec![Requirement {
                    tier: TierRequirement::Exact(4),
                    upgrade: UpgradeRequirement::Exact(5),
                    ..base(ItemKind::Weapon)
                }],
                24,
            ),
        ),
        (
            "any melee weapon at +4 or better".to_owned(),
            query(
                vec![Requirement {
                    weapon_category: Some(WeaponCategory::Melee),
                    upgrade: UpgradeRequirement::AtLeast(4),
                    ..base(ItemKind::Weapon)
                }],
                24,
            ),
        ),
    ]
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
    // One family per requirement, so nothing competes but everything has to hold.
    queries.push((
        "one of every family at +1 or better".to_owned(),
        query(
            [
                ItemKind::Weapon,
                ItemKind::Armor,
                ItemKind::Wand,
                ItemKind::Ring,
            ]
            .into_iter()
            .map(|kind| Requirement {
                upgrade: UpgradeRequirement::AtLeast(1),
                ..base(kind)
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
    if query.require_blacksmith {
        text.push_str(" +smith");
    }
    if query.exclude_blacksmith_rewards {
        text.push_str(" -smith");
    }
    for requirement in &query.requirements {
        let _ = write!(
            text,
            " [{:?}{}{}{}{}{}{}{}{}{}]",
            requirement.kind,
            requirement
                .weapon_category
                .map(|value| format!(" {value:?}"))
                .unwrap_or_default(),
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
            match requirement.effect {
                EffectRequirement::Any => String::new(),
                EffectRequirement::OneOf(set) => {
                    format!(" {:?}", set.effects().collect::<Vec<_>>())
                }
            },
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
            requirement
                .identity_group
                .map(|value| format!(" =g{value}"))
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
        let count = 1 + self.pick(4);
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
        if kind == ItemKind::Weapon {
            requirement.weapon_category = match self.below(5) {
                0 => Some(WeaponCategory::Melee),
                1 => Some(WeaponCategory::Thrown),
                _ => None,
            };
        }
        if self.chance(35) {
            requirement.item = self.next_item(kind, requirement.weapon_category);
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
        if self.chance(18) {
            let curse = self.chance(25);
            let effect = match kind {
                ItemKind::Weapon if curse => Some(Effect::Weapon(
                    WEAPON_CURSES[self.pick(WEAPON_CURSES.len())],
                )),
                ItemKind::Weapon => Some(Effect::Weapon(
                    WEAPON_EFFECTS[self.pick(WEAPON_EFFECTS.len())],
                )),
                ItemKind::Armor if curse => {
                    Some(Effect::Armor(ARMOR_CURSES[self.pick(ARMOR_CURSES.len())]))
                }
                ItemKind::Armor => {
                    Some(Effect::Armor(ARMOR_EFFECTS[self.pick(ARMOR_EFFECTS.len())]))
                }
                ItemKind::Wand | ItemKind::Ring | ItemKind::Trinket => None,
            };
            requirement.effect = effect.map_or(EffectRequirement::Any, EffectRequirement::exactly);
        }
        let curses_only = match requirement.effect {
            EffectRequirement::OneOf(set) => set.is_curses_only(),
            EffectRequirement::Any => false,
        };
        if self.chance(20) && !curses_only {
            requirement.require_uncursed = true;
        }
        if self.chance(10) {
            requirement.source = Some(SOURCES[self.pick(SOURCES.len())]);
        }
        if self.chance(25) {
            requirement.max_depth = Some(1 + self.small(24));
        }
        requirement
    }

    /// Named items, with thrown weapons drawn as often as melee ones so the
    /// separate generator category they come from stays covered. A narrowed
    /// requirement only pins items of its own weapon class.
    fn next_item(&mut self, kind: ItemKind, category: Option<WeaponCategory>) -> Option<ItemId> {
        let thrown = kind == ItemKind::Weapon
            && match category {
                Some(category) => category == WeaponCategory::Thrown,
                None => self.chance(40),
            };
        let candidates: Vec<ItemId> = shpd_seedfinder_core::catalog::ITEMS
            .iter()
            .filter(|definition| definition.kind == kind)
            .map(|definition| definition.id)
            .filter(|id| kind != ItemKind::Weapon || is_missile(*id) == thrown)
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
    WeaponEffect::Vampiric,
];

const WEAPON_CURSES: [WeaponEffect; 3] = [
    WeaponEffect::Annoying,
    WeaponEffect::Displacing,
    WeaponEffect::Polarized,
];

const ARMOR_EFFECTS: [ArmorEffect; 6] = [
    ArmorEffect::Obfuscation,
    ArmorEffect::Viscosity,
    ArmorEffect::Brimstone,
    ArmorEffect::Flow,
    ArmorEffect::Thorns,
    ArmorEffect::AntiMagic,
];

const ARMOR_CURSES: [ArmorEffect; 3] = [
    ArmorEffect::AntiEntropy,
    ArmorEffect::Corrosion,
    ArmorEffect::Metabolism,
];

const SOURCES: [ItemSource; 6] = [
    ItemSource::Heap,
    ItemSource::Chest,
    ItemSource::Shop,
    ItemSource::GhostReward,
    ItemSource::WandmakerReward,
    ItemSource::BlacksmithReward,
];
