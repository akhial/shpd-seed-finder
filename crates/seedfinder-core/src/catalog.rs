//! Stable, version-pinned catalog for searchable equipment.

/// Broad item family exposed by the query UI.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ItemKind {
    Weapon,
    Armor,
    Wand,
    Ring,
}

impl ItemKind {
    /// Highest exact upgrade accepted by the Android search UI for this family.
    #[must_use]
    pub const fn maximum_search_upgrade(self) -> u8 {
        match self {
            Self::Ring => 4,
            Self::Weapon | Self::Armor | Self::Wand => 3,
        }
    }
}

/// Stable identifiers for equipment that can be generated in a seeded world.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ItemId {
    WornShortsword,
    Cudgel,
    StuddedGloves,
    Rapier,
    Dagger,
    Shortsword,
    HandAxe,
    Spear,
    Quarterstaff,
    Dirk,
    Sickle,
    Sword,
    Mace,
    Scimitar,
    RoundShield,
    Sai,
    Whip,
    Longsword,
    BattleAxe,
    Flail,
    RunicBlade,
    AssassinsBlade,
    Crossbow,
    Katana,
    Greatsword,
    WarHammer,
    Glaive,
    Greataxe,
    Greatshield,
    StoneGauntlet,
    WarScythe,
    ThrowingStone,
    ThrowingKnife,
    ThrowingSpike,
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
    ClothArmor,
    LeatherArmor,
    MailArmor,
    ScaleArmor,
    PlateArmor,
    WandMagicMissile,
    WandFireblast,
    WandFrost,
    WandLightning,
    WandDisintegration,
    WandPrismaticLight,
    WandCorrosion,
    WandLivingEarth,
    WandBlastWave,
    WandCorruption,
    WandWarding,
    WandRegrowth,
    WandTransfusion,
    RotDart,
    IncendiaryDart,
    AdrenalineDart,
    HealingDart,
    ChillingDart,
    ShockingDart,
    PoisonDart,
    CleansingDart,
    ParalyticDart,
    HolyDart,
    DisplacingDart,
    BlindingDart,
    RingAccuracy,
    RingArcana,
    RingElements,
    RingEnergy,
    RingEvasion,
    RingForce,
    RingFuror,
    RingHaste,
    RingMight,
    RingSharpshooting,
    RingTenacity,
    RingWealth,
}

/// Static display and sprite data for one item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemDefinition {
    pub id: ItemId,
    pub stable_id: &'static str,
    pub name: &'static str,
    pub kind: ItemKind,
    /// Weapon/armor tier. Wands have no tier.
    pub tier: Option<u8>,
    /// Zero-based 16×16 cell in the upstream `items.png` atlas.
    pub sprite_index: u16,
}

macro_rules! item {
    ($variant:ident, $stable:literal, $name:literal, $kind:ident, $tier:expr, $sprite:expr) => {
        ItemDefinition {
            id: ItemId::$variant,
            stable_id: $stable,
            name: $name,
            kind: ItemKind::$kind,
            tier: $tier,
            sprite_index: $sprite,
        }
    };
}

