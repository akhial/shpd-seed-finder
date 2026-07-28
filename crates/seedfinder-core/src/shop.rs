//! Exact canonical v3.3.8 `ShopRoom.generateItems()` generation.
//!
//! Shop stock is decided during room construction. Weapons, missiles, and the
//! rare wand path first run their normal randomization and are then forcibly
//! reset to level zero, uncursed, and without an enchantment. The discarded
//! rolls are still consumed here because they affect all later level RNG.
//!
//! `ShopRoom.itemsToSpawn` is an instance cache. [`ShopRoomCache`] reproduces
//! that behavior: repeated size/paint queries for the same room do not generate
//! stock twice. A failed outer `Level.build()` creates a new Java room and must
//! therefore use a fresh cache; retries inside `RegularBuilder` reuse it.

use std::fmt;

use crate::catalog::ItemId;
use crate::equipment::EquipmentRoll;
use crate::generator::{
    BombKind, GeneratedArtifact, GeneratedEquipment, GeneratedItem, GeneratedItemFamily,
    GeneratedMissile, GeneratorError, MissileKind, random_category, random_tipped_dart,
    random_using_defaults,
};
use crate::model::{Accessibility, ItemSource, WorldItem};
use crate::rng::RandomStack;
use crate::run::{GeneratorCategory, GeneratorState};

const BAG_KINDS: [ShopBagKind; 4] = [
    ShopBagKind::VelvetPouch,
    ShopBagKind::ScrollHolder,
    ShopBagKind::PotionBandolier,
    ShopBagKind::MagicalHolster,
];

/// A shop bag class considered by Java's `ShopRoom.ChooseBag`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ShopBagKind {
    VelvetPouch,
    ScrollHolder,
    PotionBandolier,
    MagicalHolster,
}

/// Compact set of [`ShopBagKind`] values.
#[derive(Clone, Copy, Default, Eq, Hash, PartialEq)]
pub struct ShopBagSet(u8);

impl ShopBagSet {
    /// Empty bag set.
    pub const EMPTY: Self = Self(0);

    /// All four special inventory bags.
    pub const ALL: Self = Self(0b1111);

    /// Builds a set containing one bag.
    #[must_use]
    pub const fn one(bag: ShopBagKind) -> Self {
        Self(1 << bag as u8)
    }

    /// Whether this set contains `bag`.
    #[must_use]
    pub const fn contains(self, bag: ShopBagKind) -> bool {
        self.0 & Self::one(bag).0 != 0
    }

    /// Number of bags in the set.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    /// Whether no bags are present.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    const fn without(self, bag: ShopBagKind) -> Self {
        Self(self.0 & !Self::one(bag).0)
    }

    const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl fmt::Debug for ShopBagSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_set().entries(self.iter()).finish()
    }
}

impl ShopBagSet {
    /// Iterates in the source declaration order used by this model. Java does
    /// not preserve this order in its `HashMap`; ties are represented below.
    pub fn iter(self) -> impl Iterator<Item = ShopBagKind> {
        BAG_KINDS.into_iter().filter(move |bag| self.contains(*bag))
    }
}

/// Observable result of `ChooseBag`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShopBagOffer {
    /// Exactly one eligible bag had the greatest inventory-saving score.
    Deterministic(ShopBagKind),
    /// Multiple bags tied. Java iterates an ordinary identity-keyed `HashMap`,
    /// so the concrete winner is runtime-dependent rather than seed-dependent.
    RuntimeHashMapTie { candidates: ShopBagSet },
}

impl ShopBagOffer {
    /// Every concrete bag this offer can represent.
    #[must_use]
    pub const fn candidates(self) -> ShopBagSet {
        match self {
            Self::Deterministic(bag) => ShopBagSet::one(bag),
            Self::RuntimeHashMapTie { candidates } => candidates,
        }
    }
}

/// Cross-floor state for the four one-time bag drops.
///
/// `possible_remaining_masks` retains every concrete HashMap-tie outcome. The
/// ambiguity never consumes RNG and all bag classes occupy one stock slot, so
/// it cannot alter searchable shop results or the stock shuffle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShopBagState {
    possible_remaining_masks: u16,
    savings: [i32; 4],
}

