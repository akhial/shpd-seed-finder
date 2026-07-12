//! Canonical v3.3.8 run initialization.
//!
//! `Dungeon.init()` performs this work under a generator seeded with
//! `Dungeon.seed + 1`, in the exact order reproduced by [`RunState::new`].
//! The generated state is intentionally data-only so each seed-search worker
//! can own an independent copy without touching process-global state.

use crate::challenges::Challenges;
use crate::rng::RandomStack;

/// Number of entries in `Generator.Category.values()` in v3.3.8.
pub const GENERATOR_CATEGORY_COUNT: usize = 23;

/// Largest per-category item table (`TRINKET`, with 17 entries).
pub const MAX_GENERATOR_CATEGORY_ITEMS: usize = 17;

const APPEARANCE_COUNT: usize = 12;

/// Scroll classes in `Generator.Category.SCROLL.classes` order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ScrollKind {
    Upgrade,
    Identify,
    RemoveCurse,
    MirrorImage,
    Recharging,
    Teleportation,
    Lullaby,
    MagicMapping,
    Rage,
    Retribution,
    Terror,
    Transmutation,
}

/// Rune labels in the insertion order of `Scroll.runes`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ScrollLabel {
    Kaunan,
    Sowilo,
    Laguz,
    Yngvi,
    Gyfu,
    Raido,
    Isaz,
    Mannaz,
    Naudiz,
    Berkanan,
    Odal,
    Tiwaz,
}

/// Potion classes in `Generator.Category.POTION.classes` order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum PotionKind {
    Strength,
    Healing,
    MindVision,
    Frost,
    LiquidFlame,
    ToxicGas,
    Haste,
    Invisibility,
    Levitation,
    ParalyticGas,
    Purity,
    Experience,
}

/// Potion colors in the insertion order of `Potion.colors`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum PotionColor {
    Crimson,
    Amber,
    Golden,
    Jade,
    Turquoise,
    Azure,
    Indigo,
    Magenta,
    Bistre,
    Charcoal,
    Silver,
    Ivory,
}

/// Ring classes in `Generator.Category.RING.classes` order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum RingKind {
    Accuracy,
    Arcana,
    Elements,
    Energy,
    Evasion,
    Force,
    Furor,
    Haste,
    Might,
    Sharpshooting,
    Tenacity,
    Wealth,
}

/// Ring gems in the insertion order of `Ring.gems`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum RingGem {
    Garnet,
    Ruby,
    Topaz,
    Emerald,
    Onyx,
    Opal,
    Tourmaline,
    Sapphire,
    Amethyst,
    Quartz,
    Agate,
    Diamond,
}

/// The three per-run item-appearance permutations.
///
/// Each array is indexed by its corresponding `*Kind` enum and contains the
/// appearance assigned to that item class.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemAppearanceState {
    pub scroll_labels: [ScrollLabel; APPEARANCE_COUNT],
    pub potion_colors: [PotionColor; APPEARANCE_COUNT],
    pub ring_gems: [RingGem; APPEARANCE_COUNT],
}

impl ItemAppearanceState {
    /// Returns the rune assigned to a scroll class.
    #[must_use]
    pub const fn scroll_label(&self, kind: ScrollKind) -> ScrollLabel {
        self.scroll_labels[kind as usize]
    }

    /// Returns the color assigned to a potion class.
    #[must_use]
    pub const fn potion_color(&self, kind: PotionKind) -> PotionColor {
        self.potion_colors[kind as usize]
    }

    /// Returns the gem assigned to a ring class.
    #[must_use]
    pub const fn ring_gem(&self, kind: RingKind) -> RingGem {
        self.ring_gems[kind as usize]
    }
}

/// The equipment-heavy source list in `SpecialRoom.EQUIP_SPECIALS` order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum EquipmentSpecialRoom {
    WeakFloor,
    Crypt,
    Pool,
    Armory,
    Sentry,
    Statue,
    CrystalVault,
    CrystalChoice,
    Sacrifice,
}

