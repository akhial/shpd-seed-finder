//! Exact v4.0.0 `VaultLevel` loot tables: the four equipment tiers, the
//! consumable lists, the solve-item lookups, and the `itemsToSpawn` queue.
//!
//! `setupEquipmentAtTier` rolls six items per tier straight from the outer
//! depth stream: two melee weapons, a thrown weapon, a fixed armor class, a
//! wand, and a ring. Class identity comes from `Generator.randomUsingDefaults`
//! (default weights, no deck mutation) and then `Item.random()`; the vault
//! then overrides the level and re-rolls or clears the enchantment/glyph
//! with `Random.Int(3) >= lootTier`. `createEquipment` later hands the items
//! out in index order, clears their curse flag, and identifies them.
//!
//! Consumable lists are built from seeded `Random.oneOf` picks but then
//! shuffled with the unseeded `Collections.shuffle(List)`. Only the fixed
//! Potion of Healing that tiers 0–2 insert at index zero is reproducible;
//! every other draw from those lists is recorded as an opaque slot.

// Ports of upstream methods keep their unchecked Java invariants, and
// `setupEquipmentAtTier` is deliberately one long method mirroring upstream.
#![allow(clippy::missing_panics_doc, clippy::too_many_lines)]

use crate::catalog::{ArmorEffect, Effect, ItemId, WeaponEffect};
use crate::equipment::{EquipmentRoll, roll_wand};
use crate::generator::{
    FoodKind, MISSILE_TIER_2_ITEMS, MISSILE_TIER_3_ITEMS, MISSILE_TIER_4_ITEMS,
    MISSILE_TIER_5_ITEMS, RING_ITEMS, SeedKind, StoneKind, WAND_ITEMS, WEAPON_TIER_2_ITEMS,
    WEAPON_TIER_3_ITEMS, WEAPON_TIER_4_ITEMS, WEAPON_TIER_5_ITEMS,
};
use crate::rng::RandomStack;
use crate::run::{GeneratorCategory, PotionKind, ScrollKind};

/// Equipment family of a vault loot item, as the Java class hierarchy sees
/// it: wands are not `EquipableItem`s, everything else here is.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum VaultEquipmentKind {
    MeleeWeapon,
    ThrownWeapon,
    Armor,
    Wand,
    Ring,
}

/// One rolled vault equipment item after `createEquipment` cleaned it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct VaultEquipment {
    pub kind: VaultEquipmentKind,
    pub item: ItemId,
    pub upgrade: u8,
    pub effect: Option<Effect>,
    /// `Item.quantity()`: thrown weapons come in stacks of three.
    pub quantity: i32,
}

impl VaultEquipment {
    /// Java `EquipableItem` membership (`findPrizeItem(EquipableItem.class)`).
    #[must_use]
    pub const fn is_equipable(self) -> bool {
        !matches!(self.kind, VaultEquipmentKind::Wand)
    }
}

/// A consumable whose identity is fixed by the depth stream.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum VaultConsumable {
    Potion(PotionKind),
    Seed(SeedKind),
    Scroll(ScrollKind),
    Stone(StoneKind),
}

/// Every item that can appear in a vault heap or in `itemsToSpawn`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum VaultItem {
    Equipment(VaultEquipment),
    /// The plain `Dart` stack of two queued by `VaultLevel.build()`.
    Dart,
    Consumable(VaultConsumable),
    /// A draw from one of the unseeded-shuffled consumable lists. `slot` is
    /// the position within the shuffled part of the list and `refill` counts
    /// how many times that list had been rebuilt, so draws stay distinct.
    ShuffledConsumable {
        tier: u8,
        refill: u8,
        slot: u8,
    },
    Food(FoodKind),
    DwarfToken,
    VaultBeacon,
    Torch,
    ImpStatue,
    /// `Imp.Quest.rewardOptions[index]`, emitted on the City floor.
    ImpRewardOption(u8),
}

impl VaultItem {
    #[must_use]
    pub const fn equipment(self) -> Option<VaultEquipment> {
        match self {
            Self::Equipment(equipment) => Some(equipment),
            _ => None,
        }
    }

    /// `EquipableItem.class.isInstance(item)`.
    #[must_use]
    pub const fn is_equipable(self) -> bool {
        match self {
            Self::Equipment(equipment) => equipment.is_equipable(),
            Self::Dart => true,
            _ => false,
        }
    }
}