impl ShopBagState {
    /// Canonical fresh Warrior profile: the Velvet Pouch is already collected;
    /// Waterskin and Throwing Stones give Bandolier and Holster one saving each.
    #[must_use]
    pub const fn canonical_warrior() -> Self {
        let available = ShopBagSet::ALL.without(ShopBagKind::VelvetPouch);
        Self::with_scores(available, [1, 0, 1, 1])
    }

    /// Builds state for a known availability set and Java inventory-saving
    /// scores. This permits exact callers with a different hero backpack.
    #[must_use]
    pub const fn with_scores(available: ShopBagSet, savings: [i32; 4]) -> Self {
        Self {
            possible_remaining_masks: 1_u16 << available.0,
            savings,
        }
    }

    /// Union of bags that may remain after all unresolved runtime ties.
    #[must_use]
    pub fn possibly_remaining(self) -> ShopBagSet {
        let mut union = ShopBagSet::EMPTY;
        let mut mask = 0_u8;
        while mask < 16 {
            if self.possible_remaining_masks & (1_u16 << mask) != 0 {
                union = union.union(ShopBagSet(mask));
            }
            mask += 1;
        }
        union
    }

    fn choose(&mut self) -> Option<ShopBagOffer> {
        let mut offered = ShopBagSet::EMPTY;
        let mut next_states = 0_u16;

        for remaining_mask in 0_u8..16 {
            if self.possible_remaining_masks & (1_u16 << remaining_mask) == 0 {
                continue;
            }
            let remaining = ShopBagSet(remaining_mask);
            if remaining.is_empty() {
                next_states |= 1;
                continue;
            }

            let best_score = remaining
                .iter()
                .map(|bag| self.savings[bag as usize])
                .max()
                .expect("a non-empty bag set has a score");
            for bag in remaining
                .iter()
                .filter(|bag| self.savings[*bag as usize] == best_score)
            {
                offered = offered.union(ShopBagSet::one(bag));
                let next = remaining.without(bag);
                next_states |= 1_u16 << next.0;
            }
        }

        self.possible_remaining_masks = next_states;
        match offered.len() {
            0 => None,
            1 => Some(ShopBagOffer::Deterministic(
                offered
                    .iter()
                    .next()
                    .expect("singleton bag offer has one value"),
            )),
            _ => Some(ShopBagOffer::RuntimeHashMapTie {
                candidates: offered,
            }),
        }
    }
}

impl Default for ShopBagState {
    fn default() -> Self {
        Self::canonical_warrior()
    }
}

/// Relevant canonical Timekeeper's Hourglass state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ShopHourglassState {
    /// No identified, uncursed hourglass is carried.
    #[default]
    Ineligible,
    /// An identified, uncursed hourglass with the given current bag count.
    Eligible { sand_bags: i32 },
}

/// Non-RNG state mutated as shops are generated across a run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ShopRunState {
    pub bags: ShopBagState,
    pub hourglass: ShopHourglassState,
}

/// Directly constructed stock which does not call an item's `random()` method.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectShopItem {
    Alchemize { quantity: i32 },
    Bag(ShopBagOffer),
    PotionOfHealing,
    ScrollOfIdentify,
    ScrollOfRemoveCurse,
    ScrollOfMagicMapping,
    SmallRation,
    Honeypot,
    Ankh,
    StoneOfAugmentation,
    Stylus,
    Torch,
    HourglassSandBag,
}

/// One entry in the already-shuffled shop stock list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShopStockItem {
    /// Weapons, armor, missiles, and rare wands exposed to seed queries.
    Searchable(WorldItem),
    /// Exact non-searchable result from a `Generator` or item helper call.
    Generated(GeneratedItem),
    /// Constructor-only stock with no deterministic randomization.
    Direct(DirectShopItem),
}

