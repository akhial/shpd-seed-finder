// SPDX-License-Identifier: GPL-3.0-or-later

//! Deterministic selection of jointly obtainable scouted items that satisfy
//! the current requirements, delegating to the engine's matcher.

use std::collections::HashSet;

use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::model::{GeneratedWorld, WorldItem};
use shpd_seedfinder_core::query::{SearchQuery, best_match_indices};
use shpd_seedfinder_core::seed::DungeonSeed;

use crate::state::UiRequirement;

/// Selects the largest set of scouted item indices where each item satisfies
/// one distinct requirement slot and all choice, scenario, alternative, and
/// combined-upgrade groups stay compatible. The requirements need not form a
/// valid query; the engine matches whatever it is given.
pub fn scout_match_indices(
    items: &[WorldItem],
    requirements: &[UiRequirement],
    max_depth: u8,
    exclude_blacksmith_rewards: bool,
) -> HashSet<usize> {
    let query = SearchQuery {
        requirements: requirements
            .iter()
            .map(|requirement| requirement.to_core())
            .collect(),
        max_depth,
        challenges: Challenges::NONE,
        require_blacksmith: false,
        exclude_blacksmith_rewards,
        fast_mode: false,
    };
    let world = GeneratedWorld {
        seed: DungeonSeed::MIN,
        items: items.to_vec(),
    };
    best_match_indices(&query, &world).into_iter().collect()
}

#[cfg(test)]
mod tests {
    use shpd_seedfinder_core::catalog::{ItemId, ItemKind};
    use shpd_seedfinder_core::model::{Accessibility, ItemSource, WorldItem};
    use shpd_seedfinder_core::query::{UpgradeRequirement, UpgradeSum};

    use super::scout_match_indices;
    use crate::state::UiRequirement;

    fn world_item(item: ItemId, depth: u8, accessibility: Accessibility) -> WorldItem {
        WorldItem {
            item,
            upgrade: 2,
            effect: None,
            cursed: false,
            depth,
            source: ItemSource::Heap,
            accessibility,
        }
    }

    fn requirement(key: u64, kind: ItemKind, item: Option<ItemId>) -> UiRequirement {
        UiRequirement {
            kind,
            item,
            upgrade: UpgradeRequirement::Exact(2),
            ..UiRequirement::new(key)
        }
    }

    #[test]
    fn distinct_items_satisfy_distinct_requirements() {
        let items = [
            world_item(ItemId::Sword, 3, Accessibility::Independent),
            world_item(ItemId::Sword, 9, Accessibility::Independent),
        ];
        let requirements = [
            requirement(1, ItemKind::Weapon, Some(ItemId::Sword)),
            requirement(2, ItemKind::Weapon, Some(ItemId::Sword)),
        ];
        let matches = scout_match_indices(&items, &requirements, 24, false);
        assert_eq!(matches, [0, 1].into());
        // With a shallower scope, only the first copy can qualify.
        let matches = scout_match_indices(&items, &requirements, 8, false);
        assert_eq!(matches, [0].into());
    }

