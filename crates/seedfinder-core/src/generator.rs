//! Data-only replica of Shattered Pixel Dungeon's v3.3.8 `Generator`.
//!
//! Item identity decks use their private per-category generators. Tier,
//! upgrade, curse, enchantment, and glyph rolls stay on the caller's outer
//! generator. Keeping that boundary exact is essential: replaying prior item
//! choices instead of skipping one `Long()` per drop changes later identities.
//!
//! Every canonical category is represented as data. The only constructor state
//! deliberately omitted is `MissileWeapon.setID`, which comes from Java's
//! independent `SecureRandom` and does not affect the game's deterministic RNG.

use std::fmt;

use crate::catalog::ItemId;
use crate::equipment::{EquipmentRoll, roll_armor, roll_wand, roll_weapon};
use crate::rng::RandomStack;
use crate::run::{
    GENERATOR_CATEGORIES, GeneratorCategory, GeneratorState, PotionKind, RingKind, ScrollKind,
};

/// `Generator.floorSetTierProbs`, indexed by clamped floor set and item tier.
pub const FLOOR_SET_TIER_PROBABILITIES: [[f32; 5]; 5] = [
    [0.0, 75.0, 20.0, 4.0, 1.0],
    [0.0, 25.0, 50.0, 20.0, 5.0],
    [0.0, 0.0, 40.0, 50.0, 10.0],
    [0.0, 0.0, 20.0, 40.0, 40.0],
    [0.0, 0.0, 0.0, 20.0, 80.0],
];

/// `WEP_T1.classes` order. `None` is the zero-weight Mage's Staff.
pub const WEAPON_TIER_1_ITEMS: [Option<ItemId>; 6] = [
    Some(ItemId::WornShortsword),
    None,
    Some(ItemId::Dagger),
    Some(ItemId::StuddedGloves),
    Some(ItemId::Rapier),
    Some(ItemId::Cudgel),
];

/// `WEP_T2.classes` order. `None` is the zero-weight quest Pickaxe.
pub const WEAPON_TIER_2_ITEMS: [Option<ItemId>; 7] = [
    Some(ItemId::Shortsword),
    Some(ItemId::HandAxe),
    Some(ItemId::Spear),
    Some(ItemId::Quarterstaff),
    Some(ItemId::Dirk),
    Some(ItemId::Sickle),
    None,
];

/// `WEP_T3.classes` order.
pub const WEAPON_TIER_3_ITEMS: [Option<ItemId>; 6] = [
    Some(ItemId::Sword),
    Some(ItemId::Mace),
    Some(ItemId::Scimitar),
    Some(ItemId::RoundShield),
    Some(ItemId::Sai),
    Some(ItemId::Whip),
];

/// `WEP_T4.classes` order.
pub const WEAPON_TIER_4_ITEMS: [Option<ItemId>; 7] = [
    Some(ItemId::Longsword),
    Some(ItemId::BattleAxe),
    Some(ItemId::Flail),
    Some(ItemId::RunicBlade),
    Some(ItemId::AssassinsBlade),
    Some(ItemId::Crossbow),
    Some(ItemId::Katana),
];

/// `WEP_T5.classes` order.
pub const WEAPON_TIER_5_ITEMS: [Option<ItemId>; 7] = [
    Some(ItemId::Greatsword),
    Some(ItemId::WarHammer),
    Some(ItemId::Glaive),
    Some(ItemId::Greataxe),
    Some(ItemId::Greatshield),
    Some(ItemId::StoneGauntlet),
    Some(ItemId::WarScythe),
];

/// The five generic armor classes in `ARMOR.classes` order.
pub const ARMOR_ITEMS: [ItemId; 5] = [
    ItemId::ClothArmor,
    ItemId::LeatherArmor,
    ItemId::MailArmor,
    ItemId::ScaleArmor,
    ItemId::PlateArmor,
];

/// All wand classes in `WAND.classes` order.
pub const WAND_ITEMS: [ItemId; 13] = [
    ItemId::WandMagicMissile,
    ItemId::WandLightning,
    ItemId::WandDisintegration,
    ItemId::WandFireblast,
    ItemId::WandCorrosion,
    ItemId::WandBlastWave,
    ItemId::WandLivingEarth,
    ItemId::WandFrost,
    ItemId::WandPrismaticLight,
    ItemId::WandWarding,
    ItemId::WandTransfusion,
    ItemId::WandCorruption,
    ItemId::WandRegrowth,
];

/// Missile classes in their five category-table orders.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum MissileKind {
    ThrowingStone,
    ThrowingKnife,
    ThrowingSpike,
    Dart,
    FishingSpear,
    ThrowingClub,
    Shuriken,
    ThrowingSpear,
    Kunai,
    Bolas,
    Javelin,
    Tomahawk,
    HeavyBoomerang,
    Trident,
    ThrowingHammer,
    ForceCube,
}

impl MissileKind {
    /// Search-catalog identity for this thrown weapon. The zero-weight plain
    /// Dart has no searchable generated-world identity.
    #[must_use]
    pub const fn item_id(self) -> Option<ItemId> {
        match self {
            Self::ThrowingStone => Some(ItemId::ThrowingStone),
            Self::ThrowingKnife => Some(ItemId::ThrowingKnife),
            Self::ThrowingSpike => Some(ItemId::ThrowingSpike),
            Self::Dart => None,
            Self::FishingSpear => Some(ItemId::FishingSpear),
            Self::ThrowingClub => Some(ItemId::ThrowingClub),
            Self::Shuriken => Some(ItemId::Shuriken),
            Self::ThrowingSpear => Some(ItemId::ThrowingSpear),
            Self::Kunai => Some(ItemId::Kunai),
            Self::Bolas => Some(ItemId::Bolas),
            Self::Javelin => Some(ItemId::Javelin),
            Self::Tomahawk => Some(ItemId::Tomahawk),
            Self::HeavyBoomerang => Some(ItemId::HeavyBoomerang),
            Self::Trident => Some(ItemId::Trident),
            Self::ThrowingHammer => Some(ItemId::ThrowingHammer),
            Self::ForceCube => Some(ItemId::ForceCube),
        }
    }
}

/// Artifact classes in `ARTIFACT.classes` order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ArtifactKind {
    AlchemistsToolkit,
    ChaliceOfBlood,
    CloakOfShadows,
    DriedRose,
    EtherealChains,
    HolyTome,
    HornOfPlenty,
    MasterThievesArmband,
    SandalsOfNature,
    SkeletonKey,
    TalismanOfForesight,
    TimekeepersHourglass,
    UnstableSpellbook,
}

/// Food classes in `FOOD.classes` order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum FoodKind {
    Ration,
    Pasty,
    MysteryMeat,
}

/// Plant seed classes in `SEED.classes` order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum SeedKind {
    Rotberry,
    Sungrass,
    Fadeleaf,
    Icecap,
    Firebloom,
    Sorrowmoss,
    Swiftthistle,
    Blindweed,
    Stormvine,
    Earthroot,
    Mageroyal,
    Starflower,
}

impl SeedKind {
    /// Tipped-dart identity produced by `TippedDart.getTipped` for this seed.
    /// The mapping follows the insertion-ordered upstream `types` table.
    #[must_use]
    pub const fn tipped_dart_item_id(self) -> ItemId {
        match self {
            Self::Rotberry => ItemId::RotDart,
            Self::Sungrass => ItemId::HealingDart,
            Self::Fadeleaf => ItemId::DisplacingDart,
            Self::Icecap => ItemId::ChillingDart,
            Self::Firebloom => ItemId::IncendiaryDart,
            Self::Sorrowmoss => ItemId::PoisonDart,
            Self::Swiftthistle => ItemId::AdrenalineDart,
            Self::Blindweed => ItemId::BlindingDart,
            Self::Stormvine => ItemId::ShockingDart,
            Self::Earthroot => ItemId::ParalyticDart,
            Self::Mageroyal => ItemId::CleansingDart,
            Self::Starflower => ItemId::HolyDart,
        }
    }
}

/// Runestone classes in `STONE.classes` order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum StoneKind {
    Enchantment,
    Intuition,
    DetectMagic,
    Flock,
    Shock,
    Blink,
    DeepSleep,
    Clairvoyance,
    Aggression,
    Blast,
    Fear,
    Augmentation,
}

