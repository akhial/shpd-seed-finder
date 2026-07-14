//! Multi-item query validation and accessibility-aware matching.

use std::collections::BTreeMap;
use std::fmt;

use crate::catalog::{Effect, ItemId, ItemKind, item};
use crate::challenges::Challenges;
use crate::model::{GeneratedWorld, ItemSource, WorldItem};

type CandidateMatch = (usize, ItemId);
type RequirementCandidates = (Option<u8>, Vec<CandidateMatch>);

/// Upgrade predicate attached to one item requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpgradeRequirement {
    Any,
    Exact(u8),
    AtLeast(u8),
}

/// Optional tier predicate for tiered equipment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TierRequirement {
    Any,
    Exact(u8),
    AtLeast(u8),
    AtMost(u8),
}

impl TierRequirement {
    fn matches(self, tier: Option<u8>) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(wanted) => tier == Some(wanted),
            Self::AtLeast(minimum) => tier.is_some_and(|tier| tier >= minimum),
            Self::AtMost(maximum) => tier.is_some_and(|tier| tier <= maximum),
        }
    }
}

impl UpgradeRequirement {
    const fn matches(self, upgrade: u8) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(wanted) => upgrade == wanted,
            Self::AtLeast(minimum) => upgrade >= minimum,
        }
    }
}

/// One required item. `None` fields are wildcards.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Requirement {
    pub kind: ItemKind,
    pub item: Option<ItemId>,
    pub tier: TierRequirement,
    pub upgrade: UpgradeRequirement,
    pub effect: Option<Effect>,
    pub source: Option<ItemSource>,
    /// Requirements in the same non-zero group must resolve to the same item ID.
    pub identity_group: Option<u8>,
    /// Optional inclusive floor limit for this item, independent of the query's
    /// overall generation limit.
    pub max_depth: Option<u8>,
}

impl Requirement {
    #[must_use]
    pub fn matches(self, candidate: &WorldItem) -> bool {
        self.matching_identity(candidate).is_some()
    }

    fn matching_identity(self, candidate: &WorldItem) -> Option<ItemId> {
        let identity = match self.item {
            None => candidate.item,
            Some(wanted) if wanted == candidate.item => candidate.item,
            Some(wanted) if candidate.upgrade == 4 && candidate.transmuted_item == Some(wanted) => {
                wanted
            }
            Some(_) => return None,
        };
        let definition = item(identity);
        (definition.kind == self.kind
            && self.tier.matches(definition.tier)
            && self.upgrade.matches(candidate.upgrade)
            && self
                .effect
                .is_none_or(|wanted| candidate.effect == Some(wanted))
            && self.source.is_none_or(|wanted| wanted == candidate.source))
        .then_some(identity)
    }

    /// Checks that an item/effect/upgrade combination is meaningful.
    ///
    /// # Errors
    ///
    /// Returns a validation error for a category mismatch, an effect intended
    /// for another family, or an upgrade outside the UI's family-specific range.
    pub fn validate(self) -> Result<(), QueryError> {
        if self
            .item
            .is_some_and(|item_id| item(item_id).kind != self.kind)
        {
            return Err(QueryError::ItemKindMismatch);
        }
        let tierable =
            self.item.is_none() && matches!(self.kind, ItemKind::Weapon | ItemKind::Armor);
        let valid_tier = match self.tier {
            TierRequirement::Any => true,
            TierRequirement::Exact(tier) => tierable && (2..=5).contains(&tier),
            TierRequirement::AtLeast(tier) | TierRequirement::AtMost(tier) => {
                tierable && (3..=4).contains(&tier)
            }
        };
        if !valid_tier {
            return Err(QueryError::InvalidTier);
        }
        let maximum = self.kind.maximum_search_upgrade();
        let valid_upgrade = match self.upgrade {
            UpgradeRequirement::Any => true,
            UpgradeRequirement::Exact(upgrade) => (1..=maximum).contains(&upgrade),
            UpgradeRequirement::AtLeast(upgrade) => upgrade <= maximum,
        };
        if !valid_upgrade {
            return Err(QueryError::InvalidUpgrade);
        }
        if self.identity_group == Some(0) {
            return Err(QueryError::InvalidIdentityGroup);
        }
        if self
            .max_depth
            .is_some_and(|depth| !(1..=24).contains(&depth))
        {
            return Err(QueryError::InvalidDepth);
        }
        match (self.kind, self.effect) {
            (ItemKind::Weapon, None | Some(Effect::Weapon(_)))
            | (ItemKind::Armor, None | Some(Effect::Armor(_)))
            | (ItemKind::Wand | ItemKind::Ring, None) => Ok(()),
            _ => Err(QueryError::EffectKindMismatch),
        }
    }
}

