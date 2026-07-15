// SPDX-License-Identifier: GPL-3.0-or-later

//! Presets bundled with every installation.

use shpd_seedfinder_core::catalog::{ItemId, ItemKind};
use shpd_seedfinder_core::model::ItemSource;
use shpd_seedfinder_core::query::UpgradeRequirement;

use crate::state::{AppState, UiRequirement};

/// One read-only query shipped with the application.
#[derive(Clone, Debug)]
pub struct BuiltInPreset {
    pub name: &'static str,
    pub state: AppState,
}

/// Returns the protected presets in presentation order.
#[must_use]
pub fn built_in() -> [BuiltInPreset; 3] {
    [staff_21(), wand_bonanza(), ring_of_wealth_21()]
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

#[cfg(test)]
mod tests {
    use shpd_seedfinder_core::catalog::{ItemId, ItemKind};
    use shpd_seedfinder_core::model::ItemSource;
    use shpd_seedfinder_core::query::UpgradeRequirement;

    use super::built_in;

    #[test]
    fn staff_matches_requested_requirements() {
        let [staff, _, _] = built_in();
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
    fn wand_bonanza_matches_requested_requirements() {
        let [_, preset, _] = built_in();
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
        let [_, _, ring] = built_in();
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