/// Complete result of one call to `ShopRoom.generateItems()`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShopInventory {
    pub depth: u8,
    /// Java `ArrayList` order after `Collections.shuffle` on the child stream.
    pub items: Vec<ShopStockItem>,
    /// The one outer-stream `Long()` used to seed the stock shuffle.
    pub shuffle_seed: i64,
}

impl ShopInventory {
    /// Searchable stock in its shuffled placement order.
    pub fn searchable_items(&self) -> impl Iterator<Item = &WorldItem> {
        self.items.iter().filter_map(|item| match item {
            ShopStockItem::Searchable(world_item) => Some(world_item),
            ShopStockItem::Generated(_) | ShopStockItem::Direct(_) => None,
        })
    }
}

/// Per-Java-room `itemsToSpawn` cache.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShopRoomCache {
    inventory: Option<ShopInventory>,
}

impl ShopRoomCache {
    /// Returns cached stock or generates it exactly once for this room object.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsupported shop depth, corrupted version-pinned
    /// `GeneratorState`, or an internal cache invariant violation.
    pub fn items_to_spawn<'a>(
        &'a mut self,
        random: &mut RandomStack,
        generator: &mut GeneratorState,
        depth: i32,
        run_state: &mut ShopRunState,
    ) -> Result<&'a ShopInventory, ShopError> {
        if self.inventory.is_none() {
            self.inventory = Some(generate_shop_inventory(
                random, generator, depth, run_state,
            )?);
        }
        self.inventory.as_ref().ok_or(ShopError::CacheInvariant)
    }

    /// Whether this room has already consumed its shop-generation side effects.
    #[must_use]
    pub const fn is_generated(&self) -> bool {
        self.inventory.is_some()
    }
}

/// Shop-generation failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShopError {
    UnsupportedDepth(i32),
    CacheInvariant,
    Generator(GeneratorError),
    ExpectedEquipment(GeneratedItemFamily),
    ExpectedMissile(GeneratedItemFamily),
    UnsearchableMissile(MissileKind),
    UnexpectedRare(GeneratedItemFamily),
}

impl fmt::Display for ShopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedDepth(depth) => {
                write!(formatter, "no canonical shop at depth {depth}")
            }
            Self::CacheInvariant => formatter.write_str("shop cache failed to retain stock"),
            Self::Generator(error) => error.fmt(formatter),
            Self::ExpectedEquipment(family) => {
                write!(formatter, "shop expected equipment, generated {family:?}")
            }
            Self::ExpectedMissile(family) => {
                write!(formatter, "shop expected a missile, generated {family:?}")
            }
            Self::UnsearchableMissile(kind) => {
                write!(formatter, "shop selected non-searchable missile {kind:?}")
            }
            Self::UnexpectedRare(family) => {
                write!(formatter, "shop rare branch generated {family:?}")
            }
        }
    }
}

impl std::error::Error for ShopError {}

impl From<GeneratorError> for ShopError {
    fn from(error: GeneratorError) -> Self {
        Self::Generator(error)
    }
}

