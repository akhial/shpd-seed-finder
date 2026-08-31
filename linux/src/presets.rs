// SPDX-License-Identifier: GPL-3.0-or-later

//! Presets bundled with every installation.

use shpd_seedfinder_core::catalog::{ItemId, ItemKind};
use shpd_seedfinder_core::model::ItemSource;
use shpd_seedfinder_core::query::{TierRequirement, UpgradeRequirement};

use crate::state::{AppState, UiRequirement};

/// The floor limit the vault presets carry: floor 19 is the last floor the
/// Imp — and so the vault holding its levelled prizes — can appear on, so a
/// deeper scan only costs time.
const VAULT_FLOOR_LIMIT: u8 = 19;

/// One read-only query shipped with the application.
#[derive(Clone, Debug)]
pub struct BuiltInPreset {
    pub name: &'static str,
    pub state: AppState,
}

/// Returns the protected presets in presentation order.
#[must_use]
pub fn built_in() -> [BuiltInPreset; 5] {
    [
        staff_21(),
        staff_22(),
        wand_bonanza(),
        ring_of_wealth_21(),
        tier_4_weapon_26(),
    ]
}

fn wand_bonanza() -> BuiltInPreset {
    let mut state = AppState::default();
    for (upgrade, max_depth) in [
        (UpgradeRequirement::Exact(3), None),
        (UpgradeRequirement::Exact(2), Some(4)),
        (UpgradeRequirement::Exact(2), Some(4)),
        (UpgradeRequirement::Exact(2), None),
    ] {
        let key = state.claim_key();
        state.requirements.push(UiRequirement {
            key,
            kind: ItemKind::Wand,
            upgrade,
            max_depth,
            ..UiRequirement::new(key)
        });
    }
    BuiltInPreset {
        name: "Wand Bonanza",
        state,
    }
}

fn staff_21() -> BuiltInPreset {
    let mut state = AppState::default();
    for (upgrade, identity_group) in [
        (UpgradeRequirement::Exact(3), Some(1)),
        (UpgradeRequirement::Any, Some(1)),
        (UpgradeRequirement::Any, Some(1)),
        (UpgradeRequirement::AtLeast(1), None),
    ] {
        let key = state.claim_key();
        state.requirements.push(UiRequirement {
            key,
            kind: ItemKind::Wand,
            upgrade,
            identity_group,
            ..UiRequirement::new(key)
        });
    }
    BuiltInPreset {
        name: "+21 Staff",
        state,
    }
}

/// The +21 stack anchored one level higher, on the +4 wand v4.0.0's Imp
/// vault lays out among its prizes.
fn staff_22() -> BuiltInPreset {
    let mut state = AppState::default();
    for (upgrade, identity_group) in [
        (UpgradeRequirement::Exact(4), Some(1)),
        (UpgradeRequirement::Any, Some(1)),
        (UpgradeRequirement::Any, Some(1)),
        (UpgradeRequirement::AtLeast(1), None),
    ] {
        let key = state.claim_key();
        state.requirements.push(UiRequirement {
            key,
            kind: ItemKind::Wand,
            upgrade,
            identity_group,
            ..UiRequirement::new(key)
        });
    }
    state.max_depth = VAULT_FLOOR_LIMIT;
    BuiltInPreset {
        name: "+22 Staff",
        state,
    }
}

fn ring_of_wealth_21() -> BuiltInPreset {
    let mut state = AppState::default();
    for (upgrade, source) in [
        (UpgradeRequirement::Exact(4), Some(ItemSource::ImpReward)),
        (UpgradeRequirement::Exact(2), None),
        (UpgradeRequirement::Any, None),
    ] {
        let key = state.claim_key();
        state.requirements.push(UiRequirement {
            key,
            kind: ItemKind::Ring,
            item: Some(ItemId::RingWealth),
            upgrade,
            source,
            ..UiRequirement::new(key)
        });
    }
    BuiltInPreset {
        name: "+21 Ring of Wealth",
        state,
    }
}

/// A tier-4 weapon at the +5 only the vault reaches, with two more of the
/// same weapon to pour into it.
fn tier_4_weapon_26() -> BuiltInPreset {
    let mut state = AppState::default();
    for (tier, upgrade) in [
        (TierRequirement::Exact(4), UpgradeRequirement::Exact(5)),
        (TierRequirement::Any, UpgradeRequirement::Any),
        (TierRequirement::Any, UpgradeRequirement::Any),
    ] {
        let key = state.claim_key();
        state.requirements.push(UiRequirement {
            key,
            kind: ItemKind::Weapon,
            tier,
            upgrade,
            identity_group: Some(1),
            ..UiRequirement::new(key)
        });
    }
    state.max_depth = VAULT_FLOOR_LIMIT;
    BuiltInPreset {
        name: "+26 Tier 4 Weapon",
        state,
    }
}

#[cfg(test)]
mod tests {
    use shpd_seedfinder_core::catalog::{ItemId, ItemKind};
    use shpd_seedfinder_core::model::ItemSource;
    use shpd_seedfinder_core::query::{TierRequirement, UpgradeRequirement};

    use super::{VAULT_FLOOR_LIMIT, built_in};

