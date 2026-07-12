//! Canonical main-dungeon world generation through any depth 1..=24.
//!
//! The regional composites own the exact sequential state transitions for
//! their prefixes. This module is the stable search/JNI entry point and makes
//! the four boss-floor boundaries explicit.

use std::fmt;

use crate::batch::seed_for_depth_batch4;
use crate::caves_floor::{
    CanonicalCavesWorldGenerator, CavesFloorError, generate_caves_floor, generate_caves_world,
};
use crate::challenges::Challenges;
use crate::city_boss_shop::generate_city_boss_shop;
use crate::city_floor::{
    CanonicalCityWorldGenerator, CityFloorError, generate_city_floor, generate_city_world,
};
use crate::halls_floor::{
    CanonicalHallsWorldGenerator, HallsFloorError, generate_halls_floor, generate_halls_world,
};
use crate::level_prelude::LimitedDrops;
use crate::model::GeneratedWorld;
use crate::prison_floor::{
    CanonicalPrisonWorldGenerator, PrisonFloorError, generate_prison_floor, generate_prison_world,
};
use crate::quests::QuestState;
use crate::rng::{RandomStack, seed_for_depth};
use crate::run::RunState;
use crate::search::{FloorGate, WorldGenerator};
use crate::seed::DungeonSeed;
use crate::sewer_floor::{
    CanonicalSewerWorldGenerator, SewerFloorError, generate_sewer_floor, generate_sewer_world,
    remap_floor_choice_groups,
};
use crate::shop::ShopRunState;

/// Exact version-pinned generator used by production search sessions.
#[derive(Clone, Copy, Debug, Default)]
pub struct CanonicalMainWorldGenerator;

impl CanonicalMainWorldGenerator {
    /// Builds a production generator for one validated challenge mask.
    #[must_use]
    pub const fn with_challenges(challenges: Challenges) -> ConfiguredMainWorldGenerator {
        ConfiguredMainWorldGenerator { challenges }
    }
}

/// Canonical main-dungeon generator configured for one challenge mask.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfiguredMainWorldGenerator {
    challenges: Challenges,
}

impl WorldGenerator for ConfiguredMainWorldGenerator {
    fn generate(&self, seed: DungeonSeed, max_depth: u8) -> GeneratedWorld {
        generate_main_world_with_challenges(seed, max_depth, self.challenges)
            .expect("ConfiguredMainWorldGenerator accepts depths 1..=24")
    }

    fn generate_batch(&self, seeds: &[DungeonSeed], max_depth: u8) -> Vec<GeneratedWorld> {
        if self.challenges == Challenges::NONE {
            return CanonicalMainWorldGenerator.generate_batch(seeds, max_depth);
        }
        seeds
            .iter()
            .copied()
            .map(|seed| self.generate(seed, max_depth))
            .collect()
    }

    fn generate_batch_gated(
        &self,
        seeds: &[DungeonSeed],
        max_depth: u8,
        gate: &dyn FloorGate,
    ) -> Vec<Option<GeneratedWorld>> {
        if self.challenges == Challenges::NONE {
            return CanonicalMainWorldGenerator.generate_batch_gated(seeds, max_depth, gate);
        }
        seeds
            .iter()
            .copied()
            .map(|seed| {
                generate_main_world_gated_with_challenges(seed, max_depth, self.challenges, gate)
                    .expect("validated gated batch satisfies canonical invariants")
            })
            .collect()
    }
}

impl WorldGenerator for CanonicalMainWorldGenerator {
    fn generate(&self, seed: DungeonSeed, max_depth: u8) -> GeneratedWorld {
        generate_main_world(seed, max_depth)
            .expect("CanonicalMainWorldGenerator accepts depths 1..=24")
    }