/// Mirrors `ShopRoom.generateItems()` for depths 6, 11, 16, 20, and 21.
///
/// `LastShopLevel` (21) and the City boss's `ImpShopRoom` (20) share the same
/// tier-five stock branch. Gold is intentionally absent: despite being a
/// Generator category, upstream `ShopRoom.generateItems()` never creates it.
///
/// # Errors
///
/// Returns an error only for a non-shop depth or a corrupted generator state.
pub fn generate_shop_inventory(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    depth: i32,
    run_state: &mut ShopRunState,
) -> Result<ShopInventory, ShopError> {
    let depth_u8 = u8::try_from(depth).map_err(|_| ShopError::UnsupportedDepth(depth))?;
    let (weapon_category, missile_category, armor) = match depth {
        6 => (
            GeneratorCategory::WeaponTier2,
            GeneratorCategory::MissileTier2,
            ItemId::LeatherArmor,
        ),
        11 => (
            GeneratorCategory::WeaponTier3,
            GeneratorCategory::MissileTier3,
            ItemId::MailArmor,
        ),
        16 => (
            GeneratorCategory::WeaponTier4,
            GeneratorCategory::MissileTier4,
            ItemId::ScaleArmor,
        ),
        20 | 21 => (
            GeneratorCategory::WeaponTier5,
            GeneratorCategory::MissileTier5,
            ItemId::PlateArmor,
        ),
        _ => return Err(ShopError::UnsupportedDepth(depth)),
    };

    let mut items = Vec::with_capacity(if depth >= 20 { 22 } else { 20 });
    items.push(searchable(armor, depth_u8));
    if depth >= 20 {
        items.extend([
            ShopStockItem::Direct(DirectShopItem::Torch),
            ShopStockItem::Direct(DirectShopItem::Torch),
            ShopStockItem::Direct(DirectShopItem::Torch),
        ]);
    }

    let weapon = expect_equipment(random_category(random, generator, weapon_category, depth)?)?;
    items.push(searchable(weapon.item, depth_u8));

    let missile = expect_missile(random_category(random, generator, missile_category, depth)?)?;
    let missile_item = missile
        .kind
        .item_id()
        .ok_or(ShopError::UnsearchableMissile(missile.kind))?;
    items.push(searchable(missile_item, depth_u8));

    items.push(random_searchable_tipped_dart(random, depth_u8)?);
    items.push(ShopStockItem::Direct(DirectShopItem::Alchemize {
        quantity: random.int_range(2, 3),
    }));

    if let Some(bag) = run_state.bags.choose() {
        items.push(ShopStockItem::Direct(DirectShopItem::Bag(bag)));
    }

    items.push(ShopStockItem::Direct(DirectShopItem::PotionOfHealing));
    items.push(ShopStockItem::Generated(random_using_defaults(
        random,
        generator,
        GeneratorCategory::Potion,
        depth,
    )?));
    items.push(ShopStockItem::Generated(random_using_defaults(
        random,
        generator,
        GeneratorCategory::Potion,
        depth,
    )?));

    items.extend([
        ShopStockItem::Direct(DirectShopItem::ScrollOfIdentify),
        ShopStockItem::Direct(DirectShopItem::ScrollOfRemoveCurse),
        ShopStockItem::Direct(DirectShopItem::ScrollOfMagicMapping),
    ]);

    for _ in 0..2 {
        let category = if random.int_bound(2) == 0 {
            GeneratorCategory::Potion
        } else {
            GeneratorCategory::Scroll
        };
        items.push(ShopStockItem::Generated(random_using_defaults(
            random, generator, category, depth,
        )?));
    }

    items.extend([
        ShopStockItem::Direct(DirectShopItem::SmallRation),
        ShopStockItem::Direct(DirectShopItem::SmallRation),
    ]);
    items.push(match random.int_bound(4) {
        0 => ShopStockItem::Generated(crate::generator::plain_bomb()),
        1 | 2 => ShopStockItem::Generated(GeneratedItem::Bomb(BombKind::DoubleBomb)),
        _ => ShopStockItem::Direct(DirectShopItem::Honeypot),
    });
    items.extend([
        ShopStockItem::Direct(DirectShopItem::Ankh),
        ShopStockItem::Direct(DirectShopItem::StoneOfAugmentation),
    ]);

    append_hourglass_bags(&mut items, depth, &mut run_state.hourglass);
    items.push(generate_rare(random, generator, depth, depth_u8)?);

    // This child generator prevents stock size/content from perturbing the
    // remaining level stream. The seed draw itself is observable and retained.
    let shuffle_seed = random.long();
    random.push(shuffle_seed);
    random.shuffle_list(&mut items);
    random.pop();

    Ok(ShopInventory {
        depth: depth_u8,
        items,
        shuffle_seed,
    })
}

fn searchable(item: ItemId, depth: u8) -> ShopStockItem {
    ShopStockItem::Searchable(WorldItem::from_equipment_roll(
        item,
        EquipmentRoll {
            upgrade: 0,
            effect: None,
            cursed: false,
        },
        depth,
        ItemSource::Shop,
        Accessibility::Independent,
    ))
}

