//! Logic-based query feasibility: which sources can satisfy each requirement,
//! how deep generation must run, and when a partially generated seed can be
//! abandoned early.
//!
//! The rules here mirror structural facts of the v3.3.8 generator:
//!
//! - Natural equipment rolls never exceed +2 ([`crate::equipment`]), so +3
//!   weapons come only from the Sacrificial-fire room, the Ghost quest, the
//!   Blacksmith, or a special-room chest prize (the flooded-vault and sentry
//!   rooms bump a weapon, missile, or armor roll by one, and the secret maze
//!   bumps a melee weapon or armor roll — all dropped as chests); +3 armor
//!   only from the Crypt, the Ghost, the Blacksmith, or those same chest
//!   prizes; +3 wands only from the Wandmaker; and +3/+4 rings only from the
//!   Imp.
//! - Every quest resolves inside a fixed depth window (Ghost 2–4, Wandmaker
//!   7–9, Blacksmith 12–14, Imp 17–19) and spawns at most once per run, with
//!   the spawn forced on the window's final floor.
//! - Shops stock unupgraded, unenchanted items only, and quest reward choices
//!   are mutually exclusive, so each quest satisfies at most one requirement.
//!
//! Everything derived from these rules is exact: a rejected seed can never
//! match, and a shortened generation depth can never hide a match. The one
//! deliberately lossy refinement is [`SearchQuery::fast_mode`], which ignores
//! the rare Crypt/Sacrificial-fire/chest-prize +3 rolls so that +3
//! weapon/armor requirements become quest-only and inherit the Blacksmith's
//! depth-14 deadline.
//!
//! The searchable catalog contains equipment only. `NO_SCROLLS` halves the
//! scheduled Scroll of Upgrade drops, but no current requirement can target a
//! consumable or torch, so there is no challenge-dependent availability bound
//! to apply here. Its RNG knock-on effects are handled by generation itself.

use crate::catalog::{ItemKind, WeaponCategory};
use crate::model::{ItemSource, WorldItem};
use crate::query::{EffectRequirement, Requirement, SearchQuery, UpgradeRequirement};
use crate::quests::{QuestSummary, WandmakerQuestType};
use crate::search::FloorGate;

/// The four one-per-run reward quests, each offering a mutually exclusive
/// choice, so each can satisfy at most one requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Quest {
    Ghost,
    Wandmaker,
    Blacksmith,
    Imp,
}

/// Every reward quest, in dungeon order.
pub const QUESTS: [Quest; 4] = [
    Quest::Ghost,
    Quest::Wandmaker,
    Quest::Blacksmith,
    Quest::Imp,
];

impl Quest {
    /// The inclusive depth window inside which the quest can first spawn. The
    /// spawn chance reaches certainty on the final floor, so a run whose item
    /// list has no reward items past the window can never gain them.
    #[must_use]
    pub const fn window(self) -> (u8, u8) {
        match self {
            Self::Ghost => (2, 4),
            Self::Wandmaker => (7, 9),
            Self::Blacksmith => (12, 14),
            Self::Imp => (17, 19),
        }
    }

    const fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

const fn quest_for_source(source: ItemSource) -> Option<Quest> {
    match source {
        ItemSource::GhostReward => Some(Quest::Ghost),
        ItemSource::WandmakerReward => Some(Quest::Wandmaker),
        ItemSource::BlacksmithReward => Some(Quest::Blacksmith),
        // Both vault sources are one pick: the Imp lets exactly one item leave.
        ItemSource::ImpReward | ItemSource::VaultTreasure => Some(Quest::Imp),
        _ => None,
    }
}

/// What `effect` values a source's items can carry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EffectPolicy {
    /// Items never carry an enchantment or glyph (shops, wands, rings).
    Never,
    /// Curse-type effects are stripped or replaced; only good effects survive.
    GoodOnly,
    /// The full natural distribution, including curse effects.
    Any,
}

