// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared query state and presentation labels for the whole window.

use std::fmt::Write as _;

use shpd_seedfinder_core::catalog::{Effect, ItemId, ItemKind, WeaponCategory, item};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::feasibility::Quest;
use shpd_seedfinder_core::main_world::EMPTY_BOSS_FLOORS;
use shpd_seedfinder_core::model::ItemSource;
use shpd_seedfinder_core::query::{Requirement, SearchQuery, TierRequirement, UpgradeRequirement};
use shpd_seedfinder_core::quests::{
    BlacksmithQuestType, GhostQuestType, ImpTarget, QuestSummary, WandmakerQuestType,
};

/// Where a floor-limit control lands when the user moves it onto an empty
/// boss floor. A single upward step (spin button, arrow key, scroll)
/// continues to the next real floor; every other move — single steps down
/// and typed jumps in either direction — snaps to the equivalent floor
/// below, matching
/// [`shpd_seedfinder_core::main_world::normalize_floor_limit`]. Typing "10"
/// therefore means "first 10 floors" (≡ 9), never 11.
#[must_use]
pub fn floor_limit_skip_target(previous: u8, requested: u8) -> u8 {
    if !EMPTY_BOSS_FLOORS.contains(&requested) {
        requested
    } else if requested == previous.saturating_add(1) {
        requested + 1
    } else {
        requested - 1
    }
}

/// One entry in the requirement editor's category picker: an item family,
/// optionally narrowed to one weapon class.
pub type KindChoice = (ItemKind, Option<WeaponCategory>);

/// Every user-facing category choice, in presentation order. A plain weapon
/// requirement keeps matching melee and thrown weapons alike.
pub const ALL_KIND_CHOICES: &[KindChoice] = &[
    (ItemKind::Weapon, None),
    (ItemKind::Weapon, Some(WeaponCategory::Melee)),
    (ItemKind::Weapon, Some(WeaponCategory::Thrown)),
    (ItemKind::Armor, None),
    (ItemKind::Wand, None),
    (ItemKind::Ring, None),
];

/// One item requirement as edited in the interface. All predicate fields
/// mirror [`Requirement`]; `key` is a session-stable row identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiRequirement {
    pub key: u64,
    pub kind: ItemKind,
    /// Optional melee/thrown narrowing for weapon requirements.
    pub weapon_category: Option<WeaponCategory>,
    pub item: Option<ItemId>,
    pub tier: TierRequirement,
    pub upgrade: UpgradeRequirement,
    pub effect: Option<Effect>,
    pub require_uncursed: bool,
    pub source: Option<ItemSource>,
    pub identity_group: Option<u8>,
    pub max_depth: Option<u8>,
}

impl UiRequirement {
    pub const fn new(key: u64) -> Self {
        Self {
            key,
            kind: ItemKind::Weapon,
            weapon_category: None,
            item: None,
            tier: TierRequirement::Any,
            upgrade: UpgradeRequirement::Any,
            effect: None,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
        }
    }

    #[must_use]
    pub const fn to_core(self) -> Requirement {
        Requirement {
            kind: self.kind,
            weapon_category: self.weapon_category,
            item: self.item,
            tier: self.tier,
            upgrade: self.upgrade,
            effect: self.effect,
            require_uncursed: self.require_uncursed,
            source: self.source,
            identity_group: self.identity_group,
            max_depth: self.max_depth,
        }
    }

    /// The editor category choice this requirement uses.
    #[must_use]
    pub const fn kind_choice(&self) -> KindChoice {
        (self.kind, self.weapon_category)
    }