    fn generate_batch(&self, seeds: &[DungeonSeed], max_depth: u8) -> Vec<GeneratedWorld> {
        assert!(
            (1..=24).contains(&max_depth),
            "CanonicalMainWorldGenerator accepts depths 1..=24"
        );
        match max_depth {
            1..=4 => CanonicalSewerWorldGenerator.generate_batch(seeds, max_depth),
            5 => CanonicalSewerWorldGenerator.generate_batch(seeds, 4),
            6..=9 => CanonicalPrisonWorldGenerator.generate_batch(seeds, max_depth),
            10 => CanonicalPrisonWorldGenerator.generate_batch(seeds, 9),
            11..=14 => CanonicalCavesWorldGenerator.generate_batch(seeds, max_depth),
            15 => CanonicalCavesWorldGenerator.generate_batch(seeds, 14),
            16..=19 => CanonicalCityWorldGenerator.generate_batch(seeds, max_depth),
            20 => {
                let mut worlds = CanonicalHallsWorldGenerator.generate_batch(seeds, 21);
                for world in &mut worlds {
                    world.items.retain(|item| item.depth <= 20);
                }
                worlds
            }
            21..=24 => CanonicalHallsWorldGenerator.generate_batch(seeds, max_depth),
            _ => unreachable!("depth was validated above"),
        }
    }

    fn generate_batch_gated(
        &self,
        seeds: &[DungeonSeed],
        max_depth: u8,
        gate: &dyn FloorGate,
    ) -> Vec<Option<GeneratedWorld>> {
        assert!(
            (1..=24).contains(&max_depth),
            "CanonicalMainWorldGenerator accepts depths 1..=24"
        );
        let target = effective_regular_depth(max_depth);
        let depths = regular_depths(target).collect::<Vec<_>>();
        let mut output = Vec::with_capacity(seeds.len());
        let mut chunks = seeds.chunks_exact(4);
        for chunk in &mut chunks {
            let dungeon_seeds = std::array::from_fn(|index| {
                i64::try_from(chunk[index].value()).expect("base-26 seed range fits Java long")
            });
            let roots_by_depth = depths
                .iter()
                .map(|&depth| seed_for_depth_batch4(dungeon_seeds, depth, 0))
                .collect::<Vec<_>>();
            for lane in 0..4 {
                let roots = roots_by_depth
                    .iter()
                    .map(|depth_roots| depth_roots[lane])
                    .collect::<Vec<_>>();
                output.push(
                    generate_gated_world_with_roots(
                        chunk[lane],
                        target,
                        &roots,
                        Challenges::NONE,
                        gate,
                    )
                    .expect("validated gated batch satisfies canonical invariants"),
                );
            }
        }
        output.extend(chunks.remainder().iter().copied().map(|seed| {
            generate_main_world_gated(seed, target, gate)
                .expect("validated gated batch satisfies canonical invariants")
        }));
        output
    }
}

/// Generates all searchable static equipment in the canonical main-dungeon
/// prefix through `maximum_depth`.
///
/// Boss floors 5, 10, and 15 contain no searchable initial equipment and are
/// persistent-state neutral in the pinned profile. Depth 20 is different: its
/// Imp shop eagerly generates stock, so the depth-21 Halls prefix is evaluated
/// and then trimmed when callers request exactly depth 20.
///
/// # Errors
///
/// Returns [`MainWorldError::InvalidMaximumDepth`] outside 1..=24, or wraps
/// the exact regional generation failure.
pub fn generate_main_world(
    seed: DungeonSeed,
    maximum_depth: u8,
) -> Result<GeneratedWorld, MainWorldError> {
    match maximum_depth {
        1..=4 => generate_sewer_world(seed, maximum_depth).map_err(MainWorldError::Sewer),
        5 => generate_sewer_world(seed, 4).map_err(MainWorldError::Sewer),
        6..=9 => generate_prison_world(seed, maximum_depth).map_err(MainWorldError::Prison),
        10 => generate_prison_world(seed, 9).map_err(MainWorldError::Prison),
        11..=14 => generate_caves_world(seed, maximum_depth).map_err(MainWorldError::Caves),
        15 => generate_caves_world(seed, 14).map_err(MainWorldError::Caves),
        16..=19 => generate_city_world(seed, maximum_depth).map_err(MainWorldError::City),
        20 => {
            let mut world = generate_halls_world(seed, 21).map_err(MainWorldError::Halls)?;
            world.items.retain(|item| item.depth <= 20);
            Ok(world)
        }
        21..=24 => generate_halls_world(seed, maximum_depth).map_err(MainWorldError::Halls),
        _ => Err(MainWorldError::InvalidMaximumDepth(maximum_depth)),
    }
}

