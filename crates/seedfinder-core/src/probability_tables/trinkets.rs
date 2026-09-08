//! Measured +3 trinket profiles, equipped after the first brewing opportunity.
//! Each source has separate rows by floor, so late enchantment/upgrade shares
//! never leak into early-floor queries. Floors 1 and 2 retain the canonical
//! table: the earliest alchemy pot is a secret laboratory on floor 2, and its
//! effects start on the next floor.
#![allow(clippy::approx_constant)] // Sampled ratios, not mathematical constants.

use super::{DEPTHS, IDENTITY_REPEAT_LIMIT, KINDS, LINES, Supply, TIPPED_DARTS};
use crate::catalog::{ItemId, ItemKind};

mod cracked_spyglass;
mod exotic_crystals;
mod mimic_tooth;
mod mossy_clump;
mod parchment_scrap;
mod rat_skull;
mod trap_mechanism;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Profile {
    #[default]
    None,
    MimicTooth,
    ParchmentScrap,
    RatSkull,
    ExoticCrystals,
    MossyClump,
    TrapMechanism,
    CrackedSpyglass,
}

struct Tables {
    supply: &'static [Supply],
    spread: &'static [[f32; DEPTHS]; KINDS * LINES],
    repeats: &'static [[[f32; DEPTHS]; IDENTITY_REPEAT_LIMIT]; KINDS * LINES],
    tipped: &'static [f32; TIPPED_DARTS],
}

macro_rules! tables {
    ($module:ident) => {
        Tables {
            supply: $module::SUPPLY,
            spread: &$module::SLOT_SPREAD,
            repeats: &$module::IDENTITY_REPEATS,
            tipped: &$module::TIPPED_SHARES,
        }
    };
}

impl Profile {
    pub fn for_matches(mask: u32, identities: &[ItemId]) -> Self {
        if mask.is_power_of_two() {
            Self::of(identities[mask.trailing_zeros() as usize])
        } else {
            Self::None
        }
    }

    pub fn of(id: ItemId) -> Self {
        match id {
            ItemId::MimicTooth => Self::MimicTooth,
            ItemId::ParchmentScrap => Self::ParchmentScrap,
            ItemId::RatSkull => Self::RatSkull,
            ItemId::ExoticCrystals => Self::ExoticCrystals,
            ItemId::MossyClump => Self::MossyClump,
            ItemId::TrapMechanism => Self::TrapMechanism,
            ItemId::CrackedSpyglass => Self::CrackedSpyglass,
            _ => Self::None,
        }
    }

    fn tables(self) -> Tables {
        match self {
            Self::None => tables!(super),
            Self::MimicTooth => tables!(mimic_tooth),
            Self::ParchmentScrap => tables!(parchment_scrap),
            Self::RatSkull => tables!(rat_skull),
            Self::ExoticCrystals => tables!(exotic_crystals),
            Self::MossyClump => tables!(mossy_clump),
            Self::TrapMechanism => tables!(trap_mechanism),
            Self::CrackedSpyglass => tables!(cracked_spyglass),
        }
    }

    pub fn supply_for(self, kind: ItemKind) -> impl Iterator<Item = Supply> {
        let selected = self != Self::None;
        self.tables()
            .supply
            .iter()
            .filter(move |s| s.kind == kind)
            .copied()
            .map(move |mut s| {
                if selected {
                    s.depth_slots[..2].fill(0.0);
                }
                s
            })
            .chain(
                super::SUPPLY
                    .iter()
                    .filter(move |s| selected && s.kind == kind)
                    .copied()
                    .map(|mut s| {
                        s.depth_slots[2..].fill(0.0);
                        s
                    }),
            )
            .filter(|s| s.depth_slots.iter().any(|count| *count > 0.0))
    }

    pub fn spread(self, line: usize, depth: usize) -> f32 {
        if depth < 2 {
            super::SLOT_SPREAD[line][depth]
        } else {
            self.tables().spread[line][depth]
        }
    }

    pub fn repeat(self, line: usize, copies: usize, depth: usize) -> f32 {
        if depth < 2 {
            super::IDENTITY_REPEATS[line][copies][depth]
        } else {
            self.tables().repeats[line][copies][depth]
        }
    }

    pub fn tipped(self, dart: usize) -> f32 {
        self.tables().tipped[dart]
    }
}
#[cfg(test)]
mod tests {
    use super::Profile;

    #[test]
    fn selected_distributions_are_finite_and_normalised() {
        for profile in [
            Profile::MimicTooth,
            Profile::ParchmentScrap,
            Profile::RatSkull,
            Profile::ExoticCrystals,
            Profile::MossyClump,
            Profile::TrapMechanism,
            Profile::CrackedSpyglass,
        ] {
            let tables = profile.tables();
            assert!(
                tables.supply.len() > 500,
                "{profile:?} must have per-floor rows"
            );
            for supply in tables.supply {
                let total: f32 = supply.upgrades.iter().sum();
                assert!((total - 1.0).abs() < 1e-3);
                assert!(supply.options.is_finite() && supply.options >= 1.0);
                assert!((0.0..=1.0).contains(&supply.cursed));
                assert!((0.0..=1.0).contains(&supply.enchanted));
                assert!(
                    supply
                        .depth_slots
                        .iter()
                        .all(|p| p.is_finite() && *p >= 0.0)
                );
                assert_eq!(supply.depth_slots.iter().filter(|p| **p > 0.0).count(), 1);
                for row in &supply.tiers {
                    let total: f32 = row.iter().sum();
                    assert!(total.abs() < 1e-3 || (total - 1.0).abs() < 1e-3);
                }
                if let Some(levels) = supply.levels {
                    for row in levels {
                        let total: f32 = row.iter().sum();
                        assert!(total.abs() < 1e-3 || (total - 1.0).abs() < 1e-3);
                    }
                }
            }
        }
    }
}
