// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared query state and presentation labels for the whole window.

use std::fmt::Write as _;

use shpd_seedfinder_core::catalog::{Effect, ItemId, ItemKind, item};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::model::ItemSource;
use shpd_seedfinder_core::query::{Requirement, SearchQuery, TierRequirement, UpgradeRequirement};

/// Every user-facing item source, in the wire order shared with the other
/// frontends.
pub const ALL_SOURCES: &[ItemSource] = &[
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
];

/// Every user-facing item family, in presentation order.
pub const ALL_KINDS: &[ItemKind] = &[
    ItemKind::Weapon,
    ItemKind::Armor,
    ItemKind::Wand,
    ItemKind::Ring,
];

/// One item requirement as edited in the interface. All predicate fields
/// mirror [`Requirement`]; `key` is a session-stable row identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiRequirement {
    pub key: u64,
    pub kind: ItemKind,
    pub item: Option<ItemId>,
    pub tier: TierRequirement,
    pub upgrade: UpgradeRequirement,
    pub effect: Option<Effect>,
    pub source: Option<ItemSource>,
    pub identity_group: Option<u8>,
    pub max_depth: Option<u8>,
}

impl UiRequirement {
    pub const fn new(key: u64) -> Self {
        Self {
            key,
            kind: ItemKind::Weapon,
            item: None,
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Any,
            effect: None,
            source: None,
            identity_group: None,
            max_depth: None,
        }
    }

    #[must_use]
    pub const fn to_core(self) -> Requirement {
        Requirement {
            kind: self.kind,
            item: self.item,
            tier: self.tier,
            upgrade: self.upgrade,
            effect: self.effect,
            source: self.source,
            identity_group: self.identity_group,
            max_depth: self.max_depth,
        }
    }

    /// Primary row label, e.g. `Any Tier 3+ weapon` or `Ring of tenacity`.
    #[must_use]
    pub fn title(&self) -> String {
        if let Some(item_id) = self.item {
            return item(item_id).name.to_owned();
        }
        match self.tier {
            TierRequirement::Any => format!("Any {}", kind_singular(self.kind)),
            TierRequirement::Exact(tier) => {
                format!("Any Tier {tier} {}", kind_singular(self.kind))
            }
            TierRequirement::AtLeast(tier) => {
                format!("Any Tier {tier}+ {}", kind_singular(self.kind))
            }
            TierRequirement::AtMost(tier) => {
                format!("Any Tier {tier} or lower {}", kind_singular(self.kind))
            }
        }
    }

    /// Secondary row label listing the remaining predicates.
    #[must_use]
    pub fn subtitle(&self) -> String {
        let mut text = match self.upgrade {
            UpgradeRequirement::Any => "Any upgrade".to_owned(),
            UpgradeRequirement::Exact(upgrade) => format!("+{upgrade} exactly"),
            UpgradeRequirement::AtLeast(upgrade) => format!("+{upgrade} or higher"),
        };
        if let Some(effect) = self.effect {
            let _ = write!(text, " · {}", effect.wire_name());
        }
        if let Some(source) = self.source {
            let _ = write!(text, " · {}", source_label(source));
        }
        if let Some(group) = self.identity_group {
            let _ = write!(text, " · same item group {}", group_letter(group));
        }
        if let Some(depth) = self.max_depth {
            let _ = write!(text, " · by floor {depth}");
        }
        text
    }
}

/// The whole persisted query state shared by all panes.
#[derive(Clone, Debug)]
pub struct AppState {
    pub requirements: Vec<UiRequirement>,
    pub max_depth: u8,
    pub require_blacksmith: bool,
    pub exclude_blacksmith_rewards: bool,
    pub fast_mode: bool,
    pub challenges: Challenges,
    next_key: u64,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            requirements: Vec::new(),
            max_depth: 24,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: false,
            challenges: Challenges::NONE,
            next_key: 1,
        }
    }
}

impl AppState {
    /// Hands out a fresh row key, unique within this session.
    pub const fn claim_key(&mut self) -> u64 {
        let key = self.next_key;
        self.next_key += 1;
        key
    }

    /// Builds the validated engine query for the current state.
    ///
    /// # Errors
    ///
    /// Returns the human-readable validation message.
    pub fn to_query(&self) -> Result<SearchQuery, String> {
        let query = SearchQuery {
            requirements: self.requirements.iter().map(|r| r.to_core()).collect(),
            max_depth: self.max_depth,
            challenges: self.challenges,
            require_blacksmith: self.require_blacksmith && self.max_depth < 14,
            exclude_blacksmith_rewards: self.exclude_blacksmith_rewards,
            fast_mode: self.fast_mode,
        };
        query.validate().map_err(|error| error.to_string())?;
        Ok(query)
    }
}

pub const fn kind_label(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Weapon => "Weapon",
        ItemKind::Armor => "Armor",
        ItemKind::Wand => "Wand",
        ItemKind::Ring => "Ring",
    }
}

pub const fn kind_singular(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Weapon => "weapon",
        ItemKind::Armor => "armor",
        ItemKind::Wand => "wand",
        ItemKind::Ring => "ring",
    }
}