/// Challenge-aware form of [`generate_main_world`].
///
/// # Errors
///
/// Returns [`MainWorldError`] when `maximum_depth` is outside `1..=24` or a
/// regional generator rejects its inputs.
///
/// # Panics
///
/// Panics only if the ungated generation pipeline abandons a world, which the
/// open gate makes unreachable.
pub fn generate_main_world_with_challenges(
    seed: DungeonSeed,
    maximum_depth: u8,
    challenges: Challenges,
) -> Result<GeneratedWorld, MainWorldError> {
    struct OpenGate;
    impl FloorGate for OpenGate {
        fn continue_after_floor(
            &self,
            _completed_depth: u8,
            _items: &[crate::model::WorldItem],
        ) -> bool {
            true
        }
    }
    generate_main_world_gated_with_challenges(seed, maximum_depth, challenges, &OpenGate)
        .map(|world| world.expect("open gate never abandons a world"))
}

/// Regular (non-boss) depths generated for a prefix through `target`.
/// Depth 20 is included from 20 onward because its Imp shop eagerly draws
/// stock which the searchable model exposes as depth-20 items.
fn regular_depths(target: u8) -> impl Iterator<Item = u32> {
    (1..=u32::from(target)).filter(|&depth| !matches!(depth, 5 | 10 | 15))
}

/// Maps a requested depth onto the deepest state-mutating floor at or above
/// it: boss floors 5/10/15 are persistent-state neutral in the pinned profile.
const fn effective_regular_depth(maximum_depth: u8) -> u8 {
    match maximum_depth {
        5 => 4,
        10 => 9,
        15 => 14,
        other => other,
    }
}

/// Gated variant of [`generate_main_world`]: after each completed floor the
/// gate may prove the partial world unable to match, abandoning the seed's
/// remaining floors. Returns `Ok(None)` for abandoned seeds.
///
/// # Errors
///
/// Returns [`MainWorldError::InvalidMaximumDepth`] outside 1..=24, or wraps
/// the exact regional generation failure.
///
/// # Panics
///
/// Panics only if a representable seed exceeds Java's `long` range, which the
/// [`DungeonSeed`] type rules out.
pub fn generate_main_world_gated(
    seed: DungeonSeed,
    maximum_depth: u8,
    gate: &dyn FloorGate,
) -> Result<Option<GeneratedWorld>, MainWorldError> {
    if !(1..=24).contains(&maximum_depth) {
        return Err(MainWorldError::InvalidMaximumDepth(maximum_depth));
    }
    let target = effective_regular_depth(maximum_depth);
    let dungeon_seed = i64::try_from(seed.value()).expect("base-26 seed range fits Java long");
    let roots = regular_depths(target)
        .map(|depth| seed_for_depth(dungeon_seed, depth, 0))
        .collect::<Vec<_>>();
    generate_gated_world_with_roots(seed, target, &roots, Challenges::NONE, gate)
}

fn generate_main_world_gated_with_challenges(
    seed: DungeonSeed,
    maximum_depth: u8,
    challenges: Challenges,
    gate: &dyn FloorGate,
) -> Result<Option<GeneratedWorld>, MainWorldError> {
    if !(1..=24).contains(&maximum_depth) {
        return Err(MainWorldError::InvalidMaximumDepth(maximum_depth));
    }
    let target = effective_regular_depth(maximum_depth);
    let dungeon_seed = i64::try_from(seed.value()).expect("base-26 seed range fits Java long");
    let roots = regular_depths(target)
        .map(|depth| seed_for_depth(dungeon_seed, depth, 0))
        .collect::<Vec<_>>();
    generate_gated_world_with_roots(seed, target, &roots, challenges, gate)
}