/// Static per-source capabilities for one item kind: the reachable upgrade
/// interval and the effect policy. `None` when the source cannot produce the
/// kind at all.
const fn source_profile(
    source: ItemSource,
    kind: ItemKind,
    weapon_category: Option<WeaponCategory>,
    fast_mode: bool,
) -> Option<(u8, u8, EffectPolicy)> {
    use EffectPolicy::{Any, GoodOnly, Never};
    use ItemKind::{Armor, Ring, Wand, Weapon};
    use ItemSource as S;
    // The Ghost and Sacrificial-fire prizes and both statue drops roll
    // exclusively melee weapons in v3.3.8, so a thrown-narrowed weapon
    // requirement can never be satisfied by them. Every thrown-capable
    // source also rolls melee weapons, so a melee filter removes nothing.
    if matches!(kind, Weapon)
        && matches!(weapon_category, Some(WeaponCategory::Thrown))
        && matches!(
            source,
            S::GhostReward | S::SacrificialFire | S::Statue | S::ArmoredStatue
        )
    {
        return None;
    }
    Some(match (source, kind) {
        // Rare +3 rolls outside the quests: the Crypt bumps non-cursed
        // armor, the Sacrificial fire bumps its melee prize, and plain
        // chests front the flooded-vault, sentry, and secret-maze prizes,
        // each bumping one natural weapon/missile/armor roll. Fast mode
        // deliberately ignores all of these exotic paths so +3 weapon/armor
        // requirements become quest-only.
        (S::Chest, Weapon | Armor) | (S::Tomb, Armor) | (S::SacrificialFire, Weapon) => {
            if fast_mode { (0, 2, Any) } else { (0, 3, Any) }
        }
        // Plain drops and chest variants use the natural rolls, capped at +2,
        // as do crystal chests/mimics (which stock only wands and rings).
        (S::Heap | S::Chest | S::LockedChest | S::Skeleton | S::Mimic, _)
        | (S::CrystalChest | S::CrystalMimic, Wand | Ring) => (0, 2, Any),
        // Golden mimics strip curse effects; statues force a good effect.
        // Neither exceeds the natural +2 cap.
        (S::GoldenMimic, _) | (S::Statue, Weapon) | (S::ArmoredStatue, Weapon | Armor) => {
            (0, 2, GoodOnly)
        }
        // Shop stock is always +0 with no effect.
        (S::Shop, _) => (0, 0, Never),
        // Quest rewards.
        (S::GhostReward | S::BlacksmithReward, Weapon | Armor) => (0, 3, GoodOnly),
        (S::WandmakerReward, Wand) => (1, 3, Never),
        // The Imp's final-room options (v4.0.0): every weapon, thrown weapon and
        // the plate armor carry a good effect, wands and rings carry none,
        // nothing is cursed. Tier-4 weapons and thrown weapons roll +3..+5,
        // everything else +2..+4.
        (S::ImpReward, Weapon) => (2, 5, GoodOnly),
        (S::ImpReward, Armor) => (2, 4, GoodOnly),
        (S::ImpReward, Wand | Ring) => (2, 4, Never),
        // Vault treasure rooms: four loot tiers at +0..+3, plus one +4 tier-4
        // melee weapon; effects are good or absent, never curses.
        (S::VaultTreasure, Weapon) => (0, 4, GoodOnly),
        (S::VaultTreasure, Armor) => (0, 3, GoodOnly),
        (S::VaultTreasure, Wand | Ring) => (0, 3, Never),
        _ => return None,
    })
}

const fn upgrade_reachable(requirement: UpgradeRequirement, low: u8, high: u8) -> bool {
    match requirement {
        UpgradeRequirement::Any => true,
        UpgradeRequirement::Exact(wanted) => low <= wanted && wanted <= high,
        UpgradeRequirement::AtLeast(minimum) => minimum <= high,
    }
}

fn effect_reachable(
    wanted: EffectRequirement,
    policy: EffectPolicy,
    require_uncursed: bool,
) -> bool {
    let EffectRequirement::OneOf(set) = wanted else {
        return true;
    };
    // Uncursed items never carry curse-type effects, so only the good
    // members of the set stay reachable under that flag.
    let effective = if require_uncursed {
        match set.without_curses() {
            Some(set) => set,
            None => return false,
        }
    } else {
        set
    };
    match policy {
        EffectPolicy::Never => false,
        EffectPolicy::GoodOnly => !effective.is_curses_only(),
        EffectPolicy::Any => true,
    }
}

/// Whether `source` can ever produce an item satisfying `requirement`.
fn source_feasible(requirement: &Requirement, source: ItemSource, fast_mode: bool) -> bool {
    if requirement.source.is_some_and(|wanted| wanted != source) {
        return false;
    }
    let curses_only = match requirement.effect {
        EffectRequirement::OneOf(set) => set.is_curses_only(),
        EffectRequirement::Any => false,
    };
    if requirement.require_uncursed && curses_only {
        return false;
    }
    // An explicit source pin is the user's claim, not ours: honor it verbatim
    // rather than applying the fast-mode refinement.
    let fast_mode = fast_mode && requirement.source.is_none();
    source_profile(
        source,
        requirement.kind,
        requirement.weapon_category,
        fast_mode,
    )
    .is_some_and(|(low, high, policy)| {
        upgrade_reachable(requirement.upgrade, low, high)
            && effect_reachable(requirement.effect, policy, requirement.require_uncursed)
    })
}

const ALL_SOURCES: [ItemSource; 18] = [
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
    ItemSource::VaultTreasure,
];

/// Depths carrying a shop, in order. Depth 20 is the Imp's pre-Halls shop.
const SHOP_DEPTHS: [u8; 5] = [6, 11, 16, 20, 21];