/// Bundled symbolic icon name for one item family.
pub const fn kind_icon(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Weapon => "kind-weapon-symbolic",
        ItemKind::Armor => "kind-armor-symbolic",
        ItemKind::Wand => "kind-wand-symbolic",
        ItemKind::Ring => "kind-ring-symbolic",
    }
}

pub const fn source_label(source: ItemSource) -> &'static str {
    match source {
        ItemSource::Heap => "Floor",
        ItemSource::Chest => "Chest",
        ItemSource::LockedChest => "Locked chest",
        ItemSource::CrystalChest => "Crystal chest",
        ItemSource::Tomb => "Tomb",
        ItemSource::Skeleton => "Skeletal remains",
        ItemSource::SacrificialFire => "Sacrificial fire",
        ItemSource::Mimic => "Mimic",
        ItemSource::GoldenMimic => "Golden mimic",
        ItemSource::CrystalMimic => "Crystal mimic",
        ItemSource::Statue => "Animated statue",
        ItemSource::ArmoredStatue => "Armored statue",
        ItemSource::Shop => "Shop",
        ItemSource::GhostReward => "Sad ghost reward",
        ItemSource::WandmakerReward => "Wandmaker reward",
        ItemSource::BlacksmithReward => "Blacksmith reward",
        ItemSource::ImpReward => "Imp reward",
    }
}

pub const fn group_letter(group: u8) -> char {
    match group {
        1 => 'A',
        2 => 'B',
        3 => 'C',
        _ => 'D',
    }
}

/// Dungeon region name for one depth.
pub const fn region(depth: u8) -> &'static str {
    match depth {
        0..=5 => "Sewers",
        6..=10 => "Prison",
        11..=15 => "Caves",
        16..=20 => "Dwarven City",
        _ => "Demon Halls",
    }
}

/// One upstream challenge with presentation data.
pub struct ChallengeInfo {
    pub challenge: Challenges,
    pub label: &'static str,
    pub changes_generation: bool,
}

/// The nine upstream challenges, in mask order.
pub const ALL_CHALLENGES: &[ChallengeInfo] = &[
    ChallengeInfo {
        challenge: Challenges::NO_FOOD,
        label: "On diet",
        changes_generation: false,
    },
    ChallengeInfo {
        challenge: Challenges::NO_ARMOR,
        label: "Faith is my armor",
        changes_generation: false,
    },
    ChallengeInfo {
        challenge: Challenges::NO_HEALING,
        label: "Pharmacophobia",
        changes_generation: false,
    },
    ChallengeInfo {
        challenge: Challenges::NO_HERBALISM,
        label: "Barren land",
        changes_generation: true,
    },
    ChallengeInfo {
        challenge: Challenges::SWARM_INTELLIGENCE,
        label: "Swarm intelligence",
        changes_generation: false,
    },
    ChallengeInfo {
        challenge: Challenges::DARKNESS,
        label: "Into darkness",
        changes_generation: true,
    },
    ChallengeInfo {
        challenge: Challenges::NO_SCROLLS,
        label: "Forbidden runes",
        changes_generation: true,
    },
    ChallengeInfo {
        challenge: Challenges::CHAMPION_ENEMIES,
        label: "Hostile champions",
        changes_generation: false,
    },
    ChallengeInfo {
        challenge: Challenges::STRONGER_BOSSES,
        label: "Badder bosses",
        changes_generation: false,
    },
];

#[cfg(test)]
mod tests {
    use shpd_seedfinder_core::catalog::ItemId;
    use shpd_seedfinder_core::query::{TierRequirement, UpgradeRequirement};

    use super::{AppState, UiRequirement};

    #[test]
    fn labels_describe_wildcards_and_predicates() {
        let mut requirement = UiRequirement::new(1);
        assert_eq!(requirement.title(), "Any weapon");
        assert_eq!(requirement.subtitle(), "Any upgrade");

        requirement.tier = TierRequirement::AtLeast(4);
        requirement.upgrade = UpgradeRequirement::Exact(2);
        requirement.identity_group = Some(2);
        requirement.max_depth = Some(9);
        assert_eq!(requirement.title(), "Any Tier 4+ weapon");
        assert_eq!(
            requirement.subtitle(),
            "+2 exactly · same item group B · by floor 9"
        );

        requirement.tier = TierRequirement::AtMost(3);
        assert_eq!(requirement.title(), "Any Tier 3 or lower weapon");

        requirement.item = Some(ItemId::Greatsword);
        assert_eq!(requirement.title(), "Greatsword");
    }

    #[test]
    fn query_drops_blacksmith_requirement_at_depth_fourteen() {
        let mut state = AppState::default();
        let key = state.claim_key();
        state.requirements.push(UiRequirement::new(key));
        state.require_blacksmith = true;
        state.max_depth = 14;
        assert!(!state.to_query().unwrap().require_blacksmith);
        state.max_depth = 13;
        assert!(state.to_query().unwrap().require_blacksmith);
    }
}