// Private copies of the v4.0.0 rarity tables so this module can implement
// `Weapon.random()` and `Enchantment.random(toIgnore)` against the 4.0.0
// class lists without touching `equipment.rs`, whose tables are still the
// v3.3.8 ones used by the main dungeon ports.
const WEAPON_COMMON: [WeaponEffect; 5] = [
    WeaponEffect::Blazing,
    WeaponEffect::Chilling,
    WeaponEffect::Kinetic,
    WeaponEffect::Shocking,
    WeaponEffect::Venomous,
];
const WEAPON_UNCOMMON: [WeaponEffect; 8] = [
    WeaponEffect::Blocking,
    WeaponEffect::Blooming,
    WeaponEffect::Eldritch,
    WeaponEffect::Elastic,
    WeaponEffect::Lucky,
    WeaponEffect::Projecting,
    WeaponEffect::Unstable,
    WeaponEffect::Vorpal,
];
const WEAPON_RARE: [WeaponEffect; 4] = [
    WeaponEffect::Corrupting,
    WeaponEffect::Crystal,
    WeaponEffect::Grim,
    WeaponEffect::Vampiric,
];
const WEAPON_CURSES: [WeaponEffect; 10] = [
    WeaponEffect::Annoying,
    WeaponEffect::Displacing,
    WeaponEffect::Dazzling,
    WeaponEffect::Explosive,
    WeaponEffect::Friendly,
    WeaponEffect::Polarized,
    WeaponEffect::Pressurized,
    WeaponEffect::Sacrificial,
    WeaponEffect::Wayward,
    WeaponEffect::Wondrous,
];

/// v4.0.0 `Weapon.random()` / `MissileWeapon.random()` for the no-trinket
/// profile: the upgrade rolls stay on the outer stream, then a child
/// generator seeded with `Random.Long()` decides between a curse (30%), an
/// enchantment (10%, drawn with the four-entry rare list), or nothing.
fn roll_weapon_v4(random: &mut RandomStack) -> EquipmentRoll {
    let mut upgrade = 0;
    if random.int_bound(4) == 0 {
        upgrade += 1;
        if random.int_bound(5) == 0 {
            upgrade += 1;
        }
    }
    let child_seed = random.long();
    random.push(child_seed);
    let effect_roll = random.float();
    let result = if effect_roll < 0.3 {
        EquipmentRoll {
            upgrade,
            effect: Some(Effect::Weapon(select_ignoring(
                random,
                &WEAPON_CURSES,
                None,
            ))),
            cursed: true,
        }
    } else if effect_roll >= 0.9 {
        EquipmentRoll {
            upgrade,
            effect: Some(Effect::Weapon(random_weapon_enchantment_ignoring(
                random, None,
            ))),
            cursed: false,
        }
    } else {
        EquipmentRoll {
            upgrade,
            effect: None,
            cursed: false,
        }
    };
    random.pop();
    result
}
const ARMOR_COMMON: [ArmorEffect; 4] = [
    ArmorEffect::Obfuscation,
    ArmorEffect::Swiftness,
    ArmorEffect::Viscosity,
    ArmorEffect::Potential,
];
const ARMOR_UNCOMMON: [ArmorEffect; 6] = [
    ArmorEffect::Brimstone,
    ArmorEffect::Stone,
    ArmorEffect::Entanglement,
    ArmorEffect::Repulsion,
    ArmorEffect::Camouflage,
    ArmorEffect::Flow,
];
const ARMOR_RARE: [ArmorEffect; 3] = [
    ArmorEffect::Affection,
    ArmorEffect::AntiMagic,
    ArmorEffect::Thorns,
];
const RARITY_CHANCES: [f32; 3] = [50.0, 40.0, 10.0];

/// `Weapon.Enchantment.random(toIgnore)`: the current class is removed from
/// its rarity list before `Random.element`. Curses never appear in those
/// lists, so ignoring a curse changes nothing. An emptied list would recurse
/// into the no-argument overload, which cannot happen with a single ignore.
fn random_weapon_enchantment_ignoring(
    random: &mut RandomStack,
    ignore: Option<WeaponEffect>,
) -> WeaponEffect {
    let rarity = random.chances(&RARITY_CHANCES).unwrap_or_default();
    let table: &[WeaponEffect] = match rarity {
        1 => &WEAPON_UNCOMMON,
        2 => &WEAPON_RARE,
        _ => &WEAPON_COMMON,
    };
    select_ignoring(random, table, ignore)
}

/// `Armor.Glyph.random(toIgnore)`.
fn random_armor_glyph_ignoring(
    random: &mut RandomStack,
    ignore: Option<ArmorEffect>,
) -> ArmorEffect {
    let rarity = random.chances(&RARITY_CHANCES).unwrap_or_default();
    let table: &[ArmorEffect] = match rarity {
        1 => &ARMOR_UNCOMMON,
        2 => &ARMOR_RARE,
        _ => &ARMOR_COMMON,
    };
    select_ignoring(random, table, ignore)
}

fn select_ignoring<T: Copy + PartialEq>(
    random: &mut RandomStack,
    table: &[T],
    ignore: Option<T>,
) -> T {
    let candidates: Vec<T> = table
        .iter()
        .copied()
        .filter(|candidate| Some(*candidate) != ignore)
        .collect();
    let bound = i32::try_from(candidates.len()).expect("rarity table fits Java int");
    candidates[usize::try_from(random.int_bound(bound)).expect("Random.Int is non-negative")]
}

