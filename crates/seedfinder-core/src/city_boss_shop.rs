//! Stateful depth-20 `CityBossLevel` shop transition.
//!
//! Official v3.3.8 constructs and paints `ImpShopRoom` while building the City
//! boss floor. `ImpShopRoom.paint()` eagerly caches `ShopRoom.generateItems()`
//! even though the stock is not placed until the Imp quest is completed. This
//! means depth 20 is not a skippable boss floor: its searchable stock belongs
//! in seed results and its Generator mutations must reach Halls floor 21.

use crate::model::WorldItem;
use crate::rng::RandomStack;
use crate::run::RunState;
use crate::shop::{ShopError, ShopInventory, ShopRunState, generate_shop_inventory};

/// Exact initially generated content of the depth-20 Imp shop.
#[derive(Clone, Debug, PartialEq)]
pub struct CityBossShopResult {
    /// Complete shuffled `itemsToSpawn` cache retained by `ImpShopRoom`.
    pub inventory: ShopInventory,
    /// Weapon, armor, missile, tipped-dart, and possible rare-wand records.
    pub world_items: Vec<WorldItem>,
}

/// Runs the depth-20 `ImpShopRoom.paint()` generation side effect.
///
/// The caller must have pushed `Dungeon.seedForDepth(20, 0)` and must call
/// this before any other depth-20 draws. This is the exact upstream position:
/// all `CityBossLevel.build()` operations before `impShop.paint(this)` are
/// deterministic rectangle/tile writes and consume no RNG. The ordinary
/// limited-drop counters are unchanged; official bag flags and Hourglass state
/// are represented by [`ShopRunState`].
///
/// # Errors
///
/// Returns an error only if the live Generator state violates a pinned shop
/// invariant. Depth 20 itself is always supported.
pub fn generate_city_boss_shop(
    run: &mut RunState,
    shop_run: &mut ShopRunState,
    random: &mut RandomStack,
) -> Result<CityBossShopResult, ShopError> {
    let inventory = generate_shop_inventory(random, &mut run.generator, 20, shop_run)?;
    let world_items = inventory.searchable_items().cloned().collect();
    Ok(CityBossShopResult {
        inventory,
        world_items,
    })
}

#[cfg(test)]
mod tests {
    use crate::catalog::ItemId;
    use crate::caves_floor::generate_caves_floor;
    use crate::city_floor::generate_city_floor;
    use crate::level_prelude::LimitedDrops;
    use crate::model::{Accessibility, ItemSource};
    use crate::prison_floor::generate_prison_floor;
    use crate::quests::QuestState;
    use crate::rng::{RandomStack, seed_for_depth};
    use crate::run::RunState;
    use crate::seed::DungeonSeed;
    use crate::sewer_floor::generate_sewer_floor;
    use crate::shop::{ShopRunState, ShopStockItem};

    use super::generate_city_boss_shop;

    #[test]
    fn official_sequential_depth_twenty_shop_fixtures_match() {
        let fixtures: &[(&str, &[(usize, ItemId)])] = &[
            (
                "AAA-AAA-AAA",
                &[
                    (2, ItemId::PlateArmor),
                    (3, ItemId::ThrowingHammer),
                    (7, ItemId::Greatshield),
                    (16, ItemId::IncendiaryDart),
                ],
            ),
            (
                "AAA-AAA-AAB",
                &[
                    (13, ItemId::Trident),
                    (14, ItemId::HolyDart),
                    (18, ItemId::Greatshield),
                    (19, ItemId::PlateArmor),
                ],
            ),
            (
                "ABC-DEF-GHI",
                &[
                    (3, ItemId::Trident),
                    (10, ItemId::PlateArmor),
                    (15, ItemId::Glaive),
                    (17, ItemId::ParalyticDart),
                ],
            ),
        ];

        for &(code, expected) in fixtures {
            let seed = DungeonSeed::from_code(code).unwrap();
            let (mut run, mut shop_run, mut random) = regular_prefix_through_city(seed);
            let before = run.generator.clone();
            random.push(seed_for_depth(run.dungeon_seed, 20, 0));
            let result = generate_city_boss_shop(&mut run, &mut shop_run, &mut random).unwrap();
            random.pop();

            let positions = result
                .inventory
                .items
                .iter()
                .enumerate()
                .filter_map(|(index, item)| match item {
                    ShopStockItem::Searchable(world) => Some((index, world.item)),
                    ShopStockItem::Generated(_) | ShopStockItem::Direct(_) => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(positions, expected, "official Imp-shop fixture for {code}");
            assert_ne!(run.generator, before, "depth 20 must advance Generator");
            assert_eq!(result.world_items.len(), expected.len());
            for item in result.world_items {
                assert_eq!(item.depth, 20);
                assert_eq!(item.source, ItemSource::Shop);
                assert_eq!(item.accessibility, Accessibility::Independent);
                assert_eq!(item.upgrade, 0);
                assert_eq!(item.effect, None);
                assert!(!item.cursed);
            }
        }
    }

    fn regular_prefix_through_city(seed: DungeonSeed) -> (RunState, ShopRunState, RandomStack) {
        let dungeon_seed = i64::try_from(seed.value()).unwrap();
        let mut run = RunState::new(dungeon_seed);
        let mut limited = LimitedDrops::default();
        let mut quests = QuestState::new();
        let mut shop_run = ShopRunState::default();
        let mut random = RandomStack::with_base_seed(0);

        for depth in 1..=4_u32 {
            random.push(seed_for_depth(dungeon_seed, depth, 0));
            generate_sewer_floor(&mut run, &mut limited, &mut quests, depth, &mut random).unwrap();
            random.pop();
        }
        for depth in 6..=9_u32 {
            random.push(seed_for_depth(dungeon_seed, depth, 0));
            generate_prison_floor(
                &mut run,
                &mut limited,
                &mut quests,
                &mut shop_run,
                depth,
                &mut random,
            )
            .unwrap();
            random.pop();
        }
        for depth in 11..=14_u32 {
            random.push(seed_for_depth(dungeon_seed, depth, 0));
            generate_caves_floor(
                &mut run,
                &mut limited,
                &mut quests,
                &mut shop_run,
                depth,
                &mut random,
            )
            .unwrap();
            random.pop();
        }
        for depth in 16..=19_u32 {
            random.push(seed_for_depth(dungeon_seed, depth, 0));
            generate_city_floor(
                &mut run,
                &mut limited,
                &mut quests,
                &mut shop_run,
                depth,
                &mut random,
            )
            .unwrap();
            random.pop();
        }

        (run, shop_run, random)
    }
}
