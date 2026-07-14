//! Logic-based query feasibility: which sources can satisfy each requirement,
//! how deep generation must run, and when a partially generated seed can be
//! abandoned early.
//!
//! The rules here mirror structural facts of the v3.3.8 generator:
//!
//! - Natural equipment rolls never exceed +2 ([`crate::equipment`]), so +3
//!   weapons come only from the Sacrificial-fire room, the Ghost quest, or the
//!   Blacksmith; +3 armor only from the Crypt, the Ghost, or the Blacksmith;
//!   +3 wands only from the Wandmaker; and +3/+4 rings only from the Imp.
//! - Every quest resolves inside a fixed depth window (Ghost 2–4, Wandmaker
//!   7–9, Blacksmith 12–14, Imp 17–19) and spawns at most once per run, with
//!   the spawn forced on the window's final floor.
//! - Shops stock unupgraded, unenchanted items only, and quest reward choices
//!   are mutually exclusive, so each quest satisfies at most one requirement.
//!
//! Everything derived from these rules is exact: a rejected seed can never
//! match, and a shortened generation depth can never hide a match. The one
//! deliberately lossy refinement is [`SearchQuery::fast_mode`], which ignores
//! the rare Crypt/Sacrificial-fire +3 prizes so that +3 weapon/armor
//! requirements become quest-only and inherit the Blacksmith's depth-14
//! deadline.
//!
//! The searchable catalog contains equipment only. `NO_SCROLLS` halves the
//! scheduled Scroll of Upgrade drops, but no current requirement can target a
//! consumable or torch, so there is no challenge-dependent availability bound
//! to apply here. Its RNG knock-on effects are handled by generation itself.

use crate::catalog::{Effect, ItemKind};
use crate::model::{ItemSource, WorldItem};
use crate::query::{Requirement, SearchQuery, UpgradeRequirement};
use crate::search::FloorGate;

/// The four one-per-run reward quests, each offering a mutually exclusive
/// choice, so each can satisfy at most one requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum Quest {
    Ghost,
    Wandmaker,
    Blacksmith,
    Imp,
}

const QUESTS: [Quest; 4] = [
    Quest::Ghost,
    Quest::Wandmaker,
    Quest::Blacksmith,
    Quest::Imp,
];

impl Quest {
    /// The inclusive depth window inside which the quest can first spawn. The
    /// spawn chance reaches certainty on the final floor, so a run whose item
    /// list has no reward items past the window can never gain them.
    const fn window(self) -> (u8, u8) {
        match self {
            Self::Ghost => (2, 4),
            Self::Wandmaker => (7, 9),
            Self::Blacksmith => (12, 14),
            Self::Imp => (17, 19),
        }
    }