/// `Generator.randomUsingDefaults(cat)` for a fixed-table category:
/// `Random.chances(defaultProbs)` selects the class, then `Item.random()`
/// rolls it on the same outer stream.
fn random_using_defaults(
    random: &mut RandomStack,
    category: GeneratorCategory,
) -> (ItemId, EquipmentRoll) {
    let probabilities = category
        .primary_probabilities()
        .expect("vault categories have default tables");
    let index = random
        .chances(probabilities)
        .expect("vault category tables have positive weight");
    let item = match category {
        GeneratorCategory::WeaponTier2 => WEAPON_TIER_2_ITEMS[index],
        GeneratorCategory::WeaponTier3 => WEAPON_TIER_3_ITEMS[index],
        GeneratorCategory::WeaponTier4 => WEAPON_TIER_4_ITEMS[index],
        GeneratorCategory::WeaponTier5 => WEAPON_TIER_5_ITEMS[index],
        GeneratorCategory::MissileTier2 => MISSILE_TIER_2_ITEMS[index].item_id(),
        GeneratorCategory::MissileTier3 => MISSILE_TIER_3_ITEMS[index].item_id(),
        GeneratorCategory::MissileTier4 => MISSILE_TIER_4_ITEMS[index].item_id(),
        GeneratorCategory::MissileTier5 => MISSILE_TIER_5_ITEMS[index].item_id(),
        GeneratorCategory::Wand => Some(WAND_ITEMS[index]),
        GeneratorCategory::Ring => Some(RING_ITEMS[index].item_id()),
        _ => unreachable!("vault equipment uses fixed-table categories only"),
    }
    .expect("zero-weight classes are never selected");
    let roll = match category {
        GeneratorCategory::Wand | GeneratorCategory::Ring => roll_wand(random),
        _ => roll_weapon_v4(random),
    };
    (item, roll)
}

/// `Generator.randomUsingDefaults(Category.FOOD)`: `Random.chances({4,1,0})`
/// then `Food.random()`, which draws nothing.
pub fn random_food_using_defaults(random: &mut RandomStack) -> FoodKind {
    let probabilities = GeneratorCategory::Food
        .primary_probabilities()
        .expect("food has a default table");
    match random
        .chances(probabilities)
        .expect("food table has positive weight")
    {
        0 => FoodKind::Ration,
        1 => FoodKind::Pasty,
        _ => FoodKind::MysteryMeat,
    }
}

const FIRST_WEAPON_CATEGORIES: [GeneratorCategory; 4] = [
    GeneratorCategory::WeaponTier2,
    GeneratorCategory::WeaponTier2,
    GeneratorCategory::WeaponTier3,
    GeneratorCategory::WeaponTier4,
];
const SECOND_WEAPON_CATEGORIES: [GeneratorCategory; 4] = [
    GeneratorCategory::WeaponTier2,
    GeneratorCategory::WeaponTier3,
    GeneratorCategory::WeaponTier4,
    GeneratorCategory::WeaponTier5,
];
const MISSILE_CATEGORIES: [GeneratorCategory; 4] = [
    GeneratorCategory::MissileTier2,
    GeneratorCategory::MissileTier3,
    GeneratorCategory::MissileTier4,
    GeneratorCategory::MissileTier5,
];
const ARMOR_CLASSES: [ItemId; 4] = [
    ItemId::LeatherArmor,
    ItemId::MailArmor,
    ItemId::ScaleArmor,
    ItemId::PlateArmor,
];

/// `VaultLevel.equipmentLoot`, `lowerTierIdx`, `higherTierIdx`, and the
/// `generatedClasses` duplicate-rejection set.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VaultEquipmentLoot {
    tiers: [Option<[Option<VaultEquipment>; 6]>; 4],
    lower_tier_index: usize,
    higher_tier_index: usize,
    generated_classes: Vec<ItemId>,
}

impl VaultEquipmentLoot {
    /// Items still held by one tier, in slot order.
    #[must_use]
    pub fn tier(&self, tier: usize) -> Option<&[Option<VaultEquipment>; 6]> {
        self.tiers[tier].as_ref()
    }

    /// `VaultLevel.setupEquipment()`: (re)generate every tier that is unset
    /// or fully handed out, in tier order.
    pub fn setup_equipment(&mut self, random: &mut RandomStack) {
        for tier in 0..4 {
            let empty = self.tiers[tier]
                .as_ref()
                .is_none_or(|items| items.iter().all(Option::is_none));
            if empty {
                self.setup_tier(tier, random);
            }
        }
    }

