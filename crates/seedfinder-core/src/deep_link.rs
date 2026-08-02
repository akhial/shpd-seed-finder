//! Compact shareable-link codec for search queries.
//!
//! A deep link carries a whole query as a short base64url code, e.g.
//! `https://shpd-seed-seeker.web.app/#q=EAEkgA`. The payload is a versioned
//! bit stream, so codes shared today must keep decoding in every future
//! release: the numeric code tables below are frozen by tests and may only
//! ever grow at the end.

// Every narrowing cast in this module operates on a value already masked or
// bounds-checked to fewer bits than the destination type.
#![allow(clippy::cast_possible_truncation)]

use crate::catalog::{
    ALL_ARMOR_EFFECTS, ALL_WEAPON_EFFECTS, Effect, ItemId, ItemKind, WeaponCategory,
};
use crate::challenges::Challenges;
use crate::model::ItemSource;
use crate::query::{Requirement, SearchQuery, TierRequirement, UpgradeRequirement};
use crate::quests::WandmakerQuestType;

/// Canonical prefix for shared links; the code follows the `#q=` fragment.
pub const WEB_LINK_PREFIX: &str = "https://shpd-seed-seeker.web.app/#q=";

/// Custom URI scheme registered by the desktop apps.
pub const URI_SCHEME: &str = "seedseeker";

const VERSION: u8 = 1;
/// Requirement-count field width; far above anything the UIs produce.
const MAX_REQUIREMENTS: usize = 63;

const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Item sources in frozen code order (indices are part of the link format).
const SOURCE_CODES: &[ItemSource] = &[
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

/// Encodes a validated query as a bare share code (no URL prefix).
///
/// # Errors
///
/// Returns a message when the query fails validation or has more
/// requirements than the format can carry, so every produced code is
/// guaranteed to decode.
pub fn encode(query: &SearchQuery) -> Result<String, String> {
    query
        .validate()
        .map_err(|error| format!("invalid query: {error}"))?;
    if query.requirements.len() > MAX_REQUIREMENTS {
        return Err(format!(
            "a share link can carry at most {MAX_REQUIREMENTS} requirements"
        ));
    }
    for (index, requirement) in query.requirements.iter().enumerate() {
        // Like the results-file format, links restrict same-item groups to
        // what every app's editor can express (A..D), even though the engine
        // allows more.
        if requirement.identity_group.is_some_and(|group| group > 4) {
            return Err(format!(
                "requirement {}: same-item group must be between 1 and 4 (A..D)",
                index + 1
            ));
        }
    }
    let mut bits = BitWriter::default();
    bits.push(VERSION.into(), 4);
    bits.push(query.require_blacksmith.into(), 1);
    bits.push(query.exclude_blacksmith_rewards.into(), 1);
    bits.push(query.fast_mode.into(), 1);
    push_optional(&mut bits, query.max_depth != 24, || {
        (u32::from(query.max_depth) - 1, 5)
    });
    push_optional(&mut bits, query.challenges != Challenges::NONE, || {
        (query.challenges.bits().into(), 9)
    });
    push_optional(&mut bits, query.wandmaker_quest.is_some(), || {
        (
            query
                .wandmaker_quest
                .map_or(0, |quest| u32::from(quest as u8) - 1),
            2,
        )
    });
    bits.push(query.requirements.len() as u32, 6);
    for requirement in &query.requirements {
        encode_requirement(&mut bits, requirement);
    }
    Ok(base64url_encode(&bits.finish()))
}

/// Encodes a validated query as a full shareable web link.
///
/// # Errors
///
/// Propagates [`encode`] failures.
pub fn encode_link(query: &SearchQuery) -> Result<String, String> {
    Ok(format!("{WEB_LINK_PREFIX}{}", encode(query)?))
}

/// Decodes a bare share code produced by [`encode`] (any version).
///
/// # Errors
///
/// Returns a human-readable message for malformed, truncated, or
/// unsupported codes and for payloads that fail query validation.
pub fn decode(code: &str) -> Result<SearchQuery, String> {
    let bytes = base64url_decode(code.trim())?;
    let mut bits = BitReader::new(&bytes);
    let version = bits.pull(4)?;
    if version != u32::from(VERSION) {
        return Err(format!(
            "this link uses format version {version}; this app only understands \
             version {VERSION} — it may have been created by a newer release"
        ));
    }
    let require_blacksmith = bits.pull(1)? == 1;
    let exclude_blacksmith_rewards = bits.pull(1)? == 1;
    let fast_mode = bits.pull(1)? == 1;
    let max_depth = if bits.pull(1)? == 1 {
        depth_from(bits.pull(5)?)?
    } else {
        24
    };
    let challenges = if bits.pull(1)? == 1 {
        Challenges::new(bits.pull(9)? as u16).map_err(|error| error.to_string())?
    } else {
        Challenges::NONE
    };
    let wandmaker_quest = if bits.pull(1)? == 1 {
        Some(wandmaker_quest_from(bits.pull(2)?)?)
    } else {
        None
    };
    let count = bits.pull(6)?;
    let requirements = (0..count)
        .map(|index| {
            decode_requirement(&mut bits)
                .map_err(|error| format!("requirement {}: {error}", index + 1))
        })
        .collect::<Result<Vec<_>, _>>()?;
    bits.expect_exhausted()?;
    let query = SearchQuery {
        requirements,
        max_depth,
        challenges,
        require_blacksmith,
        exclude_blacksmith_rewards,
        wandmaker_quest,
        fast_mode,
    };
    query
        .validate()
        .map_err(|error| format!("invalid query: {error}"))?;
    Ok(query)
}

/// Pulls the share code out of user-facing link text.
///
/// Accepts full web links (`…#q=CODE` or `…?q=CODE`), custom-scheme links
/// (`seedseeker://q/CODE`), and bare codes. Returns `None` for text without
/// any plausible code.
#[must_use]
pub fn extract_code(text: &str) -> Option<&str> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    // A `q=` parameter introduced by `#`, `?`, or `&` wins wherever it
    // appears, so links keep working if the host page ever gains other
    // parameters.
    let mut search = text;
    while let Some(position) = search.find("q=") {
        let preceded = position > 0 && matches!(&search[position - 1..position], "#" | "?" | "&");
        let value = &search[position + 2..];
        if preceded {
            let end = value.find(['&', '#']).unwrap_or(value.len());
            return Some(&value[..end]);
        }
        search = value;
    }
    if let Some(rest) = text.strip_prefix(concat!("seedseeker", "://")) {
        return rest.rsplit('/').next().filter(|code| !code.is_empty());
    }
    if text.contains("://") || text.contains('/') {
        return None;
    }
    Some(text)
}