/// The consumable-heavy source list in `SpecialRoom.CONSUMABLE_SPECIALS` order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ConsumableSpecialRoom {
    Runestone,
    Garden,
    Library,
    Storage,
    Treasury,
    MagicWell,
    ToxicGas,
    MagicalFire,
    Traps,
    CrystalPath,
}

/// One entry in the interleaved per-run special-room queue.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SpecialRoomKind {
    Equipment(EquipmentSpecialRoom),
    Consumable(ConsumableSpecialRoom),
}

/// State established by `SpecialRoom.initForRun()`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecialRoomState {
    /// The shuffled equipment list before interleaving.
    pub equipment_order: [EquipmentSpecialRoom; 9],
    /// The shuffled consumable list before interleaving.
    pub consumable_order: [ConsumableSpecialRoom; 10],
    /// The actual queue used by later floor generation.
    pub run_specials: [SpecialRoomKind; 19],
    /// `SpecialRoom.pitNeededDepth`, reset for a new run.
    pub pit_needed_depth: i32,
}

/// Secret-room classes in `SecretRoom.ALL_SECRETS` order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum SecretRoomKind {
    Garden,
    Laboratory,
    Library,
    Larder,
    Well,
    Runestone,
    Artillery,
    ChestChasm,
    Honeypot,
    Hoard,
    Maze,
    Summoning,
}

/// State established by `SecretRoom.initForRun()`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretRoomState {
    /// Remaining secret-room counts for regions 0 through 4.
    pub region_secrets: [i32; 5],
    /// The shuffled rotating secret-room queue.
    pub run_secrets: [SecretRoomKind; 12],
}

/// `Generator.Category` declaration order. This order is observable through
/// both `values()` and the generator's `LinkedHashMap` probability decks.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum GeneratorCategory {
    Trinket,
    Weapon,
    WeaponTier1,
    WeaponTier2,
    WeaponTier3,
    WeaponTier4,
    WeaponTier5,
    Armor,
    Missile,
    MissileTier1,
    MissileTier2,
    MissileTier3,
    MissileTier4,
    MissileTier5,
    Wand,
    Ring,
    Artifact,
    Food,
    Potion,
    Seed,
    Scroll,
    Stone,
    Gold,
}

/// Exact `Generator.Category.values()` order.
pub const GENERATOR_CATEGORIES: [GeneratorCategory; GENERATOR_CATEGORY_COUNT] = [
    GeneratorCategory::Trinket,
    GeneratorCategory::Weapon,
    GeneratorCategory::WeaponTier1,
    GeneratorCategory::WeaponTier2,
    GeneratorCategory::WeaponTier3,
    GeneratorCategory::WeaponTier4,
    GeneratorCategory::WeaponTier5,
    GeneratorCategory::Armor,
    GeneratorCategory::Missile,
    GeneratorCategory::MissileTier1,
    GeneratorCategory::MissileTier2,
    GeneratorCategory::MissileTier3,
    GeneratorCategory::MissileTier4,
    GeneratorCategory::MissileTier5,
    GeneratorCategory::Wand,
    GeneratorCategory::Ring,
    GeneratorCategory::Artifact,
    GeneratorCategory::Food,
    GeneratorCategory::Potion,
    GeneratorCategory::Seed,
    GeneratorCategory::Scroll,
    GeneratorCategory::Stone,
    GeneratorCategory::Gold,
];

impl GeneratorCategory {
    /// Probability in the first overall 35-card category deck.
    #[must_use]
    pub const fn first_probability(self) -> f32 {
        match self {
            Self::Weapon | Self::Armor => 2.0,
            Self::Missile | Self::Wand | Self::Ring | Self::Seed | Self::Stone => 1.0,
            Self::Potion | Self::Scroll => 8.0,
            Self::Gold => 10.0,
            Self::Trinket
            | Self::WeaponTier1
            | Self::WeaponTier2
            | Self::WeaponTier3
            | Self::WeaponTier4
            | Self::WeaponTier5
            | Self::MissileTier1
            | Self::MissileTier2
            | Self::MissileTier3
            | Self::MissileTier4
            | Self::MissileTier5
            | Self::Artifact
            | Self::Food => 0.0,
        }
    }

