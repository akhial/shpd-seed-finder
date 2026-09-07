//! Initial catalyst offer identities. Reading this deck never mutates the world RNG.
use crate::catalog::ItemId;
use crate::generator::{GeneratedItem, TrinketKind, random_category};
use crate::model::WorldItem;
use crate::rng::RandomStack;
use crate::run::{GeneratorCategory, GeneratorState, RunState};
use crate::seed::DungeonSeed;

pub const INITIAL_OFFER_COUNT: usize = 4;

/// Complete private-deck draw order. Only the first four are initial offers;
/// the tail is diagnostic order, not a promise about gameplay transmutations.
///
/// # Panics
/// Panics only if a validated dungeon seed or the pinned trinket deck violates
/// its generation invariants.
#[must_use]
pub fn trinket_order(seed: DungeonSeed) -> [ItemId; 17] {
    order_from_generator(
        &RunState::new(i64::try_from(seed.value()).expect("seed fits i64")).generator,
    )
}

fn order_from_generator(generator: &GeneratorState) -> [ItemId; 17] {
    let mut generator = generator.clone();
    let mut random = RandomStack::with_base_seed(0);
    std::array::from_fn(|_| {
        let GeneratedItem::Trinket(kind) =
            random_category(&mut random, &mut generator, GeneratorCategory::Trinket, 1)
                .expect("valid trinket deck")
        else {
            unreachable!()
        };
        match kind {
            TrinketKind::RatSkull => ItemId::RatSkull,
            TrinketKind::ParchmentScrap => ItemId::ParchmentScrap,
            TrinketKind::PetrifiedSeed => ItemId::PetrifiedSeed,
            TrinketKind::ExoticCrystals => ItemId::ExoticCrystals,
            TrinketKind::MossyClump => ItemId::MossyClump,
            TrinketKind::DimensionalSundial => ItemId::DimensionalSundial,
            TrinketKind::ThirteenLeafClover => ItemId::ThirteenLeafClover,
            TrinketKind::TrapMechanism => ItemId::TrapMechanism,
            TrinketKind::MimicTooth => ItemId::MimicTooth,
            TrinketKind::WondrousResin => ItemId::WondrousResin,
            TrinketKind::EyeOfNewt => ItemId::EyeOfNewt,
            TrinketKind::SaltCube => ItemId::SaltCube,
            TrinketKind::VialOfBlood => ItemId::VialOfBlood,
            TrinketKind::ShardOfOblivion => ItemId::ShardOfOblivion,
            TrinketKind::ChaoticCenser => ItemId::ChaoticCenser,
            TrinketKind::FerretTuft => ItemId::FerretTuft,
            TrinketKind::CrackedSpyglass => ItemId::CrackedSpyglass,
        }
    })
}

pub(crate) fn expand_catalyst_offers(items: &mut Vec<WorldItem>, generator: &GeneratorState) {
    if !items
        .iter()
        .any(|item| item.item == ItemId::TrinketCatalyst)
    {
        return;
    }
    let order = order_from_generator(generator);
    *items = items
        .drain(..)
        .flat_map(|world_item| {
            if world_item.item == ItemId::TrinketCatalyst {
                order[..INITIAL_OFFER_COUNT]
                    .iter()
                    .map(|&item| WorldItem {
                        item,
                        ..world_item.clone()
                    })
                    .collect()
            } else {
                vec![world_item]
            }
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{ItemKind, item};
    use crate::challenges::Challenges;
    use crate::main_world::CanonicalMainWorldGenerator;
    use crate::query::{SearchQuery, scout_matches};
    use crate::search::WorldGenerator;

    #[test]
    fn complete_order_matches_beta4_jar_and_does_not_touch_generator() {
        // TrinketOracle AAA-AAA-AAA, pinned official BETA-4 JAR.
        let expected = [
            277, 280, 273, 278, 286, 279, 282, 275, 284, 274, 285, 288, 281, 276, 272, 287, 283,
        ];
        assert_eq!(
            trinket_order(DungeonSeed::MIN).map(|id| item(id).sprite_index),
            expected
        );
        let run = RunState::new(0);
        let before = run.generator.clone();
        assert_eq!(
            order_from_generator(&run.generator),
            trinket_order(DungeonSeed::MIN)
        );
        assert_eq!(run.generator, before);
    }

    #[test]
    fn offers_follow_catalyst_placement_in_scalar_and_gated_worlds() {
        let mut sources = std::collections::HashSet::new();
        for value in 0..100 {
            let seed = DungeonSeed::new(value).unwrap();
            let world = CanonicalMainWorldGenerator.generate(seed, 3);
            let offers: Vec<_> = world
                .items
                .iter()
                .filter(|i| item(i.item).kind == ItemKind::Trinket)
                .collect();
            assert_eq!(offers.len(), 4, "seed {value}");
            let location = offers[0];
            sources.insert(location.source);
            for offer in &offers {
                assert!((1..=3).contains(&offer.depth));
                assert_eq!(
                    (offer.depth, offer.source, offer.accessibility, offer.secret),
                    (
                        location.depth,
                        location.source,
                        location.accessibility,
                        location.secret
                    )
                );
                assert!(trinket_order(seed)[..4].contains(&offer.item));
            }
            // Each first offer matches, each tail entry does not, and a floor
            // limit before the actual catalyst must not claim an offer.
            for (index, id) in trinket_order(seed).iter().enumerate() {
                let query = crate::json_query::decode(&format!(
                    r#"{{"requirements":[{{"item":"{}"}}],"max_depth":3}}"#,
                    item(*id).stable_id
                ))
                .unwrap();
                assert_eq!(
                    scout_matches(&world, &query).matched_requirements,
                    usize::from(index < 4)
                );
                if index == 0 {
                    let plan = crate::feasibility::QueryPlan::analyze(&query);
                    let generated = CanonicalMainWorldGenerator.generate_batch_gated(
                        &[seed],
                        plan.generation_depth(),
                        &plan,
                    );
                    assert_eq!(generated[0].as_ref(), Some(&world));
                    if location.depth > 1 {
                        let early = SearchQuery {
                            max_depth: location.depth - 1,
                            ..query
                        };
                        assert_eq!(scout_matches(&world, &early).matched_requirements, 0);
                    }
                }
            }
        }
        assert!(sources.contains(&crate::model::ItemSource::LockedChest));
        assert!(sources.contains(&crate::model::ItemSource::Heap));
    }

    #[test]
    fn trinket_or_groups_and_shared_codecs_keep_offer_semantics() {
        let query = crate::json_query::decode(r#"{"requirements":[{"any_of":[{"item":"rat_skull"},{"item":"mimic_tooth"}]}],"max_depth":3}"#).unwrap();
        let world = CanonicalMainWorldGenerator.generate(DungeonSeed::MIN, 3);
        assert_eq!(scout_matches(&world, &query).matched_requirements, 1);
        assert_eq!(
            crate::deep_link::decode(&crate::deep_link::encode(&query).unwrap()).unwrap(),
            query
        );
        assert_eq!(
            crate::json_query::decode(&crate::json_query::encode(&query).to_string()).unwrap(),
            query
        );
        assert_eq!(query.challenges, Challenges::NONE);
        assert!(
            crate::json_query::decode(r#"{"requirements":[{"item":"rat_skull","upgrade":1}]}"#)
                .is_err()
        );
    }
}