/// All requirements must be obtainable together in the same generated world.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchQuery {
    pub requirements: Vec<Requirement>,
    pub max_depth: u8,
    /// Upstream v3.3.8 challenge mask used while generating candidate worlds.
    pub challenges: Challenges,
    /// Whether an accessible blacksmith room must exist within `max_depth`.
    pub require_blacksmith: bool,
    /// Whether Blacksmith "Smith" rewards are ineligible to satisfy item
    /// requirements. The room may still be required separately for reforging.
    pub exclude_blacksmith_rewards: bool,
    /// Trades exhaustiveness for speed: +3 weapon/armor requirements are
    /// assumed to come from quest rewards, ignoring the far rarer Crypt and
    /// Sacrificial-fire prizes. Matches are still always genuine, but seeds
    /// whose only qualifying item comes from those rooms are skipped. See
    /// [`crate::feasibility`].
    pub fast_mode: bool,
}

impl SearchQuery {
    /// Validates bounds and every requirement.
    ///
    /// # Errors
    ///
    /// Returns a [`QueryError`] when no requirements are present, the selected
    /// depth is outside the main dungeon, or a requirement is inconsistent.
    pub fn validate(&self) -> Result<(), QueryError> {
        if self.requirements.is_empty() {
            return Err(QueryError::Empty);
        }
        if !(1..=24).contains(&self.max_depth) {
            return Err(QueryError::InvalidDepth);
        }
        let mut identity_groups: BTreeMap<u8, (ItemKind, Option<ItemId>)> = BTreeMap::new();
        for requirement in &self.requirements {
            requirement.validate()?;
            if let Some(group) = requirement.identity_group {
                let current = (requirement.kind, requirement.item);
                if let Some(previous) = identity_groups.get(&group).copied() {
                    if previous.0 != current.0
                        || previous
                            .1
                            .zip(current.1)
                            .is_some_and(|(left, right)| left != right)
                    {
                        return Err(QueryError::InconsistentIdentityGroup);
                    }
                    if previous.1.is_none() && current.1.is_some() {
                        identity_groups.insert(group, current);
                    }
                } else {
                    identity_groups.insert(group, current);
                }
            }
        }
        Ok(())
    }

    /// Matches requirements as an AND query while respecting distinct item
    /// instances and mutually exclusive quest/chest reward branches.
    #[must_use]
    pub fn matches(&self, world: &GeneratedWorld) -> bool {
        if self.requirements.len() > world.items.len() {
            return false;
        }
        if self.require_blacksmith
            && !world.items.iter().any(|candidate| {
                candidate.depth <= self.max_depth
                    && candidate.source == ItemSource::BlacksmithReward
            })
        {
            return false;
        }

        let mut candidates: Vec<RequirementCandidates> = self
            .requirements
            .iter()
            .map(|requirement| {
                (
                    requirement.identity_group,
                    world
                        .items
                        .iter()
                        .enumerate()
                        .filter_map(|(index, candidate)| {
                            (candidate.depth <= self.max_depth
                                && candidate.depth
                                    <= requirement.max_depth.unwrap_or(self.max_depth)
                                && (!self.exclude_blacksmith_rewards
                                    || candidate.source != ItemSource::BlacksmithReward))
                                .then(|| {
                                    requirement
                                        .matching_identity(candidate)
                                        .map(|identity| (index, identity))
                                })
                                .flatten()
                        })
                        .collect(),
                )
            })
            .collect();
        if candidates.iter().any(|(_, values)| values.is_empty()) {
            return false;
        }

        // Fail early by assigning the most constrained requirement first.
        candidates.sort_by_key(|(_, values)| values.len());
        let mut used = vec![false; world.items.len()];
        let mut scenarios = BTreeMap::new();
        let mut identities = BTreeMap::new();
        match_recursive(
            &candidates,
            0,
            &world.items,
            &mut used,
            &mut scenarios,
            &mut identities,
        )
    }
}