/// Sequential gated composite over the canonical per-region floor generators.
/// `target` must already be boss-mapped (never 5, 10, or 15).
fn generate_gated_world_with_roots(
    seed: DungeonSeed,
    target: u8,
    roots: &[i64],
    challenges: Challenges,
    gate: &dyn FloorGate,
) -> Result<Option<GeneratedWorld>, MainWorldError> {
    let dungeon_seed = i64::try_from(seed.value()).expect("base-26 seed range fits Java long");
    let mut run = RunState::with_challenges(dungeon_seed, challenges);
    let mut limited_drops = LimitedDrops::default();
    let mut quests = QuestState::new();
    let mut shop_run = ShopRunState::default();
    let mut random = RandomStack::with_base_seed(0);
    let mut items = Vec::new();
    let mut next_choice_group = 0_u16;

    for (&root, depth) in roots.iter().zip(regular_depths(target)) {
        random.push(root);
        let mut floor_items = match depth {
            1..=4 => {
                generate_sewer_floor(
                    &mut run,
                    &mut limited_drops,
                    &mut quests,
                    depth,
                    &mut random,
                )
                .map_err(MainWorldError::Sewer)?
                .world_items
            }
            6..=9 => {
                generate_prison_floor(
                    &mut run,
                    &mut limited_drops,
                    &mut quests,
                    &mut shop_run,
                    depth,
                    &mut random,
                )
                .map_err(MainWorldError::Prison)?
                .world_items
            }
            11..=14 => {
                generate_caves_floor(
                    &mut run,
                    &mut limited_drops,
                    &mut quests,
                    &mut shop_run,
                    depth,
                    &mut random,
                )
                .map_err(MainWorldError::Caves)?
                .world_items
            }
            16..=19 => {
                generate_city_floor(
                    &mut run,
                    &mut limited_drops,
                    &mut quests,
                    &mut shop_run,
                    depth,
                    &mut random,
                )
                .map_err(MainWorldError::City)?
                .world_items
            }
            20 => {
                generate_city_boss_shop(&mut run, &mut shop_run, &mut random)
                    .map_err(|error| MainWorldError::Halls(HallsFloorError::BossShop(error)))?
                    .world_items
            }
            _ => {
                generate_halls_floor(
                    &mut run,
                    &mut limited_drops,
                    &mut quests,
                    &mut shop_run,
                    depth,
                    &mut random,
                )
                .map_err(MainWorldError::Halls)?
                .world_items
            }
        };
        random.pop();
        next_choice_group = remap_floor_choice_groups(&mut floor_items, next_choice_group);
        items.extend(floor_items);
        let completed = u8::try_from(depth).expect("main-path depths fit u8");
        if completed < target && !gate.continue_after_floor(completed, &items) {
            return Ok(None);
        }
    }
    Ok(Some(GeneratedWorld { seed, items }))
}

/// Failure while producing a canonical main-dungeon prefix.
#[derive(Clone, Debug, PartialEq)]
pub enum MainWorldError {
    InvalidMaximumDepth(u8),
    Sewer(SewerFloorError),
    Prison(PrisonFloorError),
    Caves(CavesFloorError),
    City(CityFloorError),
    Halls(HallsFloorError),
}