/// Every non-zero-probability melee/thrown weapon, generic armor, wand, and ring in v3.3.8.
pub const ITEMS: &[ItemDefinition] = &[
    item!(
        WornShortsword,
        "worn_shortsword",
        "Worn shortsword",
        Weapon,
        Some(1),
        96
    ),
    item!(Cudgel, "cudgel", "Cudgel", Weapon, Some(1), 97),
    item!(
        StuddedGloves,
        "gloves",
        "Studded gloves",
        Weapon,
        Some(1),
        98
    ),
    item!(Rapier, "rapier", "Rapier", Weapon, Some(1), 99),
    item!(Dagger, "dagger", "Dagger", Weapon, Some(1), 100),
    item!(Shortsword, "shortsword", "Shortsword", Weapon, Some(2), 104),
    item!(HandAxe, "hand_axe", "Hand axe", Weapon, Some(2), 105),
    item!(Spear, "spear", "Spear", Weapon, Some(2), 106),
    item!(
        Quarterstaff,
        "quarterstaff",
        "Quarterstaff",
        Weapon,
        Some(2),
        107
    ),
    item!(Dirk, "dirk", "Dirk", Weapon, Some(2), 108),
    item!(Sickle, "sickle", "Sickle", Weapon, Some(2), 109),
    item!(Sword, "sword", "Sword", Weapon, Some(3), 112),
    item!(Mace, "mace", "Mace", Weapon, Some(3), 113),
    item!(Scimitar, "scimitar", "Scimitar", Weapon, Some(3), 114),
    item!(
        RoundShield,
        "round_shield",
        "Round shield",
        Weapon,
        Some(3),
        115
    ),
    item!(Sai, "sai", "Sai", Weapon, Some(3), 116),
    item!(Whip, "whip", "Whip", Weapon, Some(3), 117),
    item!(Longsword, "longsword", "Longsword", Weapon, Some(4), 120),
    item!(BattleAxe, "battle_axe", "Battle axe", Weapon, Some(4), 121),
    item!(Flail, "flail", "Flail", Weapon, Some(4), 122),
    item!(
        RunicBlade,
        "runic_blade",
        "Runic blade",
        Weapon,
        Some(4),
        123
    ),
    item!(
        AssassinsBlade,
        "assassins_blade",
        "Assassin's blade",
        Weapon,
        Some(4),
        124
    ),
    item!(Crossbow, "crossbow", "Crossbow", Weapon, Some(4), 125),
    item!(Katana, "katana", "Katana", Weapon, Some(4), 126),
    item!(Greatsword, "greatsword", "Greatsword", Weapon, Some(5), 128),
    item!(WarHammer, "war_hammer", "War hammer", Weapon, Some(5), 129),
    item!(Glaive, "glaive", "Glaive", Weapon, Some(5), 130),
    item!(Greataxe, "greataxe", "Greataxe", Weapon, Some(5), 131),
    item!(
        Greatshield,
        "greatshield",
        "Greatshield",
        Weapon,
        Some(5),
        132
    ),
    item!(
        StoneGauntlet,
        "gauntlet",
        "Stone gauntlet",
        Weapon,
        Some(5),
        133
    ),
    item!(WarScythe, "war_scythe", "War scythe", Weapon, Some(5), 134),
    item!(
        ThrowingStone,
        "throwing_stone",
        "Throwing stone",
        Weapon,
        Some(1),
        147
    ),
    item!(
        ThrowingKnife,
        "throwing_knife",
        "Throwing knife",
        Weapon,
        Some(1),
        146
    ),
    item!(
        ThrowingSpike,
        "throwing_spike",
        "Throwing spike",
        Weapon,
        Some(1),
        145
    ),
    item!(
        FishingSpear,
        "fishing_spear",
        "Fishing spear",
        Weapon,
        Some(2),
        148
    ),
    item!(
        ThrowingClub,
        "throwing_club",
        "Throwing club",
        Weapon,
        Some(2),
        150
    ),
    item!(Shuriken, "shuriken", "Shuriken", Weapon, Some(2), 149),
    item!(
        ThrowingSpear,
        "throwing_spear",
        "Throwing spear",
        Weapon,
        Some(3),
        151
    ),
    item!(Kunai, "kunai", "Kunai", Weapon, Some(3), 153),
    item!(Bolas, "bolas", "Bolas", Weapon, Some(3), 152),
    item!(Javelin, "javelin", "Javelin", Weapon, Some(4), 154),
    item!(Tomahawk, "tomahawk", "Tomahawk", Weapon, Some(4), 155),
    item!(
        HeavyBoomerang,
        "heavy_boomerang",
        "Heavy boomerang",
        Weapon,
        Some(4),
        156
    ),
    item!(Trident, "trident", "Trident", Weapon, Some(5), 157),
    item!(
        ThrowingHammer,
        "throwing_hammer",
        "Throwing hammer",
        Weapon,
        Some(5),
        158
    ),
    item!(ForceCube, "force_cube", "Force cube", Weapon, Some(5), 159),
    item!(
        ClothArmor,
        "cloth_armor",
        "Cloth armor",
        Armor,
        Some(1),
        176
    ),
    item!(
        LeatherArmor,
        "leather_armor",
        "Leather armor",
        Armor,
        Some(2),
        177
    ),
    item!(MailArmor, "mail_armor", "Mail armor", Armor, Some(3), 178),
    item!(
        ScaleArmor,
        "scale_armor",
        "Scale armor",
        Armor,
        Some(4),
        179
    ),
    item!(
        PlateArmor,
        "plate_armor",
        "Plate armor",
        Armor,
        Some(5),
        180
    ),
    item!(
        WandMagicMissile,
        "wand_magic_missile",
        "Wand of magic missile",
        Wand,
        None,
        208
    ),
    item!(
        WandFireblast,
        "wand_fireblast",
        "Wand of fireblast",
        Wand,
        None,
        209
    ),
    item!(WandFrost, "wand_frost", "Wand of frost", Wand, None, 210),
    item!(
        WandLightning,
        "wand_lightning",
        "Wand of lightning",
        Wand,
        None,
        211
    ),
    item!(
        WandDisintegration,
        "wand_disintegration",
        "Wand of disintegration",
        Wand,
        None,
        212
    ),
    item!(
        WandPrismaticLight,
        "wand_prismatic_light",
        "Wand of prismatic light",
        Wand,
        None,
        213
    ),
    item!(
        WandCorrosion,
        "wand_corrosion",
        "Wand of corrosion",
        Wand,
        None,
        214
    ),
    item!(
        WandLivingEarth,
        "wand_living_earth",
        "Wand of living earth",
        Wand,
        None,
        215
    ),
    item!(
        WandBlastWave,
        "wand_blast_wave",
        "Wand of blast wave",
        Wand,
        None,
        216
    ),
    item!(
        WandCorruption,
        "wand_corruption",
        "Wand of corruption",
        Wand,
        None,
        217
    ),
    item!(
        WandWarding,
        "wand_warding",
        "Wand of warding",
        Wand,
        None,
        218
    ),
    item!(
        WandRegrowth,
        "wand_regrowth",
        "Wand of regrowth",
        Wand,
        None,
        219
    ),
    item!(
        WandTransfusion,
        "wand_transfusion",
        "Wand of transfusion",
        Wand,
        None,
        220
    ),
    item!(RotDart, "rot_dart", "Rot dart", Weapon, Some(2), 161),
    item!(
        IncendiaryDart,
        "incendiary_dart",
        "Incendiary dart",
        Weapon,
        Some(2),
        162
    ),
    item!(
        AdrenalineDart,
        "adrenaline_dart",
        "Adrenaline dart",
        Weapon,
        Some(2),
        163
    ),
    item!(
        HealingDart,
        "healing_dart",
        "Healing dart",
        Weapon,
        Some(2),
        164
    ),
    item!(
        ChillingDart,
        "chilling_dart",
        "Chilling dart",
        Weapon,
        Some(2),
        165
    ),
    item!(
        ShockingDart,
        "shocking_dart",
        "Shocking dart",
        Weapon,
        Some(2),
        166
    ),
    item!(
        PoisonDart,
        "poison_dart",
        "Poison dart",
        Weapon,
        Some(2),
        167
    ),
    item!(
        CleansingDart,
        "cleansing_dart",
        "Cleansing dart",
        Weapon,
        Some(2),
        168
    ),
    item!(
        ParalyticDart,
        "paralytic_dart",
        "Paralytic dart",
        Weapon,
        Some(2),
        169
    ),
    item!(HolyDart, "holy_dart", "Holy dart", Weapon, Some(2), 170),
    item!(
        DisplacingDart,
        "displacing_dart",
        "Displacing dart",
        Weapon,
        Some(2),
        171
    ),
    item!(
        BlindingDart,
        "blinding_dart",
        "Blinding dart",
        Weapon,
        Some(2),
        172
    ),
    item!(
        RingAccuracy,
        "ring_accuracy",
        "Ring of accuracy",
        Ring,
        None,
        224
    ),
    item!(RingArcana, "ring_arcana", "Ring of arcana", Ring, None, 225),
    item!(
        RingElements,
        "ring_elements",
        "Ring of elements",
        Ring,
        None,
        226
    ),
    item!(RingEnergy, "ring_energy", "Ring of energy", Ring, None, 227),
    item!(
        RingEvasion,
        "ring_evasion",
        "Ring of evasion",
        Ring,
        None,
        228
    ),
    item!(RingForce, "ring_force", "Ring of force", Ring, None, 229),
    item!(RingFuror, "ring_furor", "Ring of furor", Ring, None, 230),
    item!(RingHaste, "ring_haste", "Ring of haste", Ring, None, 231),
    item!(RingMight, "ring_might", "Ring of might", Ring, None, 232),
    item!(
        RingSharpshooting,
        "ring_sharpshooting",
        "Ring of sharpshooting",
        Ring,
        None,
        233
    ),
    item!(
        RingTenacity,
        "ring_tenacity",
        "Ring of tenacity",
        Ring,
        None,
        234
    ),
    item!(RingWealth, "ring_wealth", "Ring of wealth", Ring, None, 235),
];