/// Trinket classes in `TRINKET.classes` order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum TrinketKind {
    RatSkull,
    ParchmentScrap,
    PetrifiedSeed,
    ExoticCrystals,
    MossyClump,
    DimensionalSundial,
    ThirteenLeafClover,
    TrapMechanism,
    MimicTooth,
    WondrousResin,
    EyeOfNewt,
    SaltCube,
    VialOfBlood,
    ShardOfOblivion,
    ChaoticCenser,
    FerretTuft,
    CrackedSpyglass,
}

/// Result of `Bomb.random()`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BombKind {
    Bomb,
    DoubleBomb,
}

/// One generated missile stack. All ordinary missiles start with quantity 3;
/// the zero-weight plain Dart starts with quantity 2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratedMissile {
    pub kind: MissileKind,
    pub quantity: i32,
    pub roll: EquipmentRoll,
}

/// One generated ring.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratedRing {
    pub kind: RingKind,
    pub roll: EquipmentRoll,
}

/// One generated artifact. The spellbook's constructor order is retained
/// because constructing it consumes eleven weighted draws before its curse.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratedArtifact {
    pub kind: ArtifactKind,
    pub cursed: bool,
    pub spellbook_scrolls: Option<[ScrollKind; 10]>,
}

/// Every deterministic item class produced by Generator or its direct
/// Bomb/Gold/TippedDart helpers in canonical level generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratedItem {
    Equipment(GeneratedEquipment),
    Missile(GeneratedMissile),
    Ring(GeneratedRing),
    Artifact(GeneratedArtifact),
    Food(FoodKind),
    Potion { kind: PotionKind, exotic: bool },
    Seed(SeedKind),
    Scroll { kind: ScrollKind, exotic: bool },
    Stone(StoneKind),
    Gold { quantity: i32 },
    Trinket(TrinketKind),
    Bomb(BombKind),
    TippedDart { seed: SeedKind, quantity: i32 },
}

/// Coarse family for callers that only accept generated equipment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratedItemFamily {
    Equipment,
    Missile,
    Ring,
    Artifact,
    Food,
    Potion,
    Seed,
    Scroll,
    Stone,
    Gold,
    Trinket,
    Bomb,
    TippedDart,
}

impl GeneratedItem {
    #[must_use]
    pub const fn family(self) -> GeneratedItemFamily {
        match self {
            Self::Equipment(_) => GeneratedItemFamily::Equipment,
            Self::Missile(_) => GeneratedItemFamily::Missile,
            Self::Ring(_) => GeneratedItemFamily::Ring,
            Self::Artifact(_) => GeneratedItemFamily::Artifact,
            Self::Food(_) => GeneratedItemFamily::Food,
            Self::Potion { .. } => GeneratedItemFamily::Potion,
            Self::Seed(_) => GeneratedItemFamily::Seed,
            Self::Scroll { .. } => GeneratedItemFamily::Scroll,
            Self::Stone(_) => GeneratedItemFamily::Stone,
            Self::Gold { .. } => GeneratedItemFamily::Gold,
            Self::Trinket(_) => GeneratedItemFamily::Trinket,
            Self::Bomb(_) => GeneratedItemFamily::Bomb,
            Self::TippedDart { .. } => GeneratedItemFamily::TippedDart,
        }
    }

    /// Searchable weapon/armor/wand/ring representation, if this generated item is
    /// part of the equipment query catalog. Tipped darts are fixed level-zero,
    /// clean weapons; their stack quantity does not affect search matching.
    #[must_use]
    pub const fn searchable_equipment(self) -> Option<GeneratedEquipment> {
        match self {
            Self::Equipment(equipment) => Some(equipment),
            Self::Missile(missile) => match missile.kind.item_id() {
                Some(item) => Some(GeneratedEquipment {
                    item,
                    roll: missile.roll,
                }),
                None => None,
            },
            Self::TippedDart { seed, .. } => Some(GeneratedEquipment {
                item: seed.tipped_dart_item_id(),
                roll: EquipmentRoll {
                    upgrade: 0,
                    effect: None,
                    cursed: false,
                },
            }),
            Self::Ring(ring) => Some(GeneratedEquipment {
                item: ring.kind.item_id(),
                roll: ring.roll,
            }),
            Self::Artifact(_)
            | Self::Food(_)
            | Self::Potion { .. }
            | Self::Seed(_)
            | Self::Scroll { .. }
            | Self::Stone(_)
            | Self::Gold { .. }
            | Self::Trinket(_)
            | Self::Bomb(_) => None,
        }
    }
}

pub const MISSILE_TIER_1_ITEMS: [MissileKind; 4] = [
    MissileKind::ThrowingStone,
    MissileKind::ThrowingKnife,
    MissileKind::ThrowingSpike,
    MissileKind::Dart,
];
pub const MISSILE_TIER_2_ITEMS: [MissileKind; 3] = [
    MissileKind::FishingSpear,
    MissileKind::ThrowingClub,
    MissileKind::Shuriken,
];
pub const MISSILE_TIER_3_ITEMS: [MissileKind; 3] = [
    MissileKind::ThrowingSpear,
    MissileKind::Kunai,
    MissileKind::Bolas,
];
pub const MISSILE_TIER_4_ITEMS: [MissileKind; 3] = [
    MissileKind::Javelin,
    MissileKind::Tomahawk,
    MissileKind::HeavyBoomerang,
];
pub const MISSILE_TIER_5_ITEMS: [MissileKind; 3] = [
    MissileKind::Trident,
    MissileKind::ThrowingHammer,
    MissileKind::ForceCube,
];

pub const ARTIFACT_ITEMS: [ArtifactKind; 13] = [
    ArtifactKind::AlchemistsToolkit,
    ArtifactKind::ChaliceOfBlood,
    ArtifactKind::CloakOfShadows,
    ArtifactKind::DriedRose,
    ArtifactKind::EtherealChains,
    ArtifactKind::HolyTome,
    ArtifactKind::HornOfPlenty,
    ArtifactKind::MasterThievesArmband,
    ArtifactKind::SandalsOfNature,
    ArtifactKind::SkeletonKey,
    ArtifactKind::TalismanOfForesight,
    ArtifactKind::TimekeepersHourglass,
    ArtifactKind::UnstableSpellbook,
];

pub const FOOD_ITEMS: [FoodKind; 3] = [FoodKind::Ration, FoodKind::Pasty, FoodKind::MysteryMeat];
pub const RING_ITEMS: [RingKind; 12] = [
    RingKind::Accuracy,
    RingKind::Arcana,
    RingKind::Elements,
    RingKind::Energy,
    RingKind::Evasion,
    RingKind::Force,
    RingKind::Furor,
    RingKind::Haste,
    RingKind::Might,
    RingKind::Sharpshooting,
    RingKind::Tenacity,
    RingKind::Wealth,
];