/// One requirement's satisfiability horizon.
#[derive(Clone, Debug)]
struct RequirementPlan {
    requirement: Requirement,
    max_depth: u8,
    /// Bit set of quests (see [`Quest::bit`]) whose reward could satisfy the
    /// requirement inside the query's depth limit.
    quests: u8,
    /// Latest depth at which a non-quest source could still first produce a
    /// matching item, or `None` when only quests can satisfy it.
    open_deadline: Option<u8>,
}

/// Query-derived generation plan: how deep worlds must be generated and when
/// a partial world can be abandoned. Built once per search.
///
/// Combined-upgrade groups are not modelled here: they only add constraints
/// on top of their members' own predicates, so ignoring them keeps every
/// `false` answer sound and merely forgoes some early exits.
#[derive(Clone, Debug)]
pub struct QueryPlan {
    /// One entry per query slot: a plain requirement alone, or every member
    /// of an alternative group, any one of which satisfies the slot.
    slots: Vec<Vec<RequirementPlan>>,
    generation_depth: u8,
    /// Latest depth by which a required Blacksmith must have appeared.
    blacksmith_deadline: Option<u8>,
    /// Required Wandmaker variant paired with the latest depth by which its
    /// quest must have appeared.
    wandmaker_deadline: Option<(WandmakerQuestType, u8)>,
    unsatisfiable: bool,
}

impl QueryPlan {
    /// Derives the plan for a validated query.
    #[must_use]
    pub fn analyze(query: &SearchQuery) -> Self {
        let max_depth = query.max_depth;
        let mut generation_depth = 1;
        let mut slots: Vec<Vec<RequirementPlan>> = Vec::new();
        for slot in query.slots() {
            let mut members = Vec::with_capacity(slot.len());
            for requirement in slot.iter().map(|index| &query.requirements[*index]) {
                let requirement_max_depth =
                    requirement.max_depth.unwrap_or(max_depth).min(max_depth);
                let mut quests = 0_u8;
                let mut open_deadline = None;
                for source in ALL_SOURCES {
                    if !source_feasible(requirement, source, query.fast_mode) {
                        continue;
                    }
                    if query.exclude_blacksmith_rewards && source == ItemSource::BlacksmithReward {
                        continue;
                    }
                    if let Some(quest) = quest_for_source(source) {
                        let (window_start, window_end) = quest.window();
                        if window_start <= requirement_max_depth {
                            quests |= quest.bit();
                            generation_depth =
                                generation_depth.max(window_end.min(requirement_max_depth));
                        }
                    } else if source == ItemSource::Shop {
                        let deadline = SHOP_DEPTHS
                            .into_iter()
                            .rfind(|&depth| depth <= requirement_max_depth);
                        if let Some(deadline) = deadline {
                            open_deadline = Some(open_deadline.unwrap_or(0).max(deadline));
                            generation_depth = generation_depth.max(deadline);
                        }
                    } else {
                        open_deadline = Some(requirement_max_depth);
                        generation_depth = generation_depth.max(requirement_max_depth);
                    }
                }
                members.push(RequirementPlan {
                    requirement: *requirement,
                    max_depth: requirement_max_depth,
                    quests,
                    open_deadline,
                });
            }
            slots.push(members);
        }

        let blacksmith_deadline = if query.require_blacksmith {
            let (window_start, window_end) = Quest::Blacksmith.window();
            if window_start <= max_depth {
                generation_depth = generation_depth.max(window_end.min(max_depth));
                Some(window_end.min(max_depth))
            } else {
                // The window cannot open at all; mark as impossible below by
                // using a deadline of zero, which no completed floor precedes.
                Some(0)
            }
        } else {
            None
        };

        // The Wandmaker's variant is fixed when its quest room is scheduled,
        // so the filter is decided by the giver's own floor and needs the
        // prefix to reach it — even when every requirement was satisfied long
        // before.
        let wandmaker_deadline = query.wandmaker_quest.map(|variant| {
            let deadline = if *WandmakerQuestType::WINDOW.start() <= max_depth {
                let deadline = (*WandmakerQuestType::WINDOW.end()).min(max_depth);
                generation_depth = generation_depth.max(deadline);
                deadline
            } else {
                // The window cannot open at all; a deadline of zero no
                // completed floor precedes marks the query impossible below.
                0
            };
            (variant, deadline)
        });

        let mut plan = Self {
            slots,
            generation_depth,
            blacksmith_deadline,
            wandmaker_deadline,
            unsatisfiable: false,
        };
        plan.unsatisfiable = !plan.viable_after_floor(0, &[], &QuestSummary::default());
        plan
    }

    /// Whether no seed can ever match the query (for example a +4 ring with a
    /// depth limit above the Imp's window's start).
    #[must_use]
    pub const fn is_unsatisfiable(&self) -> bool {
        self.unsatisfiable
    }

    /// Deepest floor that generation must reach: past it, no source can first
    /// produce an item any requirement still needs. Never exceeds the query's
    /// depth limit.
    #[must_use]
    pub fn generation_depth(&self) -> u8 {
        self.generation_depth.clamp(1, 24)
    }