    /// `VaultLevel.setupEquipmentAtTier(lootTier)`.
    #[allow(clippy::cast_possible_truncation)] // tier is 0..=3.
    pub fn setup_tier(&mut self, tier: usize, random: &mut RandomStack) {
        let tier_i32 = i32::try_from(tier).expect("tier is 0..=3");
        let tier_u8 = tier as u8;

        let (item, roll) = loop {
            let candidate = random_using_defaults(random, FIRST_WEAPON_CATEGORIES[tier]);
            if !(tier > 1 && self.generated_classes.contains(&candidate.0)) {
                break candidate;
            }
        };
        self.generated_classes.push(item);
        let upgrade = if tier == 0 { 0 } else { tier_u8 + 1 };
        let effect = Self::weapon_effect(roll, tier_i32, random);
        let first_weapon = VaultEquipment {
            kind: VaultEquipmentKind::MeleeWeapon,
            item,
            upgrade,
            effect,
            quantity: 1,
        };

        let (item, roll) = loop {
            let candidate = random_using_defaults(random, SECOND_WEAPON_CATEGORIES[tier]);
            if !(tier > 0 && self.generated_classes.contains(&candidate.0)) {
                break candidate;
            }
        };
        self.generated_classes.push(item);
        let effect = Self::weapon_effect(roll, tier_i32, random);
        let second_weapon = VaultEquipment {
            kind: VaultEquipmentKind::MeleeWeapon,
            item,
            upgrade: tier_u8,
            effect,
            quantity: 1,
        };

        let (item, roll) = loop {
            let candidate = random_using_defaults(random, MISSILE_CATEGORIES[tier]);
            if !self.generated_classes.contains(&candidate.0) {
                break candidate;
            }
        };
        self.generated_classes.push(item);
        let effect = Self::weapon_effect(roll, tier_i32, random);
        let missile = VaultEquipment {
            kind: VaultEquipmentKind::ThrownWeapon,
            item,
            upgrade: tier_u8,
            effect,
            quantity: 3,
        };

        let armor_item = ARMOR_CLASSES[tier];
        self.generated_classes.push(armor_item);
        // A freshly constructed armor carries no glyph, so `inscribe()`
        // ignores nothing.
        let glyph = if random.int_bound(3) >= tier_i32 {
            None
        } else {
            Some(Effect::Armor(random_armor_glyph_ignoring(random, None)))
        };
        let armor = VaultEquipment {
            kind: VaultEquipmentKind::Armor,
            item: armor_item,
            upgrade: tier_u8,
            effect: glyph,
            quantity: 1,
        };

        let (item, _roll) = loop {
            let candidate = random_using_defaults(random, GeneratorCategory::Wand);
            if !(self.generated_classes.contains(&candidate.0)
                || matches!(
                    candidate.0,
                    ItemId::WandRegrowth | ItemId::WandTransfusion | ItemId::WandCorruption
                ))
            {
                break candidate;
            }
        };
        self.generated_classes.push(item);
        let wand = VaultEquipment {
            kind: VaultEquipmentKind::Wand,
            item,
            upgrade: tier_u8,
            effect: None,
            quantity: 1,
        };

        let (item, _roll) = loop {
            let candidate = random_using_defaults(random, GeneratorCategory::Ring);
            if !(self.generated_classes.contains(&candidate.0)
                || matches!(
                    candidate.0,
                    ItemId::RingWealth | ItemId::RingMight | ItemId::RingForce
                ))
            {
                break candidate;
            }
        };
        self.generated_classes.push(item);
        let ring = VaultEquipment {
            kind: VaultEquipmentKind::Ring,
            item,
            upgrade: tier_u8,
            effect: None,
            quantity: 1,
        };

        self.tiers[tier] = Some([
            Some(first_weapon),
            Some(second_weapon),
            Some(missile),
            Some(armor),
            Some(wand),
            Some(ring),
        ]);
    }

    /// `Random.Int(3) >= lootTier ? enchant(null) : enchant()`, where
    /// `enchant()` ignores whatever `Weapon.random()` already attached.
    fn weapon_effect(roll: EquipmentRoll, tier: i32, random: &mut RandomStack) -> Option<Effect> {
        if random.int_bound(3) >= tier {
            None
        } else {
            let current = match roll.effect {
                Some(Effect::Weapon(effect)) => Some(effect),
                _ => None,
            };
            Some(Effect::Weapon(random_weapon_enchantment_ignoring(
                random, current,
            )))
        }
    }

    /// `VaultLevel.createEquipment(lootTier)`.
    pub fn create_equipment(&mut self, tier: usize, random: &mut RandomStack) -> VaultEquipment {
        self.setup_equipment(random);
        let cursor = if tier >= 2 {
            &mut self.higher_tier_index
        } else {
            &mut self.lower_tier_index
        };
        let mut index = *cursor;
        if index >= 6 {
            index = 0;
            *cursor = 0;
        } else {
            *cursor += 1;
        }
        let items = self.tiers[tier]
            .as_mut()
            .expect("setupEquipment fills every tier");
        while items[index].is_none() {
            index += 1;
            if index >= 6 {
                index = 0;
            }
        }
        items[index]
            .take()
            .expect("the null-skipping loop stops on a live slot")
    }
}

