//! Measured equipment supply backing [`crate::probability`].
//!
//! How much equipment a floor produces, and how it splits over upgrades,
//! curses, enchantments, and tiers, is an emergent property of room decks,
//! chest budgets, and quest placement rather than a constant that can be read
//! off the upstream source. The numbers below are therefore sampled
//! from the canonical generator by
//! `cargo run --release --example calibrate_probability`, and
//! `tests/probability_tables.rs` re-measures a smaller sample to catch drift.
//!
//! This module owns the shape of that data; the generated file owns the
//! values.

use crate::catalog::{ItemId, ItemKind};
use crate::generator::{
    MISSILE_TIER_1_ITEMS, MISSILE_TIER_2_ITEMS, MISSILE_TIER_3_ITEMS, MISSILE_TIER_4_ITEMS,
    MISSILE_TIER_5_ITEMS, MissileKind,
};
use crate::model::ItemSource;

mod measured;

pub use measured::{IDENTITY_REPEATS, SLOT_SPREAD, SUPPLY, TIPPED_SHARES};

/// Deepest floor of the main dungeon.
pub const DEPTHS: usize = 24;

/// [`DEPTHS`] as a floor number.
pub const DEEPEST_FLOOR: u8 = 24;

const _: () = assert!(DEPTHS == DEEPEST_FLOOR as usize);

/// Equipment families tracked by the estimator.
pub const KINDS: usize = 4;

/// Every equipment family, in table order.
pub const KINDS_ORDER: [ItemKind; KINDS] = [
    ItemKind::Weapon,
    ItemKind::Armor,
    ItemKind::Wand,
    ItemKind::Ring,
];

/// Generator lines within one equipment family.
///
/// Thrown weapons and tipped darts are [`ItemKind::Weapon`] to the catalog but
/// come out of their own generator categories, in their own quantities, tiers,
/// and upgrades. Tallying them apart from melee weapons is what keeps a dart
/// bought in a shop from making swords look plentiful. Every other family has
/// only the plain line.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Line {
    Plain,
    Thrown,
    Tipped,
}

/// Generator lines per family.
pub const LINES: usize = 3;

/// Every generator line, in table order.
pub const LINES_ORDER: [Line; LINES] = [Line::Plain, Line::Thrown, Line::Tipped];

/// Dense table index for one generator line.
#[must_use]
pub const fn line_index(line: Line) -> usize {
    match line {
        Line::Plain => 0,
        Line::Thrown => 1,
        Line::Tipped => 2,
    }
}

/// The line that produces a catalog identity.
#[must_use]
pub fn line_of(item: ItemId) -> Line {
    if tipped_index(item).is_some() {
        Line::Tipped
    } else if is_missile(item) {
        Line::Thrown
    } else {
        Line::Plain
    }
}

/// Tipped darts, which the generator tips with a plant seed rather than drawing
/// from the weapon deck. They are laid out contiguously in the catalog.
pub const TIPPED_DARTS: usize = 12;

const _: () = assert!(
    ItemId::BlindingDart as usize - ItemId::RotDart as usize + 1 == TIPPED_DARTS,
    "the tipped darts must stay contiguous in the catalog"
);

/// Position of a tipped dart in [`TIPPED_SHARES`], or `None` for anything else.
#[must_use]
pub fn tipped_index(item: ItemId) -> Option<usize> {
    let offset = (item as usize).checked_sub(ItemId::RotDart as usize)?;
    (offset < TIPPED_DARTS).then_some(offset)
}

/// Five-floor regions sharing one tier table.
pub const FLOOR_SETS: usize = 5;

/// Equipment tiers.
pub const TIERS: usize = 5;

/// [`TIERS`] as a tier number.
pub const HIGHEST_TIER: u8 = 5;

const _: () = assert!(TIERS == HIGHEST_TIER as usize);

/// Highest upgrade level with its own tabled probability.
pub const MAX_TABLED_UPGRADE: usize = 4;

/// [`MAX_TABLED_UPGRADE`] as an upgrade level.
pub const HIGHEST_TABLED_UPGRADE: u8 = 4;

const _: () = assert!(MAX_TABLED_UPGRADE == HIGHEST_TABLED_UPGRADE as usize);

/// Highest number of same-identity duplicates the repeat table covers.
pub const IDENTITY_REPEAT_LIMIT: usize = 4;

/// Per-source, per-family equipment supply.
#[derive(Clone, Copy, Debug)]
pub struct Supply {
    pub kind: ItemKind,
    pub line: Line,
    pub source: ItemSource,
    /// Reward slots contributed each time this source appears, or `0` when the
    /// source scatters independent drops whose count is treated as Poisson.
    pub bundle: u8,
    /// Alternatives per slot. Rewards that exclude one another — the
    /// Wandmaker's two wands, the Blacksmith's choice of prize — occupy one
    /// slot between them, so a slot matches when any of its options does.
    pub options: f32,
    /// Whether those alternatives are upgraded and cursed as one.
    ///
    /// The Blacksmith offers its whole weapon rack at a single upgrade level,
    /// so asking for a `+3` weapon there is one chance rather than three; the
    /// Wandmaker rolls its two wands apart, so it is two.
    pub shared_roll: bool,
    /// Expected number of reward slots on each floor `1..=24`.
    pub depth_slots: [f32; DEPTHS],
    /// Probability of each upgrade level `+0..=+4`.
    pub upgrades: [f32; MAX_TABLED_UPGRADE + 1],
    /// Probability the item is cursed.
    pub cursed: f32,
    /// Probability the item carries a positive enchantment or glyph.
    pub enchanted: f32,
    /// Probability of each tier per floor set, zero for untiered families.
    pub tiers: [[f32; TIERS]; FLOOR_SETS],
}

