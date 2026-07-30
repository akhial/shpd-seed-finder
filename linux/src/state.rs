// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared query state and presentation labels for the whole window.

use std::fmt::Write as _;

use shpd_seedfinder_core::catalog::{Effect, ItemId, ItemKind, WeaponCategory, item};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::model::ItemSource;
use shpd_seedfinder_core::query::{Requirement, SearchQuery, TierRequirement, UpgradeRequirement};
use shpd_seedfinder_core::quests::{
    BlacksmithQuestType, GhostQuestType, ImpTarget, QuestSummary, WandmakerQuestType,
};

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

/// Boss floors that generate no searchable items. The core treats a floor
/// limit of 5/10/15 exactly like 4/9/14, so floor-limit selectors skip them.
/// Floor 20 stays selectable: the Imp shop gives the City boss floor stock.
pub const EMPTY_BOSS_FLOORS: [u8; 3] = [5, 10, 15];

/// Snaps an empty boss-floor limit to the equivalent floor below it
/// (5→4, 10→9, 15→14).
#[must_use]
pub fn normalize_floor_limit(depth: u8) -> u8 {
    if EMPTY_BOSS_FLOORS.contains(&depth) {
        depth - 1
    } else {
        depth
    }
}

/// Where a floor-limit control lands when the user moves it onto an empty
/// boss floor. A single upward step (spin button, arrow key, scroll)
/// continues to the next real floor; every other move — single steps down
/// and typed jumps in either direction — snaps to the equivalent floor
/// below, matching [`normalize_floor_limit`]. Typing "10" therefore means
/// "first 10 floors" (≡ 9), never 11.
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

    /// Rebuilds editor state from a decoded engine query, assigning fresh
    /// session row keys.
    #[must_use]
    pub fn from_query(query: &SearchQuery) -> Self {
        let mut state = Self {
            requirements: Vec::with_capacity(query.requirements.len()),
            max_depth: query.max_depth,
            require_blacksmith: query.require_blacksmith,
            exclude_blacksmith_rewards: query.exclude_blacksmith_rewards,
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
            require_blacksmith: self.require_blacksmith && self.max_depth < 14,
            exclude_blacksmith_rewards: self.exclude_blacksmith_rewards,
            fast_mode: self.fast_mode,
        };
        query.validate().map_err(|error| error.to_string())?;
        Ok(query)
    }
}

/// Whether two queries name a common item: some requirement of each has the
/// same kind, and either both name the same item or at least one names none
/// (a kind-level requirement subsumes every item of its kind). Scope and
/// challenge differences are irrelevant — a filter re-verifies seeds from
/// scratch — so this deliberately checks nothing else: it only estimates
/// whether the Target Set is enriched for the candidate query's matches.
#[must_use]
pub fn shares_item(candidate: &SearchQuery, base: &SearchQuery) -> bool {
    candidate.requirements.iter().any(|left| {
        base.requirements.iter().any(|right| {
            left.kind == right.kind
                && (left.item.is_none() || right.item.is_none() || left.item == right.item)
        })
    })
}

/// What pressing Start Search does with a query, per docs/search-semantics.md.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartMode {
    /// Fresh full-range scan that establishes the Target on conclusion.
    Anchor,
    /// Filter the Target Set, then resume the target's uncovered remainder.
    TargetRefine,
    /// Filter the Target Set only; the set and its coverage stay untouched.
    TargetFilter,
    /// Continue the previous detached scan (filter its results, resume its
    /// remainder). The Target is untouched.
    ContinueDetached,
    /// Fresh full-range scan that leaves the Target untouched.
    Detached,
}

/// The facts about the session's Target that the start decision reads.
pub struct TargetFacts<'a> {
    pub query: &'a SearchQuery,
    /// How many seeds the Target Set holds.
    pub set_size: usize,
    /// How many seeds the target traversal has not covered yet.
    pub remaining: u64,
}