/// Decodes any accepted link form — see [`extract_code`] and [`decode`].
///
/// # Errors
///
/// Returns a message when no code is present or the code fails to decode.
pub fn decode_text(text: &str) -> Result<SearchQuery, String> {
    let code = extract_code(text).ok_or_else(|| "the text contains no share code".to_owned())?;
    decode(code)
}

fn encode_requirement(bits: &mut BitWriter, requirement: &Requirement) {
    bits.push(kind_code(requirement.kind, requirement.weapon_category), 3);
    push_optional(&mut *bits, requirement.item.is_some(), || {
        (u32::from(requirement.item.unwrap() as u8), 7)
    });
    match requirement.tier {
        TierRequirement::Any => bits.push(0, 2),
        TierRequirement::Exact(value) => push_filter(bits, 1, value),
        TierRequirement::AtLeast(value) => push_filter(bits, 2, value),
        TierRequirement::AtMost(value) => push_filter(bits, 3, value),
    }
    match requirement.upgrade {
        UpgradeRequirement::Any => bits.push(0, 2),
        UpgradeRequirement::Exact(value) => push_filter(bits, 1, value),
        UpgradeRequirement::AtLeast(value) => push_filter(bits, 2, value),
    }
    push_optional(&mut *bits, requirement.effect.is_some(), || {
        (effect_code(requirement.effect.unwrap()), 5)
    });
    bits.push(requirement.require_uncursed.into(), 1);
    push_optional(&mut *bits, requirement.source.is_some(), || {
        (source_code(requirement.source.unwrap()), 5)
    });
    push_optional(&mut *bits, requirement.identity_group.is_some(), || {
        (requirement.identity_group.unwrap().into(), 8)
    });
    push_optional(&mut *bits, requirement.max_depth.is_some(), || {
        (u32::from(requirement.max_depth.unwrap()) - 1, 5)
    });
}