    /// Whether a seed whose floors `1..=completed_depth` produced `items` and
    /// scheduled `quests` can still satisfy every requirement. Conservative:
    /// `false` is proof that the final matcher would reject the seed, while
    /// `true` promises nothing.
    #[must_use]
    pub fn viable_after_floor(
        &self,
        completed_depth: u8,
        items: &[WorldItem],
        quests: &QuestSummary,
    ) -> bool {
        if let Some((wanted, deadline)) = self.wandmaker_deadline {
            match quests.wandmaker {
                // The variant is rolled once per run and never revised, so a
                // mismatch kills the seed on the Wandmaker's own floor.
                Some(scheduled) => {
                    if scheduled.variant != wanted {
                        return false;
                    }
                }
                None if completed_depth >= deadline => return false,
                None => {}
            }
        }

        // Slots that only quests can still satisfy, grouped by their live
        // quest bit set. An alternative group is one slot, alive while any
        // member is. Each quest offers a mutually exclusive choice, so it can
        // cover at most one slot; Hall's condition over the sixteen quest
        // subsets then decides whether an assignment exists.
        let mut quest_only = [0_u16; 16];
        for slot in &self.slots {
            let open = slot.iter().any(|plan| {
                let satisfied_by_open_item = items.iter().any(|item| {
                    item.depth <= plan.max_depth
                        && quest_for_source(item.source).is_none()
                        && plan.requirement.matches(item)
                });
                satisfied_by_open_item
                    || plan
                        .open_deadline
                        .is_some_and(|deadline| completed_depth < deadline)
            });
            if open {
                continue;
            }
            let mut live = 0_u8;
            for plan in slot {
                for quest in QUESTS {
                    if plan.quests & quest.bit() != 0
                        && Self::quest_alive(quest, plan, completed_depth, items)
                    {
                        live |= quest.bit();
                    }
                }
            }
            if live == 0 {
                return false;
            }
            quest_only[usize::from(live)] += 1;
        }
        for subset in 1_u8..16 {
            let mut needed = 0_u32;
            for mask in 1_u8..16 {
                if mask & !subset == 0 {
                    needed += u32::from(quest_only[usize::from(mask)]);
                }
            }
            if needed > subset.count_ones() {
                return false;
            }
        }

        if let Some(deadline) = self.blacksmith_deadline {
            let present = items
                .iter()
                .any(|item| item.source == ItemSource::BlacksmithReward);
            if !present && completed_depth >= deadline {
                return false;
            }
        }
        true
    }

    /// Whether `quest` could still supply an item matching `requirement`.
    /// Reward items appear all at once on the quest's floor, so any item with
    /// the quest's source marks the quest as resolved for the whole run.
    fn quest_alive(
        quest: Quest,
        plan: &RequirementPlan,
        completed_depth: u8,
        items: &[WorldItem],
    ) -> bool {
        let mut resolved = false;
        for item in items {
            if quest_for_source(item.source) == Some(quest) {
                if item.depth <= plan.max_depth && plan.requirement.matches(item) {
                    return true;
                }
                resolved = true;
            }
        }
        !resolved && completed_depth < quest.window().1.min(plan.max_depth)
    }
}

impl FloorGate for QueryPlan {
    fn continue_after_floor(
        &self,
        completed_depth: u8,
        items_so_far: &[WorldItem],
        quests_so_far: &QuestSummary,
    ) -> bool {
        self.viable_after_floor(completed_depth, items_so_far, quests_so_far)
    }
}

#[cfg(test)]
mod tests {
    use crate::catalog::{ArmorEffect, Effect, ItemId, ItemKind, WeaponEffect};
    use crate::model::{Accessibility, ItemSource, WorldItem};
    use crate::query::{
        EffectRequirement, EffectSet, Requirement, SearchQuery, TierRequirement, UpgradeRequirement,
    };

    use crate::quests::QuestSummary;

    use super::QueryPlan;

    fn requirement(kind: ItemKind, upgrade: UpgradeRequirement) -> Requirement {
        Requirement {
            kind,
            weapon_category: None,
            item: None,
            tier: TierRequirement::Any,
            upgrade,
            effect: EffectRequirement::Any,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
            alternative_group: None,
            level_sum: None,
        }
    }

    fn query(requirements: Vec<Requirement>, max_depth: u8) -> SearchQuery {
        SearchQuery {
            requirements,
            max_depth,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        }
    }

    /// No plan in these cases carries a quest filter, so the summary is
    /// irrelevant and the assertions stay about the item horizon.
    fn viable(plan: &QueryPlan, completed_depth: u8, items: &[WorldItem]) -> bool {
        plan.viable_after_floor(completed_depth, items, &QuestSummary::default())
    }

    fn item(kind_item: ItemId, upgrade: u8, depth: u8, source: ItemSource) -> WorldItem {
        WorldItem {
            item: kind_item,
            upgrade,
            effect: None,
            cursed: false,
            depth,
            source,
            accessibility: Accessibility::Independent,
            secret: false,
        }
    }