/// One `consumableLoot` list: the reproducible healing potion (for tiers
/// 0–2) followed by the unseeded-shuffled remainder.
#[derive(Clone, Debug, Eq, PartialEq)]
struct ConsumableList {
    remaining: Vec<VaultItem>,
    refills: u8,
}

/// `VaultLevel.consumableLoot`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VaultConsumables {
    lists: Option<[ConsumableList; 4]>,
    /// Seeded `Random.oneOf` picks made while building each list, in Java
    /// order, for diagnostics; their placement inside the shuffled list is
    /// not reproducible.
    pub rolled: Vec<(u8, VaultConsumable)>,
}

impl VaultConsumables {
    /// `VaultLevel.setupConsumables()`: every empty list is rebuilt.
    fn setup(&mut self, random: &mut RandomStack) {
        let lists = self.lists.get_or_insert_with(|| {
            std::array::from_fn(|_| ConsumableList {
                remaining: Vec::new(),
                refills: 0,
            })
        });
        for tier in 0..4_u8 {
            let list = &mut lists[usize::from(tier)];
            if !list.remaining.is_empty() {
                continue;
            }
            let picks = roll_consumable_tier(tier, random);
            self.rolled
                .extend(picks.iter().map(|consumable| (tier, *consumable)));
            let refill = list.refills;
            list.refills = list.refills.wrapping_add(1);
            let shuffled_count: u8 = if tier == 3 { 5 } else { 4 };
            if tier < 3 {
                list.remaining
                    .push(VaultItem::Consumable(VaultConsumable::Potion(
                        PotionKind::Healing,
                    )));
            }
            list.remaining.extend(
                (0..shuffled_count).map(|slot| VaultItem::ShuffledConsumable {
                    tier,
                    refill,
                    slot,
                }),
            );
        }
    }

    /// `VaultLevel.createConsumabe(tier)`.
    pub fn create_consumable(&mut self, tier: usize, random: &mut RandomStack) -> VaultItem {
        if self
            .lists
            .as_ref()
            .is_none_or(|lists| lists[tier].remaining.is_empty())
        {
            self.setup(random);
        }
        let lists = self.lists.as_mut().expect("setup created the lists");
        lists[tier].remaining.remove(0)
    }
}

fn one_of<T: Copy>(choices: &[T], random: &mut RandomStack) -> T {
    let bound = i32::try_from(choices.len()).expect("choice table fits Java int");
    choices[usize::try_from(random.int_bound(bound)).expect("Random.Int is non-negative")]
}

/// The four seeded `Random.oneOf` picks for one consumable tier.
fn roll_consumable_tier(tier: u8, random: &mut RandomStack) -> [VaultConsumable; 4] {
    match tier {
        0 => [
            VaultConsumable::Potion(one_of(&[PotionKind::Frost, PotionKind::Levitation], random)),
            VaultConsumable::Seed(one_of(
                &[SeedKind::Mageroyal, SeedKind::Icecap, SeedKind::Stormvine],
                random,
            )),
            VaultConsumable::Scroll(one_of(
                &[ScrollKind::MirrorImage, ScrollKind::Teleportation],
                random,
            )),
            VaultConsumable::Stone(one_of(
                &[StoneKind::Flock, StoneKind::Shock, StoneKind::Fear],
                random,
            )),
        ],
        1 => [
            VaultConsumable::Potion(one_of(
                &[PotionKind::ToxicGas, PotionKind::ParalyticGas],
                random,
            )),
            VaultConsumable::Seed(one_of(
                &[
                    SeedKind::Firebloom,
                    SeedKind::Sorrowmoss,
                    SeedKind::Blindweed,
                ],
                random,
            )),
            VaultConsumable::Scroll(one_of(
                &[ScrollKind::Recharging, ScrollKind::Terror],
                random,
            )),
            VaultConsumable::Stone(one_of(
                &[
                    StoneKind::DeepSleep,
                    StoneKind::Clairvoyance,
                    StoneKind::Aggression,
                ],
                random,
            )),
        ],
        2 => [
            VaultConsumable::Potion(one_of(
                &[PotionKind::MindVision, PotionKind::LiquidFlame],
                random,
            )),
            VaultConsumable::Seed(one_of(
                &[SeedKind::Swiftthistle, SeedKind::Sungrass],
                random,
            )),
            VaultConsumable::Scroll(one_of(
                &[ScrollKind::Lullaby, ScrollKind::MagicMapping],
                random,
            )),
            VaultConsumable::Stone(one_of(&[StoneKind::Blast, StoneKind::Blink], random)),
        ],
        _ => [
            VaultConsumable::Potion(one_of(
                &[PotionKind::Experience, PotionKind::Invisibility],
                random,
            )),
            VaultConsumable::Seed(one_of(&[SeedKind::Earthroot, SeedKind::Starflower], random)),
            VaultConsumable::Scroll(one_of(
                &[ScrollKind::Retribution, ScrollKind::Transmutation],
                random,
            )),
            VaultConsumable::Stone(one_of(
                &[StoneKind::Enchantment, StoneKind::Augmentation],
                random,
            )),
        ],
    }
}