    #[test]
    fn staff_matches_requested_requirements() {
        let [staff, _, _, _, _] = built_in();
        assert_eq!(staff.name, "+21 Staff");
        assert_eq!(staff.state.requirements.len(), 4);
        assert!(
            staff
                .state
                .requirements
                .iter()
                .all(|requirement| requirement.kind == ItemKind::Wand)
        );
        assert_eq!(
            staff
                .state
                .requirements
                .iter()
                .map(|requirement| requirement.upgrade)
                .collect::<Vec<_>>(),
            [
                UpgradeRequirement::Exact(3),
                UpgradeRequirement::Any,
                UpgradeRequirement::Any,
                UpgradeRequirement::AtLeast(1),
            ]
        );
        assert_eq!(
            staff
                .state
                .requirements
                .iter()
                .map(|requirement| requirement.identity_group)
                .collect::<Vec<_>>(),
            [Some(1), Some(1), Some(1), None]
        );
    }

    #[test]
    fn staff_22_asks_for_the_vault_wand() {
        let [_, staff, _, _, _] = built_in();
        assert_eq!(staff.name, "+22 Staff");
        assert_eq!(staff.state.max_depth, VAULT_FLOOR_LIMIT);
        assert!(
            staff
                .state
                .requirements
                .iter()
                .all(|requirement| requirement.kind == ItemKind::Wand)
        );
        assert_eq!(
            staff
                .state
                .requirements
                .iter()
                .map(|requirement| requirement.upgrade)
                .collect::<Vec<_>>(),
            [
                UpgradeRequirement::Exact(4),
                UpgradeRequirement::Any,
                UpgradeRequirement::Any,
                UpgradeRequirement::AtLeast(1),
            ]
        );
        assert_eq!(
            staff
                .state
                .requirements
                .iter()
                .map(|requirement| requirement.identity_group)
                .collect::<Vec<_>>(),
            [Some(1), Some(1), Some(1), None]
        );
    }

    #[test]
    fn tier_4_weapon_stacks_three_copies_on_a_plus_five() {
        let [_, _, _, _, weapon] = built_in();
        assert_eq!(weapon.name, "+26 Tier 4 Weapon");
        assert_eq!(weapon.state.max_depth, VAULT_FLOOR_LIMIT);
        assert_eq!(weapon.state.requirements.len(), 3);
        assert!(
            weapon
                .state
                .requirements
                .iter()
                .all(|requirement| requirement.kind == ItemKind::Weapon
                    && requirement.identity_group == Some(1))
        );
        assert_eq!(
            weapon
                .state
                .requirements
                .iter()
                .map(|requirement| (requirement.tier, requirement.upgrade))
                .collect::<Vec<_>>(),
            [
                (TierRequirement::Exact(4), UpgradeRequirement::Exact(5)),
                (TierRequirement::Any, UpgradeRequirement::Any),
                (TierRequirement::Any, UpgradeRequirement::Any),
            ]
        );
    }

    /// The vault presets sit at the engine's upgrade ceilings, so a preset is
    /// only shipped once the engine accepts the query it builds.
    #[test]
    fn every_preset_builds_a_runnable_query() {
        for preset in built_in() {
            assert!(
                preset.state.to_query().is_ok(),
                "{} does not validate: {:?}",
                preset.name,
                preset.state.to_query().err()
            );
        }
    }

    #[test]
    fn wand_bonanza_matches_requested_requirements() {
        let [_, _, preset, _, _] = built_in();
        assert_eq!(preset.name, "Wand Bonanza");
        assert!(
            preset
                .state
                .requirements
                .iter()
                .all(|requirement| requirement.kind == ItemKind::Wand && requirement.item.is_none())
        );
        assert_eq!(
            preset
                .state
                .requirements
                .iter()
                .map(|requirement| requirement.upgrade)
                .collect::<Vec<_>>(),
            [
                UpgradeRequirement::Exact(3),
                UpgradeRequirement::Exact(2),
                UpgradeRequirement::Exact(2),
                UpgradeRequirement::Exact(2),
            ]
        );
        assert_eq!(
            preset
                .state
                .requirements
                .iter()
                .map(|requirement| requirement.max_depth)
                .collect::<Vec<_>>(),
            [None, Some(4), Some(4), None]
        );
        assert!(
            preset
                .state
                .requirements
                .iter()
                .all(|requirement| requirement.identity_group.is_none())
        );
    }

    #[test]
    fn ring_of_wealth_matches_requested_requirements() {
        let [_, _, _, ring, _] = built_in();
        assert_eq!(ring.name, "+21 Ring of Wealth");
        assert!(
            ring.state
                .requirements
                .iter()
                .all(|requirement| requirement.item == Some(ItemId::RingWealth))
        );
        assert_eq!(
            ring.state.requirements[0].upgrade,
            UpgradeRequirement::Exact(4)
        );
        assert_eq!(
            ring.state.requirements[0].source,
            Some(ItemSource::ImpReward)
        );
        assert_eq!(
            ring.state
                .requirements
                .iter()
                .map(|requirement| requirement.max_depth)
                .collect::<Vec<_>>(),
            [None, None, None]
        );
        assert_eq!(
            ring.state.requirements[1].upgrade,
            UpgradeRequirement::Exact(2)
        );
        assert_eq!(ring.state.requirements[2].upgrade, UpgradeRequirement::Any);
    }
}