    /// Primary row label, e.g. `Any Tier 3+ thrown weapon` or `Ring of tenacity`.
    #[must_use]
    pub fn title(&self) -> String {
        if let Some(item_id) = self.item {
            return item(item_id).name.to_owned();
        }
        let singular = kind_choice_singular(self.kind_choice());
        match self.tier {
            TierRequirement::Any => format!("Any {singular}"),
            TierRequirement::Exact(tier) => {
                format!("Any Tier {tier} {singular}")
            }
            TierRequirement::AtLeast(tier) => {
                format!("Any Tier {tier}+ {singular}")
            }
            TierRequirement::AtMost(tier) => {
                format!("Any Tier {tier} or lower {singular}")
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
        if self.require_uncursed {
            text.push_str(" · uncursed");
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
    pub wandmaker_quest: Option<WandmakerQuestType>,
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
            wandmaker_quest: None,
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

    /// Rebuilds editor state from a decoded engine query, assigning fresh
    /// session row keys.
    #[must_use]
    pub fn from_query(query: &SearchQuery) -> Self {
        let mut state = Self {
            requirements: Vec::with_capacity(query.requirements.len()),
            max_depth: query.max_depth,
            require_blacksmith: query.require_blacksmith,
            exclude_blacksmith_rewards: query.exclude_blacksmith_rewards,
            wandmaker_quest: query.wandmaker_quest,
            fast_mode: query.fast_mode,
            challenges: query.challenges,
            next_key: 1,
        };
        for requirement in &query.requirements {
            let key = state.claim_key();
            state.requirements.push(UiRequirement {
                key,
                kind: requirement.kind,
                weapon_category: requirement.weapon_category,
                item: requirement.item,
                tier: requirement.tier,
                upgrade: requirement.upgrade,
                effect: requirement.effect,
                require_uncursed: requirement.require_uncursed,
                source: requirement.source,
                identity_group: requirement.identity_group,
                max_depth: requirement.max_depth,
            });
        }
        state
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
            // Past the last floor the Blacksmith can first appear on the
            // quest is certain, so the filter would exclude nothing.
            require_blacksmith: self.require_blacksmith
                && self.max_depth < Quest::Blacksmith.window().1,
            exclude_blacksmith_rewards: self.exclude_blacksmith_rewards,
            wandmaker_quest: self.wandmaker_quest,
            fast_mode: self.fast_mode,
        };
        query.validate().map_err(|error| error.to_string())?;
        Ok(query)
    }
}

pub const fn kind_choice_label(choice: KindChoice) -> &'static str {
    match choice {
        (ItemKind::Weapon, None) => "Weapon",
        (ItemKind::Weapon, Some(WeaponCategory::Melee)) => "Melee weapon",
        (ItemKind::Weapon, Some(WeaponCategory::Thrown)) => "Thrown weapon",
        (ItemKind::Armor, _) => "Armor",
        (ItemKind::Wand, _) => "Wand",
        (ItemKind::Ring, _) => "Ring",
    }
}

pub const fn kind_choice_singular(choice: KindChoice) -> &'static str {
    match choice {
        (ItemKind::Weapon, None) => "weapon",
        (ItemKind::Weapon, Some(WeaponCategory::Melee)) => "melee weapon",
        (ItemKind::Weapon, Some(WeaponCategory::Thrown)) => "thrown weapon",
        (ItemKind::Armor, _) => "armor",
        (ItemKind::Wand, _) => "wand",
        (ItemKind::Ring, _) => "ring",
    }
}

/// Bundled symbolic icon name for one item family, with a dedicated glyph
/// for thrown weapons.
pub const fn kind_icon(kind: ItemKind, weapon_category: Option<WeaponCategory>) -> &'static str {
    match (kind, weapon_category) {
        (ItemKind::Weapon, Some(WeaponCategory::Thrown)) => "kind-weapon-thrown-symbolic",
        (ItemKind::Weapon, _) => "kind-weapon-symbolic",
        (ItemKind::Armor, _) => "kind-armor-symbolic",
        (ItemKind::Wand, _) => "kind-wand-symbolic",
        (ItemKind::Ring, _) => "kind-ring-symbolic",
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

pub const fn ghost_quest_label(variant: GhostQuestType) -> &'static str {
    match variant {
        GhostQuestType::FetidRat => "Fetid rat",
        GhostQuestType::GnollTrickster => "Gnoll trickster",
        GhostQuestType::GreatCrab => "Great crab",
    }
}

pub const fn wandmaker_quest_label(variant: WandmakerQuestType) -> &'static str {
    match variant {
        WandmakerQuestType::CorpseDust => "Corpse dust",
        WandmakerQuestType::ElementalEmbers => "Elemental embers",
        WandmakerQuestType::Rotberry => "Rotberry",
    }
}

pub const fn blacksmith_quest_label(variant: BlacksmithQuestType) -> &'static str {
    match variant {
        BlacksmithQuestType::Crystal => "Crystal spire",
        BlacksmithQuestType::Gnoll => "Gnoll geomancer",
    }
}

pub const fn imp_target_label(target: ImpTarget) -> &'static str {
    match target {
        ImpTarget::Monk => "Monks",
        ImpTarget::Golem => "Golems",
    }
}

/// One scheduled quest prepared for presentation: the giver's name, the rolled
/// variant's label, and the giver's floor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuestRow {
    pub giver: &'static str,
    pub variant: &'static str,
    pub depth: u8,
}