/// `Level.itemsToSpawn` for the vault, with the `findPrizeItem` lookups.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VaultSpawnQueue {
    pub items: Vec<VaultItem>,
}

/// A class filter for `findPrizeItem(Class)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrizeFilter {
    Equipable,
    StoneOfBlink,
    PotionOfInvisibility,
}

impl PrizeFilter {
    const fn matches(self, item: VaultItem) -> bool {
        match self {
            Self::Equipable => item.is_equipable(),
            Self::StoneOfBlink => matches!(
                item,
                VaultItem::Consumable(VaultConsumable::Stone(StoneKind::Blink))
            ),
            Self::PotionOfInvisibility => matches!(
                item,
                VaultItem::Consumable(VaultConsumable::Potion(PotionKind::Invisibility))
            ),
        }
    }
}

impl VaultSpawnQueue {
    /// `Level.addItemToSpawn`.
    pub fn add(&mut self, item: VaultItem) {
        self.items.push(item);
    }

    /// `Level.findPrizeItem(Class match)`: the first queued instance.
    pub fn find_prize_item(&mut self, filter: PrizeFilter) -> Option<VaultItem> {
        let index = self.items.iter().position(|item| filter.matches(*item))?;
        Some(self.items.remove(index))
    }

    /// `Level.findPrizeItem()`: a Trinket Catalyst first (never queued in
    /// the vault), otherwise `Random.element(itemsToSpawn)`.
    pub fn find_random_prize_item(&mut self, random: &mut RandomStack) -> Option<VaultItem> {
        if self.items.is_empty() {
            return None;
        }
        let bound = i32::try_from(self.items.len()).expect("queue fits Java int");
        let index = usize::try_from(random.int_bound(bound)).expect("Random.Int is non-negative");
        Some(self.items.remove(index))
    }

    /// `VaultLevel.findT3SolveItem()`.
    pub fn find_t3_solve_item(&mut self, random: &mut RandomStack) -> Option<VaultItem> {
        let mut order = [PrizeFilter::StoneOfBlink, PrizeFilter::PotionOfInvisibility];
        random.shuffle_array(&mut order);
        order
            .into_iter()
            .find_map(|filter| self.find_prize_item(filter))
    }

