//! Initial catalyst offer identities. Reading this deck never mutates the world RNG.
use crate::catalog::ItemId;
use crate::generator::{GeneratedItem, TrinketKind, random_category};
use crate::model::WorldItem;
use crate::rng::RandomStack;
use crate::run::{GeneratorCategory, GeneratorState, RunState};
use crate::seed::DungeonSeed;

/// The full OR slots which request choosing a trinket. Keep every alternative:
/// even an unchecked alternative makes a multiply matching group ambiguous.
#[must_use]
pub fn selection_slots(query: &crate::query::SearchQuery) -> Vec<Vec<crate::query::Requirement>> {
    query
        .slots()
        .into_iter()
        .filter(|slot| slot.iter().any(|&i| query.requirements[i].select_trinket))
        .map(|slot| slot.into_iter().map(|i| query.requirements[i]).collect())
        .collect()
}

/// Resolve against initial offers, never against the remaining private deck.
/// Multiple distinct matching offers (within or across chosen slots) mean no trinket.
#[must_use]
pub fn resolve_selection(
    seed: DungeonSeed,
    slots: &[Vec<crate::query::Requirement>],
) -> Option<ItemId> {
    if slots.is_empty() {
        return None;
    }
    let order = trinket_order(seed);
    let mut matches = order[..INITIAL_OFFER_COUNT].iter().copied().filter(|id| {
        slots
            .iter()
            .any(|slot| slot.iter().any(|r| r.item == Some(*id)))
    });
    let selected = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(selected)
    }
}

/// Per-run, +3 generation effects. Stored alongside the RNG stack so nested
/// category/effect streams inherit the profile without thread-global state.
#[derive(Clone, Debug, Default)]
pub(crate) struct TrinketEffects {
    pub selected: Option<ItemId>,
    dungeon_seed: i64,
    feelings: Vec<bool>,
    shuffles: u32,
}

impl TrinketEffects {
    pub fn new(selected: Option<ItemId>, dungeon_seed: i64) -> Self {
        Self {
            selected,
            dungeon_seed,
            ..Self::default()
        }
    }
    pub fn is(&self, id: ItemId) -> bool {
        self.selected == Some(id)
    }
    pub fn level(&self, id: ItemId) -> i32 {
        if self.is(id) { 3 } else { -1 }
    }
    pub fn mimic_multiplier(&self) -> f32 {
        if self.is(ItemId::MimicTooth) {
            3.0
        } else {
            1.0
        }
    }
    pub fn exotic_multiplier(&self) -> f32 {
        if self.is(ItemId::RatSkull) { 5.0 } else { 1.0 }
    }
    pub fn exotic_chance(&self) -> f32 {
        if self.is(ItemId::ExoticCrystals) {
            0.8
        } else {
            0.0
        }
    }
    pub fn reveal_chance(&self) -> f32 {
        if self.is(ItemId::TrapMechanism) {
            0.4
        } else {
            0.0
        }
    }
    pub fn curse_multiplier(&self) -> f32 {
        if self.is(ItemId::ParchmentScrap) {
            0.0
        } else {
            1.0
        }
    }
    pub fn enchant_multiplier(&self) -> f32 {
        if self.is(ItemId::ParchmentScrap) {
            10.0
        } else {
            1.0
        }
    }
    pub fn next_feeling(&mut self) -> crate::level_prelude::Feeling {
        use crate::level_prelude::Feeling;
        let moss = self.is(ItemId::MossyClump);
        if self.feelings.is_empty() {
            self.feelings = if moss {
                vec![true, true, false, false, false, false]
            } else {
                vec![true, true, true, false, false, false]
            };
            let mut random = RandomStack::with_base_seed(0);
            random.push(self.dungeon_seed.wrapping_add(1));
            for _ in 0..=self.shuffles {
                random.shuffle_list(&mut self.feelings);
            }
            self.shuffles += 1;
        }
        match (moss, self.feelings.remove(0)) {
            (true, true) => Feeling::Grass,
            (true, false) => Feeling::Water,
            (false, true) => Feeling::Traps,
            (false, false) => Feeling::Chasm,
        }
    }
}

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
    fn selected_slots_resolve_uniquely_and_preserve_codecs() {
        let parse = |json: &str| crate::json_query::decode(json).unwrap();
        let single = parse(r#"{"requirements":[{"item":"mimic_tooth","select_trinket":true}]}"#);
        assert_eq!(
            resolve_selection(DungeonSeed::MIN, &selection_slots(&single)),
            Some(ItemId::MimicTooth)
        );
        let unique = parse(
            r#"{"requirements":[{"any_of":[{"item":"mimic_tooth","select_trinket":true},{"item":"rat_skull"}]}]}"#,
        );
        let ambiguous = parse(
            r#"{"requirements":[{"any_of":[{"item":"mimic_tooth","select_trinket":true},{"item":"parchment_scrap"}]}]}"#,
        );
        let absent = parse(r#"{"requirements":[{"item":"rat_skull","select_trinket":true}]}"#);
        assert_eq!(
            resolve_selection(DungeonSeed::MIN, &selection_slots(&unique)),
            Some(ItemId::MimicTooth)
        );
        assert_eq!(
            resolve_selection(DungeonSeed::MIN, &selection_slots(&ambiguous)),
            None
        );
        assert_eq!(
            resolve_selection(DungeonSeed::MIN, &selection_slots(&absent)),
            None
        );
        for query in [single, unique, ambiguous, absent] {
            assert_eq!(
                crate::json_query::decode(&crate::json_query::encode(&query).to_string()).unwrap(),
                query
            );
            assert_eq!(
                crate::deep_link::decode(&crate::deep_link::encode(&query).unwrap()).unwrap(),
                query
            );
            let mut without = query.clone();
            for r in &mut without.requirements {
                r.select_trinket = false;
            }
            assert!(!query.continues(&without));
            assert!(!without.continues(&query));
        }
        assert!(
            crate::json_query::decode(
                r#"{"requirements":[{"kind":"weapon","select_trinket":true}]}"#
            )
            .is_err()
        );
    }

    #[test]
    fn selected_generation_preserves_prebrew_floors_and_matches_gated_search() {
        use crate::main_world::generate_main_world_with_trinket;
        let seed = DungeonSeed::MIN;
        let unselected = CanonicalMainWorldGenerator.generate(seed, 24);
        let selected =
            generate_main_world_with_trinket(seed, 24, Challenges::NONE, Some(ItemId::MimicTooth))
                .unwrap();
        // Seed zero's first lab is floor 4, so effects begin on floor 6.
        assert_eq!(
            unselected
                .items
                .iter()
                .filter(|i| i.depth <= 4)
                .collect::<Vec<_>>(),
            selected
                .items
                .iter()
                .filter(|i| i.depth <= 4)
                .collect::<Vec<_>>()
        );
        assert_ne!(unselected.items, selected.items);
        let query = crate::json_query::decode(r#"{"requirements":[{"item":"mimic_tooth","select_trinket":true},{"kind":"weapon","max_depth":24}]}"#).unwrap();
        let plan = crate::feasibility::QueryPlan::analyze(&query);
        let batch = CanonicalMainWorldGenerator.generate_batch_gated(&[seed; 5], 24, &plan);
        assert!(batch.iter().all(Option::is_some));
        for world in batch.into_iter().flatten() {
            // Search may skip vault treasure when no requirement can use it.
            assert_eq!(world, selected);
        }
    }

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
        assert!(crate::json_query::decode(r#"{"requirements":[{"kind":"trinket"}]}"#).is_err());
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
