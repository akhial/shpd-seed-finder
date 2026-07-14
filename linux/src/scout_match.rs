// SPDX-License-Identifier: GPL-3.0-or-later

//! Deterministic selection of jointly obtainable scouted items that satisfy
//! the current requirements, mirroring the engine's search matcher.

use std::collections::{BTreeMap, HashSet};

use shpd_seedfinder_core::catalog::ItemId;
use shpd_seedfinder_core::model::{ItemSource, WorldItem};
use shpd_seedfinder_core::query::Requirement;

use crate::state::UiRequirement;

/// One requirement's identity group and its qualifying `(index, identity)`
/// candidates in the scouted world.
type RequirementCandidates = (Option<u8>, Vec<(usize, ItemId)>);

/// Selects the largest set of scouted item indices where each item satisfies
/// one distinct requirement and all choice or scenario groups stay compatible.
pub fn scout_match_indices(
    items: &[WorldItem],
    requirements: &[UiRequirement],
    max_depth: u8,
    exclude_blacksmith_rewards: bool,
) -> HashSet<usize> {
    let mut candidates: Vec<RequirementCandidates> = requirements
        .iter()
        .map(|requirement| {
            let core = requirement.to_core();
            (
                requirement.identity_group,
                items
                    .iter()
                    .enumerate()
                    .filter(|(_, candidate)| {
                        candidate.depth <= max_depth
                            && candidate.depth <= requirement.max_depth.unwrap_or(max_depth)
                            && (!exclude_blacksmith_rewards
                                || candidate.source != ItemSource::BlacksmithReward)
                            && core.matches(candidate)
                    })
                    .map(|(index, candidate)| (index, matched_identity(&core, candidate)))
                    .collect(),
            )
        })
        .collect();
    candidates.sort_by_key(|(_, values)| values.len());

    let mut search = BestSubset {
        candidates: &candidates,
        items,
        used: vec![false; items.len()],
        selected: Vec::new(),
        best: HashSet::new(),
        scenarios: BTreeMap::new(),
        identities: BTreeMap::new(),
    };
    search.visit(0);
    search.best
}

/// The identity a matched candidate contributes to its same-item group. Only
/// deterministic +4 Imp rings can match under a transmuted identity.
fn matched_identity(requirement: &Requirement, candidate: &WorldItem) -> ItemId {
    match requirement.item {
        Some(wanted) if candidate.item != wanted => wanted,
        _ => candidate.item,
    }
}

struct BestSubset<'a> {
    candidates: &'a [RequirementCandidates],
    items: &'a [WorldItem],
    used: Vec<bool>,
    selected: Vec<usize>,
    best: HashSet<usize>,
    scenarios: BTreeMap<u16, u64>,
    identities: BTreeMap<u8, ItemId>,
}

impl BestSubset<'_> {
    fn visit(&mut self, position: usize) {
        if position == self.candidates.len() {
            if self.selected.len() > self.best.len() {
                self.best = self.selected.iter().copied().collect();
            }
            return;
        }
        // Even satisfying every remaining requirement cannot beat the best.
        if self.selected.len() + (self.candidates.len() - position) <= self.best.len() {
            return;
        }

        let (identity_group, requirement_candidates) = &self.candidates[position];
        for &(index, identity) in requirement_candidates {
            if self.used[index] {
                continue;
            }
            let mut previous_identity = None;
            if let Some(group) = identity_group {
                if self
                    .identities
                    .get(group)
                    .is_some_and(|wanted| *wanted != identity)
                {
                    continue;
                }
                previous_identity = Some((*group, self.identities.insert(*group, identity)));
            }
            let mut previous_scenarios = None;
            if let Some((group, mask)) = self.items[index].accessibility.scenario_constraint() {
                let compatible = self.scenarios.get(&group).copied().unwrap_or(u64::MAX) & mask;
                if compatible == 0 {
                    Self::rewind(&mut self.identities, previous_identity);
                    continue;
                }
                previous_scenarios = Some((group, self.scenarios.insert(group, compatible)));
            }

            self.used[index] = true;
            self.selected.push(index);
            self.visit(position + 1);
            self.selected.pop();
            self.used[index] = false;
            Self::rewind(&mut self.scenarios, previous_scenarios);
            Self::rewind(&mut self.identities, previous_identity);
        }
        // Also consider leaving this requirement unsatisfied.
        self.visit(position + 1);
    }

    fn rewind<K: Ord, V>(map: &mut BTreeMap<K, V>, previous: Option<(K, Option<V>)>) {
        if let Some((key, previous)) = previous {
            if let Some(previous) = previous {
                map.insert(key, previous);
            } else {
                map.remove(&key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use shpd_seedfinder_core::catalog::{ItemId, ItemKind};
    use shpd_seedfinder_core::model::{Accessibility, ItemSource, WorldItem};
    use shpd_seedfinder_core::query::UpgradeRequirement;

    use super::scout_match_indices;
    use crate::state::UiRequirement;

    fn world_item(item: ItemId, depth: u8, accessibility: Accessibility) -> WorldItem {
        WorldItem {
            item,
            transmuted_item: None,
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
}