    /// `VaultLevel.findT2SolveItem()`: the same two classes, then the tier
    /// three lookup as a fallback.
    pub fn find_t2_solve_item(&mut self, random: &mut RandomStack) -> Option<VaultItem> {
        let mut order = [PrizeFilter::StoneOfBlink, PrizeFilter::PotionOfInvisibility];
        random.shuffle_array(&mut order);
        if let Some(item) = order
            .into_iter()
            .find_map(|filter| self.find_prize_item(filter))
        {
            return Some(item);
        }
        self.find_t3_solve_item(random)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_equipment_cycles_shared_indices_and_skips_taken_slots() {
        let mut random = RandomStack::with_base_seed(0);
        random.push(0x5eed);
        let mut loot = VaultEquipmentLoot::default();
        let tier0: Vec<_> = (0..4)
            .map(|_| loot.create_equipment(0, &mut random))
            .collect();
        assert_eq!(
            tier0.iter().map(|item| item.kind).collect::<Vec<_>>(),
            [
                VaultEquipmentKind::MeleeWeapon,
                VaultEquipmentKind::MeleeWeapon,
                VaultEquipmentKind::ThrownWeapon,
                VaultEquipmentKind::Armor
            ]
        );
        assert!(
            tier0
                .iter()
                .all(|item| item.upgrade == 0 && item.effect.is_none())
        );
        // The lower cursor is shared with tier one: the next tier-one item is
        // slot four (the wand), then the ring, then a wrap back to slot zero.
        let tier1: Vec<_> = (0..4)
            .map(|_| loot.create_equipment(1, &mut random))
            .collect();
        assert_eq!(
            tier1.iter().map(|item| item.kind).collect::<Vec<_>>(),
            [
                VaultEquipmentKind::Wand,
                VaultEquipmentKind::Ring,
                VaultEquipmentKind::MeleeWeapon,
                VaultEquipmentKind::MeleeWeapon
            ]
        );
        assert_eq!(tier1[0].upgrade, 1);
        assert_eq!(tier1[2].upgrade, 2);
        assert_eq!(tier1[3].upgrade, 1);
        // Tier zero still holds its wand and ring untouched.
        let remaining = loot.tier(0).unwrap();
        assert!(remaining[..4].iter().all(Option::is_none));
        assert_eq!(remaining[4].unwrap().kind, VaultEquipmentKind::Wand);
        assert_eq!(remaining[5].unwrap().kind, VaultEquipmentKind::Ring);
    }

    /// Captured from the official v4.0.0-BETA-3 JAR: `new VaultLevel()`,
    /// `Random.pushGenerator(1234567)`, `setupEquipment()`, then the listed
    /// `createEquipment` hand-outs, with `Random.Int()` sampled after each
    /// phase.
    #[test]
    fn setup_equipment_and_hand_outs_match_official_oracle() {
        use crate::catalog::{ArmorEffect, WeaponEffect};
        let mut random = RandomStack::with_base_seed(0);
        random.push(1_234_567);
        let mut loot = VaultEquipmentLoot::default();
        loot.setup_equipment(&mut random);
        assert_eq!(random.int(), -167_166_909);

        let summary = |tier: usize| -> Vec<(ItemId, u8, Option<Effect>)> {
            loot.tier(tier)
                .unwrap()
                .iter()
                .map(|item| {
                    let item = item.unwrap();
                    (item.item, item.upgrade, item.effect)
                })
                .collect()
        };
        assert_eq!(
            summary(0),
            [
                (ItemId::Spear, 0, None),
                (ItemId::Sickle, 0, None),
                (ItemId::Shuriken, 0, None),
                (ItemId::LeatherArmor, 0, None),
                (ItemId::WandLightning, 0, None),
                (ItemId::RingElements, 0, None),
            ]
        );
        assert_eq!(
            summary(1),
            [
                (ItemId::Shortsword, 2, None),
                (ItemId::Sai, 1, None),
                (ItemId::Kunai, 1, None),
                (ItemId::MailArmor, 1, None),
                (ItemId::WandCorrosion, 1, None),
                (ItemId::RingAccuracy, 1, None),
            ]
        );
        assert_eq!(
            summary(2),
            [
                (
                    ItemId::Whip,
                    3,
                    Some(Effect::Weapon(WeaponEffect::Shocking))
                ),
                (ItemId::Longsword, 2, None),
                (
                    ItemId::Tomahawk,
                    2,
                    Some(Effect::Weapon(WeaponEffect::Unstable))
                ),
                (
                    ItemId::ScaleArmor,
                    2,
                    Some(Effect::Armor(ArmorEffect::Repulsion))
                ),
                (ItemId::WandDisintegration, 2, None),
                (ItemId::RingEnergy, 2, None),
            ]
        );
        assert_eq!(
            summary(3),
            [
                (
                    ItemId::Katana,
                    4,
                    Some(Effect::Weapon(WeaponEffect::Vorpal))
                ),
                (
                    ItemId::StoneGauntlet,
                    3,
                    Some(Effect::Weapon(WeaponEffect::Vorpal))
                ),
                (
                    ItemId::Trident,
                    3,
                    Some(Effect::Weapon(WeaponEffect::Kinetic))
                ),
                (
                    ItemId::PlateArmor,
                    3,
                    Some(Effect::Armor(ArmorEffect::Brimstone))
                ),
                (ItemId::WandFireblast, 3, None),
                (ItemId::RingHaste, 3, None),
            ]
        );
        assert_eq!(loot.tier(2).unwrap()[2].unwrap().quantity, 3);

        let sequence = [0, 0, 0, 0, 1, 1, 1, 3, 2, 3, 3, 2, 2, 3, 1, 0, 3];
        let handed: Vec<ItemId> = sequence
            .into_iter()
            .map(|tier| loot.create_equipment(tier, &mut random).item)
            .collect();
        assert_eq!(
            handed,
            [
                ItemId::Spear,
                ItemId::Sickle,
                ItemId::Shuriken,
                ItemId::LeatherArmor,
                ItemId::WandCorrosion,
                ItemId::RingAccuracy,
                ItemId::Shortsword,
                ItemId::Katana,
                ItemId::Longsword,
                ItemId::Trident,
                ItemId::PlateArmor,
                ItemId::WandDisintegration,
                ItemId::RingEnergy,
                ItemId::StoneGauntlet,
                ItemId::Sai,
                ItemId::WandLightning,
                ItemId::WandFireblast,
            ]
        );
        assert_eq!(random.int(), 1_219_675_430);

        // Second official vector: only the tier contents are pinned.
        let mut random = RandomStack::with_base_seed(0);
        random.push(987_654_321);
        let mut loot = VaultEquipmentLoot::default();
        loot.setup_equipment(&mut random);
        assert_eq!(random.int(), -325_163_783);
        let tier3: Vec<(ItemId, u8, Option<Effect>)> = loot
            .tier(3)
            .unwrap()
            .iter()
            .map(|item| {
                let item = item.unwrap();
                (item.item, item.upgrade, item.effect)
            })
            .collect();
        assert_eq!(
            tier3,
            [
                (
                    ItemId::Crossbow,
                    4,
                    Some(Effect::Weapon(WeaponEffect::Chilling))
                ),
                (
                    ItemId::WarHammer,
                    3,
                    Some(Effect::Weapon(WeaponEffect::Eldritch))
                ),
                (
                    ItemId::ThrowingHammer,
                    3,
                    Some(Effect::Weapon(WeaponEffect::Chilling))
                ),
                (
                    ItemId::PlateArmor,
                    3,
                    Some(Effect::Armor(ArmorEffect::Obfuscation))
                ),
                (ItemId::WandCorrosion, 3, None),
                (ItemId::RingEnergy, 3, None),
            ]
        );
        let tier1: Vec<(ItemId, u8, Option<Effect>)> = loot
            .tier(1)
            .unwrap()
            .iter()
            .map(|item| {
                let item = item.unwrap();
                (item.item, item.upgrade, item.effect)
            })
            .collect();
        assert_eq!(
            tier1,
            [
                (ItemId::Sickle, 2, None),
                (ItemId::Sword, 1, None),
                (
                    ItemId::Bolas,
                    1,
                    Some(Effect::Weapon(WeaponEffect::Blazing))
                ),
                (ItemId::MailArmor, 1, None),
                (ItemId::WandDisintegration, 1, None),
                (ItemId::RingEvasion, 1, None),
            ]
        );
    }

    #[test]
    fn tier_three_items_are_always_enchanted_and_tier_zero_never() {
        for seed in 1..40_i64 {
            let mut random = RandomStack::with_base_seed(0);
            random.push(seed);
            let mut loot = VaultEquipmentLoot::default();
            loot.setup_equipment(&mut random);
            let tier0 = loot.tier(0).unwrap();
            assert!(tier0[..4].iter().all(|item| item.unwrap().effect.is_none()));
            let tier3 = loot.tier(3).unwrap();
            assert!(tier3[..4].iter().all(|item| item.unwrap().effect.is_some()));
            assert_eq!(tier3[0].unwrap().upgrade, 4);
            assert_eq!(tier3[1].unwrap().upgrade, 3);
            assert_eq!(tier3[3].unwrap().item, ItemId::PlateArmor);
            let wand = tier3[4].unwrap().item;
            assert!(!matches!(
                wand,
                ItemId::WandRegrowth | ItemId::WandTransfusion | ItemId::WandCorruption
            ));
            let ring = tier3[5].unwrap().item;
            assert!(!matches!(
                ring,
                ItemId::RingWealth | ItemId::RingMight | ItemId::RingForce
            ));
        }
    }

    #[test]
    fn consumable_lists_keep_the_fixed_healing_potion_first() {
        let mut random = RandomStack::with_base_seed(0);
        random.push(3);
        let mut consumables = VaultConsumables::default();
        let first = consumables.create_consumable(0, &mut random);
        assert_eq!(
            first,
            VaultItem::Consumable(VaultConsumable::Potion(PotionKind::Healing))
        );
        assert_eq!(consumables.rolled.len(), 16);
        let second = consumables.create_consumable(0, &mut random);
        assert_eq!(
            second,
            VaultItem::ShuffledConsumable {
                tier: 0,
                refill: 0,
                slot: 0
            }
        );
        let tier3 = consumables.create_consumable(3, &mut random);
        assert_eq!(
            tier3,
            VaultItem::ShuffledConsumable {
                tier: 3,
                refill: 0,
                slot: 0
            }
        );
    }

    #[test]
    fn solve_item_lookups_shuffle_before_searching() {
        let mut random = RandomStack::with_base_seed(0);
        random.push(5);
        let mut queue = VaultSpawnQueue::default();
        queue.add(VaultItem::Consumable(VaultConsumable::Stone(
            StoneKind::Blink,
        )));
        let before = random.clone();
        assert_eq!(
            queue.find_t2_solve_item(&mut random),
            Some(VaultItem::Consumable(VaultConsumable::Stone(
                StoneKind::Blink
            )))
        );
        let mut expected = before;
        let _ = expected.int_bound(2);
        assert_eq!(random.int(), expected.int());
        // Nothing left: both shuffles draw, then None.
        let before = random.clone();
        assert_eq!(queue.find_t2_solve_item(&mut random), None);
        let mut expected = before;
        let _ = expected.int_bound(2);
        let _ = expected.int_bound(2);
        assert_eq!(random.int(), expected.int());
    }
}