    const fn item_source(self) -> ItemSource {
        match self {
            Self::Ghost => ItemSource::GhostReward,
            Self::Wandmaker => ItemSource::WandmakerReward,
            Self::Blacksmith => ItemSource::BlacksmithReward,
            Self::Imp => ItemSource::ImpReward,
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
        ItemSource::ImpReward => Some(Quest::Imp),
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
    fast_mode: bool,
) -> Option<(u8, u8, EffectPolicy)> {
    use EffectPolicy::{Any, GoodOnly, Never};
    use ItemKind::{Armor, Ring, Wand, Weapon};
    use ItemSource as S;
    Some(match (source, kind) {
        // Plain drops and chest variants use the natural rolls, capped at +2,
        // as do crystal chests/mimics (which stock only wands and rings).
        (S::Heap | S::Chest | S::LockedChest | S::Skeleton | S::Mimic, _)
        | (S::CrystalChest | S::CrystalMimic, Wand | Ring) => (0, 2, Any),
        // Golden mimics strip curse effects; statues force a good effect.
        // Neither exceeds the natural +2 cap.
        (S::GoldenMimic, _) | (S::Statue, Weapon) | (S::ArmoredStatue, Weapon | Armor) => {
            (0, 2, GoodOnly)
        }
        // The Crypt bumps non-cursed armor to at most +3; in fast mode this
        // exotic path is deliberately ignored so +3 armor becomes quest-only.
        (S::Tomb, Armor) | (S::SacrificialFire, Weapon) => {
            if fast_mode {
                (0, 2, Any)
            } else {
                (0, 3, Any)
            }
        }
        // Shop stock is always +0 with no effect.
        (S::Shop, _) => (0, 0, Never),
        // Quest rewards.
        (S::GhostReward | S::BlacksmithReward, Weapon | Armor) => (0, 3, GoodOnly),
        (S::WandmakerReward, Wand) => (1, 3, Never),
        (S::ImpReward, Ring) => (2, 4, Never),
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

const fn effect_reachable(wanted: Option<Effect>, policy: EffectPolicy) -> bool {
    let Some(effect) = wanted else {
        return true;
    };
    let is_curse = match effect {
        Effect::Weapon(effect) => effect.is_curse(),
        Effect::Armor(effect) => effect.is_curse(),
    };
    match policy {
        EffectPolicy::Never => false,
        EffectPolicy::GoodOnly => !is_curse,
        EffectPolicy::Any => true,
    }
}

/// Whether `source` can ever produce an item satisfying `requirement`.
fn source_feasible(requirement: &Requirement, source: ItemSource, fast_mode: bool) -> bool {
    if requirement.source.is_some_and(|wanted| wanted != source) {
        return false;
    }
    if requirement.require_uncursed
        && (source == ItemSource::ImpReward
            || requirement.effect.is_some_and(|effect| match effect {
                Effect::Weapon(effect) => effect.is_curse(),
                Effect::Armor(effect) => effect.is_curse(),
            }))
    {
        return false;
    }
    // An explicit source pin is the user's claim, not ours: honor it verbatim
    // rather than applying the fast-mode refinement.
    let fast_mode = fast_mode && requirement.source.is_none();
    source_profile(source, requirement.kind, fast_mode).is_some_and(|(low, high, policy)| {
        upgrade_reachable(requirement.upgrade, low, high)
            && effect_reachable(requirement.effect, policy)
    })
}

const ALL_SOURCES: [ItemSource; 17] = [
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
#[derive(Clone, Debug)]
pub struct QueryPlan {
    requirements: Vec<RequirementPlan>,
    generation_depth: u8,
    /// Latest depth by which a required Blacksmith must have appeared.
    blacksmith_deadline: Option<u8>,
    unsatisfiable: bool,
}

impl QueryPlan {
    /// Derives the plan for a validated query.
    #[must_use]
    pub fn analyze(query: &SearchQuery) -> Self {
        let max_depth = query.max_depth;
        let mut generation_depth = 1;
        let mut requirements = Vec::with_capacity(query.requirements.len());
        for requirement in &query.requirements {
            let requirement_max_depth = requirement.max_depth.unwrap_or(max_depth).min(max_depth);
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
            requirements.push(RequirementPlan {
                requirement: *requirement,
                max_depth: requirement_max_depth,
                quests,
                open_deadline,
            });
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

        let mut plan = Self {
            requirements,
            generation_depth,
            blacksmith_deadline,
            unsatisfiable: false,
        };
        plan.unsatisfiable = !plan.viable_after_floor(0, &[]);
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

    /// Whether a seed whose floors `1..=completed_depth` produced `items` can
    /// still satisfy every requirement. Conservative: `false` is proof that
    /// the final matcher would reject the seed, while `true` promises nothing.
    #[must_use]
    pub fn viable_after_floor(&self, completed_depth: u8, items: &[WorldItem]) -> bool {
        // Requirements that only quests can still satisfy, grouped by their
        // live quest bit set. Each quest offers a mutually exclusive choice,
        // so it can cover at most one requirement; Hall's condition over the
        // sixteen quest subsets then decides whether an assignment exists.
        let mut quest_only = [0_u16; 16];
        for plan in &self.requirements {
            let satisfied_by_open_item = items.iter().any(|item| {
                item.depth <= plan.max_depth
                    && quest_for_source(item.source).is_none()
                    && plan.requirement.matches(item)
            });
            if satisfied_by_open_item
                || plan
                    .open_deadline
                    .is_some_and(|deadline| completed_depth < deadline)
            {
                continue;
            }
            let mut live = 0_u8;
            for quest in QUESTS {
                if plan.quests & quest.bit() != 0
                    && Self::quest_alive(quest, plan, completed_depth, items)
                {
                    live |= quest.bit();
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
        let source = quest.item_source();
        let mut resolved = false;
        for item in items {
            if item.source == source {
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
    fn continue_after_floor(&self, completed_depth: u8, items_so_far: &[WorldItem]) -> bool {
        self.viable_after_floor(completed_depth, items_so_far)
    }
}

#[cfg(test)]
mod tests {
    use crate::catalog::{ArmorEffect, Effect, ItemId, ItemKind, WeaponEffect};
    use crate::model::{Accessibility, ItemSource, WorldItem};
    use crate::query::{Requirement, SearchQuery, TierRequirement, UpgradeRequirement};

    use super::QueryPlan;

    fn requirement(kind: ItemKind, upgrade: UpgradeRequirement) -> Requirement {
        Requirement {
            kind,
            item: None,
            tier: TierRequirement::Any,
            upgrade,
            effect: None,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
        }
    }

    fn query(requirements: Vec<Requirement>, max_depth: u8) -> SearchQuery {
        SearchQuery {
            requirements,
            max_depth,
            challenges: crate::challenges::Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            fast_mode: false,
        }
    }

    fn item(kind_item: ItemId, upgrade: u8, depth: u8, source: ItemSource) -> WorldItem {
        WorldItem {
            item: kind_item,
            transmuted_item: None,
            upgrade,
            effect: None,
            cursed: false,
            depth,
            source,
            accessibility: Accessibility::Independent,
        }
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
        assert!(!plan.viable_after_floor(7, &mismatched));
        // An unresolved Wandmaker stays alive through depth eight.
        assert!(plan.viable_after_floor(8, &[]));
        assert!(!plan.viable_after_floor(9, &[]));
        // A matching reward keeps the seed alive permanently.
        let matched = [item(ItemId::WandFrost, 3, 8, ItemSource::WandmakerReward)];
        assert!(plan.viable_after_floor(9, &matched));
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
    fn uncursed_plus_four_ring_is_impossible() {
        let mut ring = requirement(ItemKind::Ring, UpgradeRequirement::Exact(4));
        ring.require_uncursed = true;

        assert!(QueryPlan::analyze(&query(vec![ring], 24)).is_unsatisfiable());
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
        assert!(!fast.viable_after_floor(4, &ghost_missed));
        // A +3 Ghost weapon leaves the armor to the Blacksmith.
        let ghost_hit = [
            item(ItemId::Sword, 3, 3, ItemSource::GhostReward),
            item(ItemId::MailArmor, 3, 3, ItemSource::GhostReward),
        ];
        assert!(fast.viable_after_floor(4, &ghost_hit));
    }

    #[test]
    fn curse_effects_exclude_good_only_sources() {
        // A cursed enchantment on a +3 weapon leaves only the Sacrificial fire.
        let cursed = Requirement {
            effect: Some(Effect::Weapon(WeaponEffect::Sacrificial)),
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
            effect: Some(Effect::Armor(ArmorEffect::Thorns)),
            require_uncursed: false,
            ..requirement(ItemKind::Armor, UpgradeRequirement::Exact(3))
        };
        let fast_good = QueryPlan::analyze(&SearchQuery {
            fast_mode: true,
            ..query(vec![good], 24)
        });
        assert!(!fast_good.is_unsatisfiable());
        assert_eq!(fast_good.generation_depth(), 14);
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
        assert!(plan.viable_after_floor(13, &wand));
        assert!(!plan.viable_after_floor(14, &wand));

        base.max_depth = 11;
        assert!(QueryPlan::analyze(&base).is_unsatisfiable());
    }

    #[test]
    fn wildcard_requirements_keep_exact_full_depth_semantics() {
        let plan = QueryPlan::analyze(&query(
            vec![requirement(ItemKind::Weapon, UpgradeRequirement::Any)],
            24,
        ));
        assert!(!plan.is_unsatisfiable());
        assert_eq!(plan.generation_depth(), 24);
        assert!(plan.viable_after_floor(23, &[]));
    }

    #[test]
    fn per_requirement_floor_limit_short_circuits_generation() {
        let limited = Requirement {
            max_depth: Some(5),
            ..requirement(ItemKind::Weapon, UpgradeRequirement::Any)
        };
        let plan = QueryPlan::analyze(&query(vec![limited], 24));
        assert_eq!(plan.generation_depth(), 5);
        assert!(plan.viable_after_floor(4, &[]));
        assert!(!plan.viable_after_floor(5, &[]));

        let in_time = [item(ItemId::Sword, 0, 5, ItemSource::Heap)];
        assert!(plan.viable_after_floor(5, &in_time));
        let too_late = [item(ItemId::Sword, 0, 6, ItemSource::Heap)];
        assert!(!plan.viable_after_floor(6, &too_late));
    }
}