/// The single gate for what Start Search does. The Target Set is the anchor:
/// a continuation of the Target Query refines it, a query sharing an item
/// filters it (always from the full set, so loosening a requirement brings
/// seeds back), and anything else scans the full range without touching it —
/// continuing the last detached run when `detached_base` says that is sound.
/// An empty Target Set holds nothing worth preserving, so a non-continuing
/// query re-anchors on this search instead of filtering nothing.
///
/// Continuation itself is the engine's [`SearchQuery::continues`], never a
/// local re-derivation. It deliberately admits equality: an unchanged query
/// keeps every previously found seed and simply resumes the scan where it
/// stopped, which is what makes a session survive a stop-and-start-again.
/// Only an explicit clear discards it.
///
/// `detached_base` is the last concluded run's query when — and only when —
/// that run was itself detached; a failed run is never a continuation base.
#[must_use]
pub fn start_mode(
    candidate: &SearchQuery,
    target: Option<&TargetFacts<'_>>,
    detached_base: Option<&SearchQuery>,
) -> StartMode {
    let Some(target) = target else {
        return StartMode::Anchor;
    };
    let continues_target = candidate.continues(target.query);
    if target.set_size == 0 {
        return if continues_target && target.remaining > 0 {
            StartMode::TargetRefine
        } else {
            StartMode::Anchor
        };
    }
    if continues_target {
        return StartMode::TargetRefine;
    }
    if shares_item(candidate, target.query) {
        return StartMode::TargetFilter;
    }
    match detached_base {
        Some(base) if candidate.continues(base) => StartMode::ContinueDetached,
        _ => StartMode::Detached,
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
    use shpd_seedfinder_core::catalog::{ItemId, ItemKind};
    use shpd_seedfinder_core::query::{TierRequirement, UpgradeRequirement};
    use shpd_seedfinder_core::quests::{
        BlacksmithQuestType, GhostQuestType, ImpTarget, QuestSummary, ScheduledQuest,
        WandmakerQuestType,
    };

    use super::{
        AppState, QuestRow, StartMode, TargetFacts, UiRequirement, blacksmith_quest_label,
        floor_limit_skip_target, ghost_quest_label, imp_target_label, normalize_floor_limit,
        quest_rows, shares_item, start_mode, wandmaker_quest_label,
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

    /// A populated Target with plenty of uncovered range.
    fn facts(query: &shpd_seedfinder_core::query::SearchQuery) -> TargetFacts<'_> {
        TargetFacts {
            query,
            set_size: 3,
            remaining: 1_000,
        }
    }

    #[test]
    fn empty_boss_floor_limits_normalize_to_the_floor_below() {
        for (limit, expected) in [(4, 4), (5, 4), (9, 9), (10, 9), (14, 14), (15, 14), (24, 24)] {
            assert_eq!(normalize_floor_limit(limit), expected);
        }
    }

    #[test]
    fn starting_refines_an_extension_of_the_target_without_asking() {
        let mut base_state = AppState::default();
        let mut first = UiRequirement::new(base_state.claim_key());
        first.kind = ItemKind::Ring;
        base_state.requirements.push(first);
        let base = base_state.to_query().unwrap();

        let mut extended_state = base_state.clone();
        let mut added = UiRequirement::new(extended_state.claim_key());
        added.kind = ItemKind::Weapon;
        added.upgrade = UpgradeRequirement::AtLeast(2);
        extended_state.requirements.push(added);
        let extended = extended_state.to_query().unwrap();

        // Adding a requirement after a concluded run refines it implicitly.
        assert_eq!(
            start_mode(&extended, Some(&facts(&base)), None),
            StartMode::TargetRefine,
            "an extending query must reuse the Target"
        );

        // Starting again with the query unchanged continues the session: the
        // filter keeps every seed and the scan picks up where it stopped. This
        // is the stop-then-start-again case, which must never wipe results.
        assert_eq!(
            start_mode(&base, Some(&facts(&base)), None),
            StartMode::TargetRefine,
            "an unchanged query must continue the previous run"
        );
        assert_eq!(
            start_mode(&extended, Some(&facts(&extended)), None),
            StartMode::TargetRefine
        );

        // Clearing the results drops the Target, so even an extending query
        // anchors a fresh session.
        assert_eq!(start_mode(&extended, None, None), StartMode::Anchor);
    }

    #[test]
    fn queries_sharing_an_item_filter_the_target_set_without_scanning() {
        let mut base_state = AppState::default();
        let mut first = UiRequirement::new(base_state.claim_key());
        first.kind = ItemKind::Ring;
        base_state.requirements.push(first);
        let base = base_state.to_query().unwrap();

        // A narrower scope breaks the continuation rule but still names a
        // ring, so the full Target Set is filtered instead of rescanned.
        let mut deeper_state = base_state.clone();
        deeper_state.max_depth = 9;
        let deeper = deeper_state.to_query().unwrap();
        assert!(!deeper.continues(&base));
        assert_eq!(
            start_mode(&deeper, Some(&facts(&base)), None),
            StartMode::TargetFilter
        );

        // Dropping back to fewer requirements is a filter too — the base is
        // the full Target Set, so loosening brings seeds back.
        let mut extended_state = base_state.clone();
        let mut added = UiRequirement::new(extended_state.claim_key());
        added.kind = ItemKind::Weapon;
        extended_state.requirements.push(added);
        let extended = extended_state.to_query().unwrap();
        assert_eq!(
            start_mode(&base, Some(&facts(&extended)), None),
            StartMode::TargetFilter
        );

        // An unrelated kind shares nothing and scans detached.
        let mut armor_state = AppState::default();
        let mut armor = UiRequirement::new(armor_state.claim_key());
        armor.kind = ItemKind::Armor;
        armor_state.requirements.push(armor);
        let armor_query = armor_state.to_query().unwrap();
        assert_eq!(
            start_mode(&armor_query, Some(&facts(&base)), None),
            StartMode::Detached
        );
    }

    #[test]
    fn sharing_compares_kinds_and_named_items_only() {
        let build = |configure: fn(&mut UiRequirement)| {
            let mut state = AppState::default();
            let mut requirement = UiRequirement::new(state.claim_key());
            configure(&mut requirement);
            state.requirements.push(requirement);
            state.to_query().unwrap()
        };
        let any_ring = build(|r| r.kind = ItemKind::Ring);
        let tenacity = build(|r| {
            r.kind = ItemKind::Ring;
            r.item = Some(ItemId::RingTenacity);
        });
        let greatsword = build(|r| {
            r.kind = ItemKind::Weapon;
            r.item = Some(ItemId::Greatsword);
        });

        // A kind-level requirement subsumes every item of its kind.
        assert!(shares_item(&any_ring, &tenacity));
        assert!(shares_item(&tenacity, &any_ring));
        assert!(shares_item(&tenacity, &tenacity));
        // Different kinds never share, and neither do two distinct named
        // items of the same kind.
        assert!(!shares_item(&greatsword, &any_ring));
        let sword = build(|r| {
            r.kind = ItemKind::Weapon;
            r.item = Some(ItemId::Sword);
        });
        assert!(!shares_item(&greatsword, &sword));

        // Scope differences are irrelevant: a filter re-verifies from scratch.
        let mut deep_ring = any_ring.clone();
        deep_ring.max_depth = 5;
        deep_ring.fast_mode = true;
        assert!(shares_item(&deep_ring, &tenacity));
    }

    #[test]
    fn unrelated_queries_continue_only_the_detached_thread() {
        let mut ring_state = AppState::default();
        let mut ring = UiRequirement::new(ring_state.claim_key());
        ring.kind = ItemKind::Ring;
        ring_state.requirements.push(ring);
        let target = ring_state.to_query().unwrap();

        let mut armor_state = AppState::default();
        let mut armor = UiRequirement::new(armor_state.claim_key());
        armor.kind = ItemKind::Armor;
        armor_state.requirements.push(armor);
        let detached = armor_state.to_query().unwrap();

        // First unrelated query: a fresh detached scan.
        assert_eq!(
            start_mode(&detached, Some(&facts(&target)), None),
            StartMode::Detached
        );
        // Extending the detached run continues it instead of rescanning.
        let mut narrowed_state = armor_state.clone();
        let mut added = UiRequirement::new(narrowed_state.claim_key());
        added.kind = ItemKind::Armor;
        added.upgrade = UpgradeRequirement::AtLeast(2);
        narrowed_state.requirements.push(added);
        let narrowed = narrowed_state.to_query().unwrap();
        assert_eq!(
            start_mode(&narrowed, Some(&facts(&target)), Some(&detached)),
            StartMode::ContinueDetached
        );
        // But never when the last concluded run was not detached (or failed):
        // without a detached base, an unrelated query rescans.
        assert_eq!(
            start_mode(&narrowed, Some(&facts(&target)), None),
            StartMode::Detached
        );
        // And the Target always wins: a query continuing the Target refines
        // it even when it would also continue the detached run.
        assert_eq!(
            start_mode(&target, Some(&facts(&target)), Some(&target)),
            StartMode::TargetRefine
        );
    }

    #[test]
    fn an_empty_target_set_resumes_a_continuation_and_reanchors_otherwise() {
        let mut ring_state = AppState::default();
        let mut ring = UiRequirement::new(ring_state.claim_key());
        ring.kind = ItemKind::Ring;
        ring_state.requirements.push(ring);
        let target = ring_state.to_query().unwrap();
        let empty = |remaining: u64| TargetFacts {
            query: &target,
            set_size: 0,
            remaining,
        };

        // A continuing query still resumes the uncovered remainder.
        assert_eq!(
            start_mode(&target, Some(&empty(1_000)), None),
            StartMode::TargetRefine
        );
        // With nothing left to scan either, the search re-anchors.
        assert_eq!(
            start_mode(&target, Some(&empty(0)), None),
            StartMode::Anchor
        );
        // Any other query re-anchors: an empty set holds nothing worth
        // preserving, even for a query that shares the ring kind.
        let mut deeper_state = ring_state.clone();
        deeper_state.max_depth = 9;
        let deeper = deeper_state.to_query().unwrap();
        assert_eq!(
            start_mode(&deeper, Some(&empty(1_000)), None),
            StartMode::Anchor
        );
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