    /// Probability in the second overall 35-card category deck.
    #[must_use]
    pub const fn second_probability(self) -> f32 {
        match self {
            Self::Weapon | Self::Missile => 2.0,
            Self::Armor | Self::Wand | Self::Artifact | Self::Seed | Self::Stone => 1.0,
            Self::Potion | Self::Scroll => 8.0,
            Self::Gold => 10.0,
            Self::Trinket
            | Self::WeaponTier1
            | Self::WeaponTier2
            | Self::WeaponTier3
            | Self::WeaponTier4
            | Self::WeaponTier5
            | Self::MissileTier1
            | Self::MissileTier2
            | Self::MissileTier3
            | Self::MissileTier4
            | Self::MissileTier5
            | Self::Ring
            | Self::Food => 0.0,
        }
    }

    /// Primary item deck, or `None` for categories without a decrementing deck.
    #[must_use]
    pub const fn primary_probabilities(self) -> Option<&'static [f32]> {
        match self {
            Self::Trinket => Some(&TRINKET_PROBS),
            Self::WeaponTier1 => Some(&WEAPON_TIER_1_PROBS),
            Self::WeaponTier2 => Some(&WEAPON_TIER_2_PROBS),
            Self::WeaponTier3 => Some(&WEAPON_TIER_3_PROBS),
            Self::WeaponTier4 | Self::WeaponTier5 => Some(&WEAPON_TIER_4_AND_5_PROBS),
            Self::MissileTier1 => Some(&MISSILE_TIER_1_PROBS),
            Self::MissileTier2 | Self::MissileTier3 | Self::MissileTier4 | Self::MissileTier5 => {
                Some(&MISSILE_TIER_2_TO_5_PROBS)
            }
            Self::Wand => Some(&WAND_PROBS),
            Self::Ring => Some(&RING_PROBS),
            Self::Artifact => Some(&ARTIFACT_PROBS),
            Self::Food => Some(&FOOD_PROBS),
            Self::Potion => Some(&POTION_PROBS_1),
            Self::Seed => Some(&SEED_PROBS),
            Self::Scroll => Some(&SCROLL_PROBS_1),
            Self::Stone => Some(&STONE_PROBS),
            Self::Weapon | Self::Armor | Self::Missile | Self::Gold => None,
        }
    }

    /// Alternate item deck. Only potions and scrolls have one in v3.3.8.
    #[must_use]
    pub const fn alternate_probabilities(self) -> Option<&'static [f32]> {
        match self {
            Self::Potion => Some(&POTION_PROBS_2),
            Self::Scroll => Some(&SCROLL_PROBS_2),
            _ => None,
        }
    }

    /// Upstream `defaultProbsTotal`, used when a two-deck category is generated
    /// without decrementing either current deck.
    #[must_use]
    pub const fn default_probabilities_total(self) -> Option<&'static [f32]> {
        match self {
            Self::Potion => Some(&POTION_PROBS_TOTAL),
            Self::Scroll => Some(&SCROLL_PROBS_TOTAL),
            _ => None,
        }
    }

    const fn non_deck_probabilities(self) -> &'static [f32] {
        match self {
            Self::Armor => &ARMOR_PROBS,
            Self::Gold => &GOLD_PROBS,
            _ => &[],
        }
    }
}

/// Fixed-capacity probability deck used to keep per-seed state allocation-free.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbabilityDeck {
    values: [f32; MAX_GENERATOR_CATEGORY_ITEMS],
    len: u8,
}

impl ProbabilityDeck {
    fn from_slice(values: &[f32]) -> Self {
        assert!(values.len() <= MAX_GENERATOR_CATEGORY_ITEMS);
        let mut result = [0.0; MAX_GENERATOR_CATEGORY_ITEMS];
        result[..values.len()].copy_from_slice(values);
        Self {
            values: result,
            len: u8::try_from(values.len()).unwrap_or_default(),
        }
    }

