//! Query probability estimates derived from the canonical v3.3.8 item tables.

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::{ArmorEffect, Effect, ItemKind, WeaponEffect, item};
use crate::generator::FLOOR_SET_TIER_PROBABILITIES;
use crate::model::ItemSource;
use crate::query::{Requirement, SearchQuery, UpgradeRequirement};

/// Estimates a query's intrinsic spawn probability before traversal starts.
///
/// Each requirement is evaluated from the same tier, identity, upgrade, and
/// modifier weights used by generation, then AND requirements are multiplied.
/// This deliberately remains fixed for a search; observed results never feed
/// back into the estimate.
#[must_use]
pub fn estimate_match_probability(query: &SearchQuery) -> f64 {
    let grouped_items: BTreeMap<_, _> = query
        .requirements
        .iter()
        .filter_map(|requirement| requirement.identity_group.zip(requirement.item))
        .collect();
    let mut seen_groups = BTreeSet::new();
    let mut probability = 1.0;
    for requirement in &query.requirements {
        let identity = if let Some(group) = requirement.identity_group {
            let first = seen_groups.insert(group);
            if let Some(group_item) = grouped_items.get(&group).copied() {
                identity_probability(
                    Requirement {
                        item: Some(group_item),
                        ..*requirement
                    },
                    query.max_depth,
                )
            } else if first {
                1.0
            } else {
                wildcard_identity_collision_probability(requirement.kind, query.max_depth)
            }
        } else {
            identity_probability(*requirement, query.max_depth)
        };
        probability *= identity
            * upgrade_probability_for_requirement(*requirement)
            * modifier_probability_for_requirement(*requirement)
            * source_availability(*requirement, query.max_depth);
    }
    probability.clamp(0.0, 1.0)
}

fn identity_probability(requirement: Requirement, max_depth: u8) -> f64 {
    let Some(item_id) = requirement.item else {
        return 1.0;
    };
    let definition = item(item_id);
    if requirement.source == Some(ItemSource::GhostReward) {
        return ghost_identity_probability(requirement);
    }
    match requirement.kind {
        ItemKind::Weapon => {
            let tier = usize::from(definition.tier.unwrap_or(1)).saturating_sub(1);
            average_tier_probability(tier, max_depth) / f64::from(weapon_identity_count(tier))
        }
        ItemKind::Armor => {
            let tier = usize::from(definition.tier.unwrap_or(1)).saturating_sub(1);
            average_tier_probability(tier, max_depth)
        }
        ItemKind::Wand => 1.0 / 13.0,
        ItemKind::Ring => 1.0 / 12.0,
    }
}

fn ghost_identity_probability(requirement: Requirement) -> f64 {
    let Some(item_id) = requirement.item else {
        return 1.0;
    };
    let definition = item(item_id);
    let tier = usize::from(definition.tier.unwrap_or(1));
    let tier_probability = match tier {
        2 => 0.50,
        3 => 0.30,
        4 => 0.15,
        5 => 0.05,
        _ => 0.0,
    };
    match requirement.kind {
        ItemKind::Armor => tier_probability,
        ItemKind::Weapon => {
            tier_probability / f64::from(weapon_identity_count(tier.saturating_sub(1)))
        }
        ItemKind::Wand | ItemKind::Ring => 0.0,
    }
}

fn wildcard_identity_collision_probability(kind: ItemKind, max_depth: u8) -> f64 {
    match kind {
        ItemKind::Wand => 1.0 / 13.0,
        ItemKind::Ring => 1.0 / 12.0,
        ItemKind::Armor => (0..5)
            .map(|tier| average_tier_probability(tier, max_depth).powi(2))
            .sum(),
        ItemKind::Weapon => (0..5)
            .map(|tier| {
                average_tier_probability(tier, max_depth).powi(2)
                    / f64::from(weapon_identity_count(tier))
            })
            .sum(),
    }
}