fn match_recursive(
    candidates: &[RequirementCandidates],
    requirement_index: usize,
    items: &[WorldItem],
    used: &mut [bool],
    scenarios: &mut BTreeMap<u16, u64>,
    identities: &mut BTreeMap<u8, ItemId>,
) -> bool {
    if requirement_index == candidates.len() {
        return true;
    }

    let (identity_group, requirement_candidates) = &candidates[requirement_index];
    for &(item_index, matched_identity) in requirement_candidates {
        if used[item_index] {
            continue;
        }
        let mut previous_identity = None;
        if let Some(group) = identity_group {
            if identities
                .get(group)
                .is_some_and(|wanted| *wanted != matched_identity)
            {
                continue;
            }
            previous_identity = Some((*group, identities.insert(*group, matched_identity)));
        }
        let mut previous_scenarios = None;
        if let Some((group, item_scenarios)) = items[item_index].accessibility.scenario_constraint()
        {
            let compatible = scenarios.get(&group).copied().unwrap_or(u64::MAX) & item_scenarios;
            if compatible == 0 {
                if let Some((identity_group, previous)) = previous_identity {
                    if let Some(previous) = previous {
                        identities.insert(identity_group, previous);
                    } else {
                        identities.remove(&identity_group);
                    }
                }
                continue;
            }
            previous_scenarios = Some((group, scenarios.insert(group, compatible)));
        }

        used[item_index] = true;
        if match_recursive(
            candidates,
            requirement_index + 1,
            items,
            used,
            scenarios,
            identities,
        ) {
            return true;
        }
        used[item_index] = false;
        if let Some((group, previous)) = previous_scenarios {
            if let Some(previous) = previous {
                scenarios.insert(group, previous);
            } else {
                scenarios.remove(&group);
            }
        }
        if let Some((group, previous)) = previous_identity {
            if let Some(previous) = previous {
                identities.insert(group, previous);
            } else {
                identities.remove(&group);
            }
        }
    }
    false
}

/// Invalid user query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryError {
    Empty,
    InvalidDepth,
    InvalidUpgrade,
    InvalidTier,
    ItemKindMismatch,
    EffectKindMismatch,
    InvalidIdentityGroup,
    InconsistentIdentityGroup,
}

impl fmt::Display for QueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Empty => "at least one item requirement is needed",
            Self::InvalidDepth => "maximum depth must be between 1 and 24",
            Self::InvalidUpgrade => "upgrade must be +1, +2, or +3 (+4 for rings)",
            Self::InvalidTier => {
                "tier filters require a wildcard weapon or armor and a non-redundant tier"
            }
            Self::ItemKindMismatch => "selected item is in a different category",
            Self::EffectKindMismatch => "selected enchantment or glyph is inapplicable",
            Self::InvalidIdentityGroup => "identity group zero is reserved for no group",
            Self::InconsistentIdentityGroup => {
                "linked item requirements must use the same category and item"
            }
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for QueryError {}

#[cfg(test)]
mod tests {
    use crate::catalog::{ItemId, ItemKind};
    use crate::model::{Accessibility, GeneratedWorld, ItemSource, WorldItem};
    use crate::seed::DungeonSeed;

    use super::{QueryError, Requirement, SearchQuery, TierRequirement, UpgradeRequirement};

    fn world_item(item: ItemId, accessibility: Accessibility) -> WorldItem {
        WorldItem {
            item,
            transmuted_item: None,
            upgrade: 2,
            effect: None,
            cursed: false,
            depth: 3,
            source: ItemSource::GhostReward,
            accessibility,
        }
    }

    fn requirement(item: ItemId) -> Requirement {
        Requirement {
            kind: crate::catalog::item(item).kind,
            item: Some(item),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Exact(2),
            effect: None,
            source: None,
            identity_group: None,
            max_depth: None,
        }
    }