    #[test]
    fn published_quest_windows_match_the_dungeon_and_the_quest_model() {
        use super::{QUESTS, Quest};

        assert_eq!(
            QUESTS.map(Quest::window),
            [(2, 4), (7, 9), (12, 14), (17, 19)]
        );
        // The Wandmaker's own window is the same fact spelled out in
        // `quests`, so the two views can never disagree.
        let (start, end) = Quest::Wandmaker.window();
        assert_eq!(
            start..=end,
            crate::quests::WandmakerQuestType::WINDOW,
            "the Wandmaker window must agree with the quest model"
        );
    }

    #[test]
    fn plus_four_ring_is_imp_only_with_a_depth_nineteen_deadline() {
        let plan = QueryPlan::analyze(&query(
            vec![requirement(ItemKind::Ring, UpgradeRequirement::Exact(4))],
            24,
        ));
        assert!(!plan.is_unsatisfiable());
        assert_eq!(plan.generation_depth(), 19);

        // Below the Imp's window the query is impossible.
        let shallow = QueryPlan::analyze(&query(
            vec![requirement(ItemKind::Ring, UpgradeRequirement::Exact(4))],
            16,
        ));
        assert!(shallow.is_unsatisfiable());
    }

    #[test]
    fn plus_three_wand_ends_at_the_wandmaker_window() {
        let plan = QueryPlan::analyze(&query(
            vec![requirement(ItemKind::Wand, UpgradeRequirement::Exact(3))],
            24,
        ));
        assert!(!plan.is_unsatisfiable());
        assert_eq!(plan.generation_depth(), 9);
        // A resolved Wandmaker without a +3 wand kills the seed immediately.
        let mismatched = [item(ItemId::WandFrost, 2, 7, ItemSource::WandmakerReward)];
        assert!(!viable(&plan, 7, &mismatched));
        // An unresolved Wandmaker stays alive through depth eight.
        assert!(viable(&plan, 8, &[]));
        assert!(!viable(&plan, 9, &[]));
        // A matching reward keeps the seed alive permanently.
        let matched = [item(ItemId::WandFrost, 3, 8, ItemSource::WandmakerReward)];
        assert!(viable(&plan, 9, &matched));
    }

    #[test]
    fn two_plus_four_rings_can_never_coexist() {
        let plan = QueryPlan::analyze(&query(
            vec![
                requirement(ItemKind::Ring, UpgradeRequirement::Exact(4)),
                requirement(ItemKind::Ring, UpgradeRequirement::AtLeast(4)),
            ],
            24,
        ));
        assert!(plan.is_unsatisfiable());
    }

    #[test]
    fn uncursed_plus_four_ring_comes_only_from_the_imp() {
        // v4.0.0 vault prizes are never cursed, so the flag no longer kills
        // the query; the ring is still quest-only and needs the Imp's floor.
        let mut ring = requirement(ItemKind::Ring, UpgradeRequirement::Exact(4));
        ring.require_uncursed = true;

        let plan = QueryPlan::analyze(&query(vec![ring], 24));
        assert!(!plan.is_unsatisfiable());
        assert_eq!(plan.generation_depth(), 19);
    }

    #[test]
    fn exclusive_quest_choices_respect_capacity_after_resolution() {
        // +3 weapon and +3 armor can come from Ghost and Blacksmith, one each.
        let plan = QueryPlan::analyze(&query(
            vec![
                requirement(ItemKind::Weapon, UpgradeRequirement::Exact(3)),
                requirement(ItemKind::Armor, UpgradeRequirement::Exact(3)),
            ],
            24,
        ));
        assert!(!plan.is_unsatisfiable());
        // Without fast mode, Crypt/Sacrifice keep both alive to full depth.
        assert_eq!(plan.generation_depth(), 24);

        let fast = QueryPlan::analyze(&SearchQuery {
            fast_mode: true,
            ..query(
                vec![
                    requirement(ItemKind::Weapon, UpgradeRequirement::Exact(3)),
                    requirement(ItemKind::Armor, UpgradeRequirement::Exact(3)),
                ],
                24,
            )
        });
        assert_eq!(fast.generation_depth(), 14);
        // The Ghost resolved without a +3: both requirements now hinge on the
        // single Blacksmith choice, which cannot cover two items.
        let ghost_missed = [
            item(ItemId::Sword, 1, 3, ItemSource::GhostReward),
            item(ItemId::MailArmor, 1, 3, ItemSource::GhostReward),
        ];
        assert!(!viable(&fast, 4, &ghost_missed));
        // A +3 Ghost weapon leaves the armor to the Blacksmith.
        let ghost_hit = [
            item(ItemId::Sword, 3, 3, ItemSource::GhostReward),
            item(ItemId::MailArmor, 3, 3, ItemSource::GhostReward),
        ];
        assert!(viable(&fast, 4, &ghost_hit));
    }

