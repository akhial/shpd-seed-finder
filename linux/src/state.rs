// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared query state and presentation labels for the whole window.

use std::fmt::Write as _;

use shpd_seedfinder_core::catalog::{Effect, ItemId, ItemKind, WeaponCategory, item};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::model::ItemSource;
use shpd_seedfinder_core::query::{
    EffectRequirement, EffectSet, Requirement, SearchQuery, TierRequirement, UpgradeRequirement,
    UpgradeSum,
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

/// Effect predicate as edited in the interface. `AnyEnchantment` expands to
/// the family's full non-curse set when building the engine query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiEffect {
    /// Wildcard: enchanted or not.
    Any,
    /// Any non-curse enchantment or glyph of the requirement's family.
    AnyEnchantment,
    /// One of the chosen effects.
    OneOf(EffectSet),
}

impl UiEffect {
    /// The single pinned effect, when the predicate names exactly one.
    #[must_use]
    pub fn single(self) -> Option<Effect> {
        match self {
            Self::OneOf(set) if set.len() == 1 => set.effects().next(),
            Self::Any | Self::AnyEnchantment | Self::OneOf(_) => None,
        }
    }
}

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
    pub effect: UiEffect,
    pub require_uncursed: bool,
    pub source: Option<ItemSource>,
    pub identity_group: Option<u8>,
    pub max_depth: Option<u8>,
    pub alternative_group: Option<u8>,
    pub upgrade_sum: Option<UpgradeSum>,
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
            effect: UiEffect::Any,
            require_uncursed: false,
            source: None,
            identity_group: None,
            max_depth: None,
            alternative_group: None,
            upgrade_sum: None,
        }
    }

    #[must_use]
    pub fn to_core(self) -> Requirement {
        Requirement {
            kind: self.kind,
            weapon_category: self.weapon_category,
            item: self.item,
            tier: self.tier,
            upgrade: self.upgrade,
            effect: match self.effect {
                UiEffect::Any => EffectRequirement::Any,
                UiEffect::AnyEnchantment => EffectSet::enchantments(self.kind)
                    .map_or(EffectRequirement::Any, EffectRequirement::OneOf),
                UiEffect::OneOf(set) => EffectRequirement::OneOf(set),
            },
            require_uncursed: self.require_uncursed,
            source: self.source,
            identity_group: self.identity_group,
            max_depth: self.max_depth,
            alternative_group: self.alternative_group,
            upgrade_sum: self.upgrade_sum,
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
        if let Some(phrase) = effect_phrase(self.kind, self.effect) {
            let _ = write!(text, " · {phrase}");
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
        if let Some(sum) = self.upgrade_sum {
            let _ = write!(
                text,
                " · combined +{} total (group {})",
                sum.minimum_total,
                group_letter(sum.group)
            );
        }
        if let Some(depth) = self.max_depth {
            let _ = write!(text, " · by floor {depth}");
        }
        text
    }
}

/// Subtitle fragment describing an effect predicate, or `None` for the
/// wildcard: a single name, up to four names joined with "or", or a count.
fn effect_phrase(kind: ItemKind, effect: UiEffect) -> Option<String> {
    let family = if kind == ItemKind::Armor {
        ("any glyph", "glyphs")
    } else {
        ("any enchantment", "enchantments")
    };
    let set = match effect {
        UiEffect::Any => return None,
        UiEffect::AnyEnchantment => return Some(family.0.to_owned()),
        UiEffect::OneOf(set) => set,
    };
    if EffectSet::enchantments(set.family()) == Some(set) {
        return Some(family.0.to_owned());
    }
    let names: Vec<&str> = set.effects().map(Effect::wire_name).collect();
    Some(match names.as_slice() {
        [] => return None,
        [single] => (*single).to_owned(),
        names if names.len() <= 4 => {
            let (last, rest) = names.split_last()?;
            format!("{} or {last}", rest.join(", "))
        }
        names => format!("any of {} {}", names.len(), family.1),
    })
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

    /// The smallest alternative-group label not yet in use.
    #[must_use]
    pub fn free_alternative_group(&self) -> u8 {
        (1..=u8::MAX)
            .find(|group| {
                !self
                    .requirements
                    .iter()
                    .any(|requirement| requirement.alternative_group == Some(*group))
            })
            .unwrap_or(u8::MAX)
    }

    /// Clears the alternative group of any requirement left alone in its
    /// group, e.g. after the other members were removed.
    pub fn dissolve_lone_alternatives(&mut self) {
        let lone: Vec<u8> = self
            .requirements
            .iter()
            .filter_map(|requirement| requirement.alternative_group)
            .filter(|group| {
                self.requirements
                    .iter()
                    .filter(|requirement| requirement.alternative_group == Some(*group))
                    .count()
                    == 1
            })
            .collect();
        for requirement in &mut self.requirements {
            if let Some(group) = requirement.alternative_group
                && lone.contains(&group)
            {
                requirement.alternative_group = None;
            }
        }
    }

    /// Copies the combined-upgrade total of the requirement with `key` to
    /// every other member of its sum group, keeping the group consistent.
    pub fn align_upgrade_sums(&mut self, key: u64) {
        let Some(sum) = self
            .requirements
            .iter()
            .find(|requirement| requirement.key == key)
            .and_then(|requirement| requirement.upgrade_sum)
        else {
            return;
        };
        for requirement in &mut self.requirements {
            if requirement.key != key
                && requirement
                    .upgrade_sum
                    .is_some_and(|other| other.group == sum.group)
            {
                requirement.upgrade_sum = Some(sum);
            }
        }
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
    use shpd_seedfinder_core::catalog::{ArmorEffect, Effect, ItemId, ItemKind, WeaponEffect};
    use shpd_seedfinder_core::query::{EffectSet, TierRequirement, UpgradeRequirement, UpgradeSum};

    use super::{AppState, UiEffect, UiRequirement};

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
    fn subtitles_phrase_effect_sets_by_size_and_family() {
        let mut requirement = UiRequirement::new(1);
        requirement.effect =
            UiEffect::OneOf(EffectSet::single(Effect::Weapon(WeaponEffect::Blazing)));
        assert_eq!(requirement.subtitle(), "Any upgrade · Blazing");

        requirement.effect = UiEffect::OneOf(
            EffectSet::from_effects([
                Effect::Weapon(WeaponEffect::Blocking),
                Effect::Weapon(WeaponEffect::Projecting),
                Effect::Weapon(WeaponEffect::Vampiric),
            ])
            .unwrap(),
        );
        assert_eq!(
            requirement.subtitle(),
            "Any upgrade · Blocking, Projecting or Vampiric"
        );

        requirement.effect = UiEffect::OneOf(
            EffectSet::from_effects(
                [
                    WeaponEffect::Blazing,
                    WeaponEffect::Chilling,
                    WeaponEffect::Kinetic,
                    WeaponEffect::Shocking,
                    WeaponEffect::Blocking,
                ]
                .map(Effect::Weapon),
            )
            .unwrap(),
        );
        assert_eq!(
            requirement.subtitle(),
            "Any upgrade · any of 5 enchantments"
        );

        requirement.effect = UiEffect::AnyEnchantment;
        assert_eq!(requirement.subtitle(), "Any upgrade · any enchantment");
        requirement.effect = UiEffect::OneOf(EffectSet::enchantments(ItemKind::Weapon).unwrap());
        assert_eq!(requirement.subtitle(), "Any upgrade · any enchantment");

        requirement.kind = ItemKind::Armor;
        requirement.effect = UiEffect::AnyEnchantment;
        assert_eq!(requirement.subtitle(), "Any upgrade · any glyph");
        requirement.effect = UiEffect::OneOf(EffectSet::single(Effect::Armor(ArmorEffect::Thorns)));
        assert_eq!(requirement.subtitle(), "Any upgrade · Thorns");
    }

    #[test]
    fn subtitles_include_combined_upgrade_totals() {
        let mut requirement = UiRequirement::new(1);
        requirement.kind = ItemKind::Ring;
        requirement.upgrade_sum = Some(UpgradeSum {
            group: 1,
            minimum_total: 2,
        });
        assert_eq!(
            requirement.subtitle(),
            "Any upgrade · combined +2 total (group A)"
        );
    }

    #[test]
    fn group_helpers_allocate_dissolve_and_align() {
        let mut state = AppState::default();
        for (alternative_group, upgrade_sum) in [
            (Some(1), None),
            (Some(1), None),
            (Some(3), None),
            (
                None,
                Some(UpgradeSum {
                    group: 2,
                    minimum_total: 2,
                }),
            ),
            (
                None,
                Some(UpgradeSum {
                    group: 2,
                    minimum_total: 5,
                }),
            ),
        ] {
            let key = state.claim_key();
            state.requirements.push(UiRequirement {
                alternative_group,
                upgrade_sum,
                ..UiRequirement::new(key)
            });
        }
        assert_eq!(state.free_alternative_group(), 2);

        state.dissolve_lone_alternatives();
        assert_eq!(state.requirements[0].alternative_group, Some(1));
        assert_eq!(state.requirements[1].alternative_group, Some(1));
        assert_eq!(state.requirements[2].alternative_group, None);

        state.align_upgrade_sums(4);
        assert_eq!(
            state.requirements[4].upgrade_sum,
            Some(UpgradeSum {
                group: 2,
                minimum_total: 2,
            })
        );
    }

    #[test]
    fn any_enchantment_expands_to_the_full_family_set() {
        let mut requirement = UiRequirement::new(1);
        requirement.effect = UiEffect::AnyEnchantment;
        assert_eq!(
            requirement.to_core().effect,
            shpd_seedfinder_core::query::EffectRequirement::OneOf(
                EffectSet::enchantments(ItemKind::Weapon).unwrap()
            )
        );
        assert_eq!(
            UiEffect::OneOf(EffectSet::single(Effect::Weapon(WeaponEffect::Lucky))).single(),
            Some(Effect::Weapon(WeaponEffect::Lucky))
        );
        assert_eq!(UiEffect::AnyEnchantment.single(), None);
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