fn average_tier_probability(tier: usize, max_depth: u8) -> f64 {
    let mut total = 0.0;
    let mut floors = 0_u32;
    for depth in 1..=max_depth {
        if depth % 5 == 0 {
            continue;
        }
        let floor_set = usize::from((depth - 1) / 5).min(4);
        total += f64::from(FLOOR_SET_TIER_PROBABILITIES[floor_set][tier]) / 100.0;
        floors += 1;
    }
    if floors == 0 {
        0.0
    } else {
        total / f64::from(floors)
    }
}

const fn weapon_identity_count(tier: usize) -> u32 {
    match tier {
        0 => 5,
        1 | 2 => 6,
        _ => 7,
    }
}

const fn upgrade_probability(kind: ItemKind, requirement: UpgradeRequirement) -> f64 {
    match requirement {
        UpgradeRequirement::Any => 1.0,
        UpgradeRequirement::Exact(upgrade) => exact_upgrade_probability(kind, upgrade),
        UpgradeRequirement::AtLeast(minimum) => {
            let maximum = kind.maximum_search_upgrade();
            let mut probability = 0.0;
            let mut upgrade = minimum;
            while upgrade <= maximum {
                probability += exact_upgrade_probability(kind, upgrade);
                upgrade += 1;
            }
            if probability > 1.0 { 1.0 } else { probability }
        }
    }
}

const fn upgrade_probability_for_requirement(requirement: Requirement) -> f64 {
    if matches!(requirement.source, Some(ItemSource::GhostReward)) {
        return ghost_upgrade_probability(requirement.upgrade);
    }
    upgrade_probability(requirement.kind, requirement.upgrade)
}

const fn ghost_upgrade_probability(requirement: UpgradeRequirement) -> f64 {
    match requirement {
        UpgradeRequirement::Any | UpgradeRequirement::AtLeast(0) => 1.0,
        UpgradeRequirement::Exact(0) | UpgradeRequirement::AtLeast(1) => 0.50,
        UpgradeRequirement::Exact(1) => 0.30,
        UpgradeRequirement::Exact(2) => 0.15,
        UpgradeRequirement::AtLeast(2) => 0.20,
        UpgradeRequirement::Exact(3) | UpgradeRequirement::AtLeast(3) => 0.05,
        UpgradeRequirement::Exact(_) | UpgradeRequirement::AtLeast(_) => 0.0,
    }
}

const fn exact_upgrade_probability(kind: ItemKind, upgrade: u8) -> f64 {
    match (kind, upgrade) {
        (_, 0) => match kind {
            ItemKind::Weapon | ItemKind::Armor => 0.75,
            ItemKind::Wand | ItemKind::Ring => 2.0 / 3.0,
        },
        (ItemKind::Weapon | ItemKind::Armor, 1) => 0.20,
        (ItemKind::Weapon | ItemKind::Armor, 2 | 3) => 0.05,
        (ItemKind::Wand | ItemKind::Ring, 1) | (ItemKind::Ring, 3) => 4.0 / 15.0,
        // Wandmaker and Imp rewards add one and two upgrades respectively.
        (ItemKind::Wand | ItemKind::Ring, 2) | (ItemKind::Wand, 3) | (ItemKind::Ring, 4) => {
            1.0 / 15.0
        }
        _ => 0.0,
    }
}

const fn modifier_probability(effect: Option<Effect>) -> f64 {
    match effect {
        None => 1.0,
        Some(Effect::Weapon(effect)) if effect.is_curse() => 0.30 / 8.0,
        Some(Effect::Weapon(effect)) => 0.10 * weapon_enchantment_probability(effect),
        Some(Effect::Armor(effect)) if effect.is_curse() => 0.30 / 8.0,
        Some(Effect::Armor(effect)) => 0.15 * armor_glyph_probability(effect),
    }
}

fn modifier_probability_for_requirement(requirement: Requirement) -> f64 {
    if requirement.source != Some(ItemSource::GhostReward) {
        return modifier_probability(requirement.effect);
    }
    match requirement.effect {
        None => 1.0,
        Some(Effect::Weapon(effect)) if effect.is_curse() => 0.0,
        Some(Effect::Weapon(effect)) => 0.20 * weapon_enchantment_probability(effect),
        Some(Effect::Armor(effect)) if effect.is_curse() => 0.0,
        Some(Effect::Armor(effect)) => 0.20 * armor_glyph_probability(effect),
    }
}