/// The quests scheduled in one world, in dungeon order.
#[must_use]
pub fn quest_rows(quests: QuestSummary) -> Vec<QuestRow> {
    let mut rows = Vec::with_capacity(4);
    if let Some(quest) = quests.ghost {
        rows.push(QuestRow {
            giver: "Sad ghost",
            variant: ghost_quest_label(quest.variant),
            depth: quest.depth,
        });
    }
    if let Some(quest) = quests.wandmaker {
        rows.push(QuestRow {
            giver: "Wandmaker",
            variant: wandmaker_quest_label(quest.variant),
            depth: quest.depth,
        });
    }
    if let Some(quest) = quests.blacksmith {
        rows.push(QuestRow {
            giver: "Blacksmith",
            variant: blacksmith_quest_label(quest.variant),
            depth: quest.depth,
        });
    }
    if let Some(quest) = quests.imp {
        rows.push(QuestRow {
            giver: "Imp",
            variant: imp_target_label(quest.variant),
            depth: quest.depth,
        });
    }
    rows
}

/// One upstream challenge with presentation data.
pub struct ChallengeInfo {
    pub challenge: Challenges,
    pub label: &'static str,
}

/// The nine upstream challenges, in mask order.
pub const ALL_CHALLENGES: &[ChallengeInfo] = &[
    ChallengeInfo {
        challenge: Challenges::NO_FOOD,
        label: "On diet",
    },
    ChallengeInfo {
        challenge: Challenges::NO_ARMOR,
        label: "Faith is my armor",
    },
    ChallengeInfo {
        challenge: Challenges::NO_HEALING,
        label: "Pharmacophobia",
    },
    ChallengeInfo {
        challenge: Challenges::NO_HERBALISM,
        label: "Barren land",
    },
    ChallengeInfo {
        challenge: Challenges::SWARM_INTELLIGENCE,
        label: "Swarm intelligence",
    },
    ChallengeInfo {
        challenge: Challenges::DARKNESS,
        label: "Into darkness",
    },
    ChallengeInfo {
        challenge: Challenges::NO_SCROLLS,
        label: "Forbidden runes",
    },
    ChallengeInfo {
        challenge: Challenges::CHAMPION_ENEMIES,
        label: "Hostile champions",
    },
    ChallengeInfo {
        challenge: Challenges::STRONGER_BOSSES,
        label: "Badder bosses",
    },
];

#[cfg(test)]
mod tests {
    use shpd_seedfinder_core::catalog::{ItemId, ItemKind};
    use shpd_seedfinder_core::query::{TierRequirement, UpgradeRequirement};
    use shpd_seedfinder_core::quests::{
        BlacksmithQuestType, GhostQuestType, ImpTarget, QuestSummary, ScheduledQuest,
        WandmakerQuestType,
    };

    use super::{
        AppState, QuestRow, UiRequirement, blacksmith_quest_label, floor_limit_skip_target,
        ghost_quest_label, imp_target_label, quest_rows, wandmaker_quest_label,
    };

    #[test]
    fn refinement_requires_identical_scope_and_no_fewer_requirements() {
        let mut base_state = AppState::default();
        let mut first = UiRequirement::new(base_state.claim_key());
        first.kind = ItemKind::Ring;
        first.upgrade = UpgradeRequirement::AtLeast(2);
        base_state.requirements.push(first);
        let base = base_state.to_query().unwrap();

        // Adding a requirement refines; row keys are irrelevant.
        let mut extended_state = base_state.clone();
        let mut added = UiRequirement::new(999);
        added.kind = ItemKind::Weapon;
        added.upgrade = UpgradeRequirement::Exact(3);
        extended_state.requirements.push(added);
        let extended = extended_state.to_query().unwrap();
        assert!(extended.continues(&base));

        // An identical query still qualifies: the filter keeps every seed and
        // the scan resumes, so a stopped session continues instead of resetting.
        assert!(base.continues(&base));

        // Tightening a base requirement strengthens the query, so it still
        // continues: every match it can find was already a base match.
        let mut tightened = extended.clone();
        tightened.requirements[0].upgrade = UpgradeRequirement::AtLeast(3);
        assert!(tightened.continues(&base));
        let mut named = extended.clone();
        named.requirements[0].item = Some(ItemId::RingArcana);
        assert!(named.continues(&base));

        // Dropping a requirement, loosening a base requirement, and any
        // scope change all force a fresh search instead.
        assert!(!base.continues(&extended));
        let mut loosened = extended.clone();
        loosened.requirements[0].upgrade = UpgradeRequirement::AtLeast(1);
        assert!(!loosened.continues(&base));
        let mut deeper = extended.clone();
        deeper.max_depth = 9;
        assert!(!deeper.continues(&base));
        let mut fast = extended.clone();
        fast.fast_mode = true;
        assert!(!fast.continues(&base));

        // Duplicates are counted as a multiset: two copies of the base
        // requirement satisfy a two-copy base, one copy does not.
        let mut doubled_base = base.clone();
        doubled_base.requirements.push(base.requirements[0]);
        let mut doubled_extended = doubled_base.clone();
        doubled_extended.requirements.push(extended.requirements[1]);
        assert!(doubled_extended.continues(&doubled_base));
        assert!(!extended.continues(&doubled_base));
    }