fn decode_requirement(bits: &mut BitReader<'_>) -> Result<Requirement, String> {
    let (kind, weapon_category) = kind_from(bits.pull(3)?)?;
    let item = if bits.pull(1)? == 1 {
        Some(item_from(bits.pull(7)?)?)
    } else {
        None
    };
    let tier = match (bits.pull(2)?, &mut *bits) {
        (0, _) => TierRequirement::Any,
        (1, bits) => TierRequirement::Exact(bits.pull(3)? as u8),
        (2, bits) => TierRequirement::AtLeast(bits.pull(3)? as u8),
        (_, bits) => TierRequirement::AtMost(bits.pull(3)? as u8),
    };
    let upgrade = match (bits.pull(2)?, &mut *bits) {
        (0, _) => UpgradeRequirement::Any,
        (1, bits) => UpgradeRequirement::Exact(bits.pull(3)? as u8),
        (2, bits) => UpgradeRequirement::AtLeast(bits.pull(3)? as u8),
        (mode, _) => return Err(format!("unknown upgrade mode {mode}")),
    };
    let effect = if bits.pull(1)? == 1 {
        Some(effect_from(kind, bits.pull(5)?)?)
    } else {
        None
    };
    let require_uncursed = bits.pull(1)? == 1;
    let source = if bits.pull(1)? == 1 {
        Some(source_from(bits.pull(5)?)?)
    } else {
        None
    };
    let identity_group = if bits.pull(1)? == 1 {
        match bits.pull(8)? as u8 {
            group @ 1..=4 => Some(group),
            _ => return Err("same-item group must be between 1 and 4 (A..D)".to_owned()),
        }
    } else {
        None
    };
    let max_depth = if bits.pull(1)? == 1 {
        Some(depth_from(bits.pull(5)?)?)
    } else {
        None
    };
    Ok(Requirement {
        kind,
        weapon_category,
        item,
        tier,
        upgrade,
        effect,
        require_uncursed,
        source,
        identity_group,
        max_depth,
    })
}

fn push_optional(bits: &mut BitWriter, present: bool, value: impl FnOnce() -> (u32, u32)) {
    bits.push(present.into(), 1);
    if present {
        let (value, width) = value();
        bits.push(value, width);
    }
}

fn push_filter(bits: &mut BitWriter, mode: u32, value: u8) {
    bits.push(mode, 2);
    bits.push(value.into(), 3);
}

const fn kind_code(kind: ItemKind, weapon_category: Option<WeaponCategory>) -> u32 {
    match (kind, weapon_category) {
        (ItemKind::Weapon, None) => 0,
        (ItemKind::Weapon, Some(WeaponCategory::Melee)) => 1,
        (ItemKind::Weapon, Some(WeaponCategory::Thrown)) => 2,
        (ItemKind::Armor, _) => 3,
        (ItemKind::Wand, _) => 4,
        (ItemKind::Ring, _) => 5,
    }
}

fn kind_from(code: u32) -> Result<(ItemKind, Option<WeaponCategory>), String> {
    match code {
        0 => Ok((ItemKind::Weapon, None)),
        1 => Ok((ItemKind::Weapon, Some(WeaponCategory::Melee))),
        2 => Ok((ItemKind::Weapon, Some(WeaponCategory::Thrown))),
        3 => Ok((ItemKind::Armor, None)),
        4 => Ok((ItemKind::Wand, None)),
        5 => Ok((ItemKind::Ring, None)),
        _ => Err(format!("unknown category code {code}")),
    }
}

/// Wandmaker variants ride the wire order used everywhere else (corpse dust,
/// elemental embers, rotberry), biased down by one so the three of them fit in
/// two bits.
fn wandmaker_quest_from(code: u32) -> Result<WandmakerQuestType, String> {
    match code {
        0 => Ok(WandmakerQuestType::CorpseDust),
        1 => Ok(WandmakerQuestType::ElementalEmbers),
        2 => Ok(WandmakerQuestType::Rotberry),
        _ => Err(format!("unknown Wandmaker quest code {code}")),
    }
}

fn item_from(code: u32) -> Result<ItemId, String> {
    ALL_ITEM_IDS
        .get(code as usize)
        .copied()
        .ok_or_else(|| format!("unknown item code {code}"))
}

fn effect_code(effect: Effect) -> u32 {
    match effect {
        Effect::Weapon(effect) => effect as u32,
        Effect::Armor(effect) => effect as u32,
    }
}