impl RingKind {
    /// Stable searchable catalog identity corresponding to this Java ring class.
    #[must_use]
    pub const fn item_id(self) -> ItemId {
        match self {
            Self::Accuracy => ItemId::RingAccuracy,
            Self::Arcana => ItemId::RingArcana,
            Self::Elements => ItemId::RingElements,
            Self::Energy => ItemId::RingEnergy,
            Self::Evasion => ItemId::RingEvasion,
            Self::Force => ItemId::RingForce,
            Self::Furor => ItemId::RingFuror,
            Self::Haste => ItemId::RingHaste,
            Self::Might => ItemId::RingMight,
            Self::Sharpshooting => ItemId::RingSharpshooting,
            Self::Tenacity => ItemId::RingTenacity,
            Self::Wealth => ItemId::RingWealth,
        }
    }
}
pub const POTION_ITEMS: [PotionKind; 12] = [
    PotionKind::Strength,
    PotionKind::Healing,
    PotionKind::MindVision,
    PotionKind::Frost,
    PotionKind::LiquidFlame,
    PotionKind::ToxicGas,
    PotionKind::Haste,
    PotionKind::Invisibility,
    PotionKind::Levitation,
    PotionKind::ParalyticGas,
    PotionKind::Purity,
    PotionKind::Experience,
];
pub const SEED_ITEMS: [SeedKind; 12] = [
    SeedKind::Rotberry,
    SeedKind::Sungrass,
    SeedKind::Fadeleaf,
    SeedKind::Icecap,
    SeedKind::Firebloom,
    SeedKind::Sorrowmoss,
    SeedKind::Swiftthistle,
    SeedKind::Blindweed,
    SeedKind::Stormvine,
    SeedKind::Earthroot,
    SeedKind::Mageroyal,
    SeedKind::Starflower,
];
pub const SCROLL_ITEMS: [ScrollKind; 12] = [
    ScrollKind::Upgrade,
    ScrollKind::Identify,
    ScrollKind::RemoveCurse,
    ScrollKind::MirrorImage,
    ScrollKind::Recharging,
    ScrollKind::Teleportation,
    ScrollKind::Lullaby,
    ScrollKind::MagicMapping,
    ScrollKind::Rage,
    ScrollKind::Retribution,
    ScrollKind::Terror,
    ScrollKind::Transmutation,
];
pub const STONE_ITEMS: [StoneKind; 12] = [
    StoneKind::Enchantment,
    StoneKind::Intuition,
    StoneKind::DetectMagic,
    StoneKind::Flock,
    StoneKind::Shock,
    StoneKind::Blink,
    StoneKind::DeepSleep,
    StoneKind::Clairvoyance,
    StoneKind::Aggression,
    StoneKind::Blast,
    StoneKind::Fear,
    StoneKind::Augmentation,
];
pub const TRINKET_ITEMS: [TrinketKind; 17] = [
    TrinketKind::RatSkull,
    TrinketKind::ParchmentScrap,
    TrinketKind::PetrifiedSeed,
    TrinketKind::ExoticCrystals,
    TrinketKind::MossyClump,
    TrinketKind::DimensionalSundial,
    TrinketKind::ThirteenLeafClover,
    TrinketKind::TrapMechanism,
    TrinketKind::MimicTooth,
    TrinketKind::WondrousResin,
    TrinketKind::EyeOfNewt,
    TrinketKind::SaltCube,
    TrinketKind::VialOfBlood,
    TrinketKind::ShardOfOblivion,
    TrinketKind::ChaoticCenser,
    TrinketKind::FerretTuft,
    TrinketKind::CrackedSpyglass,
];

/// One target item after its class and mutable equipment properties are rolled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratedEquipment {
    pub item: ItemId,
    pub roll: EquipmentRoll,
}

/// Generator invariant and typed-wrapper failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratorError {
    ExpectedEquipment(GeneratedItemFamily),
    NotEquipmentCategory(GeneratorCategory),
    MissingCategorySeed(GeneratorCategory),
    EmptyCategoryDeck(GeneratorCategory),
    IdentityIndexOutOfRange {
        category: GeneratorCategory,
        index: usize,
    },
    /// A zero-weight, non-searchable class was made selectable by invalid or
    /// externally modified state. Its private identity draw/deck mutation has
    /// happened, but its outer-stream `Item.random()` was not consumed.
    NonTargetIdentity {
        category: GeneratorCategory,
        index: usize,
        class_name: &'static str,
    },
}

impl fmt::Display for GeneratorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedEquipment(family) => {
                write!(formatter, "expected equipment but generated {family:?}")
            }
            Self::NotEquipmentCategory(category) => {
                write!(formatter, "category {category:?} is not target equipment")
            }
            Self::MissingCategorySeed(category) => {
                write!(
                    formatter,
                    "category {category:?} has no deterministic deck seed"
                )
            }
            Self::EmptyCategoryDeck(category) => {
                write!(formatter, "category {category:?} stayed empty after reset")
            }
            Self::IdentityIndexOutOfRange { category, index } => write!(
                formatter,
                "category {category:?} produced out-of-range identity index {index}"
            ),
            Self::NonTargetIdentity {
                category,
                index,
                class_name,
            } => write!(
                formatter,
                "category {category:?} selected unsupported class {class_name} at index {index}; identity/deck state was consumed but Item.random RNG was not"
            ),
        }
    }
}

impl std::error::Error for GeneratorError {}

/// Mirrors `Generator.generalReset()` without changing `usingFirstDeck`.
pub fn general_reset(generator: &mut GeneratorState) {
    for (index, category) in GENERATOR_CATEGORIES.into_iter().enumerate() {
        generator.category_probabilities[index] = if generator.using_first_deck {
            category.first_probability()
        } else {
            category.second_probability()
        };
    }
}

/// Selects and removes one card from the overall 35-card category deck.
///
/// If the current deck is empty, this toggles decks, resets all overall
/// weights, and consumes the selection draw from the new deck.
///
/// # Panics
///
/// Panics only if the version-pinned first and second deck definitions have
/// both been replaced with non-positive weights.
#[must_use]
pub fn select_overall_category(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
) -> GeneratorCategory {
    let mut selected = ordered_map_chances(random, &generator.category_probabilities);
    if selected.is_none() {
        generator.using_first_deck = !generator.using_first_deck;
        general_reset(generator);
        selected = ordered_map_chances(random, &generator.category_probabilities);
    }
    let index = selected.expect("an overall generator deck always contains 35 cards");
    generator.category_probabilities[index] -= 1.0;
    GENERATOR_CATEGORIES[index]
}

/// Selects from the combined 70-card default category weights without
/// decrementing either overall deck.
///
/// # Panics
///
/// Panics only if `default_category_probabilities` has been externally
/// replaced with entirely non-positive weights.
#[must_use]
pub fn select_default_overall_category(
    random: &mut RandomStack,
    generator: &GeneratorState,
) -> GeneratorCategory {
    let index = ordered_map_chances(random, &generator.default_category_probabilities)
        .expect("default overall generator probabilities total 70");
    GENERATOR_CATEGORIES[index]
}

/// Mirrors no-argument `Generator.random()` at the supplied dungeon depth.
///
/// # Errors
///
/// Returns an invariant error only when version-pinned category state has been
/// externally corrupted.
pub fn random(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    depth: i32,
) -> Result<GeneratedItem, GeneratorError> {
    let category = select_overall_category(random, generator);
    // Level-generated seeds deliberately bypass their private seed deck.
    if category == GeneratorCategory::Seed {
        random_using_defaults(random, generator, category, depth)
    } else {
        random_category(random, generator, category, depth)
    }
}

/// Mirrors `Generator.random(category)` for every canonical category.
///
/// # Errors
///
/// Returns an invariant error only for corrupted category state or identity
/// tables.
pub fn random_category(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    category: GeneratorCategory,
    depth: i32,
) -> Result<GeneratedItem, GeneratorError> {
    let floor_set = depth / 5;
    match category {
        GeneratorCategory::Weapon => {
            random_weapon(random, generator, floor_set, false).map(GeneratedItem::Equipment)
        }
        GeneratorCategory::Armor => random_armor(random, floor_set).map(GeneratedItem::Equipment),
        GeneratorCategory::Missile => {
            random_missile(random, generator, floor_set, false).map(GeneratedItem::Missile)
        }
        GeneratorCategory::Artifact => random_artifact_or_ring(random, generator),
        GeneratorCategory::Gold => {
            random
                .chances(&[1.0])
                .ok_or(GeneratorError::EmptyCategoryDeck(category))?;
            Ok(random_gold(random, depth))
        }
        _ => {
            let index = select_seeded_identity_index(random, generator, category)?;
            randomize_identity(random, category, index, true)
        }
    }
}

/// Target-only compatibility wrapper. A non-equipment result is fully
/// generated before the typed error is returned, so RNG parity is preserved.
///
/// # Errors
///
/// Returns [`GeneratorError::ExpectedEquipment`] for a non-equipment result,
/// or an invariant error from [`random`].
pub fn random_target(
    outer_random: &mut RandomStack,
    generator: &mut GeneratorState,
    depth: i32,
) -> Result<GeneratedEquipment, GeneratorError> {
    match random(outer_random, generator, depth)? {
        GeneratedItem::Equipment(equipment) => Ok(equipment),
        other => Err(GeneratorError::ExpectedEquipment(other.family())),
    }
}

/// Mirrors the target-producing category overload of `Generator.random`.
///
/// # Errors
///
/// Returns [`GeneratorError::NotEquipmentCategory`] for a non-target category,
/// or an invariant error for corrupted deck state.
pub fn random_category_target(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    category: GeneratorCategory,
    floor_set: i32,
) -> Result<GeneratedEquipment, GeneratorError> {
    match category {
        GeneratorCategory::Weapon => random_weapon(random, generator, floor_set, false),
        GeneratorCategory::Armor => random_armor(random, floor_set),
        GeneratorCategory::Wand => random_wand(random, generator, false),
        GeneratorCategory::WeaponTier1
        | GeneratorCategory::WeaponTier2
        | GeneratorCategory::WeaponTier3
        | GeneratorCategory::WeaponTier4
        | GeneratorCategory::WeaponTier5 => {
            random_weapon_from_tier(random, generator, category, false)
        }
        _ => Err(GeneratorError::NotEquipmentCategory(category)),
    }
}