    #[test]
    fn single_upward_steps_skip_forward_and_everything_else_snaps_down() {
        // Spinning up from the floor below an empty boss floor lands above it.
        assert_eq!(floor_limit_skip_target(4, 5), 6);
        assert_eq!(floor_limit_skip_target(9, 10), 11);
        assert_eq!(floor_limit_skip_target(14, 15), 16);
        // Spinning down lands on the equivalent floor below.
        assert_eq!(floor_limit_skip_target(6, 5), 4);
        assert_eq!(floor_limit_skip_target(11, 10), 9);
        assert_eq!(floor_limit_skip_target(16, 15), 14);
        // Typed jumps snap down: "10" means the first 10 floors (≡ 9), never 11.
        assert_eq!(floor_limit_skip_target(4, 10), 9);
        assert_eq!(floor_limit_skip_target(24, 15), 14);
        assert_eq!(floor_limit_skip_target(4, 15), 14);
        assert_eq!(floor_limit_skip_target(20, 5), 4);
        // Non-boss floors pass through untouched.
        assert_eq!(floor_limit_skip_target(4, 6), 6);
        assert_eq!(floor_limit_skip_target(24, 1), 1);
        assert_eq!(floor_limit_skip_target(1, 24), 24);
    }

    #[test]
    fn labels_describe_wildcards_and_predicates() {
        let mut requirement = UiRequirement::new(1);
        assert_eq!(requirement.title(), "Any weapon");
        assert_eq!(requirement.subtitle(), "Any upgrade");

        requirement.tier = TierRequirement::AtLeast(4);
        requirement.upgrade = UpgradeRequirement::Exact(2);
        requirement.identity_group = Some(2);
        requirement.max_depth = Some(9);
        requirement.require_uncursed = true;
        assert_eq!(requirement.title(), "Any Tier 4+ weapon");
        assert_eq!(
            requirement.subtitle(),
            "+2 exactly · uncursed · same item group B · by floor 9"
        );

        requirement.tier = TierRequirement::AtMost(3);
        assert_eq!(requirement.title(), "Any Tier 3 or lower weapon");

        requirement.item = Some(ItemId::Greatsword);
        assert_eq!(requirement.title(), "Greatsword");
    }

    #[test]
    fn weapon_category_narrows_labels_and_the_core_query() {
        use shpd_seedfinder_core::catalog::WeaponCategory;

        let mut requirement = UiRequirement::new(1);
        requirement.weapon_category = Some(WeaponCategory::Thrown);
        assert_eq!(requirement.title(), "Any thrown weapon");
        requirement.tier = TierRequirement::Exact(5);
        assert_eq!(requirement.title(), "Any Tier 5 thrown weapon");
        assert_eq!(
            requirement.to_core().weapon_category,
            Some(WeaponCategory::Thrown)
        );
        assert!(requirement.to_core().validate().is_ok());

        requirement.weapon_category = Some(WeaponCategory::Melee);
        requirement.tier = TierRequirement::Any;
        assert_eq!(requirement.title(), "Any melee weapon");
    }

    #[test]
    fn quest_labels_name_every_variant() {
        assert_eq!(ghost_quest_label(GhostQuestType::FetidRat), "Fetid rat");
        assert_eq!(
            ghost_quest_label(GhostQuestType::GnollTrickster),
            "Gnoll trickster"
        );
        assert_eq!(ghost_quest_label(GhostQuestType::GreatCrab), "Great crab");
        assert_eq!(
            wandmaker_quest_label(WandmakerQuestType::CorpseDust),
            "Corpse dust"
        );
        assert_eq!(
            wandmaker_quest_label(WandmakerQuestType::ElementalEmbers),
            "Elemental embers"
        );
        assert_eq!(
            wandmaker_quest_label(WandmakerQuestType::Rotberry),
            "Rotberry"
        );
        assert_eq!(
            blacksmith_quest_label(BlacksmithQuestType::Crystal),
            "Crystal spire"
        );
        assert_eq!(
            blacksmith_quest_label(BlacksmithQuestType::Gnoll),
            "Gnoll geomancer"
        );
        assert_eq!(imp_target_label(ImpTarget::Monk), "Monks");
        assert_eq!(imp_target_label(ImpTarget::Golem), "Golems");
    }