    #[test]
    fn and_query_requires_distinct_item_occurrences() {
        let query = SearchQuery {
            requirements: vec![requirement(ItemId::Sword), requirement(ItemId::Sword)],
            max_depth: 4,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: false,
        };
        let one = GeneratedWorld {
            seed: DungeonSeed::MIN,
            items: vec![world_item(ItemId::Sword, Accessibility::Independent)],
        };
        assert!(!query.matches(&one));
        let two = GeneratedWorld {
            seed: DungeonSeed::MIN,
            items: vec![
                world_item(ItemId::Sword, Accessibility::Independent),
                world_item(ItemId::Sword, Accessibility::Independent),
            ],
        };
        assert!(query.matches(&two));
    }

    #[test]
    fn plus_four_ring_can_match_its_single_transmutation_roll() {
        let mut ring = world_item(ItemId::RingAccuracy, Accessibility::Independent);
        ring.upgrade = 4;
        ring.transmuted_item = Some(ItemId::RingWealth);
        let world = GeneratedWorld {
            seed: DungeonSeed::new(0).unwrap(),
            items: vec![ring],
        };
        let wealth = SearchQuery {
            requirements: vec![Requirement {
                kind: ItemKind::Ring,
                item: Some(ItemId::RingWealth),
                tier: TierRequirement::Any,
                upgrade: UpgradeRequirement::Exact(4),
                effect: None,
                source: Some(ItemSource::GhostReward),
                identity_group: None,
                max_depth: None,
            }],
            max_depth: 24,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: false,
        };
        assert!(wealth.matches(&world));

        let mut accuracy_and_wealth = wealth;
        accuracy_and_wealth.requirements.push(Requirement {
            kind: ItemKind::Ring,
            item: Some(ItemId::RingAccuracy),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Exact(4),
            effect: None,
            source: None,
            identity_group: None,
            max_depth: None,
        });
        assert!(!accuracy_and_wealth.matches(&world));
    }

    #[test]
    fn lower_level_ring_does_not_use_a_transmutation_roll() {
        let mut ring = world_item(ItemId::RingAccuracy, Accessibility::Independent);
        ring.upgrade = 3;
        ring.transmuted_item = Some(ItemId::RingWealth);
        let requirement = Requirement {
            kind: ItemKind::Ring,
            item: Some(ItemId::RingWealth),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Exact(3),
            effect: None,
            source: None,
            identity_group: None,
            max_depth: None,
        };

        assert!(!requirement.matches(&ring));
    }

    #[test]
    fn requirement_floor_limit_is_inclusive() {
        let world = GeneratedWorld {
            seed: DungeonSeed::MIN,
            items: vec![world_item(ItemId::Sword, Accessibility::Independent)],
        };
        let mut limited = requirement(ItemId::Sword);
        limited.max_depth = Some(2);
        let mut query = SearchQuery {
            requirements: vec![limited],
            max_depth: 24,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: false,
        };
        assert!(!query.matches(&world));
        query.requirements[0].max_depth = Some(3);
        assert!(query.matches(&world));
    }

    #[test]
    fn mutually_exclusive_rewards_cannot_satisfy_and_query() {
        let query = SearchQuery {
            requirements: vec![requirement(ItemId::Sword), requirement(ItemId::MailArmor)],
            max_depth: 4,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: false,
        };
        let world = GeneratedWorld {
            seed: DungeonSeed::MIN,
            items: vec![
                world_item(
                    ItemId::Sword,
                    Accessibility::Choice {
                        group: 1,
                        option: 0,
                    },
                ),
                world_item(
                    ItemId::MailArmor,
                    Accessibility::Choice {
                        group: 1,
                        option: 1,
                    },
                ),
            ],
        };
        assert!(!query.matches(&world));
    }

    #[test]
    fn same_choice_option_and_independent_rewards_can_match() {
        let query = SearchQuery {
            requirements: vec![requirement(ItemId::Sword), requirement(ItemId::MailArmor)],
            max_depth: 4,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: false,
        };
        let world = GeneratedWorld {
            seed: DungeonSeed::MIN,
            items: vec![
                world_item(
                    ItemId::Sword,
                    Accessibility::Choice {
                        group: 2,
                        option: 0,
                    },
                ),
                world_item(
                    ItemId::MailArmor,
                    Accessibility::Choice {
                        group: 2,
                        option: 0,
                    },
                ),
            ],
        };
        assert!(query.matches(&world));
    }