/// Weapon enchantments and curses. Array ordering matches upstream RNG arrays.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum WeaponEffect {
    Blazing,
    Chilling,
    Kinetic,
    Shocking,
    Blocking,
    Blooming,
    Elastic,
    Lucky,
    Projecting,
    Unstable,
    Corrupting,
    Grim,
    Vampiric,
    Annoying,
    Displacing,
    Dazzling,
    Explosive,
    Sacrificial,
    Wayward,
    Polarized,
    Friendly,
}

impl WeaponEffect {
    #[must_use]
    pub const fn is_curse(self) -> bool {
        (self as u8) >= (Self::Annoying as u8)
    }

    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Blazing => "Blazing",
            Self::Chilling => "Chilling",
            Self::Kinetic => "Kinetic",
            Self::Shocking => "Shocking",
            Self::Blocking => "Blocking",
            Self::Blooming => "Blooming",
            Self::Elastic => "Elastic",
            Self::Lucky => "Lucky",
            Self::Projecting => "Projecting",
            Self::Unstable => "Unstable",
            Self::Corrupting => "Corrupting",
            Self::Grim => "Grim",
            Self::Vampiric => "Vampiric",
            Self::Annoying => "Annoying",
            Self::Displacing => "Displacing",
            Self::Dazzling => "Dazzling",
            Self::Explosive => "Explosive",
            Self::Sacrificial => "Sacrificial",
            Self::Wayward => "Wayward",
            Self::Polarized => "Polarized",
            Self::Friendly => "Friendly",
        }
    }
}