/// Mirrors no-argument `Generator.randomUsingDefaults()`.
///
/// # Errors
///
/// Returns an invariant error only for corrupted generator state.
pub fn random_using_defaults_overall(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    depth: i32,
) -> Result<GeneratedItem, GeneratorError> {
    let category = select_default_overall_category(random, generator);
    random_using_defaults(random, generator, category, depth)
}

/// Mirrors `Generator.randomUsingDefaults(category)` for every category.
/// Decks remain unchanged except artifacts, which upstream always generates
/// through their unique deck (and can fall back to a deck-backed ring).
///
/// # Errors
///
/// Returns an invariant error only for corrupted category state or tables.
pub fn random_using_defaults(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    category: GeneratorCategory,
    depth: i32,
) -> Result<GeneratedItem, GeneratorError> {
    let floor_set = depth / 5;
    match category {
        GeneratorCategory::Weapon => {
            random_weapon_using_defaults(random, floor_set).map(GeneratedItem::Equipment)
        }
        GeneratorCategory::Missile => {
            random_missile_using_defaults(random, floor_set).map(GeneratedItem::Missile)
        }
        GeneratorCategory::Armor | GeneratorCategory::Gold | GeneratorCategory::Artifact => {
            random_category(random, generator, category, depth)
        }
        _ => {
            let index = select_default_identity_index(random, category)?;
            // The two-deck POTION/SCROLL branch returns immediately upstream,
            // before the ExoticCrystals check.
            randomize_identity(random, category, index, false)
        }
    }
}

/// Mirrors `Generator.randomWeapon(floorSet, useDefaults)`.
///
/// # Errors
///
/// Returns an invariant error when the selected tier lacks its canonical seed
/// or produces an invalid identity index.
pub fn random_weapon(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    floor_set: i32,
    use_defaults: bool,
) -> Result<GeneratedEquipment, GeneratorError> {
    if use_defaults {
        return random_weapon_using_defaults(random, floor_set);
    }
    let category = weapon_tier_category(select_floor_tier(random, floor_set));
    random_weapon_from_tier(random, generator, category, false)
}

/// The `useDefaults=true` weapon path, which consumes class identity from the
/// outer stream and leaves all category decks unchanged.
///
/// # Errors
///
/// Returns an identity invariant error if the version-pinned tables were
/// externally corrupted.
pub fn random_weapon_using_defaults(
    random: &mut RandomStack,
    floor_set: i32,
) -> Result<GeneratedEquipment, GeneratorError> {
    let category = weapon_tier_category(select_floor_tier(random, floor_set));
    let index = select_default_identity_index(random, category)?;
    let item = weapon_identity(category, index)?;
    Ok(GeneratedEquipment {
        item,
        roll: roll_weapon(random),
    })
}

/// Mirrors `Generator.randomArmor(floorSet)`.
///
/// # Errors
///
/// Returns an identity invariant error if the tier table produces an invalid
/// index.
pub fn random_armor(
    random: &mut RandomStack,
    floor_set: i32,
) -> Result<GeneratedEquipment, GeneratorError> {
    let index = select_floor_tier(random, floor_set);
    let item = ARMOR_ITEMS
        .get(index)
        .copied()
        .ok_or(GeneratorError::IdentityIndexOutOfRange {
            category: GeneratorCategory::Armor,
            index,
        })?;
    Ok(GeneratedEquipment {
        item,
        roll: roll_armor(random),
    })
}

/// Generates a deck-backed wand (`Generator.random(WAND)`) or, when requested,
/// its `randomUsingDefaults(WAND)` equivalent.
///
/// # Errors
///
/// Returns an invariant error if the wand deck seed, weights, or identity
/// table are invalid.
pub fn random_wand(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    use_defaults: bool,
) -> Result<GeneratedEquipment, GeneratorError> {
    if use_defaults {
        return random_wand_using_defaults(random);
    }
    let index = select_seeded_identity_index(random, generator, GeneratorCategory::Wand)?;
    let item = wand_identity(index)?;
    Ok(GeneratedEquipment {
        item,
        roll: roll_wand(random),
    })
}

/// The wand defaults path; identity and item randomization both use the outer
/// generator and no wand-deck state changes.
///
/// # Errors
///
/// Returns an identity invariant error if the default wand table is invalid.
pub fn random_wand_using_defaults(
    random: &mut RandomStack,
) -> Result<GeneratedEquipment, GeneratorError> {
    let index = select_default_identity_index(random, GeneratorCategory::Wand)?;
    let item = wand_identity(index)?;
    Ok(GeneratedEquipment {
        item,
        roll: roll_wand(random),
    })
}

/// Mirrors `Generator.randomMissile(floorSet, useDefaults)`.
///
/// # Errors
///
/// Returns an invariant error for corrupted tier deck state or tables.
pub fn random_missile(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    floor_set: i32,
    use_defaults: bool,
) -> Result<GeneratedMissile, GeneratorError> {
    if use_defaults {
        return random_missile_using_defaults(random, floor_set);
    }
    let category = missile_tier_category(select_floor_tier(random, floor_set));
    random_missile_from_tier(random, generator, category, false)
}

/// Default-probability missile path; identity is selected on the outer stream.
///
/// # Errors
///
/// Returns an invariant error for corrupted version-pinned tables.
pub fn random_missile_using_defaults(
    random: &mut RandomStack,
    floor_set: i32,
) -> Result<GeneratedMissile, GeneratorError> {
    let category = missile_tier_category(select_floor_tier(random, floor_set));
    let index = select_default_identity_index(random, category)?;
    randomize_missile(random, category, index)
}

/// Mirrors `Generator.randomArtifact()` without its category-level ring
/// fallback. An exhausted artifact deck returns `Ok(None)` and still advances
/// the artifact dropped counter, matching upstream.
///
/// # Errors
///
/// Returns an invariant error for a missing artifact seed or invalid identity.
pub fn random_artifact(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
) -> Result<Option<GeneratedArtifact>, GeneratorError> {
    let Some(index) = select_artifact_identity_index(random, generator)? else {
        return Ok(None);
    };
    let kind =
        ARTIFACT_ITEMS
            .get(index)
            .copied()
            .ok_or(GeneratorError::IdentityIndexOutOfRange {
                category: GeneratorCategory::Artifact,
                index,
            })?;
    Ok(Some(randomize_artifact(random, kind)))
}

/// Mirrors `Generator.removeArtifact` for the canonical artifact table.
#[must_use]
pub fn remove_artifact(generator: &mut GeneratorState, artifact: ArtifactKind) -> bool {
    let index = artifact as usize;
    let probabilities = generator
        .category_mut(GeneratorCategory::Artifact)
        .probabilities
        .as_mut_slice();
    if probabilities
        .get(index)
        .is_some_and(|probability| *probability > 0.0)
    {
        probabilities[index] = 0.0;
        true
    } else {
        false
    }
}

/// Mirrors `new Gold().random()`; unlike `Generator.random(GOLD)`, this does
/// not consume the one-entry category identity chance first.
#[must_use]
pub fn random_gold(random: &mut RandomStack, depth: i32) -> GeneratedItem {
    let minimum = 30_i32.wrapping_add(depth.wrapping_mul(10));
    let maximum = 60_i32.wrapping_add(depth.wrapping_mul(20));
    GeneratedItem::Gold {
        quantity: random.int_range(minimum, maximum),
    }
}

/// Mirrors `new Bomb().random()`.
#[must_use]
pub fn random_bomb(random: &mut RandomStack) -> GeneratedItem {
    GeneratedItem::Bomb(if random.int_bound(4) == 0 {
        BombKind::DoubleBomb
    } else {
        BombKind::Bomb
    })
}

/// A directly constructed, non-randomized shop bomb.
#[must_use]
pub const fn plain_bomb() -> GeneratedItem {
    GeneratedItem::Bomb(BombKind::Bomb)
}

/// Mirrors `TippedDart.randomTipped(quantity)` under canonical seed defaults.
///
/// # Errors
///
/// Returns an invariant error only if the fixed seed table selects an invalid
/// identity.
pub fn random_tipped_dart(
    random: &mut RandomStack,
    quantity: i32,
) -> Result<GeneratedItem, GeneratorError> {
    let index = select_default_identity_index(random, GeneratorCategory::Seed)?;
    let seed = seed_identity(index)?;
    Ok(GeneratedItem::TippedDart { seed, quantity })
}