    #[test]
    fn scenario_masks_model_prerequisite_paths_without_false_choices() {
        let sword = world_item(
            ItemId::Sword,
            Accessibility::Scenarios {
                group: 7,
                mask: 0b0011,
            },
        );
        let armor = world_item(
            ItemId::MailArmor,
            Accessibility::Scenarios {
                group: 7,
                mask: 0b0110,
            },
        );
        let wand = world_item(
            ItemId::WandFrost,
            Accessibility::Scenarios {
                group: 7,
                mask: 0b1100,
            },
        );
        let world = GeneratedWorld {
            seed: DungeonSeed::MIN,
            items: vec![sword, armor, wand],
        };

        let compatible = SearchQuery {
            requirements: vec![requirement(ItemId::Sword), requirement(ItemId::MailArmor)],
            max_depth: 4,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: false,
        };
        assert!(compatible.matches(&world));

        let incompatible = SearchQuery {
            requirements: vec![requirement(ItemId::Sword), requirement(ItemId::WandFrost)],
            max_depth: 4,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: false,
        };
        assert!(!incompatible.matches(&world));
    }

    #[test]
    fn validation_rejects_wrong_category() {
        let invalid = Requirement {
            kind: ItemKind::Wand,
            item: Some(ItemId::Sword),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Exact(2),
            effect: None,
            source: None,
            identity_group: None,
            max_depth: None,
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn plus_four_is_valid_only_for_rings() {
        let ring = Requirement {
            kind: ItemKind::Ring,
            item: Some(ItemId::RingSharpshooting),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Exact(4),
            effect: None,
            source: None,
            identity_group: None,
            max_depth: None,
        };
        assert_eq!(ring.validate(), Ok(()));

        let wand = Requirement {
            kind: ItemKind::Wand,
            item: Some(ItemId::WandFrost),
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Exact(4),
            effect: None,
            source: None,
            identity_group: None,
            max_depth: None,
        };
        assert_eq!(wand.validate(), Err(QueryError::InvalidUpgrade));
    }

    #[test]
    fn tier_predicates_match_exact_minimum_and_maximum_tiers() {
        let tier_five = Requirement {
            kind: ItemKind::Weapon,
            item: None,
            tier: TierRequirement::Exact(5),
            upgrade: UpgradeRequirement::Exact(2),
            effect: None,
            source: None,
            identity_group: None,
            max_depth: None,
        };
        assert!(tier_five.matches(&world_item(ItemId::Greatsword, Accessibility::Independent)));
        assert!(!tier_five.matches(&world_item(ItemId::Longsword, Accessibility::Independent)));

        let tier_four_plus = Requirement {
            tier: TierRequirement::AtLeast(4),
            ..tier_five
        };
        assert!(tier_four_plus.matches(&world_item(ItemId::Longsword, Accessibility::Independent)));
        assert!(
            tier_four_plus.matches(&world_item(ItemId::Greatsword, Accessibility::Independent))
        );
        assert!(!tier_four_plus.matches(&world_item(ItemId::Sword, Accessibility::Independent)));

        let tier_four_or_lower = Requirement {
            tier: TierRequirement::AtMost(4),
            ..tier_five
        };
        assert!(
            tier_four_or_lower.matches(&world_item(ItemId::Longsword, Accessibility::Independent))
        );
        assert!(tier_four_or_lower.matches(&world_item(ItemId::Sword, Accessibility::Independent)));
        assert!(
            !tier_four_or_lower
                .matches(&world_item(ItemId::Greatsword, Accessibility::Independent))
        );

        let invalid = Requirement {
            kind: ItemKind::Wand,
            ..tier_five
        };
        assert_eq!(invalid.validate(), Err(QueryError::InvalidTier));

        let tier_one = Requirement {
            tier: TierRequirement::Exact(1),
            ..tier_five
        };
        assert_eq!(tier_one.validate(), Err(QueryError::InvalidTier));

        let redundant_maximum = Requirement {
            tier: TierRequirement::AtMost(5),
            ..tier_five
        };
        assert_eq!(redundant_maximum.validate(), Err(QueryError::InvalidTier));

        for redundant in [
            TierRequirement::AtLeast(2),
            TierRequirement::AtLeast(5),
            TierRequirement::AtMost(2),
        ] {
            assert_eq!(
                Requirement {
                    tier: redundant,
                    ..tier_five
                }
                .validate(),
                Err(QueryError::InvalidTier)
            );
        }
    }

    #[test]
    fn linked_wands_require_distinct_copies_and_a_blacksmith_in_range() {
        let linked = |upgrade, source| Requirement {
            kind: ItemKind::Wand,
            item: None,
            tier: TierRequirement::Any,
            upgrade,
            effect: None,
            source,
            identity_group: Some(1),
            max_depth: None,
        };
        let mut query = SearchQuery {
            requirements: vec![
                linked(
                    UpgradeRequirement::Exact(3),
                    Some(ItemSource::WandmakerReward),
                ),
                linked(UpgradeRequirement::AtLeast(0), None),
                linked(UpgradeRequirement::AtLeast(0), None),
                Requirement {
                    kind: ItemKind::Wand,
                    item: None,
                    tier: TierRequirement::Any,
                    upgrade: UpgradeRequirement::Exact(1),
                    effect: None,
                    source: None,
                    identity_group: None,
                    max_depth: None,
                },
            ],
            max_depth: 14,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: true,
            exclude_blacksmith_rewards: false,
            fast_mode: false,
        };
        let make = |item, upgrade, depth, source| WorldItem {
            item,
            transmuted_item: None,
            upgrade,
            effect: None,
            cursed: false,
            depth,
            source,
            accessibility: Accessibility::Independent,
        };
        let world = GeneratedWorld {
            seed: DungeonSeed::MIN,
            items: vec![
                make(ItemId::WandFrost, 3, 7, ItemSource::WandmakerReward),
                make(ItemId::WandFrost, 0, 2, ItemSource::Heap),
                make(ItemId::WandFrost, 1, 4, ItemSource::Chest),
                make(ItemId::WandLightning, 1, 5, ItemSource::Heap),
                make(ItemId::Sword, 2, 13, ItemSource::BlacksmithReward),
            ],
        };

        assert_eq!(query.validate(), Ok(()));
        assert!(query.matches(&world));

        let mut wrong_type = world.clone();
        wrong_type.items[2].item = ItemId::WandLightning;
        assert!(!query.matches(&wrong_type));

        query.max_depth = 12;
        assert!(!query.matches(&world));
    }

    #[test]
    fn smith_rewards_can_be_excluded_without_hiding_the_blacksmith() {
        let mut query = SearchQuery {
            requirements: vec![requirement(ItemId::Sword)],
            max_depth: 14,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: true,
            exclude_blacksmith_rewards: true,
            fast_mode: false,
        };
        let make = |source| WorldItem {
            item: ItemId::Sword,
            transmuted_item: None,
            upgrade: 2,
            effect: None,
            cursed: false,
            depth: 13,
            source,
            accessibility: Accessibility::Independent,
        };
        let smith_only = GeneratedWorld {
            seed: DungeonSeed::MIN,
            items: vec![make(ItemSource::BlacksmithReward)],
        };

        assert!(!query.matches(&smith_only));

        let mut reforging_setup = smith_only.clone();
        reforging_setup.items.push(make(ItemSource::Heap));
        assert!(query.matches(&reforging_setup));

        query.require_blacksmith = false;
        let no_blacksmith = GeneratedWorld {
            seed: DungeonSeed::MIN,
            items: vec![make(ItemSource::Heap)],
        };
        assert!(query.matches(&no_blacksmith));
    }

    #[test]
    fn wildcard_does_not_hide_conflicting_concrete_identity_group_members() {
        let linked = |item| Requirement {
            kind: ItemKind::Wand,
            item,
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Any,
            effect: None,
            source: None,
            identity_group: Some(1),
            max_depth: None,
        };
        let query = SearchQuery {
            requirements: vec![
                linked(Some(ItemId::WandFrost)),
                linked(None),
                linked(Some(ItemId::WandLightning)),
            ],
            max_depth: 24,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: false,
        };

        assert_eq!(query.validate(), Err(QueryError::InconsistentIdentityGroup));
    }
}