fn effect_from(kind: ItemKind, code: u32) -> Result<Effect, String> {
    let effect = match kind {
        ItemKind::Weapon => ALL_WEAPON_EFFECTS
            .get(code as usize)
            .copied()
            .map(Effect::Weapon),
        ItemKind::Armor => ALL_ARMOR_EFFECTS
            .get(code as usize)
            .copied()
            .map(Effect::Armor),
        ItemKind::Wand | ItemKind::Ring => None,
    };
    effect.ok_or_else(|| format!("effect code {code} is not valid for this kind"))
}

fn source_code(source: ItemSource) -> u32 {
    // SOURCE_CODES covers every variant, so the lookup cannot fail.
    SOURCE_CODES
        .iter()
        .position(|candidate| *candidate == source)
        .unwrap_or_default() as u32
}

fn source_from(code: u32) -> Result<ItemSource, String> {
    SOURCE_CODES
        .get(code as usize)
        .copied()
        .ok_or_else(|| format!("unknown source code {code}"))
}

fn depth_from(raw: u32) -> Result<u8, String> {
    match raw + 1 {
        depth @ 1..=24 => Ok(depth as u8),
        depth => Err(format!("floor {depth} is outside the dungeon")),
    }
}

#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    used: u32,
}

impl BitWriter {
    /// Appends the low `width` bits of `value`, most significant first.
    fn push(&mut self, value: u32, width: u32) {
        debug_assert!(width <= 32 && (width == 32 || value < (1 << width)));
        for offset in (0..width).rev() {
            if self.used % 8 == 0 {
                self.bytes.push(0);
            }
            let bit = (value >> offset) & 1;
            let slot = self.bytes.last_mut().expect("pushed above");
            *slot |= (bit as u8) << (7 - self.used % 8);
            self.used += 1;
        }
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

struct BitReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> BitReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    /// Reads `width` bits, most significant first.
    fn pull(&mut self, width: u32) -> Result<u32, String> {
        let mut value = 0;
        for _ in 0..width {
            let byte = self
                .bytes
                .get(self.cursor / 8)
                .ok_or_else(|| "the code is truncated".to_owned())?;
            value = (value << 1) | u32::from((byte >> (7 - self.cursor % 8)) & 1);
            self.cursor += 1;
        }
        Ok(value)
    }

    /// Requires every remaining bit to be final-byte zero padding.
    fn expect_exhausted(&mut self) -> Result<(), String> {
        let remaining = self.bytes.len() * 8 - self.cursor;
        if remaining >= 8 || self.pull(remaining as u32)? != 0 {
            return Err("the code has trailing data".to_owned());
        }
        Ok(())
    }
}

fn base64url_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let word = chunk.iter().enumerate().fold(0u32, |word, (index, byte)| {
            word | u32::from(*byte) << (16 - 8 * index)
        });
        for position in 0..=chunk.len() {
            let index = (word >> (18 - 6 * position)) & 0x3f;
            output.push(char::from(BASE64URL[index as usize]));
        }
    }
    output
}