    /// Returns only the category's meaningful entries, in upstream class order.
    #[must_use]
    pub fn as_slice(&self) -> &[f32] {
        &self.values[..usize::from(self.len)]
    }

    /// Mutable remaining weights for later item generation.
    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.values[..usize::from(self.len)]
    }

    /// Number of item classes in this category.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    /// Whether this category has no direct class table.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Mutable state for one generator category.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeneratorCategoryState {
    pub category: GeneratorCategory,
    /// The current per-item deck after `Generator.reset(category)`.
    pub probabilities: ProbabilityDeck,
    /// Whether `probabilities` is the alternate deck.
    pub using_second_probabilities: bool,
    /// Private deterministic generator seed used for item identity draws.
    pub seed: Option<i64>,
    /// Number of identity draws already made from `seed`.
    pub dropped: i32,
}

impl GeneratorCategoryState {
    /// Mirrors `Generator.reset(category)`, including alternating the two
    /// potion/scroll decks rather than always returning to the primary one.
    pub fn reset(&mut self) {
        let Some(primary) = self.category.primary_probabilities() else {
            return;
        };
        self.probabilities = if let Some(alternate) = self.category.alternate_probabilities() {
            self.using_second_probabilities = !self.using_second_probabilities;
            if self.using_second_probabilities {
                ProbabilityDeck::from_slice(alternate)
            } else {
                ProbabilityDeck::from_slice(primary)
            }
        } else {
            ProbabilityDeck::from_slice(primary)
        };
    }
}

/// State established by `Generator.fullReset()` and `generalReset()`.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratorState {
    pub using_first_deck: bool,
    /// Remaining weights in the selected overall category deck.
    pub category_probabilities: [f32; GENERATOR_CATEGORY_COUNT],
    /// Combined first+second weights used by `randomUsingDefaults()`.
    pub default_category_probabilities: [f32; GENERATOR_CATEGORY_COUNT],
    /// Per-category state, strictly in [`GENERATOR_CATEGORIES`] order.
    pub categories: [GeneratorCategoryState; GENERATOR_CATEGORY_COUNT],
}

impl GeneratorState {
    /// Fetches category state without a map lookup. The enum discriminants and
    /// vector order both match Java's declaration order.
    #[must_use]
    pub fn category(&self, category: GeneratorCategory) -> &GeneratorCategoryState {
        &self.categories[category as usize]
    }

    /// Mutable form of [`Self::category`].
    #[must_use]
    pub fn category_mut(&mut self, category: GeneratorCategory) -> &mut GeneratorCategoryState {
        &mut self.categories[category as usize]
    }
}

/// All run-global state produced while the `seed + 1` generator is active.
#[derive(Clone, Debug, PartialEq)]
pub struct RunState {
    pub dungeon_seed: i64,
    pub challenges: Challenges,
    pub appearances: ItemAppearanceState,
    pub special_rooms: SpecialRoomState,
    pub secret_rooms: SecretRoomState,
    pub generator: GeneratorState,
}

impl RunState {
    /// Mirrors the deterministic portion of `Dungeon.init()` for v3.3.8.
    #[must_use]
    pub fn new(dungeon_seed: i64) -> Self {
        Self::with_challenges(dungeon_seed, Challenges::NONE)
    }

    /// Mirrors the deterministic portion of `Dungeon.init()` with a validated
    /// v3.3.8 challenge mask.
    #[must_use]
    pub fn with_challenges(dungeon_seed: i64, challenges: Challenges) -> Self {
        let mut random = RandomStack::with_base_seed(0);
        random.push(dungeon_seed.wrapping_add(1));

        // Dungeon.init() calls these in this exact order.
        let appearances = init_appearances(&mut random);
        let special_rooms = init_special_rooms(&mut random);
        let secret_rooms = init_secret_rooms(&mut random);
        let generator = init_generator(&mut random);

        random.pop();
        Self {
            dungeon_seed,
            challenges,
            appearances,
            special_rooms,
            secret_rooms,
            generator,
        }
    }
}

