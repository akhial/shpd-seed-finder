// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared query state and presentation labels for the whole window.

use std::fmt::Write as _;

use shpd_seedfinder_core::catalog::{Effect, ItemId, ItemKind, WeaponCategory, item};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::feasibility::Quest;
use shpd_seedfinder_core::main_world::EMPTY_BOSS_FLOORS;
use shpd_seedfinder_core::model::ItemSource;
use shpd_seedfinder_core::query::{
    EffectRequirement, EffectSet, LevelSum, Requirement, SearchQuery, TierRequirement,
    UpgradeRequirement,
};
use shpd_seedfinder_core::quests::{
    BlacksmithQuestType, GhostQuestType, ImpQuestType, QuestSummary, WandmakerQuestType,
};

use crate::relations::{self, BoardItem};

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
    pub effect: EffectRequirement,
    pub require_uncursed: bool,
    pub source: Option<ItemSource>,
    pub identity_group: Option<u8>,
    pub max_depth: Option<u8>,
    /// Members of one alternative group form a single "any of these" slot.
    pub alternative_group: Option<u8>,
    /// Membership in a combined-level group; never set on an alternative.
    pub level_sum: Option<LevelSum>,
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
            effect: EffectRequirement::Any,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
            alternative_group: None,
            level_sum: None,
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
            alternative_group: self.alternative_group,
            level_sum: self.level_sum,
        }
    }

    /// The one effect this requirement pins, when its effect set holds
    /// exactly one member; wider sets and the wildcard give `None`.
    #[must_use]
    pub fn pinned_effect(&self) -> Option<Effect> {
        match self.effect {
            EffectRequirement::OneOf(set) if set.count() == 1 => set.effects().next(),
            _ => None,
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

    /// The short name a board chip shows: the item, or its wildcard family.
    /// The tier rides beside it as a tag, so it stays out of the name.
    #[must_use]
    pub fn chip_name(&self) -> String {
        match self.item {
            Some(item_id) => item(item_id).name.to_owned(),
            None => format!("Any {}", chip_family(self.kind_choice())),
        }
    }

    /// The line under a chip's name: everything it asks of one item, in the
    /// order the editor lays the controls out.
    #[must_use]
    pub fn subtitle(&self) -> String {
        let mut text = match self.upgrade {
            UpgradeRequirement::Any => "any upgrade".to_owned(),
            UpgradeRequirement::Exact(upgrade) => format!("exactly +{upgrade}"),
            UpgradeRequirement::AtLeast(upgrade) => format!("+{upgrade} or higher"),
        };
        if let Some(effect) = effect_label(self.effect) {
            let _ = write!(text, " \u{b7} {effect}");
        }
        if self.require_uncursed {
            text.push_str(" \u{b7} uncursed");
        }
        if let Some(source) = self.source {
            let _ = write!(text, " \u{b7} {}", source_label(source));
        }
        if let Some(depth) = self.max_depth {
            let _ = write!(text, " \u{b7} floors 1\u{2013}{depth}");
        }
        text
    }
}

/// The effect predicate as label text: `None` for the wildcard, "any
/// enchantment" for the full non-curse family set, otherwise the members
/// joined with "or" in catalog order.
#[must_use]
pub fn effect_label(effect: EffectRequirement) -> Option<String> {
    let EffectRequirement::OneOf(set) = effect else {
        return None;
    };
    if EffectSet::enchantments(set.family()) == Some(set) {
        return Some("any enchantment".to_owned());
    }
    let names: Vec<_> = set.effects().map(Effect::wire_name).collect();
    Some(names.join(" or "))
}

/// The whole persisted query state shared by all panes.
#[derive(Clone, Debug)]
pub struct AppState {
    pub requirements: Vec<UiRequirement>,
    pub max_depth: u8,
    pub require_blacksmith: bool,
    pub exclude_blacksmith_rewards: bool,
    pub wandmaker_quest: Option<WandmakerQuestType>,
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
                alternative_group: requirement.alternative_group,
                level_sum: requirement.level_sum,
            });
        }
        state
    }

    /// The state as an engine query exactly as the user left it, without
    /// checking that it is a runnable search: persistence and the seed
    /// scout read half-finished queries too.
    #[must_use]
    pub fn unvalidated_query(&self) -> SearchQuery {
        SearchQuery {
            requirements: self.requirements.iter().map(|r| r.to_core()).collect(),
            max_depth: self.max_depth,
            challenges: self.challenges,
            require_blacksmith: self.require_blacksmith,
            exclude_blacksmith_rewards: self.exclude_blacksmith_rewards,
            wandmaker_quest: self.wandmaker_quest,
        }
    }

    /// Builds the validated engine query for the current state.
    ///
    /// # Errors
    ///
    /// Returns the human-readable validation message.
    pub fn to_query(&self) -> Result<SearchQuery, String> {
        let mut query = self.unvalidated_query();
        // Past the last floor the Blacksmith can first appear on the
        // quest is certain, so the filter would exclude nothing.
        query.require_blacksmith =
            self.require_blacksmith && self.max_depth < Quest::Blacksmith.window().1;
        query.validate().map_err(|error| error.to_string())?;
        Ok(query)
    }

    #[must_use]
    pub fn requirement(&self, key: u64) -> Option<&UiRequirement> {
        self.requirements.iter().find(|r| r.key == key)
    }

    /// The board's collapsed view of the requirement list: one entry per
    /// chip or either/or cluster, with a stack's copies folded away.
    #[must_use]
    pub fn board(&self) -> Vec<BoardItem> {
        relations::board_items(&self.requirements)
    }

    /// How many entries the board shows — what the pane counts as
    /// requirements once alternatives and stacks collapse.
    #[must_use]
    pub fn board_count(&self) -> usize {
        relations::board_count(&self.requirements)
    }

    #[must_use]
    pub fn row_index(&self, key: u64) -> Option<usize> {
        self.requirements.iter().position(|row| row.key == key)
    }

    /// The board entry the row `key` belongs to.
    #[must_use]
    pub fn board_item(&self, key: u64) -> Option<BoardItem> {
        relations::item_of_key(&self.requirements, key)
    }

    /// The stack shape the editor needs for the row `key`.
    #[must_use]
    pub fn stack_shape(&self, key: u64) -> StackShape {
        self.board_item(key)
            .map_or_else(StackShape::lone, |item| StackShape {
                count: item.stack_count(),
                total: item.total,
                copy_depth: relations::copy_depth_of(&self.requirements, &item),
                in_cluster: item.cluster.is_some(),
            })
    }

    /// Stores the editor's result: the row's own fields plus the stack shape
    /// it asked for. A row whose key is not on the board is a new chip.
    pub fn apply_edit(
        &mut self,
        result: UiRequirement,
        count: usize,
        total: Option<u8>,
        copy_depth: Option<u8>,
    ) {
        let index = self.row_index(result.key);
        self.requirements = relations::apply_edit(
            &self.requirements,
            index,
            result,
            count,
            total,
            copy_depth,
            &mut self.next_key,
        );
    }

    /// Makes the row `source` an either/or alternative of the row `target`.
    pub fn join(&mut self, source: u64, target: u64) {
        let (Some(source), Some(target)) = (self.row_index(source), self.row_index(target)) else {
            return;
        };
        self.requirements = relations::join_alternatives(&self.requirements, source, target);
    }

    /// Pulls the row `key` out of its cluster, back onto the board alone.
    pub fn detach(&mut self, key: u64) {
        let Some(index) = self.row_index(key) else {
            return;
        };
        self.requirements = relations::detach(&self.requirements, index);
    }

    /// Deletes what the row `key` stands for: a cluster member on its own, a
    /// lone chip together with the hidden copies of its stack.
    pub fn remove(&mut self, key: u64) {
        let Some(index) = self.row_index(key) else {
            return;
        };
        let Some(item) = self.board_item(key) else {
            return;
        };
        self.requirements = if item.cluster.is_some() {
            relations::remove_member(&self.requirements, index)
        } else {
            relations::remove_item(&self.requirements, &item)
        };
    }

    /// Whether the entry holding the row `key` can ask for more than one
    /// item; a cluster spanning two categories cannot.
    #[must_use]
    pub fn can_stack(&self, key: u64) -> bool {
        self.board_item(key)
            .is_some_and(|item| relations::can_stack(&self.requirements, &item))
    }

    /// Sets how many items the entry holding the row `key` asks for.
    pub fn set_stack_count(&mut self, key: u64, count: usize) {
        let Some(item) = self.board_item(key) else {
            return;
        };
        self.requirements =
            relations::set_stack_count(&self.requirements, &item, count, &mut self.next_key);
    }

    /// Sets or clears the combined level of the entry holding the row `key`.
    pub fn set_stack_total(&mut self, key: u64, total: Option<u8>) {
        let Some(item) = self.board_item(key) else {
            return;
        };
        self.requirements = relations::set_stack_total(&self.requirements, &item, total);
    }

    /// Checks that `draft` would leave the whole query valid once stored
    /// with [`Self::apply_edit`], for the editor to report before saving.
    ///
    /// # Errors
    ///
    /// Returns the human-readable message.
    pub fn validate_draft(
        &self,
        draft: &UiRequirement,
        count: usize,
        total: Option<u8>,
        copy_depth: Option<u8>,
    ) -> Result<(), String> {
        draft
            .to_core()
            .validate()
            .map_err(|error| error.to_string())?;
        let mut preview = self.clone();
        preview.apply_edit(*draft, count, total, copy_depth);
        preview
            .unvalidated_query()
            .validate()
            .map_err(|error| error.to_string())
    }
}