fn base64url_decode(text: &str) -> Result<Vec<u8>, String> {
    let digits = text
        .bytes()
        .map(|byte| {
            BASE64URL
                .iter()
                .position(|candidate| *candidate == byte)
                .map(|position| position as u32)
                .ok_or_else(|| {
                    "the code contains characters that are not part of a share link".to_owned()
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if digits.len() % 4 == 1 {
        return Err("the code is truncated".to_owned());
    }
    let mut bytes = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.chunks(4) {
        let word = chunk.iter().enumerate().fold(0u32, |word, (index, digit)| {
            word | digit << (18 - 6 * index)
        });
        for position in 0..chunk.len() - 1 {
            bytes.push((word >> (16 - 8 * position)) as u8);
        }
    }
    Ok(bytes)
}

/// Every item in frozen code order (indices are part of the link format).
const ALL_ITEM_IDS: &[ItemId] = &[
    ItemId::WornShortsword,
    ItemId::Cudgel,
    ItemId::StuddedGloves,
    ItemId::Rapier,
    ItemId::Dagger,
    ItemId::Shortsword,
    ItemId::HandAxe,
    ItemId::Spear,
    ItemId::Quarterstaff,
    ItemId::Dirk,
    ItemId::Sickle,
    ItemId::Sword,
    ItemId::Mace,
    ItemId::Scimitar,
    ItemId::RoundShield,
    ItemId::Sai,
    ItemId::Whip,
    ItemId::Longsword,
    ItemId::BattleAxe,
    ItemId::Flail,
    ItemId::RunicBlade,
    ItemId::AssassinsBlade,
    ItemId::Crossbow,
    ItemId::Katana,
    ItemId::Greatsword,
    ItemId::WarHammer,
    ItemId::Glaive,
    ItemId::Greataxe,
    ItemId::Greatshield,
    ItemId::StoneGauntlet,
    ItemId::WarScythe,
    ItemId::ThrowingStone,
    ItemId::ThrowingKnife,
    ItemId::ThrowingSpike,
    ItemId::FishingSpear,
    ItemId::ThrowingClub,
    ItemId::Shuriken,
    ItemId::ThrowingSpear,
    ItemId::Kunai,
    ItemId::Bolas,
    ItemId::Javelin,
    ItemId::Tomahawk,
    ItemId::HeavyBoomerang,
    ItemId::Trident,
    ItemId::ThrowingHammer,
    ItemId::ForceCube,
    ItemId::ClothArmor,
    ItemId::LeatherArmor,
    ItemId::MailArmor,
    ItemId::ScaleArmor,
    ItemId::PlateArmor,
    ItemId::WandMagicMissile,
    ItemId::WandFireblast,
    ItemId::WandFrost,
    ItemId::WandLightning,
    ItemId::WandDisintegration,
    ItemId::WandPrismaticLight,
    ItemId::WandCorrosion,
    ItemId::WandLivingEarth,
    ItemId::WandBlastWave,
    ItemId::WandCorruption,
    ItemId::WandWarding,
    ItemId::WandRegrowth,
    ItemId::WandTransfusion,
    ItemId::RotDart,
    ItemId::IncendiaryDart,
    ItemId::AdrenalineDart,
    ItemId::HealingDart,
    ItemId::ChillingDart,
    ItemId::ShockingDart,
    ItemId::PoisonDart,
    ItemId::CleansingDart,
    ItemId::ParalyticDart,
    ItemId::HolyDart,
    ItemId::DisplacingDart,
    ItemId::BlindingDart,
    ItemId::RingAccuracy,
    ItemId::RingArcana,
    ItemId::RingElements,
    ItemId::RingEnergy,
    ItemId::RingEvasion,
    ItemId::RingForce,
    ItemId::RingFuror,
    ItemId::RingHaste,
    ItemId::RingMight,
    ItemId::RingSharpshooting,
    ItemId::RingTenacity,
    ItemId::RingWealth,
];

#[cfg(test)]
mod tests {
    use crate::catalog::{
        ALL_ARMOR_EFFECTS, ALL_WEAPON_EFFECTS, Effect, ItemId, ItemKind, WeaponCategory,
        WeaponEffect, item,
    };
    use crate::challenges::Challenges;
    use crate::json_query;
    use crate::model::ItemSource;
    use crate::query::{Requirement, SearchQuery, TierRequirement, UpgradeRequirement};
    use crate::quests::WandmakerQuestType;

    use super::{
        ALL_ITEM_IDS, SOURCE_CODES, decode, decode_text, encode, encode_link, extract_code,
    };

    fn wildcard(kind: ItemKind) -> Requirement {
        Requirement {
            kind,
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

    fn minimal(requirements: Vec<Requirement>) -> SearchQuery {
        SearchQuery {
            requirements,
            max_depth: 24,
            challenges: Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        }
    }

    #[test]
    fn round_trips_a_minimal_query_in_a_handful_of_characters() {
        let query = minimal(vec![wildcard(ItemKind::Wand)]);
        let code = encode(&query).unwrap();
        assert!(code.len() <= 8, "unexpectedly long code: {code}");
        assert_eq!(decode(&code).unwrap(), query);
    }

    #[test]
    fn round_trips_a_fully_loaded_query() {
        let query = SearchQuery {
            requirements: vec![
                Requirement {
                    kind: ItemKind::Weapon,
                    weapon_category: Some(WeaponCategory::Melee),
                    item: Some(ItemId::WarScythe),
                    tier: TierRequirement::Any,
                    upgrade: UpgradeRequirement::AtLeast(2),
                    effect: Some(Effect::Weapon(WeaponEffect::Grim)),
                    require_uncursed: true,
                    source: Some(ItemSource::SacrificialFire),
                    identity_group: Some(4),
                    max_depth: Some(21),
                },
                Requirement {
                    kind: ItemKind::Armor,
                    weapon_category: None,
                    item: None,
                    tier: TierRequirement::AtLeast(4),
                    upgrade: UpgradeRequirement::Exact(3),
                    effect: None,
                    require_uncursed: false,
                    source: None,
                    identity_group: None,
                    max_depth: None,
                },
                Requirement {
                    item: Some(ItemId::RingWealth),
                    upgrade: UpgradeRequirement::Exact(4),
                    ..wildcard(ItemKind::Ring)
                },
            ],
            max_depth: 19,
            challenges: Challenges::NO_FOOD | Challenges::STRONGER_BOSSES,
            require_blacksmith: true,
            exclude_blacksmith_rewards: true,
            wandmaker_quest: Some(WandmakerQuestType::Rotberry),
            fast_mode: true,
        };
        let code = encode(&query).unwrap();
        assert_eq!(decode(&code).unwrap(), query);
    }

    #[test]
    fn round_trips_every_wandmaker_filter() {
        for variant in WandmakerQuestType::ALL {
            let query = SearchQuery {
                wandmaker_quest: Some(variant),
                ..minimal(vec![wildcard(ItemKind::Wand)])
            };
            assert_eq!(decode(&encode(&query).unwrap()).unwrap(), query);
        }
        // An unfiltered query spends one bit and decodes back to "any".
        let any = minimal(vec![wildcard(ItemKind::Wand)]);
        assert_eq!(
            decode(&encode(&any).unwrap()).unwrap().wandmaker_quest,
            None
        );
    }

    #[test]
    fn round_trips_every_item_effect_source_and_challenge() {
        for definition in ALL_ITEM_IDS.iter().map(|id| item(*id)) {
            let query = minimal(vec![Requirement {
                item: Some(definition.id),
                ..wildcard(definition.kind)
            }]);
            assert_eq!(decode(&encode(&query).unwrap()).unwrap(), query);
        }
        for (kind, effects) in [
            (ItemKind::Weapon, ALL_WEAPON_EFFECTS.len()),
            (ItemKind::Armor, ALL_ARMOR_EFFECTS.len()),
        ] {
            for code in 0..effects {
                let query = minimal(vec![Requirement {
                    effect: Some(super::effect_from(kind, code as u32).unwrap()),
                    ..wildcard(kind)
                }]);
                assert_eq!(decode(&encode(&query).unwrap()).unwrap(), query);
            }
        }
        for source in SOURCE_CODES {
            let query = minimal(vec![Requirement {
                source: Some(*source),
                ..wildcard(ItemKind::Wand)
            }]);
            assert_eq!(decode(&encode(&query).unwrap()).unwrap(), query);
        }
        for bit in 0..9 {
            let query = SearchQuery {
                challenges: Challenges::new(1 << bit).unwrap(),
                ..minimal(vec![wildcard(ItemKind::Ring)])
            };
            assert_eq!(decode(&encode(&query).unwrap()).unwrap(), query);
        }
    }

    #[test]
    fn matches_the_json_document_round_trip() {
        let document = r#"{
            "max_depth": 12,
            "require_blacksmith": true,
            "challenges": ["barren_land", "badder_bosses"],
            "requirements": [
                {"item": "ring_tenacity", "upgrade": 4, "source": "imp_reward"},
                {"kind": "melee_weapon", "tier": {"exact": 5}, "effect": "Blazing"},
                {"kind": "wand", "upgrade": {"at_least": 2}, "identity_group": 3,
                 "uncursed": true, "max_depth": 9}
            ]
        }"#;
        let query = json_query::decode(document).unwrap();
        let decoded = decode(&encode(&query).unwrap()).unwrap();
        assert_eq!(decoded, query);
        assert_eq!(json_query::encode(&decoded), json_query::encode(&query));
    }

    #[test]
    fn refuses_to_encode_an_invalid_query() {
        assert!(encode(&minimal(Vec::new())).is_err());
        let mismatched = minimal(vec![Requirement {
            item: Some(ItemId::Sword),
            ..wildcard(ItemKind::Ring)
        }]);
        assert!(encode(&mismatched).is_err());
    }

    /// Same-item groups are capped at what every editor can express (A..D),
    /// matching the results-file format, on both ends of the codec.
    #[test]
    fn same_item_groups_above_four_are_rejected() {
        let query = minimal(vec![Requirement {
            identity_group: Some(5),
            ..wildcard(ItemKind::Wand)
        }]);
        assert!(encode(&query).unwrap_err().contains("A..D"));

        // A hand-crafted stream carrying group 200: the wire field is eight
        // bits, so out-of-range groups must die in the decoder.
        let mut bits = super::BitWriter::default();
        bits.push(1, 4); // version
        bits.push(0, 3); // flags
        bits.push(0, 1); // max depth absent
        bits.push(0, 1); // challenges absent
        bits.push(0, 1); // Wandmaker filter absent
        bits.push(1, 6); // one requirement
        bits.push(4, 3); // wand
        bits.push(0, 1); // item absent
        bits.push(0, 2); // tier any
        bits.push(0, 2); // upgrade any
        bits.push(0, 1); // effect absent
        bits.push(0, 1); // cursed allowed
        bits.push(0, 1); // source absent
        bits.push(1, 1); // identity group present
        bits.push(200, 8);
        bits.push(0, 1); // requirement depth absent
        let code = super::base64url_encode(&bits.finish());
        assert!(decode(&code).unwrap_err().contains("A..D"));
    }

    #[test]
    fn rejects_malformed_codes() {
        assert!(decode("").is_err());
        assert!(decode("!!!").is_err());
        assert!(decode("A").is_err());
        // Unsupported future version (bits 0100 in the top nibble).
        assert!(decode("QAAA").unwrap_err().contains("version 4"));
        let code = encode(&minimal(vec![wildcard(ItemKind::Wand)])).unwrap();
        assert!(decode(&code[..code.len() - 1]).is_err());
        assert!(decode(&format!("{code}AAAA")).is_err());
    }

    /// Six of the eight category codes are spoken for, so a corrupted or
    /// truncated code can land on 6 or 7. Every rejection has to name what it
    /// rejected — a blank reason reaches the user as a bare "requirement 1: ".
    #[test]
    fn unknown_category_codes_are_named_in_the_error() {
        for code in 6..=7 {
            let mut bits = super::BitWriter::default();
            bits.push(1, 4); // version
            bits.push(0, 3); // flags
            bits.push(0, 1); // max depth absent
            bits.push(0, 1); // challenges absent
            bits.push(0, 1); // Wandmaker filter absent
            bits.push(1, 6); // one requirement
            bits.push(code, 3);
            let error = decode(&super::base64url_encode(&bits.finish())).unwrap_err();
            assert!(
                error.contains(&format!("category code {code}")),
                "unhelpful message: {error:?}"
            );
        }
    }

    #[test]
    fn extracts_codes_from_every_supported_link_form() {
        let query = minimal(vec![wildcard(ItemKind::Wand)]);
        let link = encode_link(&query).unwrap();
        let code = encode(&query).unwrap();
        assert_eq!(extract_code(&link), Some(code.as_str()));
        assert_eq!(decode_text(&link).unwrap(), query);
        assert_eq!(decode_text(&format!("  {link}  ")).unwrap(), query);
        assert_eq!(
            extract_code(&format!("https://example.com/?utm=1&q={code}#top")),
            Some(code.as_str())
        );
        assert_eq!(
            extract_code(&format!("seedseeker://q/{code}")),
            Some(code.as_str())
        );
        assert_eq!(extract_code(&code), Some(code.as_str()));
        assert_eq!(extract_code(""), None);
        assert_eq!(extract_code("https://example.com/"), None);
        assert!(decode_text("https://example.com/").is_err());
    }

    /// The code tables are part of the persisted link format: entries may be
    /// appended, but existing positions must never change. If this test fails,
    /// restore the order here and map any new catalog entries to fresh codes
    /// at the end of the table.
    #[test]
    #[allow(clippy::too_many_lines)]
    fn code_tables_are_frozen() {
        let expected_items = [
            "worn_shortsword",
            "cudgel",
            "gloves",
            "rapier",
            "dagger",
            "shortsword",
            "hand_axe",
            "spear",
            "quarterstaff",
            "dirk",
            "sickle",
            "sword",
            "mace",
            "scimitar",
            "round_shield",
            "sai",
            "whip",
            "longsword",
            "battle_axe",
            "flail",
            "runic_blade",
            "assassins_blade",
            "crossbow",
            "katana",
            "greatsword",
            "war_hammer",
            "glaive",
            "greataxe",
            "greatshield",
            "gauntlet",
            "war_scythe",
            "throwing_stone",
            "throwing_knife",
            "throwing_spike",
            "fishing_spear",
            "throwing_club",
            "shuriken",
            "throwing_spear",
            "kunai",
            "bolas",
            "javelin",
            "tomahawk",
            "heavy_boomerang",
            "trident",
            "throwing_hammer",
            "force_cube",
            "cloth_armor",
            "leather_armor",
            "mail_armor",
            "scale_armor",
            "plate_armor",
            "wand_magic_missile",
            "wand_fireblast",
            "wand_frost",
            "wand_lightning",
            "wand_disintegration",
            "wand_prismatic_light",
            "wand_corrosion",
            "wand_living_earth",
            "wand_blast_wave",
            "wand_corruption",
            "wand_warding",
            "wand_regrowth",
            "wand_transfusion",
            "rot_dart",
            "incendiary_dart",
            "adrenaline_dart",
            "healing_dart",
            "chilling_dart",
            "shocking_dart",
            "poison_dart",
            "cleansing_dart",
            "paralytic_dart",
            "holy_dart",
            "displacing_dart",
            "blinding_dart",
            "ring_accuracy",
            "ring_arcana",
            "ring_elements",
            "ring_energy",
            "ring_evasion",
            "ring_force",
            "ring_furor",
            "ring_haste",
            "ring_might",
            "ring_sharpshooting",
            "ring_tenacity",
            "ring_wealth",
        ];
        assert_eq!(
            ALL_ITEM_IDS
                .iter()
                .map(|id| item(*id).stable_id)
                .collect::<Vec<_>>(),
            expected_items
        );
        // Every catalog item must be representable in a link.
        assert_eq!(ALL_ITEM_IDS.len(), crate::catalog::ITEMS.len());

        let expected_weapon_effects = [
            "Blazing",
            "Chilling",
            "Kinetic",
            "Shocking",
            "Blocking",
            "Blooming",
            "Elastic",
            "Lucky",
            "Projecting",
            "Unstable",
            "Corrupting",
            "Grim",
            "Vampiric",
            "Annoying",
            "Displacing",
            "Dazzling",
            "Explosive",
            "Sacrificial",
            "Wayward",
            "Polarized",
            "Friendly",
        ];
        let expected_armor_effects = [
            "Obfuscation",
            "Swiftness",
            "Viscosity",
            "Potential",
            "Brimstone",
            "Stone",
            "Entanglement",
            "Repulsion",
            "Camouflage",
            "Flow",
            "Affection",
            "Anti-Magic",
            "Thorns",
            "Anti-Entropy",
            "Corrosion",
            "Displacement",
            "Metabolism",
            "Multiplicity",
            "Stench",
            "Overgrowth",
            "Bulk",
        ];
        assert_eq!(
            ALL_WEAPON_EFFECTS
                .iter()
                .map(|effect| effect.wire_name())
                .collect::<Vec<_>>(),
            expected_weapon_effects
        );
        assert_eq!(
            ALL_ARMOR_EFFECTS
                .iter()
                .map(|effect| effect.wire_name())
                .collect::<Vec<_>>(),
            expected_armor_effects
        );

        let expected_sources = [
            "heap",
            "chest",
            "locked_chest",
            "crystal_chest",
            "tomb",
            "skeleton",
            "sacrificial_fire",
            "mimic",
            "golden_mimic",
            "crystal_mimic",
            "statue",
            "armored_statue",
            "shop",
            "ghost_reward",
            "wandmaker_reward",
            "blacksmith_reward",
            "imp_reward",
        ];
        assert_eq!(
            SOURCE_CODES
                .iter()
                .map(|source| json_query::source_name(*source))
                .collect::<Vec<_>>(),
            expected_sources
        );

        // Challenge bits are the upstream mask bits, pinned in mask order.
        let expected_challenges: Vec<u16> = (0..9).map(|bit| 1 << bit).collect();
        assert_eq!(
            json_query::CHALLENGE_NAMES
                .iter()
                .map(|(_, challenge)| challenge.bits())
                .collect::<Vec<_>>(),
            expected_challenges
        );
    }

    /// A known code must decode identically forever; this pins the byte-level
    /// format of version 1.
    #[test]
    fn version_one_codes_are_stable() {
        let query = SearchQuery {
            requirements: vec![Requirement {
                item: Some(ItemId::WandFireblast),
                upgrade: UpgradeRequirement::AtLeast(3),
                ..wildcard(ItemKind::Wand)
            }],
            max_depth: 24,
            challenges: Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        };
        let code = encode(&query).unwrap();
        assert_eq!(code, "EAGWhMA");
        assert_eq!(decode("EAGWhMA").unwrap(), query);
    }
}
