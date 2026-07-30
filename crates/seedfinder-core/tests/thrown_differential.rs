//! One-off differential sweep for the melee/thrown feasibility change.
//! Compares plan-gated search against brute-force full generation + matcher.

use shpd_seedfinder_core::catalog::{ItemKind, WeaponCategory};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::feasibility::QueryPlan;
use shpd_seedfinder_core::main_world::CanonicalMainWorldGenerator;
use shpd_seedfinder_core::main_world::generate_main_world;
use shpd_seedfinder_core::model::ItemSource;
use shpd_seedfinder_core::query::{Requirement, SearchQuery, TierRequirement, UpgradeRequirement};
use shpd_seedfinder_core::search::WorldGenerator;
use shpd_seedfinder_core::seed::DungeonSeed;

const SEEDS: u64 = 3_000;

fn requirement(category: Option<WeaponCategory>, upgrade: UpgradeRequirement) -> Requirement {
    Requirement {
        kind: ItemKind::Weapon,
        weapon_category: category,
        item: None,
        tier: TierRequirement::Any,
        upgrade,
        effect: None,
        require_uncursed: false,
        source: None,
        identity_group: None,
        max_depth: None,
    }
}

fn query(requirements: Vec<Requirement>, fast_mode: bool) -> SearchQuery {
    SearchQuery {
        requirements,
        max_depth: 24,
        challenges: Challenges::NONE,
        require_blacksmith: false,
        exclude_blacksmith_rewards: false,
        fast_mode,
    }
}

#[test]
#[ignore = "one-off verification sweep"]
fn thrown_plans_never_skip_a_matching_seed() {
    let seeds = (0..SEEDS)
        .map(|value| DungeonSeed::new(value).unwrap())
        .collect::<Vec<_>>();
    let full_worlds = seeds
        .iter()
        .map(|&seed| generate_main_world(seed, 24).unwrap())
        .collect::<Vec<_>>();

    let categories = [
        None,
        Some(WeaponCategory::Melee),
        Some(WeaponCategory::Thrown),
    ];
    let upgrades = [
        UpgradeRequirement::Any,
        UpgradeRequirement::Exact(0),
        UpgradeRequirement::Exact(1),
        UpgradeRequirement::Exact(2),
        UpgradeRequirement::Exact(3),
        UpgradeRequirement::AtLeast(1),
        UpgradeRequirement::AtLeast(2),
        UpgradeRequirement::AtLeast(3),
    ];
    let sources = [
        None,
        Some(ItemSource::Heap),
        Some(ItemSource::Chest),
        Some(ItemSource::Statue),
        Some(ItemSource::ArmoredStatue),
        Some(ItemSource::SacrificialFire),
        Some(ItemSource::GhostReward),
        Some(ItemSource::BlacksmithReward),
        Some(ItemSource::GoldenMimic),
        Some(ItemSource::Skeleton),
    ];

    let mut checked = 0_u64;
    let mut positive = 0_u64;
    let mut unsat_queries = 0_u64;
    for fast_mode in [false, true] {
        for &category in &categories {
            for &upgrade in &upgrades {
                for &source in &sources {
                    let q = query(
                        vec![Requirement {
                            source,
                            ..requirement(category, upgrade)
                        }],
                        fast_mode,
                    );
                    if q.validate().is_err() {
                        continue;
                    }
                    let plan = QueryPlan::analyze(&q);
                    if plan.is_unsatisfiable() {
                        unsat_queries += 1;
                        // The plan's impossibility claim must hold against
                        // brute force: no fully generated world may match.
                        // (Only exact for non-fast mode; fast mode is
                        // documented as deliberately lossy.)
                        if !fast_mode {
                            for (index, world) in full_worlds.iter().enumerate() {
                                assert!(
                                    !q.matches(world),
                                    "plan called {q:?} unsatisfiable but seed {} matches",
                                    seeds[index]
                                );
                            }
                        }
                        continue;
                    }
                    let gated = CanonicalMainWorldGenerator.generate_batch_gated(
                        &seeds,
                        plan.generation_depth(),
                        &plan,
                    );
                    for (index, gated_world) in gated.iter().enumerate() {
                        let expected = q.matches(&full_worlds[index]);
                        let actual = gated_world.as_ref().is_some_and(|world| q.matches(world));
                        if fast_mode {
                            // Fast mode may miss exotic matches by design but
                            // must never invent one.
                            assert!(
                                !actual || expected,
                                "fast mode invented a match for seed {} under {q:?}",
                                seeds[index]
                            );
                        } else {
                            assert_eq!(
                                actual, expected,
                                "seed {} disagreed for {q:?}",
                                seeds[index]
                            );
                        }
                        positive += u64::from(expected);
                    }
                    checked += 1;
                }
            }
        }
    }
    println!(
        "checked {checked} satisfiable queries ({unsat_queries} unsat) over {SEEDS} seeds; {positive} positive outcomes"
    );
    assert!(positive > 0);
}

/// Outside fast mode the plan is exact: gated search must agree with brute
/// force for every thrown query, including +3 via special-room chest prizes.
#[test]
#[ignore = "one-off verification sweep"]
fn non_fast_thrown_queries_agree_with_brute_force() {
    let seeds = (0..SEEDS)
        .map(|value| DungeonSeed::new(value).unwrap())
        .collect::<Vec<_>>();
    let full_worlds = seeds
        .iter()
        .map(|&seed| generate_main_world(seed, 24).unwrap())
        .collect::<Vec<_>>();
    for upgrade in [
        UpgradeRequirement::Any,
        UpgradeRequirement::Exact(3),
        UpgradeRequirement::AtLeast(2),
        UpgradeRequirement::AtLeast(3),
    ] {
        let q = query(
            vec![requirement(Some(WeaponCategory::Thrown), upgrade)],
            false,
        );
        let plan = QueryPlan::analyze(&q);
        assert!(!plan.is_unsatisfiable());
        let gated = CanonicalMainWorldGenerator.generate_batch_gated(
            &seeds,
            plan.generation_depth(),
            &plan,
        );
        for (index, gated_world) in gated.iter().enumerate() {
            let expected = q.matches(&full_worlds[index]);
            let actual = gated_world.as_ref().is_some_and(|world| q.matches(world));
            assert_eq!(
                actual, expected,
                "gated search disagreed on seed {} for thrown {upgrade:?}",
                seeds[index]
            );
        }
    }
}