    #[test]
    fn curse_effects_exclude_good_only_sources() {
        // A cursed enchantment on a +3 weapon leaves only the Sacrificial fire.
        let cursed = Requirement {
            effect: EffectRequirement::exactly(Effect::Weapon(WeaponEffect::Sacrificial)),
            require_uncursed: false,
            ..requirement(ItemKind::Weapon, UpgradeRequirement::Exact(3))
        };
        let plan = QueryPlan::analyze(&query(vec![cursed], 24));
        assert!(!plan.is_unsatisfiable());
        assert_eq!(plan.generation_depth(), 24);
        // In fast mode the Sacrificial fire is ignored, so nothing remains.
        let fast = QueryPlan::analyze(&SearchQuery {
            fast_mode: true,
            ..query(vec![cursed], 24)
        });
        assert!(fast.is_unsatisfiable());

        // A good glyph on +3 armor is still reachable via Ghost/Blacksmith.
        let good = Requirement {
            effect: EffectRequirement::exactly(Effect::Armor(ArmorEffect::Thorns)),
            require_uncursed: false,
            ..requirement(ItemKind::Armor, UpgradeRequirement::Exact(3))
        };
        let fast_good = QueryPlan::analyze(&SearchQuery {
            fast_mode: true,
            ..query(vec![good], 24)
        });
        assert!(!fast_good.is_unsatisfiable());
        assert_eq!(fast_good.generation_depth(), 14);

        // A mixed set stays reachable through good-only sources, and an
        // uncursed pure-curse set is impossible anywhere.
        let mixed = Requirement {
            effect: EffectRequirement::OneOf(
                EffectSet::from_effects([
                    Effect::Weapon(WeaponEffect::Sacrificial),
                    Effect::Weapon(WeaponEffect::Blazing),
                ])
                .unwrap(),
            ),
            require_uncursed: false,
            ..requirement(ItemKind::Weapon, UpgradeRequirement::Exact(3))
        };
        let fast_mixed = QueryPlan::analyze(&SearchQuery {
            fast_mode: true,
            ..query(vec![mixed], 24)
        });
        assert!(!fast_mixed.is_unsatisfiable());
        assert_eq!(fast_mixed.generation_depth(), 14);
        // Any enchantment on an uncursed +3 weapon keeps the quest sources.
        let any_enchantment = Requirement {
            effect: EffectRequirement::OneOf(EffectSet::enchantments(ItemKind::Weapon).unwrap()),
            require_uncursed: true,
            ..requirement(ItemKind::Weapon, UpgradeRequirement::Exact(3))
        };
        let fast_any = QueryPlan::analyze(&SearchQuery {
            fast_mode: true,
            ..query(vec![any_enchantment], 24)
        });
        assert!(!fast_any.is_unsatisfiable());
        assert_eq!(fast_any.generation_depth(), 14);
    }

    #[test]
    fn alternative_groups_stay_alive_while_any_member_can_still_match() {
        // A +3 wand (Wandmaker-only) or a +4 ring (Imp-only): the slot lives
        // until both quests have resolved without a matching prize.
        let wand = Requirement {
            alternative_group: Some(1),
            ..requirement(ItemKind::Wand, UpgradeRequirement::Exact(3))
        };
        let ring = Requirement {
            alternative_group: Some(1),
            ..requirement(ItemKind::Ring, UpgradeRequirement::Exact(4))
        };
        let plan = QueryPlan::analyze(&query(vec![wand, ring], 24));
        assert!(!plan.is_unsatisfiable());
        // Generation must reach the deepest member's horizon.
        assert_eq!(plan.generation_depth(), 19);
        // A Wandmaker that resolved without the wand leaves the Imp alive.
        let missed_wand = [item(ItemId::WandFrost, 2, 8, ItemSource::WandmakerReward)];
        assert!(viable(&plan, 9, &missed_wand));
        // Once the Imp also resolves without a +4 ring, the slot is dead.
        let both_missed = [
            item(ItemId::WandFrost, 2, 8, ItemSource::WandmakerReward),
            item(ItemId::RingMight, 3, 18, ItemSource::ImpReward),
        ];
        assert!(!viable(&plan, 19, &both_missed));
        // A matching member keeps the slot alive permanently.
        let ring_hit = [
            item(ItemId::WandFrost, 2, 8, ItemSource::WandmakerReward),
            item(ItemId::RingMight, 4, 18, ItemSource::ImpReward),
        ];
        assert!(viable(&plan, 19, &ring_hit));
        // Two quest-only slots still need two quests: a second group whose
        // members both hinge on the Imp is pruned with the first.
        let imp_only = Requirement {
            alternative_group: Some(2),
            ..requirement(ItemKind::Ring, UpgradeRequirement::Exact(4))
        };
        let two_slots = QueryPlan::analyze(&query(vec![wand, ring, imp_only], 24));
        assert!(!two_slots.is_unsatisfiable());
        assert!(!viable(&two_slots, 9, &missed_wand));
        // A group whose members are all impossible is unsatisfiable.
        let impossible = QueryPlan::analyze(&query(vec![ring, wand], 6));
        assert!(impossible.is_unsatisfiable());
    }