/// What the editor needs to know about the chip's stack.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StackShape {
    pub count: usize,
    pub total: Option<u8>,
    /// The floor limit the extra copies share, when they carry one.
    pub copy_depth: Option<u8>,
    /// A cluster member's stack belongs to the cluster, not the editor.
    pub in_cluster: bool,
}

impl StackShape {
    /// The shape of a chip that is not on the board yet: one item, no stack.
    #[must_use]
    pub const fn lone() -> Self {
        Self {
            count: 1,
            total: None,
            copy_depth: None,
            in_cluster: false,
        }
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

/// The board chip's wildcard name: shorter than the editor's, because the
/// chip already shows the family's icon beside it.
pub const fn chip_family(choice: KindChoice) -> &'static str {
    match choice {
        (ItemKind::Weapon, None) => "weapon",
        (ItemKind::Weapon, Some(WeaponCategory::Melee)) => "melee",
        (ItemKind::Weapon, Some(WeaponCategory::Thrown)) => "thrown",
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
        ItemSource::VaultTreasure => "Vault treasure",
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

pub const fn imp_target_label(variant: ImpQuestType) -> &'static str {
    match variant {
        ImpQuestType::Vault => "Vault",
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
        BlacksmithQuestType, GhostQuestType, ImpQuestType, QuestSummary, ScheduledQuest,
        WandmakerQuestType,
    };

    use super::{
        AppState, QuestRow, UiRequirement, blacksmith_quest_label, floor_limit_skip_target,
        ghost_quest_label, imp_target_label, quest_rows, source_label, wandmaker_quest_label,
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
        assert_eq!(requirement.chip_name(), "Any weapon");
        assert_eq!(requirement.subtitle(), "any upgrade");

        requirement.tier = TierRequirement::AtLeast(4);
        requirement.upgrade = UpgradeRequirement::Exact(2);
        requirement.identity_group = Some(2);
        requirement.max_depth = Some(9);
        requirement.require_uncursed = true;
        assert_eq!(requirement.title(), "Any Tier 4+ weapon");
        // The chip keeps the tier out of the name: it rides beside it as a tag.
        assert_eq!(requirement.chip_name(), "Any weapon");
        assert_eq!(requirement.subtitle(), "exactly +2 · uncursed · floors 1–9");

        requirement.tier = TierRequirement::AtMost(3);
        assert_eq!(requirement.title(), "Any Tier 3 or lower weapon");

        requirement.item = Some(ItemId::Greatsword);
        assert_eq!(requirement.title(), "Greatsword");
        assert_eq!(requirement.chip_name(), "Greatsword");
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
        assert_eq!(imp_target_label(ImpQuestType::Vault), "Vault");
    }

    #[test]
    fn source_labels_name_every_item_source() {
        use shpd_seedfinder_core::model::ItemSource;

        // The source picker offers `ItemSource::ALL` verbatim, so every entry
        // needs a label; a new engine source shows up here first.
        for source in ItemSource::ALL {
            assert!(!source_label(*source).is_empty());
        }
        assert_eq!(source_label(ItemSource::ImpReward), "Imp reward");
        assert_eq!(source_label(ItemSource::VaultTreasure), "Vault treasure");
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
                variant: ImpQuestType::Vault,
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
                    variant: "Vault",
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
    fn share_links_carry_the_v4_effects_and_the_weapon_ceiling() {
        use shpd_seedfinder_core::catalog::{Effect, WeaponEffect};
        use shpd_seedfinder_core::deep_link;
        use shpd_seedfinder_core::query::{EffectRequirement, EffectSet};

        // A set naming Crystal only fits the wider effect mask of link format
        // three, and +5 is the ceiling weapons alone reach.
        let mut state = AppState::default();
        let key = state.claim_key();
        state.requirements.push(UiRequirement {
            upgrade: UpgradeRequirement::Exact(5),
            effect: EffectRequirement::OneOf(
                EffectSet::from_effects([
                    Effect::Weapon(WeaponEffect::Blazing),
                    Effect::Weapon(WeaponEffect::Crystal),
                ])
                .unwrap(),
            ),
            ..UiRequirement::new(key)
        });

        let query = state.to_query().unwrap();
        let link = deep_link::encode_link(&query).unwrap();
        let decoded = deep_link::decode_text(&link).unwrap();
        assert_eq!(decoded, query);

        let restored = AppState::from_query(&decoded);
        assert_eq!(restored.to_query().unwrap(), query);
        assert_eq!(
            restored.requirements[0].subtitle(),
            "exactly +5 \u{b7} Blazing or Crystal"
        );
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

    #[test]
    fn labels_describe_effect_sets_and_predicates() {
        use shpd_seedfinder_core::catalog::{ArmorEffect, Effect, WeaponEffect};
        use shpd_seedfinder_core::query::{EffectRequirement, EffectSet};

        let mut requirement = UiRequirement::new(1);
        requirement.effect = EffectRequirement::exactly(Effect::Weapon(WeaponEffect::Blazing));
        assert_eq!(requirement.subtitle(), "any upgrade · Blazing");
        assert_eq!(
            requirement.pinned_effect(),
            Some(Effect::Weapon(WeaponEffect::Blazing))
        );

        requirement.effect = EffectRequirement::OneOf(
            EffectSet::from_effects([
                Effect::Weapon(WeaponEffect::Projecting),
                Effect::Weapon(WeaponEffect::Blocking),
            ])
            .unwrap(),
        );
        // Catalog order, not selection order.
        assert_eq!(
            requirement.subtitle(),
            "any upgrade · Blocking or Projecting"
        );
        assert_eq!(requirement.pinned_effect(), None);

        requirement.kind = ItemKind::Armor;
        requirement.effect =
            EffectRequirement::OneOf(EffectSet::enchantments(ItemKind::Armor).unwrap());
        requirement.require_uncursed = true;
        assert_eq!(
            requirement.subtitle(),
            "any upgrade · any enchantment · uncursed"
        );

        requirement.effect = EffectRequirement::exactly(Effect::Armor(ArmorEffect::Stone));
        requirement.require_uncursed = false;
        requirement.upgrade = UpgradeRequirement::AtLeast(1);
        requirement.max_depth = Some(4);
        assert_eq!(requirement.subtitle(), "+1 or higher · Stone · floors 1–4");
    }

    #[test]
    fn the_board_collapses_alternatives_and_stacks_into_one_entry_each() {
        let mut state = AppState::default();
        let spear = state.claim_key();
        state.requirements.push(UiRequirement {
            item: Some(ItemId::Spear),
            upgrade: UpgradeRequirement::Exact(3),
            ..UiRequirement::new(spear)
        });
        let ring = state.claim_key();
        state.requirements.push(UiRequirement {
            kind: ItemKind::Ring,
            ..UiRequirement::new(ring)
        });
        assert_eq!(state.board_count(), 2);

        // Dropping the ring on the spear makes one either/or entry, and one
        // slot for the engine.
        state.join(ring, spear);
        assert_eq!(state.board_count(), 1);
        let item = state.board_item(spear).unwrap();
        assert_eq!(item.members.len(), 2);
        assert!(item.cluster.is_some());
        assert_eq!(state.unvalidated_query().slot_count(), 1);

        // A cluster spanning two categories cannot anchor a stack: a copy
        // would have to name a kind, and "spear or ring" names none.
        state.set_stack_count(spear, 2);
        assert_eq!(state.requirements.len(), 2);
        assert_eq!(state.board_count(), 1);
        assert_eq!(state.board_item(spear).unwrap().stack_count(), 1);
        assert!(!state.can_stack(spear));

        // Pulling the ring back out leaves a plain spear chip, which can.
        state.detach(ring);
        assert_eq!(state.board_count(), 2);
        assert!(state.can_stack(spear));
        state.set_stack_count(spear, 2);
        assert_eq!(state.requirements.len(), 3);
        assert_eq!(state.board_item(spear).unwrap().stack_count(), 2);
        assert_eq!(state.board_item(ring).unwrap().stack_count(), 1);
        assert!(
            state
                .requirements
                .iter()
                .all(|r| r.identity_group.is_none())
        );
        assert!(state.to_query().is_ok());

        // Removing the spear takes its hidden copy with it.
        state.remove(spear);
        assert_eq!(state.requirements.len(), 1);
        assert_eq!(state.requirements[0].key, ring);
    }

    #[test]
    fn a_combined_level_stack_is_built_and_checked_through_the_editor() {
        let mut state = AppState::default();
        let key = state.claim_key();
        let ring = UiRequirement {
            kind: ItemKind::Ring,
            item: Some(ItemId::RingMight),
            ..UiRequirement::new(key)
        };
        state.apply_edit(ring, 2, Some(4), None);
        assert_eq!(state.requirements.len(), 2);
        assert!(
            state
                .requirements
                .iter()
                .all(|r| r.level_sum.map(|sum| sum.minimum_total) == Some(4))
        );
        assert_eq!(state.board_count(), 1);
        let shape = state.stack_shape(key);
        assert_eq!(shape.count, 2);
        assert_eq!(shape.total, Some(4));
        assert!(!shape.in_cluster);
        assert!(state.to_query().is_ok());

        // A ring reaches +4 (five levels), but only one per world — the Imp
        // vault's prize; every other ring stops at +2 (three levels). Two
        // rings therefore reach eight levels together, three eleven.
        assert!(state.validate_draft(&ring, 2, Some(8), None).is_ok());
        assert_eq!(
            state.validate_draft(&ring, 2, Some(9), None).unwrap_err(),
            "combined level group A needs 9 levels but its items can reach at most 8"
        );
        assert!(state.validate_draft(&ring, 3, Some(11), None).is_ok());
        assert_eq!(
            state.validate_draft(&ring, 3, Some(12), None).unwrap_err(),
            "combined level group A needs 12 levels but its items can reach at most 11"
        );

        // The badge lowers the total without going through the editor.
        state.set_stack_total(key, Some(3));
        assert_eq!(state.board_item(key).unwrap().total, Some(3));

        // Giving up on counting levels returns the stack to plain repeats.
        state.set_stack_total(key, None);
        assert!(state.requirements.iter().all(|r| r.level_sum.is_none()));
        assert_eq!(state.board_item(key).unwrap().stack_count(), 2);
        assert!(state.to_query().is_ok());
    }

    #[test]
    fn a_stack_of_copies_carries_its_own_floor_limit() {
        let mut state = AppState::default();
        let key = state.claim_key();
        let armor = UiRequirement {
            kind: ItemKind::Armor,
            upgrade: UpgradeRequirement::Exact(3),
            max_depth: Some(4),
            ..UiRequirement::new(key)
        };
        state.apply_edit(armor, 2, None, Some(9));
        let shape = state.stack_shape(key);
        assert_eq!(shape.count, 2);
        assert_eq!(shape.copy_depth, Some(9));
        // The named +3 armor keeps its own floor; the copy keeps the other.
        assert_eq!(state.requirements[0].max_depth, Some(4));
        assert_eq!(state.requirements[1].max_depth, Some(9));
        assert!(state.to_query().is_ok());
    }
}