impl fmt::Display for MainWorldError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMaximumDepth(depth) => {
                write!(formatter, "main-world depth must be in 1..=24, got {depth}")
            }
            Self::Sewer(error) => error.fmt(formatter),
            Self::Prison(error) => error.fmt(formatter),
            Self::Caves(error) => error.fmt(formatter),
            Self::City(error) => error.fmt(formatter),
            Self::Halls(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for MainWorldError {}

#[cfg(test)]
mod tests {
    use crate::catalog::{ItemId, ItemKind};
    use crate::feasibility::QueryPlan;
    use crate::model::{ItemSource, WorldItem};
    use crate::query::{Requirement, SearchQuery, TierRequirement, UpgradeRequirement};
    use crate::search::{FloorGate, WorldGenerator};
    use crate::seed::DungeonSeed;

    use super::{
        CanonicalMainWorldGenerator, MainWorldError, generate_main_world, generate_main_world_gated,
    };

    struct OpenGate;

    impl FloorGate for OpenGate {
        fn continue_after_floor(&self, _depth: u8, _items: &[WorldItem]) -> bool {
            true
        }
    }

    #[test]
    fn gated_generation_with_an_open_gate_matches_ungated_generation() {
        for depth in [1, 4, 5, 9, 12, 15, 17, 20, 24] {
            for seed_value in [0_u64, 5, 26, 4_093] {
                let seed = DungeonSeed::new(seed_value).unwrap();
                assert_eq!(
                    generate_main_world_gated(seed, depth, &OpenGate)
                        .unwrap()
                        .expect("an open gate never abandons a seed"),
                    generate_main_world(seed, depth).unwrap(),
                    "depth {depth} seed {seed_value}"
                );
            }
        }
    }

    #[test]
    fn gated_search_agrees_with_brute_force_matching() {
        let wildcard = |kind, upgrade| Requirement {
            kind,
            item: None,
            tier: TierRequirement::Any,
            upgrade,
            effect: None,
            source: None,
            identity_group: None,
            max_depth: None,
        };
        let query = |requirements: Vec<Requirement>, fast_mode| SearchQuery {
            requirements,
            max_depth: 24,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode,
        };
        let queries = [
            // Imp-only: exercises depth-19 truncation and post-quest aborts.
            query(
                vec![wildcard(ItemKind::Ring, UpgradeRequirement::Exact(4))],
                false,
            ),
            // Wandmaker-only: exercises the depth-9 deadline.
            query(
                vec![wildcard(ItemKind::Wand, UpgradeRequirement::Exact(3))],
                false,
            ),
            // Ghost/Blacksmith/Crypt/Sacrifice interplay, exact semantics.
            query(
                vec![
                    wildcard(ItemKind::Weapon, UpgradeRequirement::Exact(3)),
                    wildcard(ItemKind::Armor, UpgradeRequirement::Exact(3)),
                ],
                false,
            ),
            // Mixed rare + ordinary requirement keeps full depth.
            query(
                vec![
                    wildcard(ItemKind::Ring, UpgradeRequirement::AtLeast(3)),
                    wildcard(ItemKind::Wand, UpgradeRequirement::AtLeast(1)),
                ],
                false,
            ),
        ];
        let seeds = (0..32)
            .map(|value| DungeonSeed::new(value).unwrap())
            .collect::<Vec<_>>();
        let full_worlds = seeds
            .iter()
            .map(|&seed| generate_main_world(seed, 24).unwrap())
            .collect::<Vec<_>>();

        let mut total_matches = 0;
        for query in &queries {
            let plan = QueryPlan::analyze(query);
            assert!(!plan.is_unsatisfiable());
            let gated = CanonicalMainWorldGenerator.generate_batch_gated(
                &seeds,
                plan.generation_depth(),
                &plan,
            );
            for (index, gated_world) in gated.iter().enumerate() {
                let expected = query.matches(&full_worlds[index]);
                let actual = gated_world
                    .as_ref()
                    .is_some_and(|world| query.matches(world));
                assert_eq!(
                    actual, expected,
                    "seed {} disagreed for {query:?}",
                    seeds[index]
                );
                total_matches += usize::from(expected);
            }
        }
        // Seed AAA-AAA-AAF carries a +4 Imp ring, so the comparison is not
        // vacuously passing on all-negative outcomes.
        assert!(total_matches > 0);
    }

    #[test]
    fn fast_mode_finds_only_genuine_matches() {
        let query = SearchQuery {
            requirements: vec![Requirement {
                kind: ItemKind::Armor,
                item: None,
                tier: TierRequirement::Any,
                upgrade: UpgradeRequirement::Exact(3),
                effect: None,
                source: None,
                identity_group: None,
                max_depth: None,
            }],
            max_depth: 24,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: true,
        };
        let plan = QueryPlan::analyze(&query);
        assert_eq!(plan.generation_depth(), 14);
        let seeds = (0..32)
            .map(|value| DungeonSeed::new(value).unwrap())
            .collect::<Vec<_>>();
        let gated = CanonicalMainWorldGenerator.generate_batch_gated(
            &seeds,
            plan.generation_depth(),
            &plan,
        );
        for (index, gated_world) in gated.iter().enumerate() {
            if let Some(world) = gated_world {
                if query.matches(world) {
                    // Every fast-mode match must be a genuine full-depth match.
                    let full = generate_main_world(seeds[index], 24).unwrap();
                    assert!(query.matches(&full));
                }
            }
        }
    }

    #[test]
    fn rejects_depths_outside_the_main_searchable_dungeon() {
        let seed = DungeonSeed::MIN;
        assert_eq!(
            generate_main_world(seed, 0),
            Err(MainWorldError::InvalidMaximumDepth(0))
        );
        assert_eq!(
            generate_main_world(seed, 25),
            Err(MainWorldError::InvalidMaximumDepth(25))
        );
    }

    #[test]
    fn state_neutral_boss_depths_match_their_preceding_regular_prefixes() {
        let seed = DungeonSeed::MIN;
        for (boss, regular) in [(5, 4), (10, 9), (15, 14)] {
            assert_eq!(
                generate_main_world(seed, boss).unwrap(),
                generate_main_world(seed, regular).unwrap()
            );
        }
    }

    #[test]
    fn depth_twenty_includes_official_imp_shop_stock_without_halls_items() {
        let world = generate_main_world(DungeonSeed::MIN, 20).unwrap();
        assert!(world.items.iter().all(|item| item.depth <= 20));
        let depth_twenty = world
            .items
            .iter()
            .filter(|item| item.depth == 20)
            .map(|item| item.item)
            .collect::<Vec<_>>();
        assert_eq!(
            depth_twenty,
            vec![
                ItemId::PlateArmor,
                ItemId::ThrowingHammer,
                ItemId::Greatshield,
                ItemId::IncendiaryDart,
            ]
        );
    }

    #[test]
    fn ghost_rewards_are_reported_once_on_the_ghost_floor() {
        let seed = DungeonSeed::from_code("AAA-GPY-IJW").unwrap();
        let world = generate_main_world(seed, 24).unwrap();
        let ghost_rewards = world
            .items
            .iter()
            .filter(|item| item.source == ItemSource::GhostReward)
            .collect::<Vec<_>>();

        assert_eq!(ghost_rewards.len(), 2);
        assert!(ghost_rewards.iter().all(|item| item.depth == 3));
        assert!(matches!(
            (ghost_rewards[0].accessibility, ghost_rewards[1].accessibility),
            (
                crate::model::Accessibility::Choice { group: first, option: 0 },
                crate::model::Accessibility::Choice { group: second, option: 1 },
            ) if first == second
        ));
    }

    #[test]
    fn java_oracle_plus_four_imp_transmutation_fixture_matches() {
        // Official v3.3.8 oracle command:
        // tooling/oracle/run.sh AAA-AAA-AAF 17 --transmute-imp
        // RingOfSharpshooting +4 -> RingOfAccuracy +4.
        let seed = DungeonSeed::from_code("AAA-AAA-AAF").unwrap();
        let world = generate_main_world(seed, 24).unwrap();
        let imp_ring = world.items.iter().find(|value| {
            value.item == ItemId::RingSharpshooting
                && value.upgrade == 4
                && value.depth == 17
                && value.cursed
                && value.source == ItemSource::ImpReward
        });
        assert!(imp_ring.is_some());
        let transmuted_item = imp_ring
            .and_then(|ring| ring.transmuted_item)
            .expect("+4 Imp rings expose one transmutation roll");
        assert_eq!(transmuted_item, ItemId::RingAccuracy);
        let query = SearchQuery {
            requirements: vec![Requirement {
                kind: ItemKind::Ring,
                item: Some(ItemId::RingSharpshooting),
                tier: TierRequirement::Any,
                upgrade: crate::query::UpgradeRequirement::Exact(4),
                effect: None,
                source: None,
                identity_group: None,
                max_depth: None,
            }],
            max_depth: 24,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: false,
        };
        assert_eq!(query.validate(), Ok(()));
        assert!(query.matches(&world));

        let transmuted_query = SearchQuery {
            requirements: vec![Requirement {
                item: Some(transmuted_item),
                ..query.requirements[0]
            }],
            ..query
        };
        assert!(transmuted_query.matches(&world));
    }

    #[test]
    fn canonical_depth_twenty_four_batch_matches_scalar_generation() {
        let seeds = [
            DungeonSeed::MIN,
            DungeonSeed::new(1).unwrap(),
            DungeonSeed::new(2).unwrap(),
            DungeonSeed::new(3).unwrap(),
            DungeonSeed::new(26).unwrap(),
            DungeonSeed::from_code("ABC-DEF-GHI").unwrap(),
            DungeonSeed::new(DungeonSeed::MAX.value() - 1).unwrap(),
            DungeonSeed::MAX,
        ];
        let scalar = seeds
            .iter()
            .copied()
            .map(|seed| CanonicalMainWorldGenerator.generate(seed, 24))
            .collect::<Vec<_>>();
        assert_eq!(
            CanonicalMainWorldGenerator.generate_batch(&seeds, 24),
            scalar
        );
    }

    #[test]
    fn explicit_zero_challenge_generator_is_byte_for_byte_compatible() {
        let configured =
            CanonicalMainWorldGenerator::with_challenges(crate::challenges::Challenges::NONE);
        for value in [0, 1, 5, 26, 8_687_205_886] {
            let seed = DungeonSeed::new(value).unwrap();
            let configured_world = configured.generate(seed, 24);
            let legacy_world = generate_main_world(seed, 24).unwrap();
            assert_eq!(
                crate::wire::encode_scout_world(&configured_world).unwrap(),
                crate::wire::encode_scout_world(&legacy_world).unwrap(),
            );
        }
    }
}