    #[test]
    fn shop_pinned_requirements_use_shop_depths() {
        let shop = Requirement {
            source: Some(ItemSource::Shop),
            ..requirement(ItemKind::Weapon, UpgradeRequirement::Any)
        };
        let plan = QueryPlan::analyze(&query(vec![shop], 24));
        assert_eq!(plan.generation_depth(), 21);
        let shallow = QueryPlan::analyze(&query(vec![shop], 12));
        assert_eq!(shallow.generation_depth(), 11);
        let impossible = QueryPlan::analyze(&SearchQuery {
            requirements: vec![Requirement {
                upgrade: UpgradeRequirement::AtLeast(1),
                ..shop
            }],
            ..query(vec![], 24)
        });
        assert!(impossible.is_unsatisfiable());
    }

    #[test]
    fn melee_only_sources_never_satisfy_thrown_requirements() {
        use crate::catalog::WeaponCategory;

        // The Ghost, Sacrificial fire, and both statue kinds roll melee
        // weapons only, so pinning one with a thrown filter is impossible.
        for source in [
            ItemSource::GhostReward,
            ItemSource::SacrificialFire,
            ItemSource::Statue,
            ItemSource::ArmoredStatue,
        ] {
            let thrown = Requirement {
                weapon_category: Some(WeaponCategory::Thrown),
                source: Some(source),
                ..requirement(ItemKind::Weapon, UpgradeRequirement::Any)
            };
            assert!(
                QueryPlan::analyze(&query(vec![thrown], 24)).is_unsatisfiable(),
                "{source:?}"
            );
            let melee = Requirement {
                weapon_category: Some(WeaponCategory::Melee),
                ..thrown
            };
            assert!(
                !QueryPlan::analyze(&query(vec![melee], 24)).is_unsatisfiable(),
                "{source:?}"
            );
        }

        // Unpinned thrown requirements stay satisfiable through open drops,
        // and a +3 in fast mode becomes Blacksmith-only: the Ghost no longer
        // counts, so the plan ends at the Blacksmith's depth-14 deadline.
        let thrown = Requirement {
            weapon_category: Some(WeaponCategory::Thrown),
            ..requirement(ItemKind::Weapon, UpgradeRequirement::Any)
        };
        assert!(!QueryPlan::analyze(&query(vec![thrown], 24)).is_unsatisfiable());
        let fast = QueryPlan::analyze(&SearchQuery {
            fast_mode: true,
            ..query(
                vec![Requirement {
                    upgrade: UpgradeRequirement::Exact(3),
                    ..thrown
                }],
                24,
            )
        });
        assert!(!fast.is_unsatisfiable());
        assert_eq!(fast.generation_depth(), 14);
        let ghost_resolved = [item(ItemId::Sword, 3, 3, ItemSource::GhostReward)];
        assert!(viable(&fast, 4, &ghost_resolved));
        let blacksmith_resolved = [item(ItemId::Sword, 3, 13, ItemSource::BlacksmithReward)];
        assert!(!viable(&fast, 14, &blacksmith_resolved));
    }

    #[test]
    fn chest_prizes_reach_plus_three_outside_fast_mode() {
        use crate::catalog::WeaponCategory;

        // The flooded-vault, sentry, and secret-maze rooms bump one natural
        // weapon/missile/armor roll and drop it as a plain chest, so a
        // chest-pinned +3 must stay satisfiable — seed AAA-AAA-ACO carries a
        // +3 thrown weapon in exactly such a chest at depth 24.
        for (kind, category) in [
            (ItemKind::Weapon, None),
            (ItemKind::Weapon, Some(WeaponCategory::Melee)),
            (ItemKind::Weapon, Some(WeaponCategory::Thrown)),
            (ItemKind::Armor, None),
        ] {
            let pinned = Requirement {
                weapon_category: category,
                source: Some(ItemSource::Chest),
                ..requirement(kind, UpgradeRequirement::Exact(3))
            };
            assert!(
                !QueryPlan::analyze(&query(vec![pinned], 24)).is_unsatisfiable(),
                "{kind:?} {category:?}"
            );
            // A source pin is honored verbatim even in a fast-mode query.
            let fast_pinned = SearchQuery {
                fast_mode: true,
                ..query(vec![pinned], 24)
            };
            assert!(
                !QueryPlan::analyze(&fast_pinned).is_unsatisfiable(),
                "fast pinned {kind:?} {category:?}"
            );
        }

        // No chest path upgrades wands or rings past the natural rolls.
        let wand = Requirement {
            source: Some(ItemSource::Chest),
            ..requirement(ItemKind::Wand, UpgradeRequirement::Exact(3))
        };
        assert!(QueryPlan::analyze(&query(vec![wand], 24)).is_unsatisfiable());

        // With chests open at any depth, an unpinned thrown +3 must not
        // inherit the Blacksmith's depth-14 deadline outside fast mode: the
        // depth-24 chest prize of seed AAA-AAA-ACO would otherwise be
        // silently skipped.
        let thrown = Requirement {
            weapon_category: Some(WeaponCategory::Thrown),
            ..requirement(ItemKind::Weapon, UpgradeRequirement::Exact(3))
        };
        let plan = QueryPlan::analyze(&query(vec![thrown], 24));
        assert!(!plan.is_unsatisfiable());
        assert_eq!(plan.generation_depth(), 24);
        assert!(viable(&plan, 14, &[]));
    }