    #[test]
    fn mutually_exclusive_choices_never_both_match() {
        let items = [
            world_item(
                ItemId::Sword,
                3,
                Accessibility::Choice {
                    group: 1,
                    option: 0,
                },
            ),
            world_item(
                ItemId::MailArmor,
                3,
                Accessibility::Choice {
                    group: 1,
                    option: 1,
                },
            ),
        ];
        let requirements = [
            requirement(1, ItemKind::Weapon, Some(ItemId::Sword)),
            requirement(2, ItemKind::Armor, Some(ItemId::MailArmor)),
        ];
        let matches = scout_match_indices(&items, &requirements, 24, false);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn uncursed_requirement_rejects_cursed_copies() {
        let clean = world_item(ItemId::Sword, 3, Accessibility::Independent);
        let mut cursed = clean.clone();
        cursed.cursed = true;
        let mut wanted = requirement(1, ItemKind::Weapon, Some(ItemId::Sword));
        wanted.require_uncursed = true;

        assert_eq!(
            scout_match_indices(&[clean, cursed.clone()], &[wanted], 24, false),
            [0].into()
        );
        assert!(scout_match_indices(&[cursed], &[wanted], 24, false).is_empty());
    }

    #[test]
    fn identity_groups_bind_wildcards_to_one_item() {
        let items = [
            world_item(ItemId::WandFrost, 2, Accessibility::Independent),
            world_item(ItemId::WandLightning, 4, Accessibility::Independent),
            world_item(ItemId::WandFrost, 6, Accessibility::Independent),
        ];
        let linked = |key| UiRequirement {
            identity_group: Some(1),
            ..requirement(key, ItemKind::Wand, None)
        };
        let matches = scout_match_indices(&items, &[linked(1), linked(2)], 24, false);
        assert_eq!(matches, [0, 2].into());
    }

    #[test]
    fn blacksmith_rewards_can_be_excluded() {
        let mut smith = world_item(ItemId::Sword, 12, Accessibility::Independent);
        smith.source = ItemSource::BlacksmithReward;
        let items = [smith];
        let requirements = [requirement(1, ItemKind::Weapon, Some(ItemId::Sword))];
        assert_eq!(
            scout_match_indices(&items, &requirements, 24, false).len(),
            1
        );
        assert!(scout_match_indices(&items, &requirements, 24, true).is_empty());
    }

    #[test]
    fn an_alternative_group_is_satisfied_by_one_member() {
        let alternative = |key, item| UiRequirement {
            alternative_group: Some(1),
            ..requirement(key, ItemKind::Weapon, Some(item))
        };
        let requirements = [alternative(1, ItemId::Spear), alternative(2, ItemId::Sword)];
        let items = [world_item(ItemId::Sword, 3, Accessibility::Independent)];
        assert_eq!(
            scout_match_indices(&items, &requirements, 24, false),
            [0].into()
        );
    }

    #[test]
    fn sum_groups_count_only_when_the_total_is_met() {
        let ring = |key| UiRequirement {
            identity_group: Some(1),
            upgrade_sum: Some(UpgradeSum {
                group: 1,
                minimum_total: 2,
            }),
            upgrade: UpgradeRequirement::Any,
            ..requirement(key, ItemKind::Ring, Some(ItemId::RingMight))
        };
        let make = |upgrade| WorldItem {
            upgrade,
            ..world_item(ItemId::RingMight, 3, Accessibility::Independent)
        };
        let pair = [ring(1), ring(2)];
        // +0 and +2 total the wanted +2, as do two +1 rings.
        assert_eq!(
            scout_match_indices(&[make(0), make(2)], &pair, 24, false),
            [0, 1].into()
        );
        assert_eq!(
            scout_match_indices(&[make(1), make(1)], &pair, 24, false),
            [0, 1].into()
        );
        // Two +0 rings fall short and neither is highlighted.
        assert!(scout_match_indices(&[make(0), make(0)], &pair, 24, false).is_empty());
    }

    #[test]
    fn a_failed_sum_pair_leaves_independent_matches_highlighted() {
        let ring = |key| UiRequirement {
            identity_group: Some(1),
            upgrade_sum: Some(UpgradeSum {
                group: 1,
                minimum_total: 6,
            }),
            upgrade: UpgradeRequirement::Any,
            ..requirement(key, ItemKind::Ring, Some(ItemId::RingMight))
        };
        let requirements = [
            ring(1),
            ring(2),
            requirement(3, ItemKind::Weapon, Some(ItemId::Sword)),
        ];
        let items = [
            world_item(ItemId::RingMight, 2, Accessibility::Independent),
            world_item(ItemId::RingMight, 4, Accessibility::Independent),
            world_item(ItemId::Sword, 3, Accessibility::Independent),
        ];
        assert_eq!(
            scout_match_indices(&items, &requirements, 24, false),
            [2].into()
        );
    }
}