fn random_searchable_tipped_dart(
    random: &mut RandomStack,
    depth: u8,
) -> Result<ShopStockItem, ShopError> {
    let item = random_tipped_dart(random, 2)?;
    let family = item.family();
    let equipment = item
        .searchable_equipment()
        .ok_or(ShopError::ExpectedEquipment(family))?;
    Ok(ShopStockItem::Searchable(WorldItem::from_equipment_roll(
        equipment.item,
        equipment.roll,
        depth,
        ItemSource::Shop,
        Accessibility::Independent,
    )))
}

fn expect_equipment(item: GeneratedItem) -> Result<GeneratedEquipment, ShopError> {
    match item {
        GeneratedItem::Equipment(equipment) => Ok(equipment),
        other => Err(ShopError::ExpectedEquipment(other.family())),
    }
}

fn expect_missile(item: GeneratedItem) -> Result<GeneratedMissile, ShopError> {
    match item {
        GeneratedItem::Missile(missile) => Ok(missile),
        other => Err(ShopError::ExpectedMissile(other.family())),
    }
}

fn generate_rare(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    depth: i32,
    depth_u8: u8,
) -> Result<ShopStockItem, ShopError> {
    let generated = match random.int_bound(10) {
        0 => Some(random_category(
            random,
            generator,
            GeneratorCategory::Wand,
            depth,
        )?),
        1 => Some(random_category(
            random,
            generator,
            GeneratorCategory::Ring,
            depth,
        )?),
        2 => Some(random_category(
            random,
            generator,
            GeneratorCategory::Artifact,
            depth,
        )?),
        _ => None,
    };

    match generated {
        None => Ok(ShopStockItem::Direct(DirectShopItem::Stylus)),
        Some(GeneratedItem::Equipment(equipment)) => Ok(searchable(equipment.item, depth_u8)),
        Some(GeneratedItem::Ring(ring)) => Ok(searchable(ring.kind.item_id(), depth_u8)),
        Some(GeneratedItem::Artifact(artifact)) => Ok(ShopStockItem::Generated(
            GeneratedItem::Artifact(GeneratedArtifact {
                kind: artifact.kind,
                cursed: false,
                spellbook_scrolls: artifact.spellbook_scrolls,
            }),
        )),
        Some(other) => Err(ShopError::UnexpectedRare(other.family())),
    }
}