/// Convenience wrapper around [`RunState::new`].
#[must_use]
pub fn initialize_run(dungeon_seed: i64) -> RunState {
    RunState::new(dungeon_seed)
}

fn init_appearances(random: &mut RandomStack) -> ItemAppearanceState {
    let scroll_labels = draw_item_status_permutation(
        random,
        [
            ScrollLabel::Kaunan,
            ScrollLabel::Sowilo,
            ScrollLabel::Laguz,
            ScrollLabel::Yngvi,
            ScrollLabel::Gyfu,
            ScrollLabel::Raido,
            ScrollLabel::Isaz,
            ScrollLabel::Mannaz,
            ScrollLabel::Naudiz,
            ScrollLabel::Berkanan,
            ScrollLabel::Odal,
            ScrollLabel::Tiwaz,
        ],
    );
    let potion_colors = draw_item_status_permutation(
        random,
        [
            PotionColor::Crimson,
            PotionColor::Amber,
            PotionColor::Golden,
            PotionColor::Jade,
            PotionColor::Turquoise,
            PotionColor::Azure,
            PotionColor::Indigo,
            PotionColor::Magenta,
            PotionColor::Bistre,
            PotionColor::Charcoal,
            PotionColor::Silver,
            PotionColor::Ivory,
        ],
    );
    let ring_gems = draw_item_status_permutation(
        random,
        [
            RingGem::Garnet,
            RingGem::Ruby,
            RingGem::Topaz,
            RingGem::Emerald,
            RingGem::Onyx,
            RingGem::Opal,
            RingGem::Tourmaline,
            RingGem::Sapphire,
            RingGem::Amethyst,
            RingGem::Quartz,
            RingGem::Agate,
            RingGem::Diamond,
        ],
    );
    ItemAppearanceState {
        scroll_labels,
        potion_colors,
        ring_gems,
    }
}

fn draw_item_status_permutation<T: Copy, const N: usize>(
    random: &mut RandomStack,
    appearances: [T; N],
) -> [T; N] {
    let mut remaining = appearances;
    let mut remaining_len = N;
    // Do not special-case the last element: ItemStatusHandler calls
    // Random.Int(1), which still consumes a Java RNG draw.
    std::array::from_fn(|_| {
        let bound = i32::try_from(remaining_len).expect("fixed appearance list fits in i32");
        let index = usize::try_from(random.int_bound(bound)).unwrap_or_default();
        let selected = remaining[index];
        for move_index in index..remaining_len - 1 {
            remaining[move_index] = remaining[move_index + 1];
        }
        remaining_len -= 1;
        selected
    })
}

fn init_special_rooms(random: &mut RandomStack) -> SpecialRoomState {
    let mut equipment_order = [
        EquipmentSpecialRoom::WeakFloor,
        EquipmentSpecialRoom::Crypt,
        EquipmentSpecialRoom::Pool,
        EquipmentSpecialRoom::Armory,
        EquipmentSpecialRoom::Sentry,
        EquipmentSpecialRoom::Statue,
        EquipmentSpecialRoom::CrystalVault,
        EquipmentSpecialRoom::CrystalChoice,
        EquipmentSpecialRoom::Sacrifice,
    ];
    let mut consumable_order = [
        ConsumableSpecialRoom::Runestone,
        ConsumableSpecialRoom::Garden,
        ConsumableSpecialRoom::Library,
        ConsumableSpecialRoom::Storage,
        ConsumableSpecialRoom::Treasury,
        ConsumableSpecialRoom::MagicWell,
        ConsumableSpecialRoom::ToxicGas,
        ConsumableSpecialRoom::MagicalFire,
        ConsumableSpecialRoom::Traps,
        ConsumableSpecialRoom::CrystalPath,
    ];

    // Both upstream values are ArrayLists, so Collections.shuffle's reverse
    // Fisher-Yates form is required here (not Random.shuffle(Object[])).
    random.shuffle_list(&mut equipment_order);
    random.shuffle_list(&mut consumable_order);

    // Upstream starts with the sole extra consumable, then alternates lists.
    let mut run_specials = [SpecialRoomKind::Consumable(ConsumableSpecialRoom::Runestone); 19];
    run_specials[0] = SpecialRoomKind::Consumable(consumable_order[0]);
    for index in 0..equipment_order.len() {
        run_specials[1 + index * 2] = SpecialRoomKind::Equipment(equipment_order[index]);
        run_specials[2 + index * 2] = SpecialRoomKind::Consumable(consumable_order[index + 1]);
    }

    SpecialRoomState {
        equipment_order,
        consumable_order,
        run_specials,
        pit_needed_depth: -1,
    }
}