/// Mirrors the observable result of `Generator.undoDrop(concreteItem)` for all
/// searchable equipment: it is deliberately a no-op.
///
/// Upstream tests `concreteClass.isAssignableFrom(category.superClass)`, whose
/// direction is reversed for a concrete weapon/wand/armor class. Wandmaker and
/// Blacksmith duplicate rerolls therefore keep their consumed deck cards.
pub const fn undo_drop(_generator: &mut GeneratorState, _item: ItemId) {}

fn random_weapon_from_tier(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    category: GeneratorCategory,
    use_defaults: bool,
) -> Result<GeneratedEquipment, GeneratorError> {
    let index = if use_defaults {
        select_default_identity_index(random, category)?
    } else {
        select_seeded_identity_index(random, generator, category)?
    };
    let item = weapon_identity(category, index)?;
    Ok(GeneratedEquipment {
        item,
        // Item.random() resumes on the outer generator after the category
        // generator has been popped.
        roll: roll_weapon(random),
    })
}

fn random_missile_from_tier(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    category: GeneratorCategory,
    use_defaults: bool,
) -> Result<GeneratedMissile, GeneratorError> {
    let index = if use_defaults {
        select_default_identity_index(random, category)?
    } else {
        select_seeded_identity_index(random, generator, category)?
    };
    randomize_missile(random, category, index)
}

fn random_artifact_or_ring(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
) -> Result<GeneratedItem, GeneratorError> {
    if let Some(artifact) = random_artifact(random, generator)? {
        Ok(GeneratedItem::Artifact(artifact))
    } else {
        let index = select_seeded_identity_index(random, generator, GeneratorCategory::Ring)?;
        randomize_identity(random, GeneratorCategory::Ring, index, true)
    }
}

fn select_seeded_identity_index(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    category: GeneratorCategory,
) -> Result<usize, GeneratorError> {
    let category_state = generator.category(category);
    let seed = category_state
        .seed
        .ok_or(GeneratorError::MissingCategorySeed(category))?;
    let dropped = category_state.dropped;

    random.push(seed);
    let mut skipped = 0_i32;
    while skipped < dropped {
        random.long();
        skipped += 1;
    }

    let selected = {
        let category_state = generator.category_mut(category);
        let mut selected = random.chances(category_state.probabilities.as_slice());
        if selected.is_none() {
            category_state.reset();
            selected = random.chances(category_state.probabilities.as_slice());
        }
        if let Some(index) = selected {
            category_state.probabilities.as_mut_slice()[index] -= 1.0;
        }
        selected
    };
    random.pop();

    let index = selected.ok_or(GeneratorError::EmptyCategoryDeck(category))?;
    let category_state = generator.category_mut(category);
    category_state.dropped = category_state.dropped.wrapping_add(1);
    Ok(index)
}

fn select_artifact_identity_index(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
) -> Result<Option<usize>, GeneratorError> {
    let category = GeneratorCategory::Artifact;
    let category_state = generator.category(category);
    let seed = category_state
        .seed
        .ok_or(GeneratorError::MissingCategorySeed(category))?;
    let dropped = category_state.dropped;

    random.push(seed);
    let mut skipped = 0_i32;
    while skipped < dropped {
        random.long();
        skipped += 1;
    }
    let selected = random.chances(generator.category(category).probabilities.as_slice());
    random.pop();

    let category_state = generator.category_mut(category);
    category_state.dropped = category_state.dropped.wrapping_add(1);
    if let Some(index) = selected {
        category_state.probabilities.as_mut_slice()[index] -= 1.0;
    }
    Ok(selected)
}

fn randomize_identity(
    random: &mut RandomStack,
    category: GeneratorCategory,
    index: usize,
    check_exotic: bool,
) -> Result<GeneratedItem, GeneratorError> {
    match category {
        GeneratorCategory::WeaponTier1
        | GeneratorCategory::WeaponTier2
        | GeneratorCategory::WeaponTier3
        | GeneratorCategory::WeaponTier4
        | GeneratorCategory::WeaponTier5 => Ok(GeneratedItem::Equipment(GeneratedEquipment {
            item: weapon_identity(category, index)?,
            roll: roll_weapon(random),
        })),
        GeneratorCategory::MissileTier1
        | GeneratorCategory::MissileTier2
        | GeneratorCategory::MissileTier3
        | GeneratorCategory::MissileTier4
        | GeneratorCategory::MissileTier5 => {
            randomize_missile(random, category, index).map(GeneratedItem::Missile)
        }
        GeneratorCategory::Wand => Ok(GeneratedItem::Equipment(GeneratedEquipment {
            item: wand_identity(index)?,
            roll: roll_wand(random),
        })),
        GeneratorCategory::Ring => Ok(GeneratedItem::Ring(GeneratedRing {
            kind: ring_identity(index)?,
            roll: roll_wand(random),
        })),
        GeneratorCategory::Trinket => Ok(GeneratedItem::Trinket(trinket_identity(index)?)),
        GeneratorCategory::Food => Ok(GeneratedItem::Food(food_identity(index)?)),
        GeneratorCategory::Potion => {
            // No Exotic Crystals is equipped in the canonical profile. The
            // deck-backed path still consumes this Float() check.
            if check_exotic {
                random.float();
            }
            Ok(GeneratedItem::Potion {
                kind: potion_identity(index)?,
                exotic: false,
            })
        }
        GeneratorCategory::Seed => Ok(GeneratedItem::Seed(seed_identity(index)?)),
        GeneratorCategory::Scroll => {
            if check_exotic {
                random.float();
            }
            Ok(GeneratedItem::Scroll {
                kind: scroll_identity(index)?,
                exotic: false,
            })
        }
        GeneratorCategory::Stone => Ok(GeneratedItem::Stone(stone_identity(index)?)),
        _ => Err(GeneratorError::IdentityIndexOutOfRange { category, index }),
    }
}

fn randomize_missile(
    random: &mut RandomStack,
    category: GeneratorCategory,
    index: usize,
) -> Result<GeneratedMissile, GeneratorError> {
    let kind = missile_identity(category, index)?;
    Ok(GeneratedMissile {
        kind,
        quantity: if kind == MissileKind::Dart { 2 } else { 3 },
        roll: roll_weapon(random),
    })
}

fn randomize_artifact(random: &mut RandomStack, kind: ArtifactKind) -> GeneratedArtifact {
    // UnstableSpellbook.setupScrolls() runs in its constructor, before
    // Artifact.random() performs the curse Float().
    let spellbook_scrolls =
        (kind == ArtifactKind::UnstableSpellbook).then(|| random_spellbook_scrolls(random));
    GeneratedArtifact {
        kind,
        cursed: random.float() < 0.3,
        spellbook_scrolls,
    }
}

fn random_spellbook_scrolls(random: &mut RandomStack) -> [ScrollKind; 10] {
    let source = GeneratorCategory::Scroll
        .default_probabilities_total()
        .expect("scrolls have a combined default deck");
    let mut probabilities = [0.0_f32; 12];
    probabilities.copy_from_slice(source);
    let mut result = [ScrollKind::Identify; 10];
    let mut result_len = 0;
    while let Some(index) = random.chances(&probabilities) {
        probabilities[index] = 0.0;
        let scroll = SCROLL_ITEMS[index];
        if scroll != ScrollKind::Transmutation {
            result[result_len] = scroll;
            result_len += 1;
        }
    }
    debug_assert_eq!(result_len, result.len());
    result
}

fn select_default_identity_index(
    random: &mut RandomStack,
    category: GeneratorCategory,
) -> Result<usize, GeneratorError> {
    let probabilities = category
        .default_probabilities_total()
        .or_else(|| category.primary_probabilities())
        .ok_or(GeneratorError::EmptyCategoryDeck(category))?;
    random
        .chances(probabilities)
        .ok_or(GeneratorError::EmptyCategoryDeck(category))
}

