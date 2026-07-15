//! Generated-world result model, including mutually exclusive rewards.

use crate::catalog::{Effect, ItemId};
use crate::equipment::EquipmentRoll;
use crate::seed::DungeonSeed;

/// Where an item can be obtained in the generated world.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ItemSource {
    Heap,
    Chest,
    LockedChest,
    CrystalChest,
    Tomb,
    Skeleton,
    SacrificialFire,
    Mimic,
    GoldenMimic,
    CrystalMimic,
    Statue,
    ArmoredStatue,
    Shop,
    GhostReward,
    WandmakerReward,
    BlacksmithReward,
    ImpReward,
}

/// Co-acquisition constraints for a generated reward.
///
/// Most rewards are independent. Simple quest/chest choices use one option,
/// while rooms with keys and prerequisite paths enumerate their finite set of
/// feasible acquisition plans as a bit mask. Items from the same group can be
/// obtained together exactly when their masks have at least one common bit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Accessibility {
    Independent,
    Choice {
        /// Stable within one world; all rewards in the same choice share it.
        group: u16,
        /// Rewards with the same option can be obtained together.
        option: u8,
    },
    Scenarios {
        /// Stable within one world; all constrained rewards from the same
        /// decision topology share it.
        group: u16,
        /// Nonzero bit set of feasible acquisition plans containing this
        /// reward. At most 64 plans are needed by v3.3.8 room topologies.
        mask: u64,
    },
}

impl Accessibility {
    /// Converts a simple option or explicit scenario set into the common mask
    /// representation used by the query matcher.
    #[must_use]
    pub const fn scenario_constraint(self) -> Option<(u16, u64)> {
        match self {
            Self::Independent => None,
            Self::Choice { group, option } => {
                let mask = if option < 64 { 1_u64 << option } else { 0 };
                Some((group, mask))
            }
            Self::Scenarios { group, mask } => Some((group, mask)),
        }
    }
}

/// One deterministically generated, world-searchable item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldItem {
    pub item: ItemId,
    pub upgrade: u8,
    pub effect: Option<Effect>,
    pub cursed: bool,
    pub depth: u8,
    pub source: ItemSource,
    pub accessibility: Accessibility,
}

impl WorldItem {
    /// Builds a searchable record without reinterpreting the equipment roll.
    #[must_use]
    pub const fn from_equipment_roll(
        item: ItemId,
        roll: EquipmentRoll,
        depth: u8,
        source: ItemSource,
        accessibility: Accessibility,
    ) -> Self {
        Self {
            item,
            upgrade: roll.upgrade,
            effect: roll.effect,
            cursed: roll.cursed,
            depth,
            source,
            accessibility,
        }
    }
}

/// Searchable output for one seed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedWorld {
    pub seed: DungeonSeed,
    pub items: Vec<WorldItem>,
}
