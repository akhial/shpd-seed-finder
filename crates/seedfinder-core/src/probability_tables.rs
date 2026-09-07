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

use crate::catalog::{EXTRA_UPGRADE_MAXIMUM, ItemId, ItemKind};
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

/// Highest upgrade level with its own tabled probability. It is the highest
/// the generator rolls, so no bucket has to absorb the levels above it.
pub const HIGHEST_TABLED_UPGRADE: u8 = EXTRA_UPGRADE_MAXIMUM;

/// [`HIGHEST_TABLED_UPGRADE`] as a table width.
pub const MAX_TABLED_UPGRADE: usize = HIGHEST_TABLED_UPGRADE as usize;

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
    /// Probability of each upgrade level `+0..=+5`.
    pub upgrades: [f32; MAX_TABLED_UPGRADE + 1],
    /// Probability the item is cursed.
    pub cursed: f32,
    /// Probability the item carries a positive enchantment or glyph.
    pub enchanted: f32,
    /// Probability of each tier per floor set, zero for untiered families.
    pub tiers: [[f32; TIERS]; FLOOR_SETS],
    /// Upgrade shares per tier, for the sources that settle both at once, or
    /// `None` where a tier and a level are rolled apart and [`Supply::tiers`]
    /// and [`Supply::upgrades`] can simply be multiplied.
    ///
    /// See [`locks_levels_to_tiers`]. Which tiers a source reaches varies by
    /// depth, but the lock itself does not, so this is conditioned on tier
    /// alone: read `levels[tier - 1][upgrade]` as the share of that tier's
    /// items carrying that level.
    pub levels: Option<&'static TierLevels>,
}

/// The upgrade level an item of each tier carries, for a source that fixes the
/// two together.
pub type TierLevels = [[f32; MAX_TABLED_UPGRADE + 1]; TIERS];

/// Whether a source settles an item's tier and its upgrade level with one
/// draw rather than two.
///
/// Both of the Imp's hoards do. Its vault stocks four fixed shelves and hands
/// items off them one at a time, so which shelf an item came from fixes both
/// numbers: the tier-4 armor is always the third shelf's, and always `+2`. Its
/// own reward flips a coin between a tier-5 melee at `+2..=+4` beside a tier-4
/// thrown at `+3..=+5`, and the same pair with the lines swapped — so whichever
/// weapon is tier 4 is the one levelled furthest.
///
/// Scoring a tier and a level apart at either invents items neither ever
/// hands out: a `+3` armor below tier 5, or a `+5` tier-5 weapon. Every other
/// source does draw the two apart, and the marginals describe those exactly.
#[must_use]
pub const fn locks_levels_to_tiers(source: ItemSource) -> bool {
    matches!(source, ItemSource::VaultTreasure | ItemSource::ImpReward)
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
///
/// # Panics
/// Panics for trinket offers, which have no measured equipment supply row.
#[must_use]
pub const fn kind_index(kind: ItemKind) -> usize {
    match kind {
        ItemKind::Weapon => 0,
        ItemKind::Armor => 1,
        ItemKind::Wand => 2,
        ItemKind::Ring => 3,
        ItemKind::Trinket | ItemKind::Artifact => panic!("no equipment supply table for this kind"),
    }
}

/// Reward slots one appearance of a quest or shop contributes to a family.
///
/// Quest rewards and shop stock arrive as a fixed bundle on a single floor
/// rather than as scattered drops, so their floor slot counts are the bundle
/// size times the probability that the quest or shop landed on that floor.
/// Sources that scatter independent drops return `0` and are treated as
/// Poisson, which is the one thing this still decides for a quest: only a
/// shop's bundle scales anything, since [`prize_group`] sources are lifted
/// out of the per-family supply and spent as one pick each.
///
/// The Imp's City-floor prizes are a fixed six: a tier-5 weapon with a
/// tier-4 thrown weapon or the reverse (two weapon slots, one per line), the
/// plate armor, the wand, and one ring slot for the ring beside the
/// artifact-or-ring option. The vault's treasure rooms vary; their counts are
/// what a typical vault places, rounded up: about eight weapons and thrown
/// weapons, three to four armors, and two to three wands and rings.
#[must_use]
pub const fn bundle_size(source: ItemSource, kind: ItemKind) -> u8 {
    match (source, kind) {
        (ItemSource::VaultTreasure, ItemKind::Weapon) => 8,
        (ItemSource::VaultTreasure, ItemKind::Armor) => 4,
        (ItemSource::Shop, ItemKind::Weapon)
        | (ItemSource::VaultTreasure, ItemKind::Wand | ItemKind::Ring) => 3,
        (ItemSource::ImpReward, ItemKind::Weapon) => 2,
        (ItemSource::BlacksmithReward, ItemKind::Weapon)
        | (ItemSource::WandmakerReward, ItemKind::Wand)
        | (ItemSource::GhostReward, ItemKind::Weapon | ItemKind::Armor)
        | (ItemSource::BlacksmithReward | ItemSource::Shop, ItemKind::Armor)
        | (ItemSource::Shop, ItemKind::Wand | ItemKind::Ring)
        | (ItemSource::ImpReward, ItemKind::Armor | ItemKind::Wand | ItemKind::Ring) => 1,
        _ => 0,
    }
}

/// The quest whose single prize a source belongs to.
///
/// A quest giver lays its prizes out as one mutually exclusive choice and the
/// player carries exactly one away, whatever family it belongs to: the Ghost
/// offers a weapon or an armor, the Wandmaker two wands, the Blacksmith a
/// reforge or one of its rack. The Imp's rewards and its vault's treasure
/// share a group because the Escape Crystal lets one item out between them.
///
/// This is the same rule [`crate::feasibility`] plans against, and it spans
/// families, so the estimator has to resolve every family at once to honour
/// it — see [`crate::probability`].
#[must_use]
pub const fn prize_group(source: ItemSource) -> Option<PrizeGroup> {
    match source {
        ItemSource::GhostReward => Some(PrizeGroup::Ghost),
        ItemSource::WandmakerReward => Some(PrizeGroup::Wandmaker),
        ItemSource::BlacksmithReward => Some(PrizeGroup::Blacksmith),
        ItemSource::ImpReward | ItemSource::VaultTreasure => Some(PrizeGroup::Imp),
        _ => None,
    }
}

/// A quest's prize pool: everything it lays out, of which one item leaves.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PrizeGroup {
    Ghost,
    Wandmaker,
    Blacksmith,
    Imp,
}

/// Every prize group, in table order.
pub const PRIZE_GROUPS: [PrizeGroup; 4] = [
    PrizeGroup::Ghost,
    PrizeGroup::Wandmaker,
    PrizeGroup::Blacksmith,
    PrizeGroup::Imp,
];

/// Row of [`SLOT_SPREAD`] covering one family's line.
#[must_use]
pub const fn spread_index(kind: ItemKind, line: Line) -> usize {
    kind_index(kind) + KINDS * line_index(line)
}

/// Supply rows for one equipment family.
pub fn supply_for(kind: ItemKind) -> impl Iterator<Item = &'static Supply> {
    SUPPLY.iter().filter(move |supply| supply.kind == kind)
}