fn source_availability(requirement: Requirement, max_depth: u8) -> f64 {
    if requirement.source != Some(ItemSource::GhostReward) {
        return 1.0;
    }
    match max_depth {
        0 | 1 => 0.0,
        2 => 1.0 / 3.0,
        3 => 2.0 / 3.0,
        _ => 1.0,
    }
}

const fn weapon_enchantment_probability(effect: WeaponEffect) -> f64 {
    match effect as u8 {
        0..=3 => 0.50 / 4.0,
        4..=9 => 0.40 / 6.0,
        _ => 0.10 / 3.0,
    }
}

const fn armor_glyph_probability(effect: ArmorEffect) -> f64 {
    match effect as u8 {
        0..=3 => 0.50 / 4.0,
        4..=9 => 0.40 / 6.0,
        _ => 0.10 / 3.0,
    }
}

#[cfg(test)]
mod tests {
    use crate::catalog::{ArmorEffect, Effect, ItemId, ItemKind, WeaponEffect};
    use crate::model::ItemSource;
    use crate::query::{Requirement, SearchQuery, UpgradeRequirement};

    use super::estimate_match_probability;

    fn query(requirements: Vec<Requirement>) -> SearchQuery {
        SearchQuery {
            requirements,
            max_depth: 24,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: false,
        }
    }

    #[test]
    fn combines_canonical_identity_upgrade_and_modifier_weights() {
        let probability = estimate_match_probability(&query(vec![
            Requirement {
                kind: ItemKind::Weapon,
                item: Some(ItemId::Sword),
                upgrade: UpgradeRequirement::Exact(2),
                effect: Some(Effect::Weapon(WeaponEffect::Lucky)),
                source: None,
                identity_group: None,
                max_depth: None,
            },
            Requirement {
                kind: ItemKind::Armor,
                item: Some(ItemId::PlateArmor),
                upgrade: UpgradeRequirement::Exact(1),
                effect: Some(Effect::Armor(ArmorEffect::Brimstone)),
                source: None,
                identity_group: None,
                max_depth: None,
            },
        ]));

        assert!((probability - 7.857_777_777_777_78e-9).abs() < 1e-20);
    }

    #[test]
    fn same_item_groups_require_additional_matching_instances() {
        let probability = estimate_match_probability(&query(vec![
            Requirement {
                kind: ItemKind::Armor,
                item: Some(ItemId::MailArmor),
                upgrade: UpgradeRequirement::Exact(3),
                effect: None,
                source: Some(ItemSource::GhostReward),
                identity_group: None,
                max_depth: None,
            },
            Requirement {
                kind: ItemKind::Wand,
                item: None,
                upgrade: UpgradeRequirement::Exact(3),
                effect: None,
                source: None,
                identity_group: Some(1),
                max_depth: None,
            },
            Requirement {
                kind: ItemKind::Wand,
                item: None,
                upgrade: UpgradeRequirement::Any,
                effect: None,
                source: None,
                identity_group: Some(1),
                max_depth: None,
            },
            Requirement {
                kind: ItemKind::Wand,
                item: None,
                upgrade: UpgradeRequirement::Any,
                effect: None,
                source: None,
                identity_group: Some(1),
                max_depth: None,
            },
            Requirement {
                kind: ItemKind::Wand,
                item: None,
                upgrade: UpgradeRequirement::AtLeast(1),
                effect: None,
                source: None,
                identity_group: None,
                max_depth: None,
            },
            Requirement {
                kind: ItemKind::Ring,
                item: None,
                upgrade: UpgradeRequirement::Exact(4),
                effect: None,
                source: None,
                identity_group: None,
                max_depth: None,
            },
        ]));

        assert!(
            (probability - 1.577_909_270_216_57e-7).abs() < 1e-18,
            "{probability:e}"
        );
    }
}