// Mirrors Random.chances(HashMap<K, Float>), whose insertion order comes from
// Generator's LinkedHashMap. Unlike the float[] overload, negative values are
// not clamped to zero.
fn ordered_map_chances(random: &mut RandomStack, probabilities: &[f32]) -> Option<usize> {
    let mut total = 0.0_f32;
    for probability in probabilities {
        total += probability;
    }
    if total <= 0.0 {
        return None;
    }

    let value = random.float_bound(total);
    let mut cumulative = probabilities.first().copied().unwrap_or_default();
    for index in 0..probabilities.len() {
        if value < cumulative {
            return Some(index);
        }
        if let Some(next) = probabilities.get(index + 1) {
            cumulative += next;
        }
    }
    // The Java overload would overrun `probs[i + 1]` here. Canonical weights
    // and nextFloat's [0, 1) range make this branch unreachable.
    panic!("ordered map chances failed to select below its positive total")
}

fn select_floor_tier(random: &mut RandomStack, floor_set: i32) -> usize {
    let clamped = usize::try_from(floor_set.clamp(0, 4)).unwrap_or_default();
    random
        .chances(&FLOOR_SET_TIER_PROBABILITIES[clamped])
        .expect("every floor-set tier table has positive weight")
}

const fn weapon_tier_category(index: usize) -> GeneratorCategory {
    match index {
        0 => GeneratorCategory::WeaponTier1,
        1 => GeneratorCategory::WeaponTier2,
        2 => GeneratorCategory::WeaponTier3,
        3 => GeneratorCategory::WeaponTier4,
        _ => GeneratorCategory::WeaponTier5,
    }
}

const fn missile_tier_category(index: usize) -> GeneratorCategory {
    match index {
        0 => GeneratorCategory::MissileTier1,
        1 => GeneratorCategory::MissileTier2,
        2 => GeneratorCategory::MissileTier3,
        3 => GeneratorCategory::MissileTier4,
        _ => GeneratorCategory::MissileTier5,
    }
}

fn weapon_identity(category: GeneratorCategory, index: usize) -> Result<ItemId, GeneratorError> {
    let identities: &[Option<ItemId>] = match category {
        GeneratorCategory::WeaponTier1 => &WEAPON_TIER_1_ITEMS,
        GeneratorCategory::WeaponTier2 => &WEAPON_TIER_2_ITEMS,
        GeneratorCategory::WeaponTier3 => &WEAPON_TIER_3_ITEMS,
        GeneratorCategory::WeaponTier4 => &WEAPON_TIER_4_ITEMS,
        GeneratorCategory::WeaponTier5 => &WEAPON_TIER_5_ITEMS,
        _ => {
            return Err(GeneratorError::IdentityIndexOutOfRange { category, index });
        }
    };
    let identity = identities
        .get(index)
        .ok_or(GeneratorError::IdentityIndexOutOfRange { category, index })?;
    identity.ok_or_else(|| GeneratorError::NonTargetIdentity {
        category,
        index,
        class_name: match (category, index) {
            (GeneratorCategory::WeaponTier1, 1) => "MagesStaff",
            (GeneratorCategory::WeaponTier2, 6) => "Pickaxe",
            _ => "unknown zero-weight class",
        },
    })
}

fn wand_identity(index: usize) -> Result<ItemId, GeneratorError> {
    WAND_ITEMS
        .get(index)
        .copied()
        .ok_or(GeneratorError::IdentityIndexOutOfRange {
            category: GeneratorCategory::Wand,
            index,
        })
}

fn missile_identity(
    category: GeneratorCategory,
    index: usize,
) -> Result<MissileKind, GeneratorError> {
    let identities: &[MissileKind] = match category {
        GeneratorCategory::MissileTier1 => &MISSILE_TIER_1_ITEMS,
        GeneratorCategory::MissileTier2 => &MISSILE_TIER_2_ITEMS,
        GeneratorCategory::MissileTier3 => &MISSILE_TIER_3_ITEMS,
        GeneratorCategory::MissileTier4 => &MISSILE_TIER_4_ITEMS,
        GeneratorCategory::MissileTier5 => &MISSILE_TIER_5_ITEMS,
        _ => {
            return Err(GeneratorError::IdentityIndexOutOfRange { category, index });
        }
    };
    identities
        .get(index)
        .copied()
        .ok_or(GeneratorError::IdentityIndexOutOfRange { category, index })
}

fn ring_identity(index: usize) -> Result<RingKind, GeneratorError> {
    RING_ITEMS
        .get(index)
        .copied()
        .ok_or(GeneratorError::IdentityIndexOutOfRange {
            category: GeneratorCategory::Ring,
            index,
        })
}

fn trinket_identity(index: usize) -> Result<TrinketKind, GeneratorError> {
    TRINKET_ITEMS
        .get(index)
        .copied()
        .ok_or(GeneratorError::IdentityIndexOutOfRange {
            category: GeneratorCategory::Trinket,
            index,
        })
}

fn food_identity(index: usize) -> Result<FoodKind, GeneratorError> {
    FOOD_ITEMS
        .get(index)
        .copied()
        .ok_or(GeneratorError::IdentityIndexOutOfRange {
            category: GeneratorCategory::Food,
            index,
        })
}

fn potion_identity(index: usize) -> Result<PotionKind, GeneratorError> {
    POTION_ITEMS
        .get(index)
        .copied()
        .ok_or(GeneratorError::IdentityIndexOutOfRange {
            category: GeneratorCategory::Potion,
            index,
        })
}

fn seed_identity(index: usize) -> Result<SeedKind, GeneratorError> {
    SEED_ITEMS
        .get(index)
        .copied()
        .ok_or(GeneratorError::IdentityIndexOutOfRange {
            category: GeneratorCategory::Seed,
            index,
        })
}

fn scroll_identity(index: usize) -> Result<ScrollKind, GeneratorError> {
    SCROLL_ITEMS
        .get(index)
        .copied()
        .ok_or(GeneratorError::IdentityIndexOutOfRange {
            category: GeneratorCategory::Scroll,
            index,
        })
}

fn stone_identity(index: usize) -> Result<StoneKind, GeneratorError> {
    STONE_ITEMS
        .get(index)
        .copied()
        .ok_or(GeneratorError::IdentityIndexOutOfRange {
            category: GeneratorCategory::Stone,
            index,
        })
}

#[cfg(test)]
mod tests {
    use crate::catalog::{ArmorEffect, Effect, ItemId, WeaponEffect};
    use crate::equipment::EquipmentRoll;
    use crate::rng::RandomStack;
    use crate::run::{GeneratorCategory, PotionKind, RingKind, RunState, ScrollKind};

    use super::{
        ArtifactKind, BombKind, FoodKind, GENERATOR_CATEGORIES, GeneratedArtifact,
        GeneratedEquipment, GeneratedItem, GeneratedMissile, GeneratedRing, MissileKind, SeedKind,
        StoneKind, TrinketKind, general_reset, random_armor, random_bomb, random_category,
        random_gold, random_target, random_using_defaults, random_wand, random_weapon,
        select_overall_category, undo_drop,
    };

    const ABC_SEED: i64 = 8_687_205_886;

    fn outer(seed: i64) -> RandomStack {
        let mut random = RandomStack::with_base_seed(0);
        random.push(seed);
        random
    }

    fn expected(
        item: ItemId,
        upgrade: u8,
        cursed: bool,
        effect: Option<Effect>,
    ) -> GeneratedEquipment {
        GeneratedEquipment {
            item,
            roll: EquipmentRoll {
                upgrade,
                effect,
                cursed,
            },
        }
    }

    #[test]
    fn nonzero_missile_classes_map_to_searchable_weapon_ids() {
        assert_eq!(MissileKind::Dart.item_id(), None);
        assert_eq!(
            MissileKind::ThrowingStone.item_id(),
            Some(ItemId::ThrowingStone)
        );
        assert_eq!(
            MissileKind::HeavyBoomerang.item_id(),
            Some(ItemId::HeavyBoomerang)
        );
        assert_eq!(MissileKind::ForceCube.item_id(), Some(ItemId::ForceCube));
    }

    #[test]
    fn all_upstream_seed_types_map_to_their_tipped_dart_identities() {
        let mappings = [
            (SeedKind::Rotberry, ItemId::RotDart),
            (SeedKind::Sungrass, ItemId::HealingDart),
            (SeedKind::Fadeleaf, ItemId::DisplacingDart),
            (SeedKind::Icecap, ItemId::ChillingDart),
            (SeedKind::Firebloom, ItemId::IncendiaryDart),
            (SeedKind::Sorrowmoss, ItemId::PoisonDart),
            (SeedKind::Swiftthistle, ItemId::AdrenalineDart),
            (SeedKind::Blindweed, ItemId::BlindingDart),
            (SeedKind::Stormvine, ItemId::ShockingDart),
            (SeedKind::Earthroot, ItemId::ParalyticDart),
            (SeedKind::Mageroyal, ItemId::CleansingDart),
            (SeedKind::Starflower, ItemId::HolyDart),
        ];
        for (seed, item) in mappings {
            assert_eq!(seed.tipped_dart_item_id(), item);
            assert_eq!(
                GeneratedItem::TippedDart { seed, quantity: 2 }.searchable_equipment(),
                Some(expected(item, 0, false, None))
            );
        }
    }