fn append_hourglass_bags(
    items: &mut Vec<ShopStockItem>,
    depth: i32,
    hourglass: &mut ShopHourglassState,
) {
    let ShopHourglassState::Eligible { sand_bags } = hourglass else {
        return;
    };
    let fraction = match depth {
        6 => 0.20_f32,
        11 => 0.25_f32,
        16 => 0.50_f32,
        20 | 21 => 0.80_f32,
        _ => return,
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let count = ((5 - *sand_bags) as f32 * fraction).ceil() as i32;
    for _ in 0..count.max(0) {
        items.push(ShopStockItem::Direct(DirectShopItem::HourglassSandBag));
        *sand_bags += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DirectShopItem, ShopBagKind, ShopBagOffer, ShopBagSet, ShopHourglassState, ShopRoomCache,
        ShopRunState, ShopStockItem, generate_shop_inventory,
    };
    use crate::catalog::{ItemId, ItemKind, item as catalog_item};
    use crate::generator::{BombKind, GeneratedItem};
    use crate::rng::RandomStack;
    use crate::run::{PotionKind, RunState, ScrollKind};

    #[test]
    fn actual_java_oracle_matches_all_four_stock_streams() {
        let mut generator = RunState::new(0).generator;
        let mut shop_state = ShopRunState::default();

        let fixtures: &[(i32, i64, &[ItemId])] = &[
            (
                6,
                -2_576_934_230_888_777_338_i64,
                &[
                    ItemId::ParalyticDart,
                    ItemId::Spear,
                    ItemId::LeatherArmor,
                    ItemId::WandFrost,
                    ItemId::Shuriken,
                ],
            ),
            (
                11,
                -2_322_033_253_472_854_929_i64,
                &[
                    ItemId::MailArmor,
                    ItemId::DisplacingDart,
                    ItemId::ThrowingSpear,
                    ItemId::Sai,
                ],
            ),
            (
                16,
                -1_642_119_509_135_141_100_i64,
                &[
                    ItemId::Tomahawk,
                    ItemId::Crossbow,
                    ItemId::ScaleArmor,
                    ItemId::IncendiaryDart,
                ],
            ),
            (
                20,
                -5_415_521_948_692_282_354_i64,
                &[
                    ItemId::Greatshield,
                    ItemId::AdrenalineDart,
                    ItemId::WandBlastWave,
                    ItemId::ThrowingHammer,
                    ItemId::PlateArmor,
                ],
            ),
        ];
        for &(depth, next, expected_searchable) in fixtures {
            let mut random = RandomStack::with_base_seed(0);
            random.push(i64::from(depth));
            let inventory =
                generate_shop_inventory(&mut random, &mut generator, depth, &mut shop_state)
                    .unwrap();

            let searchable: Vec<ItemId> =
                inventory.searchable_items().map(|item| item.item).collect();
            assert_eq!(searchable, expected_searchable);
            for item in inventory.searchable_items() {
                assert_eq!(item.upgrade, 0);
                assert_eq!(item.effect, None);
                assert!(!item.cursed);
                assert_eq!(item.source, crate::model::ItemSource::Shop);
            }
            assert_eq!(random.long(), next);
            random.pop();
        }
    }

    #[test]
    fn normalized_java_item_order_fixture_matches_depth_six() {
        let mut generator = RunState::new(0).generator;
        let mut state = ShopRunState::default();
        let mut random = RandomStack::with_base_seed(0);
        random.push(6);
        let inventory =
            generate_shop_inventory(&mut random, &mut generator, 6, &mut state).unwrap();
        let tokens: Vec<&'static str> = inventory.items.iter().map(token).collect();
        assert_eq!(
            tokens,
            [
                "augmentation",
                "healing",
                "tipped-earthroot",
                "bag",
                "spear",
                "ration",
                "leather",
                "remove-curse",
                "magic-mapping",
                "wand-frost",
                "ankh",
                "remove-curse",
                "ration",
                "alchemize-2",
                "shuriken",
                "potion-mind-vision",
                "potion-paralytic-gas",
                "double-bomb",
                "identify",
                "potion-purity",
            ]
        );
    }

    #[test]
    fn bag_tie_is_metadata_only_and_three_shops_exhaust_all_remaining_bags() {
        let mut state = ShopRunState::default();
        let first = state.bags.choose().unwrap();
        assert_eq!(
            first,
            ShopBagOffer::RuntimeHashMapTie {
                candidates: ShopBagSet::one(ShopBagKind::PotionBandolier)
                    .union(ShopBagSet::one(ShopBagKind::MagicalHolster))
            }
        );
        assert!(state.bags.choose().is_some());
        assert_eq!(
            state.bags.choose(),
            Some(ShopBagOffer::Deterministic(ShopBagKind::ScrollHolder))
        );
        assert!(state.bags.choose().is_none());
        assert!(state.bags.possibly_remaining().is_empty());
    }

    #[test]
    fn same_room_cache_consumes_no_second_rng_or_generator_state() {
        let mut generator = RunState::new(0).generator;
        let mut state = ShopRunState::default();
        let mut random = RandomStack::with_base_seed(0);
        random.push(6);
        let mut cache = ShopRoomCache::default();
        let first = cache
            .items_to_spawn(&mut random, &mut generator, 6, &mut state)
            .unwrap()
            .clone();
        let generator_after_first = generator.clone();
        let bag_after_first = state.bags;
        let second = cache
            .items_to_spawn(&mut random, &mut generator, 6, &mut state)
            .unwrap();
        assert_eq!(&first, second);
        assert_eq!(generator, generator_after_first);
        assert_eq!(state.bags, bag_after_first);
        assert_eq!(random.long(), -2_576_934_230_888_777_338);
        random.pop();
    }

    #[test]
    fn hourglass_bag_counts_match_java_ceil_rules_without_rng() {
        let mut generator = RunState::new(0).generator;
        let mut state = ShopRunState {
            hourglass: ShopHourglassState::Eligible { sand_bags: 0 },
            ..ShopRunState::default()
        };
        for (depth, expected_total) in [(6, 1), (11, 1), (16, 2), (20, 1)] {
            let mut random = RandomStack::with_base_seed(0);
            random.push(depth.into());
            let inventory =
                generate_shop_inventory(&mut random, &mut generator, depth, &mut state).unwrap();
            let count = inventory
                .items
                .iter()
                .filter(|item| {
                    matches!(
                        item,
                        ShopStockItem::Direct(DirectShopItem::HourglassSandBag)
                    )
                })
                .count();
            assert_eq!(count, expected_total);
            random.pop();
        }
        assert_eq!(
            state.hourglass,
            ShopHourglassState::Eligible { sand_bags: 5 }
        );
    }

    #[test]
    fn rare_ring_and_artifact_are_cleared_after_full_randomization() {
        let mut saw_ring = false;
        let mut saw_artifact = false;
        for seed in 0..500_i64 {
            let mut generator = RunState::new(0).generator;
            let mut state = ShopRunState::default();
            let mut random = RandomStack::with_base_seed(0);
            random.push(seed);
            let inventory =
                generate_shop_inventory(&mut random, &mut generator, 6, &mut state).unwrap();
            for item in inventory.items {
                match item {
                    ShopStockItem::Searchable(world)
                        if catalog_item(world.item).kind == ItemKind::Ring =>
                    {
                        saw_ring = true;
                        assert_eq!(world.upgrade, 0);
                        assert_eq!(world.effect, None);
                        assert!(!world.cursed);
                    }
                    ShopStockItem::Generated(GeneratedItem::Artifact(artifact)) => {
                        saw_artifact = true;
                        assert!(!artifact.cursed);
                    }
                    _ => {}
                }
            }
            random.pop();
            if saw_ring && saw_artifact {
                break;
            }
        }
        assert!(saw_ring && saw_artifact);
    }

    fn token(item: &ShopStockItem) -> &'static str {
        match item {
            ShopStockItem::Searchable(world) => match world.item {
                ItemId::Spear => "spear",
                ItemId::LeatherArmor => "leather",
                ItemId::WandFrost => "wand-frost",
                ItemId::Shuriken => "shuriken",
                ItemId::ParalyticDart => "tipped-earthroot",
                _ => "other-searchable",
            },
            ShopStockItem::Generated(GeneratedItem::Potion {
                kind: PotionKind::MindVision,
                exotic: false,
            }) => "potion-mind-vision",
            ShopStockItem::Generated(GeneratedItem::Potion {
                kind: PotionKind::ParalyticGas,
                exotic: false,
            }) => "potion-paralytic-gas",
            ShopStockItem::Generated(GeneratedItem::Potion {
                kind: PotionKind::Purity,
                exotic: false,
            }) => "potion-purity",
            ShopStockItem::Generated(GeneratedItem::Scroll {
                kind: ScrollKind::RemoveCurse,
                exotic: false,
            })
            | ShopStockItem::Direct(DirectShopItem::ScrollOfRemoveCurse) => "remove-curse",
            ShopStockItem::Generated(GeneratedItem::Bomb(BombKind::DoubleBomb)) => "double-bomb",
            ShopStockItem::Direct(DirectShopItem::StoneOfAugmentation) => "augmentation",
            ShopStockItem::Direct(DirectShopItem::PotionOfHealing) => "healing",
            ShopStockItem::Direct(DirectShopItem::Bag(_)) => "bag",
            ShopStockItem::Direct(DirectShopItem::SmallRation) => "ration",
            ShopStockItem::Direct(DirectShopItem::ScrollOfMagicMapping) => "magic-mapping",
            ShopStockItem::Direct(DirectShopItem::Ankh) => "ankh",
            ShopStockItem::Direct(DirectShopItem::Alchemize { quantity: 2 }) => "alchemize-2",
            ShopStockItem::Direct(DirectShopItem::ScrollOfIdentify) => "identify",
            _ => "other",
        }
    }
}