/// Armor glyphs and curses. Array ordering matches upstream RNG arrays.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ArmorEffect {
    Obfuscation,
    Swiftness,
    Viscosity,
    Potential,
    Brimstone,
    Stone,
    Entanglement,
    Repulsion,
    Camouflage,
    Flow,
    Affection,
    AntiMagic,
    Thorns,
    AntiEntropy,
    Corrosion,
    Displacement,
    Metabolism,
    Multiplicity,
    Stench,
    Overgrowth,
    Bulk,
}

impl ArmorEffect {
    #[must_use]
    pub const fn is_curse(self) -> bool {
        (self as u8) >= (Self::AntiEntropy as u8)
    }

    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Obfuscation => "Obfuscation",
            Self::Swiftness => "Swiftness",
            Self::Viscosity => "Viscosity",
            Self::Potential => "Potential",
            Self::Brimstone => "Brimstone",
            Self::Stone => "Stone",
            Self::Entanglement => "Entanglement",
            Self::Repulsion => "Repulsion",
            Self::Camouflage => "Camouflage",
            Self::Flow => "Flow",
            Self::Affection => "Affection",
            Self::AntiMagic => "Anti-Magic",
            Self::Thorns => "Thorns",
            Self::AntiEntropy => "Anti-Entropy",
            Self::Corrosion => "Corrosion",
            Self::Displacement => "Displacement",
            Self::Metabolism => "Metabolism",
            Self::Multiplicity => "Multiplicity",
            Self::Stench => "Stench",
            Self::Overgrowth => "Overgrowth",
            Self::Bulk => "Bulk",
        }
    }
}

/// Equipment modifier used by a requirement or generated item.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Effect {
    Weapon(WeaponEffect),
    Armor(ArmorEffect),
}

impl Effect {
    /// Parses the stable human-readable modifier names used by the Android wire
    /// protocol. Matching is ASCII case-insensitive.
    #[must_use]
    pub fn from_wire_name(kind: ItemKind, name: &str) -> Option<Self> {
        match kind {
            ItemKind::Weapon => ALL_WEAPON_EFFECTS
                .iter()
                .copied()
                .find(|effect| effect.wire_name().eq_ignore_ascii_case(name))
                .map(Self::Weapon),
            ItemKind::Armor => ALL_ARMOR_EFFECTS
                .iter()
                .copied()
                .find(|effect| effect.wire_name().eq_ignore_ascii_case(name))
                .map(Self::Armor),
            ItemKind::Wand | ItemKind::Ring => None,
        }
    }

    #[must_use]
    pub const fn is_curse(self) -> bool {
        match self {
            Self::Weapon(effect) => effect.is_curse(),
            Self::Armor(effect) => effect.is_curse(),
        }
    }

    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Weapon(effect) => effect.wire_name(),
            Self::Armor(effect) => effect.wire_name(),
        }
    }
}