fn init_secret_rooms(random: &mut RandomStack) -> SecretRoomState {
    // Keep the five unconditional Float() calls. Regions with an integer base
    // still consume a roll before comparing it with a zero fractional chance.
    let base_region_secrets = [
        (2, 0.0_f32),
        (2, 0.25_f32),
        (2, 0.5_f32),
        (2, 0.75_f32),
        (3, 0.0_f32),
    ];
    let mut region_secrets = [0_i32; 5];
    for (index, (whole, extra_chance)) in base_region_secrets.into_iter().enumerate() {
        region_secrets[index] = whole;
        if random.float() < extra_chance {
            region_secrets[index] += 1;
        }
    }

    let mut run_secrets = [
        SecretRoomKind::Garden,
        SecretRoomKind::Laboratory,
        SecretRoomKind::Library,
        SecretRoomKind::Larder,
        SecretRoomKind::Well,
        SecretRoomKind::Runestone,
        SecretRoomKind::Artillery,
        SecretRoomKind::ChestChasm,
        SecretRoomKind::Honeypot,
        SecretRoomKind::Hoard,
        SecretRoomKind::Maze,
        SecretRoomKind::Summoning,
    ];
    random.shuffle_list(&mut run_secrets);

    SecretRoomState {
        region_secrets,
        run_secrets,
    }
}

fn init_generator(random: &mut RandomStack) -> GeneratorState {
    let using_first_deck = random.int_bound(2) == 0;
    let category_probabilities = std::array::from_fn(|index| {
        let category = GENERATOR_CATEGORIES[index];
        if using_first_deck {
            category.first_probability()
        } else {
            category.second_probability()
        }
    });
    let default_category_probabilities = std::array::from_fn(|index| {
        let category = GENERATOR_CATEGORIES[index];
        category.first_probability() + category.second_probability()
    });

    let categories = std::array::from_fn(|index| {
        let category = GENERATOR_CATEGORIES[index];
        // Java's short-circuit means this Int(2) exists only for POTION and
        // SCROLL. reset() then toggles that initial value once.
        let mut using_second_probabilities =
            category.alternate_probabilities().is_some() && random.int_bound(2) == 0;

        let probabilities = if let Some(primary) = category.primary_probabilities() {
            if let Some(alternate) = category.alternate_probabilities() {
                using_second_probabilities = !using_second_probabilities;
                if using_second_probabilities {
                    ProbabilityDeck::from_slice(alternate)
                } else {
                    ProbabilityDeck::from_slice(primary)
                }
            } else {
                ProbabilityDeck::from_slice(primary)
            }
        } else {
            ProbabilityDeck::from_slice(category.non_deck_probabilities())
        };

        let seed = category
            .primary_probabilities()
            .is_some()
            .then(|| random.long());
        GeneratorCategoryState {
            category,
            probabilities,
            using_second_probabilities,
            seed,
            dropped: 0,
        }
    });

    GeneratorState {
        using_first_deck,
        category_probabilities,
        default_category_probabilities,
        categories,
    }
}