    #[test]
    fn mixed_target_sequence_matches_actual_java_generator() {
        let mut generator = RunState::new(ABC_SEED).generator;
        let mut random = outer(1_234);
        assert_eq!(
            random_weapon(&mut random, &mut generator, 0, false).unwrap(),
            expected(
                ItemId::Sword,
                1,
                true,
                Some(Effect::Weapon(WeaponEffect::Sacrificial))
            )
        );
        assert_eq!(
            random_wand(&mut random, &mut generator, false).unwrap(),
            expected(ItemId::WandMagicMissile, 1, false, None)
        );
        assert_eq!(
            random_armor(&mut random, 0).unwrap(),
            expected(ItemId::MailArmor, 1, false, None)
        );
        assert_eq!(
            random_weapon(&mut random, &mut generator, 0, false).unwrap(),
            expected(ItemId::Quarterstaff, 1, false, None)
        );
        assert_eq!(
            random_wand(&mut random, &mut generator, false).unwrap(),
            expected(ItemId::WandTransfusion, 0, false, None)
        );
        assert_eq!(
            random_weapon(&mut random, &mut generator, 4, false).unwrap(),
            expected(
                ItemId::Crossbow,
                0,
                true,
                Some(Effect::Weapon(WeaponEffect::Annoying))
            )
        );
    }

    #[test]
    fn forced_overall_target_paths_match_actual_java_generator() {
        let fixtures = [
            (
                GeneratorCategory::Weapon,
                expected(
                    ItemId::Quarterstaff,
                    0,
                    true,
                    Some(Effect::Weapon(WeaponEffect::Sacrificial)),
                ),
            ),
            (
                GeneratorCategory::Armor,
                expected(
                    ItemId::LeatherArmor,
                    0,
                    true,
                    Some(Effect::Armor(ArmorEffect::Multiplicity)),
                ),
            ),
            (
                GeneratorCategory::Wand,
                expected(ItemId::WandMagicMissile, 0, false, None),
            ),
        ];
        for (category, expected) in fixtures {
            let mut generator = RunState::new(ABC_SEED).generator;
            generator.category_probabilities.fill(0.0);
            generator.category_probabilities[category as usize] = 1.0;
            let mut random = outer(1_234);
            assert_eq!(
                random_target(&mut random, &mut generator, 0).unwrap(),
                expected
            );
            assert!(generator.category_probabilities[category as usize].abs() < f32::EPSILON);
        }
    }