pub const ALL_WEAPON_EFFECTS: &[WeaponEffect] = &[
    WeaponEffect::Blazing,
    WeaponEffect::Chilling,
    WeaponEffect::Kinetic,
    WeaponEffect::Shocking,
    WeaponEffect::Blocking,
    WeaponEffect::Blooming,
    WeaponEffect::Elastic,
    WeaponEffect::Lucky,
    WeaponEffect::Projecting,
    WeaponEffect::Unstable,
    WeaponEffect::Corrupting,
    WeaponEffect::Grim,
    WeaponEffect::Vampiric,
    WeaponEffect::Annoying,
    WeaponEffect::Displacing,
    WeaponEffect::Dazzling,
    WeaponEffect::Explosive,
    WeaponEffect::Sacrificial,
    WeaponEffect::Wayward,
    WeaponEffect::Polarized,
    WeaponEffect::Friendly,
];

pub const ALL_ARMOR_EFFECTS: &[ArmorEffect] = &[
    ArmorEffect::Obfuscation,
    ArmorEffect::Swiftness,
    ArmorEffect::Viscosity,
    ArmorEffect::Potential,
    ArmorEffect::Brimstone,
    ArmorEffect::Stone,
    ArmorEffect::Entanglement,
    ArmorEffect::Repulsion,
    ArmorEffect::Camouflage,
    ArmorEffect::Flow,
    ArmorEffect::Affection,
    ArmorEffect::AntiMagic,
    ArmorEffect::Thorns,
    ArmorEffect::AntiEntropy,
    ArmorEffect::Corrosion,
    ArmorEffect::Displacement,
    ArmorEffect::Metabolism,
    ArmorEffect::Multiplicity,
    ArmorEffect::Stench,
    ArmorEffect::Overgrowth,
    ArmorEffect::Bulk,
];

/// Finds catalog data by stable identifier.
#[must_use]
pub fn item_by_stable_id(stable_id: &str) -> Option<&'static ItemDefinition> {
    ITEMS.iter().find(|item| item.stable_id == stable_id)
}

/// Finds catalog data by compact engine identifier.
#[must_use]
pub fn item(item_id: ItemId) -> &'static ItemDefinition {
    &ITEMS[item_id as usize]
}

#[cfg(test)]
mod tests {
    use super::{ITEMS, ItemId, item, item_by_stable_id};

    #[test]
    fn compact_ids_are_catalog_indices() {
        for (index, definition) in ITEMS.iter().enumerate() {
            assert_eq!(definition.id as usize, index);
            assert_eq!(item(definition.id), definition);
            assert_eq!(item_by_stable_id(definition.stable_id), Some(definition));
        }
        assert_eq!(ItemId::WandTransfusion as u8, 63);
        assert_eq!(ItemId::RotDart as u8, 64);
        assert_eq!(item(ItemId::WandTransfusion).sprite_index, 220);
        assert_eq!(item(ItemId::ThrowingSpike).tier, Some(1));
        assert_eq!(item(ItemId::BlindingDart).sprite_index, 172);
        assert_eq!(ItemId::RingAccuracy as u8, 76);
        assert_eq!(item(ItemId::RingWealth).sprite_index, 235);
    }

    #[test]
    fn zero_probability_items_are_not_searchable() {
        assert!(item_by_stable_id("mages_staff").is_none());
        assert!(item_by_stable_id("pickaxe").is_none());
        assert!(item_by_stable_id("dart").is_none());
        assert!(item_by_stable_id("warrior_armor").is_none());
    }

    #[test]
    fn android_modifier_names_round_trip() {
        use super::{ArmorEffect, Effect, ItemKind, WeaponEffect};

        for effect in super::ALL_WEAPON_EFFECTS {
            let wrapped = Effect::Weapon(*effect);
            assert_eq!(
                Effect::from_wire_name(ItemKind::Weapon, wrapped.wire_name()),
                Some(wrapped)
            );
        }
        for effect in super::ALL_ARMOR_EFFECTS {
            let wrapped = Effect::Armor(*effect);
            assert_eq!(
                Effect::from_wire_name(ItemKind::Armor, wrapped.wire_name()),
                Some(wrapped)
            );
        }
        assert_eq!(
            Effect::from_wire_name(ItemKind::Weapon, "shocking"),
            Some(Effect::Weapon(WeaponEffect::Shocking))
        );
        assert_eq!(
            Effect::from_wire_name(ItemKind::Armor, "anti-magic"),
            Some(Effect::Armor(ArmorEffect::AntiMagic))
        );
    }
}