const TRINKET_PROBS: [f32; 17] = [1.0; 17];
const WEAPON_TIER_1_PROBS: [f32; 6] = [2.0, 0.0, 2.0, 2.0, 2.0, 2.0];
const WEAPON_TIER_2_PROBS: [f32; 7] = [2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 0.0];
const WEAPON_TIER_3_PROBS: [f32; 6] = [2.0; 6];
const WEAPON_TIER_4_AND_5_PROBS: [f32; 7] = [2.0; 7];
const ARMOR_PROBS: [f32; 11] = [1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
const MISSILE_TIER_1_PROBS: [f32; 4] = [3.0, 3.0, 3.0, 0.0];
const MISSILE_TIER_2_TO_5_PROBS: [f32; 3] = [3.0; 3];
const WAND_PROBS: [f32; 13] = [3.0; 13];
const RING_PROBS: [f32; 12] = [3.0; 12];
const ARTIFACT_PROBS: [f32; 13] = [
    1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
];
const FOOD_PROBS: [f32; 3] = [4.0, 1.0, 0.0];
const POTION_PROBS_1: [f32; 12] = [0.0, 3.0, 2.0, 1.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
const POTION_PROBS_2: [f32; 12] = [0.0, 3.0, 2.0, 2.0, 1.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0];
const POTION_PROBS_TOTAL: [f32; 12] = [0.0, 6.0, 4.0, 3.0, 3.0, 3.0, 2.0, 2.0, 2.0, 2.0, 2.0, 1.0];
const SEED_PROBS: [f32; 12] = [0.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 1.0];
const SCROLL_PROBS_1: [f32; 12] = POTION_PROBS_1;
const SCROLL_PROBS_2: [f32; 12] = POTION_PROBS_2;
const SCROLL_PROBS_TOTAL: [f32; 12] = POTION_PROBS_TOTAL;
const STONE_PROBS: [f32; 12] = [0.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 0.0];
const GOLD_PROBS: [f32; 1] = [1.0];

#[cfg(test)]
mod tests {
    use super::{GeneratorCategory, RunState};

    const ABC_SEED: i64 = 8_687_205_886;

    #[test]
    fn abc_item_appearance_permutations_match_java_oracle() {
        let state = RunState::new(ABC_SEED);
        assert_eq!(
            state.appearances.scroll_labels.map(|value| value as u8),
            [11, 3, 7, 10, 9, 6, 5, 0, 1, 8, 2, 4]
        );
        assert_eq!(
            state.appearances.potion_colors.map(|value| value as u8),
            [10, 9, 5, 0, 4, 11, 7, 2, 8, 3, 6, 1]
        );
        assert_eq!(
            state.appearances.ring_gems.map(|value| value as u8),
            [7, 3, 9, 0, 2, 1, 5, 4, 8, 11, 6, 10]
        );
    }

    #[test]
    fn abc_special_and_secret_room_queues_match_java_oracle() {
        let state = RunState::new(ABC_SEED);
        assert_eq!(
            state.special_rooms.equipment_order.map(|value| value as u8),
            [6, 0, 2, 7, 8, 4, 1, 3, 5]
        );
        assert_eq!(
            state
                .special_rooms
                .consumable_order
                .map(|value| value as u8),
            [7, 4, 9, 8, 6, 0, 2, 3, 1, 5]
        );
        assert_eq!(state.secret_rooms.region_secrets, [2, 2, 3, 2, 3]);
        assert_eq!(
            state.secret_rooms.run_secrets.map(|value| value as u8),
            [11, 4, 0, 2, 1, 8, 9, 7, 6, 5, 3, 10]
        );
    }

    #[test]
    fn abc_generator_full_reset_matches_java_oracle() {
        let state = RunState::new(ABC_SEED);
        let generator = &state.generator;
        assert!(!generator.using_first_deck);
        let expected_seeds = [
            (GeneratorCategory::Trinket, -4_695_439_314_574_286_393),
            (GeneratorCategory::WeaponTier1, -7_087_463_195_500_793_105),
            (GeneratorCategory::WeaponTier2, 7_858_022_018_598_556_288),
            (GeneratorCategory::WeaponTier3, -6_775_965_674_471_277_851),
            (GeneratorCategory::WeaponTier4, -7_212_385_995_610_277_938),
            (GeneratorCategory::WeaponTier5, -7_403_109_313_941_917_054),
            (GeneratorCategory::MissileTier1, -9_170_872_269_705_539_489),
            (GeneratorCategory::MissileTier2, -7_434_748_464_558_411_409),
            (GeneratorCategory::MissileTier3, 2_783_876_392_130_401_091),
            (GeneratorCategory::MissileTier4, 1_424_271_270_849_567_865),
            (GeneratorCategory::MissileTier5, -2_780_000_151_338_487_926),
            (GeneratorCategory::Wand, -1_590_198_080_093_345_829),
            (GeneratorCategory::Ring, -8_087_589_864_901_533_141),
            (GeneratorCategory::Artifact, 3_683_885_015_942_691_267),
            (GeneratorCategory::Food, -8_109_206_354_888_079_833),
            (GeneratorCategory::Potion, 2_428_620_041_165_352_887),
            (GeneratorCategory::Seed, -7_900_717_903_334_316_398),
            (GeneratorCategory::Scroll, -6_020_804_758_004_630_272),
            (GeneratorCategory::Stone, -8_199_938_468_147_729_105),
        ];
        for (category, seed) in expected_seeds {
            assert_eq!(
                generator.category(category).seed,
                Some(seed),
                "{category:?}"
            );
        }
        for category in [
            GeneratorCategory::Weapon,
            GeneratorCategory::Armor,
            GeneratorCategory::Missile,
            GeneratorCategory::Gold,
        ] {
            assert_eq!(generator.category(category).seed, None, "{category:?}");
        }
        assert!(
            generator
                .categories
                .iter()
                .all(|category| category.dropped == 0)
        );
        assert!((generator.category_probabilities.iter().sum::<f32>() - 35.0).abs() < f32::EPSILON);
        assert!(
            (generator.default_category_probabilities.iter().sum::<f32>() - 70.0).abs()
                < f32::EPSILON
        );
        for category in [GeneratorCategory::Potion, GeneratorCategory::Scroll] {
            let deck = generator.category(category);
            assert!(!deck.using_second_probabilities);
            assert_eq!(
                deck.probabilities.as_slice(),
                category.primary_probabilities().unwrap()
            );
        }
    }

    #[test]
    fn alternate_deck_reset_toggles_instead_of_selecting_primary() {
        let mut state = RunState::new(ABC_SEED);
        for category in [GeneratorCategory::Potion, GeneratorCategory::Scroll] {
            let deck = state.generator.category_mut(category);
            let before = deck.using_second_probabilities;
            deck.reset();
            assert_ne!(deck.using_second_probabilities, before);
            assert_eq!(
                deck.probabilities.as_slice(),
                if deck.using_second_probabilities {
                    category.alternate_probabilities().unwrap()
                } else {
                    category.primary_probabilities().unwrap()
                }
            );
        }
    }

    #[test]
    fn two_deck_initial_selection_matches_java_short_circuit_and_toggle() {
        // This fixture selects POTION's alternate deck but SCROLL's primary
        // deck, exercising both outcomes of fullReset's Int(2) draws.
        let state = RunState::new(42);
        let potion = state.generator.category(GeneratorCategory::Potion);
        let scroll = state.generator.category(GeneratorCategory::Scroll);
        assert!(potion.using_second_probabilities);
        assert_eq!(
            potion.probabilities.as_slice(),
            GeneratorCategory::Potion.alternate_probabilities().unwrap()
        );
        assert!(!scroll.using_second_probabilities);
        assert_eq!(
            scroll.probabilities.as_slice(),
            GeneratorCategory::Scroll.primary_probabilities().unwrap()
        );
        assert_eq!(
            state.generator.category(GeneratorCategory::Stone).seed,
            Some(-1_826_233_252_457_039_333)
        );
    }
}