    #[test]
    fn defaults_paths_use_outer_rng_and_do_not_mutate_category_decks() {
        let mut generator = RunState::new(ABC_SEED).generator;
        let before = generator.clone();
        let mut random = outer(1_234);
        assert_eq!(
            random_using_defaults(&mut random, &mut generator, GeneratorCategory::Weapon, 0)
                .unwrap(),
            GeneratedItem::Equipment(expected(
                ItemId::Mace,
                0,
                true,
                Some(Effect::Weapon(WeaponEffect::Sacrificial))
            ))
        );
        assert_eq!(
            random_using_defaults(&mut random, &mut generator, GeneratorCategory::Wand, 0).unwrap(),
            GeneratedItem::Equipment(expected(ItemId::WandFireblast, 1, false, None))
        );
        assert_eq!(
            random_using_defaults(&mut random, &mut generator, GeneratorCategory::Armor, 0)
                .unwrap(),
            GeneratedItem::Equipment(expected(ItemId::LeatherArmor, 0, false, None))
        );
        assert_eq!(generator, before);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn mixed_all_category_sequence_matches_actual_java_generator() {
        let mut generator = RunState::new(ABC_SEED).generator;
        let mut random = outer(2_468);
        let depth = 7;

        assert_eq!(
            random_category(
                &mut random,
                &mut generator,
                GeneratorCategory::Potion,
                depth
            )
            .unwrap(),
            GeneratedItem::Potion {
                kind: PotionKind::Invisibility,
                exotic: false
            }
        );
        assert_eq!(
            random_category(
                &mut random,
                &mut generator,
                GeneratorCategory::Scroll,
                depth
            )
            .unwrap(),
            GeneratedItem::Scroll {
                kind: ScrollKind::Terror,
                exotic: false
            }
        );
        assert_eq!(
            random_category(&mut random, &mut generator, GeneratorCategory::Ring, depth).unwrap(),
            GeneratedItem::Ring(GeneratedRing {
                kind: RingKind::Wealth,
                roll: EquipmentRoll {
                    upgrade: 1,
                    effect: None,
                    cursed: true
                }
            })
        );
        assert_eq!(
            super::random_missile(&mut random, &mut generator, 2, false).unwrap(),
            GeneratedMissile {
                kind: MissileKind::ThrowingSpear,
                quantity: 3,
                roll: EquipmentRoll {
                    upgrade: 1,
                    effect: Some(Effect::Weapon(WeaponEffect::Wayward)),
                    cursed: true
                }
            }
        );
        assert_eq!(
            random_category(&mut random, &mut generator, GeneratorCategory::Food, depth).unwrap(),
            GeneratedItem::Food(FoodKind::Pasty)
        );
        assert_eq!(
            random_category(&mut random, &mut generator, GeneratorCategory::Seed, depth).unwrap(),
            GeneratedItem::Seed(SeedKind::Sungrass)
        );
        assert_eq!(
            random_category(&mut random, &mut generator, GeneratorCategory::Stone, depth).unwrap(),
            GeneratedItem::Stone(StoneKind::Clairvoyance)
        );
        assert_eq!(
            random_category(&mut random, &mut generator, GeneratorCategory::Gold, depth).unwrap(),
            GeneratedItem::Gold { quantity: 141 }
        );
        assert_eq!(
            random_category(
                &mut random,
                &mut generator,
                GeneratorCategory::Trinket,
                depth
            )
            .unwrap(),
            GeneratedItem::Trinket(TrinketKind::VialOfBlood)
        );
        assert_eq!(
            random_category(
                &mut random,
                &mut generator,
                GeneratorCategory::Artifact,
                depth
            )
            .unwrap(),
            GeneratedItem::Artifact(GeneratedArtifact {
                kind: ArtifactKind::TalismanOfForesight,
                cursed: false,
                spellbook_scrolls: None
            })
        );

        let defaults = [
            (
                GeneratorCategory::Potion,
                GeneratedItem::Potion {
                    kind: PotionKind::ToxicGas,
                    exotic: false,
                },
            ),
            (
                GeneratorCategory::Scroll,
                GeneratedItem::Scroll {
                    kind: ScrollKind::Identify,
                    exotic: false,
                },
            ),
            (
                GeneratorCategory::Ring,
                GeneratedItem::Ring(GeneratedRing {
                    kind: RingKind::Energy,
                    roll: EquipmentRoll {
                        upgrade: 2,
                        effect: None,
                        cursed: false,
                    },
                }),
            ),
            (
                GeneratorCategory::Food,
                GeneratedItem::Food(FoodKind::Ration),
            ),
            (
                GeneratorCategory::Seed,
                GeneratedItem::Seed(SeedKind::Sungrass),
            ),
            (
                GeneratorCategory::Stone,
                GeneratedItem::Stone(StoneKind::Aggression),
            ),
            (
                GeneratorCategory::Gold,
                GeneratedItem::Gold { quantity: 158 },
            ),
        ];
        for (category, expected) in defaults {
            assert_eq!(
                random_using_defaults(&mut random, &mut generator, category, depth).unwrap(),
                expected
            );
        }
        assert_eq!(
            random_bomb(&mut random),
            GeneratedItem::Bomb(BombKind::Bomb)
        );
        assert_eq!(
            random_gold(&mut random, depth),
            GeneratedItem::Gold { quantity: 151 }
        );
        assert_eq!(
            super::random(&mut random, &mut generator, depth).unwrap(),
            GeneratedItem::Missile(GeneratedMissile {
                kind: MissileKind::ThrowingClub,
                quantity: 3,
                roll: EquipmentRoll {
                    upgrade: 0,
                    effect: None,
                    cursed: false
                }
            })
        );
        assert_eq!(random.long(), -6_226_261_600_093_983_228);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn artifact_uniqueness_spellbook_constructor_and_ring_fallback_match_java() {
        let mut generator = RunState::new(ABC_SEED).generator;
        let mut random = outer(97_531);
        let depth = 12;
        let expected_artifacts = [
            (ArtifactKind::TalismanOfForesight, false),
            (ArtifactKind::SkeletonKey, false),
            (ArtifactKind::AlchemistsToolkit, true),
            (ArtifactKind::MasterThievesArmband, false),
            (ArtifactKind::HornOfPlenty, false),
            (ArtifactKind::TimekeepersHourglass, false),
            (ArtifactKind::ChaliceOfBlood, false),
            (ArtifactKind::DriedRose, true),
        ];
        for (kind, cursed) in expected_artifacts {
            assert_eq!(
                random_category(
                    &mut random,
                    &mut generator,
                    GeneratorCategory::Artifact,
                    depth
                )
                .unwrap(),
                GeneratedItem::Artifact(GeneratedArtifact {
                    kind,
                    cursed,
                    spellbook_scrolls: None
                })
            );
        }
        assert_eq!(
            random_category(
                &mut random,
                &mut generator,
                GeneratorCategory::Artifact,
                depth
            )
            .unwrap(),
            GeneratedItem::Artifact(GeneratedArtifact {
                kind: ArtifactKind::UnstableSpellbook,
                cursed: true,
                spellbook_scrolls: Some([
                    ScrollKind::Retribution,
                    ScrollKind::Terror,
                    ScrollKind::Recharging,
                    ScrollKind::RemoveCurse,
                    ScrollKind::Identify,
                    ScrollKind::MagicMapping,
                    ScrollKind::Lullaby,
                    ScrollKind::MirrorImage,
                    ScrollKind::Rage,
                    ScrollKind::Teleportation,
                ])
            })
        );
        for (kind, cursed) in [
            (ArtifactKind::EtherealChains, false),
            (ArtifactKind::SandalsOfNature, false),
        ] {
            assert_eq!(
                random_category(
                    &mut random,
                    &mut generator,
                    GeneratorCategory::Artifact,
                    depth
                )
                .unwrap(),
                GeneratedItem::Artifact(GeneratedArtifact {
                    kind,
                    cursed,
                    spellbook_scrolls: None
                })
            );
        }
        assert_eq!(
            random_category(
                &mut random,
                &mut generator,
                GeneratorCategory::Artifact,
                depth
            )
            .unwrap(),
            GeneratedItem::Ring(GeneratedRing {
                kind: RingKind::Wealth,
                roll: EquipmentRoll {
                    upgrade: 0,
                    effect: None,
                    cursed: true
                }
            })
        );
        assert_eq!(
            random_category(
                &mut random,
                &mut generator,
                GeneratorCategory::Artifact,
                depth
            )
            .unwrap(),
            GeneratedItem::Ring(GeneratedRing {
                kind: RingKind::Accuracy,
                roll: EquipmentRoll {
                    upgrade: 2,
                    effect: None,
                    cursed: false
                }
            })
        );
        assert_eq!(random.long(), -3_901_995_299_017_043_590);
        assert_eq!(generator.category(GeneratorCategory::Artifact).dropped, 13);
        assert_eq!(generator.category(GeneratorCategory::Ring).dropped, 2);
    }

    #[test]
    fn wand_private_deck_replay_matches_java_long_skip_fixture() {
        let mut generator = RunState::new(ABC_SEED).generator;
        let mut random = outer(-987_654_321);
        let expected_rolls = [
            expected(ItemId::WandMagicMissile, 0, false, None),
            expected(ItemId::WandTransfusion, 0, true, None),
            expected(ItemId::WandWarding, 0, true, None),
            expected(ItemId::WandTransfusion, 0, false, None),
            expected(ItemId::WandCorruption, 1, true, None),
            expected(ItemId::WandFrost, 0, false, None),
            expected(ItemId::WandMagicMissile, 0, true, None),
            expected(ItemId::WandBlastWave, 0, false, None),
        ];
        for expected in expected_rolls {
            assert_eq!(
                random_wand(&mut random, &mut generator, false).unwrap(),
                expected
            );
        }
        let wand = generator.category(GeneratorCategory::Wand);
        assert_eq!(wand.dropped, 8);
        assert!((wand.probabilities.as_slice().iter().sum::<f32>() - 31.0).abs() < f32::EPSILON);
    }

    #[test]
    fn exhausted_wand_deck_resets_but_seed_replay_keeps_advancing() {
        let mut generator = RunState::new(ABC_SEED).generator;
        let mut random = outer(999);
        let mut tail = Vec::new();
        for index in 0..42 {
            let generated = random_wand(&mut random, &mut generator, false).unwrap();
            if index >= 37 {
                tail.push(generated.item);
            }
        }
        assert_eq!(
            tail,
            [
                ItemId::WandFrost,
                ItemId::WandCorruption,
                ItemId::WandFrost,
                ItemId::WandFrost,
                ItemId::WandMagicMissile,
            ]
        );
        let wand = generator.category(GeneratorCategory::Wand);
        assert_eq!(wand.dropped, 42);
        assert!((wand.probabilities.as_slice().iter().sum::<f32>() - 36.0).abs() < f32::EPSILON);
    }

    #[test]
    fn overall_35_card_deck_exhausts_then_toggles() {
        let mut generator = RunState::new(ABC_SEED).generator;
        assert!(!generator.using_first_deck);
        let expected = generator.category_probabilities;
        let mut counts = [0_u8; 23];
        let mut random = outer(77);
        for _ in 0..35 {
            let category = select_overall_category(&mut random, &mut generator);
            counts[category as usize] += 1;
        }
        for (index, probability) in expected.into_iter().enumerate() {
            assert!((f32::from(counts[index]) - probability).abs() < f32::EPSILON);
        }
        assert!(
            generator
                .category_probabilities
                .iter()
                .all(|value| *value == 0.0)
        );

        let selected = select_overall_category(&mut random, &mut generator);
        assert!(generator.using_first_deck);
        assert!(
            (generator.category_probabilities[selected as usize]
                - (selected.first_probability() - 1.0))
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn overall_selection_uses_linked_map_negative_weight_semantics() {
        let mut generator = RunState::new(ABC_SEED).generator;
        generator.category_probabilities.fill(0.0);
        generator.category_probabilities[GeneratorCategory::Trinket as usize] = -1.0;
        generator.category_probabilities[GeneratorCategory::Weapon as usize] = 1.0;
        generator.category_probabilities[GeneratorCategory::Armor as usize] = 1.0;
        let mut random = outer(0);
        assert_eq!(
            select_overall_category(&mut random, &mut generator),
            GeneratorCategory::Armor
        );
    }

    #[test]
    fn non_target_overall_result_is_fully_randomized() {
        let mut generator = RunState::new(ABC_SEED).generator;
        generator.using_first_deck = false;
        generator.category_probabilities.fill(0.0);
        generator.category_probabilities[GeneratorCategory::Gold as usize] = 1.0;
        let mut random = outer(0);
        assert!(matches!(
            super::random(&mut random, &mut generator, 0).unwrap(),
            GeneratedItem::Gold { .. }
        ));
        assert!(
            generator.category_probabilities[GeneratorCategory::Gold as usize].abs() < f32::EPSILON
        );
    }

    #[test]
    fn concrete_equipment_undo_drop_preserves_upstream_no_op_quirk() {
        let mut generator = RunState::new(ABC_SEED).generator;
        let mut random = outer(5);
        let _ = random_wand(&mut random, &mut generator, false).unwrap();
        let before = generator.clone();
        undo_drop(&mut generator, ItemId::WandFrost);
        assert_eq!(generator, before);
    }

    #[test]
    fn general_reset_uses_selected_deck_without_toggling_it() {
        let mut generator = RunState::new(ABC_SEED).generator;
        generator.using_first_deck = true;
        generator.category_probabilities.fill(-10.0);
        general_reset(&mut generator);
        for category in GENERATOR_CATEGORIES {
            assert!(
                (generator.category_probabilities[category as usize]
                    - category.first_probability())
                .abs()
                    < f32::EPSILON
            );
        }
    }
}