/// Every source that can hold searchable equipment, in table order.
#[must_use]
pub const fn sources() -> &'static [ItemSource] {
    &[
        ItemSource::Heap,
        ItemSource::Chest,
        ItemSource::LockedChest,
        ItemSource::CrystalChest,
        ItemSource::Tomb,
        ItemSource::Skeleton,
        ItemSource::SacrificialFire,
        ItemSource::Mimic,
        ItemSource::GoldenMimic,
        ItemSource::CrystalMimic,
        ItemSource::Statue,
        ItemSource::ArmoredStatue,
        ItemSource::Shop,
        ItemSource::GhostReward,
        ItemSource::WandmakerReward,
        ItemSource::BlacksmithReward,
        ItemSource::ImpReward,
        ItemSource::VaultTreasure,
    ]
}

/// Dense table index for one source.
#[must_use]
pub const fn source_index(source: ItemSource) -> usize {
    match source {
        ItemSource::Heap => 0,
        ItemSource::Chest => 1,
        ItemSource::LockedChest => 2,
        ItemSource::CrystalChest => 3,
        ItemSource::Tomb => 4,
        ItemSource::Skeleton => 5,
        ItemSource::SacrificialFire => 6,
        ItemSource::Mimic => 7,
        ItemSource::GoldenMimic => 8,
        ItemSource::CrystalMimic => 9,
        ItemSource::Statue => 10,
        ItemSource::ArmoredStatue => 11,
        ItemSource::Shop => 12,
        ItemSource::GhostReward => 13,
        ItemSource::WandmakerReward => 14,
        ItemSource::BlacksmithReward => 15,
        ItemSource::ImpReward => 16,
        ItemSource::VaultTreasure => 17,
    }
}

/// Whether a catalog identity is a thrown weapon.
#[must_use]
pub fn is_missile(item: ItemId) -> bool {
    missile_tier(item).is_some()
}

/// Thrown-weapon tier, or `None` for anything else.
#[must_use]
pub fn missile_tier(item: ItemId) -> Option<u8> {
    (1..=5).find(|tier| {
        missile_tier_items(*tier)
            .iter()
            .any(|kind| kind.item_id() == Some(item))
    })
}

/// Thrown weapons of one tier that can be generated.
#[must_use]
pub fn missile_tier_items(tier: u8) -> &'static [MissileKind] {
    match tier {
        1 => &MISSILE_TIER_1_ITEMS,
        2 => &MISSILE_TIER_2_ITEMS,
        3 => &MISSILE_TIER_3_ITEMS,
        4 => &MISSILE_TIER_4_ITEMS,
        _ => &MISSILE_TIER_5_ITEMS,
    }
}

/// Dense table index for one equipment family.
#[must_use]
pub const fn kind_index(kind: ItemKind) -> usize {
    match kind {
        ItemKind::Weapon => 0,
        ItemKind::Armor => 1,
        ItemKind::Wand => 2,
        ItemKind::Ring => 3,
    }
}

/// Reward slots one appearance of a quest or shop contributes to a family.
///
/// Quest rewards and shop stock arrive as a fixed bundle on a single floor
/// rather than as scattered drops, so their floor slot counts are the bundle
/// size times the probability that the quest or shop landed on that floor.
/// Sources that scatter independent drops return `0` and are treated as
/// Poisson.
#[must_use]
pub const fn bundle_size(source: ItemSource, kind: ItemKind) -> u8 {
    match (source, kind) {
        (ItemSource::Shop, ItemKind::Weapon) => 3,
        (ItemSource::BlacksmithReward, ItemKind::Weapon)
        | (ItemSource::WandmakerReward, ItemKind::Wand)
        | (ItemSource::GhostReward, ItemKind::Weapon | ItemKind::Armor)
        | (ItemSource::BlacksmithReward | ItemSource::Shop, ItemKind::Armor)
        | (ItemSource::Shop, ItemKind::Wand | ItemKind::Ring)
        | (ItemSource::ImpReward, ItemKind::Ring) => 1,
        _ => 0,
    }
}

/// Whether a source places all of its items on a single floor of a window.
///
/// Quests run once per dungeon, so their per-floor counts are alternatives:
/// a Ghost that appeared on floor two cannot also appear on floor three. Shops
/// restock on every shop floor and are therefore not exclusive.
#[must_use]
pub const fn appears_once(source: ItemSource) -> bool {
    matches!(
        source,
        ItemSource::GhostReward
            | ItemSource::WandmakerReward
            | ItemSource::BlacksmithReward
            | ItemSource::ImpReward
            | ItemSource::VaultTreasure
    )
}

/// Row of [`SLOT_SPREAD`] covering one family's line.
#[must_use]
pub const fn spread_index(kind: ItemKind, line: Line) -> usize {
    kind_index(kind) + KINDS * line_index(line)
}

/// Supply rows for one equipment family.
pub fn supply_for(kind: ItemKind) -> impl Iterator<Item = &'static Supply> {
    SUPPLY.iter().filter(move |supply| supply.kind == kind)
}