    #[test]
    fn quest_rows_keep_dungeon_order_and_skip_missing_quests() {
        assert!(quest_rows(QuestSummary::default()).is_empty());

        // Seed AAA-AAA-AAA's canonical schedule.
        let summary = QuestSummary {
            ghost: Some(ScheduledQuest {
                variant: GhostQuestType::GreatCrab,
                depth: 4,
            }),
            wandmaker: Some(ScheduledQuest {
                variant: WandmakerQuestType::ElementalEmbers,
                depth: 9,
            }),
            blacksmith: Some(ScheduledQuest {
                variant: BlacksmithQuestType::Crystal,
                depth: 13,
            }),
            imp: Some(ScheduledQuest {
                variant: ImpTarget::Golem,
                depth: 19,
            }),
        };
        assert_eq!(
            quest_rows(summary),
            vec![
                QuestRow {
                    giver: "Sad ghost",
                    variant: "Great crab",
                    depth: 4,
                },
                QuestRow {
                    giver: "Wandmaker",
                    variant: "Elemental embers",
                    depth: 9,
                },
                QuestRow {
                    giver: "Blacksmith",
                    variant: "Crystal spire",
                    depth: 13,
                },
                QuestRow {
                    giver: "Imp",
                    variant: "Golems",
                    depth: 19,
                },
            ]
        );

        let partial = QuestSummary {
            wandmaker: Some(ScheduledQuest {
                variant: WandmakerQuestType::Rotberry,
                depth: 8,
            }),
            ..QuestSummary::default()
        };
        assert_eq!(
            quest_rows(partial),
            vec![QuestRow {
                giver: "Wandmaker",
                variant: "Rotberry",
                depth: 8,
            }]
        );
    }

    #[test]
    fn wandmaker_quest_survives_the_query_round_trip() {
        let mut state = AppState::default();
        let key = state.claim_key();
        state.requirements.push(UiRequirement::new(key));
        assert_eq!(state.to_query().unwrap().wandmaker_quest, None);

        state.wandmaker_quest = Some(WandmakerQuestType::Rotberry);
        let query = state.to_query().unwrap();
        assert_eq!(query.wandmaker_quest, Some(WandmakerQuestType::Rotberry));
        assert_eq!(
            AppState::from_query(&query).wandmaker_quest,
            Some(WandmakerQuestType::Rotberry)
        );
    }

    #[test]
    fn share_links_round_trip_the_whole_editor_state() {
        use shpd_seedfinder_core::catalog::{ItemKind, WeaponCategory};
        use shpd_seedfinder_core::challenges::Challenges;
        use shpd_seedfinder_core::deep_link;

        let mut state = AppState::default();
        let key = state.claim_key();
        state.requirements.push(UiRequirement {
            weapon_category: Some(WeaponCategory::Melee),
            tier: TierRequirement::AtLeast(4),
            upgrade: UpgradeRequirement::Exact(2),
            require_uncursed: true,
            max_depth: Some(9),
            ..UiRequirement::new(key)
        });
        let key = state.claim_key();
        state.requirements.push(UiRequirement {
            kind: ItemKind::Ring,
            item: Some(ItemId::RingTenacity),
            identity_group: Some(2),
            ..UiRequirement::new(key)
        });
        state.max_depth = 13;
        state.require_blacksmith = true;
        state.wandmaker_quest = Some(WandmakerQuestType::ElementalEmbers);
        state.fast_mode = true;
        state.challenges = Challenges::NO_SCROLLS;

        let query = state.to_query().unwrap();
        let link = deep_link::encode_link(&query).unwrap();
        assert!(link.starts_with(deep_link::WEB_LINK_PREFIX));
        let decoded = deep_link::decode_text(&link).unwrap();
        assert_eq!(decoded, query);

        // A received link restores editor state that produces the identical
        // query, so copying the link again shares the same search.
        let restored = AppState::from_query(&decoded);
        assert_eq!(restored.to_query().unwrap(), query);

        // The custom-scheme form the desktop handler receives decodes too.
        let code = link.strip_prefix(deep_link::WEB_LINK_PREFIX).unwrap();
        let uri = format!("{}://q/{code}", deep_link::URI_SCHEME);
        assert_eq!(deep_link::decode_text(&uri).unwrap(), query);
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