    #[test]
    fn require_blacksmith_bounds_depth_and_liveness() {
        let mut base = query(
            vec![requirement(ItemKind::Wand, UpgradeRequirement::Exact(3))],
            24,
        );
        base.require_blacksmith = true;
        let plan = QueryPlan::analyze(&base);
        assert!(!plan.is_unsatisfiable());
        assert_eq!(plan.generation_depth(), 14);
        let wand = [item(ItemId::WandFrost, 3, 8, ItemSource::WandmakerReward)];
        assert!(viable(&plan, 13, &wand));
        assert!(!viable(&plan, 14, &wand));

        base.max_depth = 11;
        assert!(QueryPlan::analyze(&base).is_unsatisfiable());
    }

    #[test]
    fn wandmaker_quest_filter_bounds_depth_and_prunes_on_the_rolled_variant() {
        use crate::quests::{ScheduledQuest, WandmakerQuestType};

        let quested = |max_depth| SearchQuery {
            wandmaker_quest: Some(WandmakerQuestType::Rotberry),
            ..query(
                vec![requirement(ItemKind::Weapon, UpgradeRequirement::Any)],
                max_depth,
            )
        };
        let scheduled = |variant| QuestSummary {
            wandmaker: Some(ScheduledQuest { variant, depth: 8 }),
            ..QuestSummary::default()
        };

        // Open weapon drops alone would run to depth 24; the filter only ever
        // extends generation, never shortens it.
        let plan = QueryPlan::analyze(&quested(24));
        assert!(!plan.is_unsatisfiable());
        assert_eq!(plan.generation_depth(), 24);

        // A wrong variant kills the seed the moment the Wandmaker appears,
        // long before the item horizon runs out.
        assert!(!plan.viable_after_floor(8, &[], &scheduled(WandmakerQuestType::ElementalEmbers)));
        assert!(plan.viable_after_floor(8, &[], &scheduled(WandmakerQuestType::Rotberry)));
        // No Wandmaker yet is fine until its window closes.
        assert!(plan.viable_after_floor(8, &[], &QuestSummary::default()));
        assert!(!plan.viable_after_floor(9, &[], &QuestSummary::default()));

        // A shallow query that stops before the Prison must still reach the
        // Wandmaker's floor, and one that cannot is impossible.
        let shallow = QueryPlan::analyze(&SearchQuery {
            requirements: vec![Requirement {
                max_depth: Some(3),
                ..requirement(ItemKind::Weapon, UpgradeRequirement::Any)
            }],
            ..quested(24)
        });
        assert_eq!(shallow.generation_depth(), 9);
        assert!(QueryPlan::analyze(&quested(6)).is_unsatisfiable());
        // Floors seven and eight can host the giver, so they stay possible.
        assert!(!QueryPlan::analyze(&quested(7)).is_unsatisfiable());
        assert_eq!(QueryPlan::analyze(&quested(7)).generation_depth(), 7);
    }

    #[test]
    fn wildcard_requirements_keep_exact_full_depth_semantics() {
        let plan = QueryPlan::analyze(&query(
            vec![requirement(ItemKind::Weapon, UpgradeRequirement::Any)],
            24,
        ));
        assert!(!plan.is_unsatisfiable());
        assert_eq!(plan.generation_depth(), 24);
        assert!(viable(&plan, 23, &[]));
    }

    #[test]
    fn per_requirement_floor_limit_short_circuits_generation() {
        let limited = Requirement {
            max_depth: Some(5),
            ..requirement(ItemKind::Weapon, UpgradeRequirement::Any)
        };
        let plan = QueryPlan::analyze(&query(vec![limited], 24));
        assert_eq!(plan.generation_depth(), 5);
        assert!(viable(&plan, 4, &[]));
        assert!(!viable(&plan, 5, &[]));

        let in_time = [item(ItemId::Sword, 0, 5, ItemSource::Heap)];
        assert!(viable(&plan, 5, &in_time));
        let too_late = [item(ItemId::Sword, 0, 6, ItemSource::Heap)];
        assert!(!viable(&plan, 6, &too_late));
    }
}
