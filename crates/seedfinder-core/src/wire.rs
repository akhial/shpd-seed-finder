//! Dependency-free binary protocol shared with the Android JNI adapter.
//!
//! Queries no longer have a binary form: every query-taking bridge call takes
//! the canonical JSON query document of [`crate::json_query`], which
//! frontends already build for share links and results files. What stays
//! binary is what the bridges hand back or take as a seed: `SSR1` result
//! batches, `SSQ2` scouting requests and `SSC3` scouted worlds.

use std::fmt;

use crate::catalog::{Effect, item, item_by_stable_id};
use crate::challenges::Challenges;
use crate::model::{Accessibility, GeneratedWorld, ItemSource, WorldItem};
#[cfg(feature = "json-query")]
use crate::query::SearchQuery;
use crate::quests::{
    BlacksmithQuestType, GhostQuestType, ImpQuestType, QuestSummary, ScheduledQuest,
    WandmakerQuestType,
};
use crate::run::RingGems;
use crate::seed::DungeonSeed;

const SCOUT_REQUEST_MAGIC_V2: &[u8; 4] = b"SSQ2";
/// Ring classes, and so gem-table bytes, in one `SSC3` packet.
const RING_GEM_COUNT: usize = 12;
const RESULT_MAGIC: &[u8; 4] = b"SSR1";
const SCOUT_RESULT_MAGIC: &[u8; 4] = b"SSC3";
/// Requirement ceiling of a bridge request; far above anything the UIs
/// produce, and what the retired binary layout's count field could hold.
#[cfg(feature = "json-query")]
const MAX_REQUIREMENTS: usize = 64;

/// Decodes a search request: the canonical JSON query document of
/// [`crate::json_query`], which frontends already build for share links and
/// results files and can therefore hand to every query-taking bridge call
/// without a second encoder. A UTF-8 byte-order mark and leading whitespace,
/// which some platform JSON writers emit, are tolerated.
///
/// # Errors
///
/// Returns [`WireError::InvalidUtf8`] for a request that is not UTF-8,
/// [`WireError::InvalidRequirementCount`] for more than 64 requirements, and
/// otherwise surfaces the query codec's own message as
/// [`WireError::InvalidQueryDocument`] — for malformed JSON, unknown fields,
/// items or effects, and inconsistent queries alike — so bridges can show the
/// user which requirement or field is wrong.
#[cfg(feature = "json-query")]
pub fn decode_query(packet: &[u8]) -> Result<SearchQuery, WireError> {
    let document = packet.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(packet);
    let document = document.trim_ascii_start();
    let contents = std::str::from_utf8(document).map_err(|_| WireError::InvalidUtf8)?;
    let query = crate::json_query::decode(contents).map_err(WireError::InvalidQueryDocument)?;
    if query.requirements.len() > MAX_REQUIREMENTS {
        return Err(WireError::InvalidRequirementCount);
    }
    Ok(query)
}
/// Encodes the seed-only `SSR1` result batch consumed by Android.
///
/// # Errors
///
/// Returns [`WireError::TooManyResults`] when the batch cannot fit its `u16`
/// count field.
pub fn encode_results(worlds: &[GeneratedWorld]) -> Result<Vec<u8>, WireError> {
    let count = u16::try_from(worlds.len()).map_err(|_| WireError::TooManyResults)?;
    let mut output = Vec::with_capacity(6 + worlds.len() * 12);
    output.extend_from_slice(RESULT_MAGIC);
    output.extend_from_slice(&count.to_be_bytes());
    for world in worlds {
        let code = world.seed.to_code();
        output.push(u8::try_from(code.len()).unwrap_or_default());
        output.extend_from_slice(code.as_bytes());
    }
    Ok(output)
}

/// Empty but valid poll response.
#[must_use]
pub fn empty_results() -> Vec<u8> {
    let mut output = Vec::with_capacity(6);
    output.extend_from_slice(RESULT_MAGIC);
    output.extend_from_slice(&0_u16.to_be_bytes());
    output
}

/// Decodes a challenge-aware scouting request.
///
/// `SSQ2` requests contain the magic, a little-endian `u16` challenge mask,
/// and the UTF-8 seed code in all remaining bytes. Any request without the
/// `SSQ2` prefix is a legacy raw UTF-8 seed code with no challenges.
///
/// # Errors
///
/// Returns [`WireError`] when the V2 mask is truncated or invalid, or when the
/// remaining request is not one user-enterable dungeon seed.
pub fn decode_scout_request(request: &[u8]) -> Result<(DungeonSeed, Challenges), WireError> {
    let (seed_code, challenges) =
        if let Some(payload) = request.strip_prefix(SCOUT_REQUEST_MAGIC_V2) {
            let mask = payload
                .get(..2)
                .ok_or(WireError::Truncated)?
                .try_into()
                .map(u16::from_le_bytes)
                .map_err(|_| WireError::Truncated)?;
            let challenges = Challenges::new(mask).map_err(|_| WireError::InvalidChallenges)?;
            (&payload[2..], challenges)
        } else {
            (request, Challenges::NONE)
        };
    let code = std::str::from_utf8(seed_code).map_err(|_| WireError::InvalidUtf8)?;
    let seed = DungeonSeed::from_code(code).map_err(|_| WireError::InvalidSeedCode)?;
    Ok((seed, challenges))
}

/// Decodes the seed from either an `SSQ2` or legacy scouting request.
///
/// # Errors
///
/// Returns the same validation errors as [`decode_scout_request`].
pub fn decode_scout_seed(request: &[u8]) -> Result<DungeonSeed, WireError> {
    decode_scout_request(request).map(|(seed, _)| seed)
}

/// Encodes every searchable item in one generated world for scouting mode.
///
/// `SSC3` is big-endian and self-delimiting:
///
/// ```text
/// magic[4], seed:utf8_u8,
/// ring_gems[12],
/// quest_count:u8,
/// repeated { quest:u8, variant:u8, depth:u8 },
/// item_count:u16,
/// repeated {
///   stable_item_id:utf8_u16, depth:u8, exact_upgrade:u8,
///   flags:u8 (bit 0 = cursed, bit 1 = in a secret room), effect_wire_name:utf8_u16,
///   source:u8, accessibility_tag:u8, accessibility_payload
/// }
/// ```
///
/// `ring_gems` is the run's [`RingGems::ordinals`]: one gem ordinal per ring
/// class, in the catalog's ring order. It rides with the manifest because it is
/// a property of the same run — like the quests, and unlike anything an item
/// carries — and because a frontend that draws a ring from its catalog cell
/// alone shows the same twelve colours for every seed. `SSC3` replaces `SSC2`,
/// which had no room for it.
///
/// Quest entries are emitted in strictly ascending quest order — Ghost `1`,
/// Wandmaker `2`, Blacksmith `3`, Imp `4` — and each quest appears at most
/// once. Variants reuse the game's one-based values: Ghost fetid rat `1`,
/// gnoll trickster `2`, great crab `3`; Wandmaker corpse dust `1`, elemental
/// embers `2`, rotberry `3`; Blacksmith crystal `1`, gnoll `2`; Imp vault `1`
/// (its only 4.0.0 variant). The depth byte is the quest giver's canonical
/// floor.
///
/// Accessibility payloads are empty for independent items, `group:u16,
/// option:u8` for choices, and `group:u16, mask:u64` for explicit scenarios.
/// Item order is preserved exactly from [`GeneratedWorld::items`].
///
/// # Errors
///
/// Returns an error if the item count or a UTF-8 field exceeds its declared
/// protocol width, or a quest depth leaves its canonical floor range. Catalog
/// fields in the pinned game version always fit.
pub fn encode_scout_world(world: &GeneratedWorld) -> Result<Vec<u8>, WireError> {
    let count = u16::try_from(world.items.len()).map_err(|_| WireError::TooManyWorldItems)?;
    let seed = world.seed.to_code();
    let seed_length = u8::try_from(seed.len()).map_err(|_| WireError::FieldTooLong)?;

    let mut output = Vec::with_capacity(7 + seed.len() + world.items.len() * 32);
    output.extend_from_slice(SCOUT_RESULT_MAGIC);
    output.push(seed_length);
    output.extend_from_slice(seed.as_bytes());
    output.extend_from_slice(&world.ring_gems.ordinals());
    encode_quest_summary(world.quests, &mut output)?;
    output.extend_from_slice(&count.to_be_bytes());

    for world_item in &world.items {
        let definition = item(world_item.item);
        if !(1..=24).contains(&world_item.depth) {
            return Err(WireError::InvalidItemDepth);
        }
        if world_item.upgrade > definition.kind.maximum_search_upgrade() {
            return Err(WireError::InvalidItemUpgrade);
        }
        push_utf8_u16(&mut output, definition.stable_id)?;
        output.push(world_item.depth);
        output.push(world_item.upgrade);
        output.push(u8::from(world_item.cursed) | (u8::from(world_item.secret) << 1));
        push_utf8_u16(&mut output, world_item.effect.map_or("", Effect::wire_name))?;
        output.push(source_wire_id(world_item.source));
        match world_item.accessibility {
            Accessibility::Independent => output.push(0),
            Accessibility::Choice { group, option } => {
                if option >= 64 {
                    return Err(WireError::InvalidAccessibility);
                }
                output.push(1);
                output.extend_from_slice(&group.to_be_bytes());
                output.push(option);
            }
            Accessibility::Scenarios { group, mask } => {
                if mask == 0 {
                    return Err(WireError::InvalidAccessibility);
                }
                output.push(2);
                output.extend_from_slice(&group.to_be_bytes());
                output.extend_from_slice(&mask.to_be_bytes());
            }
        }
    }
    Ok(output)
}

fn encode_quest_summary(quests: QuestSummary, output: &mut Vec<u8>) -> Result<(), WireError> {
    let entries = [
        quests
            .ghost
            .map(|quest| (GHOST_QUEST_WIRE_ID, quest.variant as u8, quest.depth)),
        quests
            .wandmaker
            .map(|quest| (WANDMAKER_QUEST_WIRE_ID, quest.variant as u8, quest.depth)),
        quests
            .blacksmith
            .map(|quest| (BLACKSMITH_QUEST_WIRE_ID, quest.variant as u8, quest.depth)),
        quests.imp.map(|quest| {
            (
                IMP_QUEST_WIRE_ID,
                imp_target_wire_id(quest.variant),
                quest.depth,
            )
        }),
    ];
    let scheduled = entries.iter().flatten();
    output.push(u8::try_from(scheduled.clone().count()).expect("at most four quests"));
    for &(quest, variant, depth) in scheduled {
        if !quest_depth_range(quest).contains(&depth) {
            return Err(WireError::InvalidQuestDepth);
        }
        output.extend_from_slice(&[quest, variant, depth]);
    }
    Ok(())
}

fn decode_quest_summary(input: &mut Input<'_>) -> Result<QuestSummary, WireError> {
    let mut quests = QuestSummary::default();
    let count = input.u8()?;
    if count > 4 {
        return Err(WireError::InvalidQuestCount);
    }
    let mut previous_quest = 0;
    for _ in 0..count {
        let quest = input.u8()?;
        let variant = input.u8()?;
        let depth = input.u8()?;
        // Strictly ascending quest IDs keep the encoding canonical and rule
        // out duplicates.
        if quest <= previous_quest {
            return Err(WireError::InvalidQuestOrder);
        }
        previous_quest = quest;
        if !quest_depth_range(quest).contains(&depth) {
            return Err(WireError::InvalidQuestDepth);
        }
        match quest {
            GHOST_QUEST_WIRE_ID => {
                let variant = match variant {
                    1 => GhostQuestType::FetidRat,
                    2 => GhostQuestType::GnollTrickster,
                    3 => GhostQuestType::GreatCrab,
                    _ => return Err(WireError::UnknownQuestVariant),
                };
                quests.ghost = Some(ScheduledQuest { variant, depth });
            }
            WANDMAKER_QUEST_WIRE_ID => {
                let variant = WandmakerQuestType::from_wire_id(variant)
                    .ok_or(WireError::UnknownQuestVariant)?;
                quests.wandmaker = Some(ScheduledQuest { variant, depth });
            }
            BLACKSMITH_QUEST_WIRE_ID => {
                let variant = match variant {
                    1 => BlacksmithQuestType::Crystal,
                    2 => BlacksmithQuestType::Gnoll,
                    _ => return Err(WireError::UnknownQuestVariant),
                };
                quests.blacksmith = Some(ScheduledQuest { variant, depth });
            }
            IMP_QUEST_WIRE_ID => {
                let variant = match variant {
                    1 => ImpQuestType::Vault,
                    _ => return Err(WireError::UnknownQuestVariant),
                };
                quests.imp = Some(ScheduledQuest { variant, depth });
            }
            _ => return Err(WireError::UnknownQuest),
        }
    }
    Ok(quests)
}

const GHOST_QUEST_WIRE_ID: u8 = 1;
const WANDMAKER_QUEST_WIRE_ID: u8 = 2;
const BLACKSMITH_QUEST_WIRE_ID: u8 = 3;
const IMP_QUEST_WIRE_ID: u8 = 4;

const fn imp_target_wire_id(variant: ImpQuestType) -> u8 {
    match variant {
        ImpQuestType::Vault => 1,
    }
}

/// Canonical floors that can host each quest giver.
const fn quest_depth_range(quest: u8) -> std::ops::RangeInclusive<u8> {
    match quest {
        GHOST_QUEST_WIRE_ID => 2..=4,
        WANDMAKER_QUEST_WIRE_ID => 7..=9,
        BLACKSMITH_QUEST_WIRE_ID => 12..=14,
        IMP_QUEST_WIRE_ID => 17..=19,
        // Unknown quests are rejected before their depth is range-checked.
        _ => 1..=24,
    }
}

/// Decodes an `SSC3` scouting response.
///
/// This is primarily the executable protocol specification and makes native
/// round-trip tests cover every source/accessibility branch. Android uses the
/// same field layout directly to attach catalog display metadata.
///
/// # Errors
///
/// Returns [`WireError`] for malformed lengths, identifiers, flags, enum
/// values, accessibility constraints, quest entries, or trailing bytes.
pub fn decode_scout_world(packet: &[u8]) -> Result<GeneratedWorld, WireError> {
    let mut input = Input::new(packet);
    if input.take(4)? != SCOUT_RESULT_MAGIC {
        return Err(WireError::BadMagic);
    }
    let seed = DungeonSeed::from_code(input.utf8_u8()?).map_err(|_| WireError::InvalidSeedCode)?;
    let ring_gems = input
        .take(RING_GEM_COUNT)?
        .try_into()
        .ok()
        .and_then(RingGems::from_ordinals)
        .ok_or(WireError::InvalidRingGems)?;
    let quests = decode_quest_summary(&mut input)?;
    let count = usize::from(input.u16()?);
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        let definition = item_by_stable_id(input.utf8_u16()?).ok_or(WireError::UnknownItem)?;
        let depth = input.u8()?;
        if !(1..=24).contains(&depth) {
            return Err(WireError::InvalidItemDepth);
        }
        let upgrade = input.u8()?;
        if upgrade > definition.kind.maximum_search_upgrade() {
            return Err(WireError::InvalidItemUpgrade);
        }
        let flags = input.u8()?;
        if flags & !0b11 != 0 {
            return Err(WireError::InvalidFlags);
        }
        let effect_name = input.utf8_u16()?;
        let effect = if effect_name.is_empty() {
            None
        } else {
            Some(
                Effect::from_wire_name(definition.kind, effect_name)
                    .ok_or(WireError::UnknownModifier)?,
            )
        };
        let source = source_from_wire_id(input.u8()?).ok_or(WireError::UnknownItemSource)?;
        let accessibility = match input.u8()? {
            0 => Accessibility::Independent,
            1 => {
                let group = input.u16()?;
                let option = input.u8()?;
                if option >= 64 {
                    return Err(WireError::InvalidAccessibility);
                }
                Accessibility::Choice { group, option }
            }
            2 => {
                let group = input.u16()?;
                let mask = input.u64()?;
                if mask == 0 {
                    return Err(WireError::InvalidAccessibility);
                }
                Accessibility::Scenarios { group, mask }
            }
            _ => return Err(WireError::InvalidAccessibility),
        };
        items.push(WorldItem {
            item: definition.id,
            upgrade,
            effect,
            cursed: flags & 1 != 0,
            depth,
            source,
            accessibility,
            secret: flags & 0b10 != 0,
        });
    }
    if !input.is_empty() {
        return Err(WireError::TrailingData);
    }
    Ok(GeneratedWorld {
        seed,
        items,
        quests,
        ring_gems,
    })
}

fn push_utf8_u16(output: &mut Vec<u8>, value: &str) -> Result<(), WireError> {
    let length = u16::try_from(value.len()).map_err(|_| WireError::FieldTooLong)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

const fn source_wire_id(source: ItemSource) -> u8 {
    match source {
        ItemSource::Heap => 0,
        ItemSource::Chest => 1,
        ItemSource::LockedChest => 2,
        ItemSource::CrystalChest => 3,
        ItemSource::Tomb => 4,
        ItemSource::Skeleton => 5,
        ItemSource::SacrificialFire => 6,
        ItemSource::Mimic => 7,
        ItemSource::GoldenMimic => 8,
        ItemSource::CrystalMimic => 9,
        ItemSource::Statue => 10,
        ItemSource::ArmoredStatue => 11,
        ItemSource::Shop => 12,
        ItemSource::GhostReward => 13,
        ItemSource::WandmakerReward => 14,
        ItemSource::BlacksmithReward => 15,
        ItemSource::ImpReward => 16,
        ItemSource::VaultTreasure => 17,
    }
}

const fn source_from_wire_id(id: u8) -> Option<ItemSource> {
    Some(match id {
        0 => ItemSource::Heap,
        1 => ItemSource::Chest,
        2 => ItemSource::LockedChest,
        3 => ItemSource::CrystalChest,
        4 => ItemSource::Tomb,
        5 => ItemSource::Skeleton,
        6 => ItemSource::SacrificialFire,
        7 => ItemSource::Mimic,
        8 => ItemSource::GoldenMimic,
        9 => ItemSource::CrystalMimic,
        10 => ItemSource::Statue,
        11 => ItemSource::ArmoredStatue,
        12 => ItemSource::Shop,
        13 => ItemSource::GhostReward,
        14 => ItemSource::WandmakerReward,
        15 => ItemSource::BlacksmithReward,
        16 => ItemSource::ImpReward,
        17 => ItemSource::VaultTreasure,
        _ => return None,
    })
}

struct Input<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Input<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], WireError> {
        let end = self.offset.checked_add(count).ok_or(WireError::Truncated)?;
        let result = self
            .bytes
            .get(self.offset..end)
            .ok_or(WireError::Truncated)?;
        self.offset = end;
        Ok(result)
    }

    fn u8(&mut self) -> Result<u8, WireError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, WireError> {
        let bytes: [u8; 2] = self.take(2)?.try_into().map_err(|_| WireError::Truncated)?;
        Ok(u16::from_be_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, WireError> {
        let bytes: [u8; 8] = self.take(8)?.try_into().map_err(|_| WireError::Truncated)?;
        Ok(u64::from_be_bytes(bytes))
    }

    fn utf8_u8(&mut self) -> Result<&'a str, WireError> {
        let length = usize::from(self.u8()?);
        std::str::from_utf8(self.take(length)?).map_err(|_| WireError::InvalidUtf8)
    }

    fn utf8_u16(&mut self) -> Result<&'a str, WireError> {
        let length = usize::from(self.u16()?);
        std::str::from_utf8(self.take(length)?).map_err(|_| WireError::InvalidUtf8)
    }

    fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

/// Android/native packet validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WireError {
    BadMagic,
    Truncated,
    InvalidUtf8,
    InvalidSeedCode,
    /// A query request with more requirements than a bridge accepts.
    InvalidRequirementCount,
    UnknownItem,
    UnknownModifier,
    TrailingData,
    TooManyResults,
    TooManyWorldItems,
    FieldTooLong,
    InvalidFlags,
    InvalidChallenges,
    InvalidItemDepth,
    InvalidItemUpgrade,
    UnknownItemSource,
    InvalidAccessibility,
    InvalidQuestCount,
    InvalidRingGems,
    InvalidQuestOrder,
    InvalidQuestDepth,
    UnknownQuest,
    UnknownQuestVariant,
    /// A JSON query document the query codec rejected, with its message.
    InvalidQueryDocument(String),
}

impl fmt::Display for WireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::BadMagic => "unexpected packet magic or schema version",
            Self::Truncated => "packet ended before a declared field",
            Self::InvalidUtf8 => "packet contains invalid UTF-8",
            Self::InvalidSeedCode => "seed code must contain nine A-Z characters",
            Self::InvalidRequirementCount => "a query may carry at most 64 requirements",
            Self::UnknownItem => "packet names an unknown item ID",
            Self::UnknownModifier => "packet names an unknown enchantment or glyph",
            Self::TrailingData => "packet has trailing bytes",
            Self::TooManyResults => "result batch exceeds the protocol limit",
            Self::TooManyWorldItems => "scouted world exceeds the protocol item limit",
            Self::FieldTooLong => "packet string exceeds its declared field width",
            Self::InvalidFlags => "packet contains unknown flag bits",
            Self::InvalidChallenges => "packet challenge mask must be in 0..=511",
            Self::InvalidItemDepth => "scouted item depth must be in 1..=24",
            Self::InvalidItemUpgrade => {
                "scouted item upgrade exceeds its kind's ceiling (+5 weapons, +4 otherwise)"
            }
            Self::UnknownItemSource => "packet names an unknown item source",
            Self::InvalidAccessibility => "packet contains an invalid accessibility constraint",
            Self::InvalidQuestCount => "scouted world lists more than four quests",
            Self::InvalidRingGems => "packet ring gems are not a permutation of the twelve gems",
            Self::InvalidQuestOrder => "packet quest entries must have ascending unique IDs",
            Self::InvalidQuestDepth => "packet quest depth leaves its canonical floor range",
            Self::UnknownQuest => "packet names an unknown quest",
            Self::UnknownQuestVariant => "packet names an unknown quest variant",
            Self::InvalidQueryDocument(message) => message,
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for WireError {}

#[cfg(test)]
mod tests {
    use crate::catalog::{ArmorEffect, Effect, ITEMS, ItemId, ItemKind, WeaponEffect, item};
    use crate::challenges::Challenges;
    use crate::json_query;
    use crate::main_world::CanonicalMainWorldGenerator;
    use crate::model::{Accessibility, GeneratedWorld, ItemSource, WorldItem};
    use crate::query::{
        EffectRequirement, EffectSet, LevelSum, Requirement, SearchQuery, TierRequirement,
        UpgradeRequirement,
    };
    use crate::quests::WandmakerQuestType;
    use crate::run::RingGems;

    /// Every `SSC3` packet opens with the magic, the length-prefixed seed
    /// code, and the run's twelve-byte gem table. The quest block starts here.
    const SCOUT_HEADER: usize = 4 + 1 + 11 + super::RING_GEM_COUNT;
    use crate::search::WorldGenerator;
    use crate::seed::DungeonSeed;

    use super::{
        WireError, decode_query, decode_scout_request, decode_scout_seed, decode_scout_world,
        empty_results, encode_results, encode_scout_world,
    };

    const SOURCES: [ItemSource; 18] = [
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

    /// The request bytes a frontend sends for a query: its canonical JSON
    /// document.
    fn request(query: &SearchQuery) -> Vec<u8> {
        json_query::encode(query).to_string().into_bytes()
    }

    /// The codec message a request was rejected with.
    fn rejection(packet: &[u8]) -> String {
        match decode_query(packet) {
            Err(WireError::InvalidQueryDocument(message)) => message,
            other => panic!("expected a document error, got {other:?}"),
        }
    }

    #[test]
    fn query_requests_round_trip_every_query_field() {
        let query = SearchQuery {
            requirements: vec![
                Requirement {
                    kind: ItemKind::Armor,
                    weapon_category: None,
                    item: None,
                    tier: TierRequirement::AtMost(4),
                    upgrade: UpgradeRequirement::AtLeast(1),
                    effect: EffectRequirement::exactly(Effect::Armor(ArmorEffect::Thorns)),
                    require_uncursed: true,
                    source: Some(ItemSource::Chest),
                    identity_group: Some(2),
                    max_depth: Some(14),
                    alternative_group: None,
                    level_sum: None,
                },
                Requirement {
                    kind: ItemKind::Weapon,
                    weapon_category: None,
                    item: None,
                    tier: TierRequirement::Exact(3),
                    upgrade: UpgradeRequirement::Any,
                    effect: EffectRequirement::Any,
                    require_uncursed: false,
                    source: None,
                    identity_group: None,
                    max_depth: None,
                    alternative_group: None,
                    level_sum: None,
                },
                Requirement {
                    kind: ItemKind::Armor,
                    weapon_category: None,
                    item: None,
                    tier: TierRequirement::AtLeast(4),
                    upgrade: UpgradeRequirement::Exact(2),
                    effect: EffectRequirement::Any,
                    require_uncursed: false,
                    source: None,
                    identity_group: None,
                    max_depth: None,
                    alternative_group: None,
                    level_sum: None,
                },
            ],
            max_depth: 20,
            challenges: Challenges::new(104).unwrap(),
            require_blacksmith: true,
            exclude_blacksmith_rewards: true,
            wandmaker_quest: Some(WandmakerQuestType::ElementalEmbers),
            fast_mode: true,
        };
        let packet = request(&query);
        assert_eq!(packet[0], b'{');
        assert_eq!(decode_query(&packet), Ok(query));
    }

    #[test]
    fn query_requests_name_melee_and_thrown_kinds() {
        use crate::catalog::WeaponCategory;

        // A plain weapon kind matches melee and thrown weapons alike.
        let plain = decode_query(br#"{"requirements":[{"kind":"weapon"}]}"#).unwrap();
        assert_eq!(plain.requirements[0].kind, ItemKind::Weapon);
        assert_eq!(plain.requirements[0].weapon_category, None);

        for (name, category) in [
            ("melee_weapon", WeaponCategory::Melee),
            ("thrown_weapon", WeaponCategory::Thrown),
        ] {
            let packet = format!(r#"{{"requirements":[{{"kind":"{name}"}}]}}"#);
            let decoded = decode_query(packet.as_bytes()).unwrap();
            assert_eq!(decoded.requirements[0].kind, ItemKind::Weapon);
            assert_eq!(decoded.requirements[0].weapon_category, Some(category));
            assert_eq!(decode_query(&request(&decoded)), Ok(decoded));
        }

        // A thrown filter accepts a matching pinned item and rejects others.
        let consistent =
            decode_query(br#"{"requirements":[{"kind":"thrown_weapon","item":"shuriken"}]}"#)
                .unwrap();
        assert_eq!(consistent.requirements[0].item, Some(ItemId::Shuriken));
        assert!(
            rejection(br#"{"requirements":[{"kind":"thrown_weapon","item":"sword"}]}"#)
                .contains("melee/thrown filters require")
        );
        assert!(rejection(br#"{"requirements":[{"kind":"potion"}]}"#).contains("potion"));
    }

    #[test]
    fn query_requests_round_trip_effect_sets_alternatives_and_level_sums() {
        let query = SearchQuery {
            requirements: vec![
                Requirement {
                    kind: ItemKind::Weapon,
                    weapon_category: None,
                    item: Some(ItemId::Greatshield),
                    tier: TierRequirement::Any,
                    upgrade: UpgradeRequirement::Exact(2),
                    effect: EffectRequirement::OneOf(
                        EffectSet::from_effects([
                            Effect::Weapon(WeaponEffect::Blocking),
                            Effect::Weapon(WeaponEffect::Projecting),
                            Effect::Weapon(WeaponEffect::Wondrous),
                        ])
                        .unwrap(),
                    ),
                    require_uncursed: false,
                    source: None,
                    identity_group: None,
                    max_depth: None,
                    alternative_group: Some(1),
                    level_sum: None,
                },
                Requirement {
                    kind: ItemKind::Armor,
                    weapon_category: None,
                    item: None,
                    tier: TierRequirement::Any,
                    upgrade: UpgradeRequirement::Any,
                    effect: EffectRequirement::OneOf(
                        EffectSet::enchantments(ItemKind::Armor).unwrap(),
                    ),
                    require_uncursed: true,
                    source: None,
                    identity_group: None,
                    max_depth: None,
                    alternative_group: Some(1),
                    level_sum: None,
                },
                Requirement {
                    kind: ItemKind::Ring,
                    weapon_category: None,
                    item: Some(ItemId::RingMight),
                    tier: TierRequirement::Any,
                    upgrade: UpgradeRequirement::Any,
                    effect: EffectRequirement::Any,
                    require_uncursed: false,
                    source: None,
                    identity_group: None,
                    max_depth: None,
                    alternative_group: None,
                    level_sum: Some(LevelSum {
                        group: 2,
                        minimum_total: 4,
                    }),
                },
                Requirement {
                    kind: ItemKind::Ring,
                    weapon_category: None,
                    item: Some(ItemId::RingMight),
                    tier: TierRequirement::Any,
                    upgrade: UpgradeRequirement::Any,
                    effect: EffectRequirement::Any,
                    require_uncursed: false,
                    source: None,
                    identity_group: None,
                    max_depth: None,
                    alternative_group: None,
                    level_sum: Some(LevelSum {
                        group: 2,
                        minimum_total: 4,
                    }),
                },
            ],
            max_depth: 24,
            challenges: Challenges::NONE,
            require_blacksmith: false,
            exclude_blacksmith_rewards: false,
            wandmaker_quest: None,
            fast_mode: false,
        };
        assert_eq!(decode_query(&request(&query)), Ok(query));
    }

    #[test]
    fn query_requests_reject_malformed_effect_lists_and_sum_groups() {
        // A one-of list must not be empty.
        assert!(
            rejection(br#"{"requirements":[{"item":"sword","effect":[]}]}"#)
                .contains("at least one entry")
        );
        // A list naming an effect of another family is unknown.
        assert!(
            rejection(br#"{"requirements":[{"item":"sword","effect":["Thorns"]}]}"#)
                .contains("Thorns")
        );
        // A combined-level total without a group is malformed.
        assert!(
            rejection(
                br#"{"requirements":[{"item":"sword","level_sum":{"group":0,"at_least":3}}]}"#
            )
            .contains("group")
        );
        // A sum group whose total is unattainable fails query validation.
        assert!(
            rejection(
                br#"{"requirements":[{"item":"ring_might","level_sum":{"group":1,"at_least":9}}]}"#
            )
            .contains("level")
        );
        // Levels combine across rings only.
        assert!(
            rejection(
                br#"{"requirements":[{"item":"sword","level_sum":{"group":1,"at_least":3}}]}"#
            )
            .contains("rings")
        );
    }

    #[test]
    fn query_requests_are_canonical_json_documents() {
        let document = br#"{"requirements":[
            {"any_of":[{"item":"spear","upgrade":3},{"item":"sword","upgrade":1}]},
            {"kind":"armor","effect":"any_enchantment"}
        ],"max_depth":12,"fast_mode":true}"#;
        let query = decode_query(document).unwrap();
        assert_eq!(query.max_depth, 12);
        assert!(query.fast_mode);
        assert_eq!(query.slot_count(), 2);
        assert_eq!(query.requirements.len(), 3);
        // The canonical re-encoding of the same query decodes identically,
        // and so does the document behind a byte-order mark or leading
        // whitespace.
        assert_eq!(decode_query(&request(&query)), Ok(query.clone()));
        let mut with_bom = b"\xEF\xBB\xBF\n  ".to_vec();
        with_bom.extend_from_slice(document);
        assert_eq!(decode_query(&with_bom), Ok(query));

        // Documents the query codec rejects — malformed JSON, unknown fields
        // or an invalid query — are all invalid requests, and so is anything
        // that is not a document at all: the retired binary form included.
        assert!(rejection(b"{").contains("invalid JSON"));
        assert!(rejection(b"").contains("invalid JSON"));
        assert!(rejection(b"SSF9\x18\0\0\0\0\0\x01").contains("invalid JSON"));
        assert!(rejection(b"bad!????????").contains("invalid JSON"));
        assert!(rejection(br#"{"requirements":[],"maximum_depth":4}"#).contains("maximum_depth"));
        assert!(rejection(br#"{"requirements":[]}"#).contains("at least one item requirement"));
        assert!(
            rejection(br#"{"requirements":[{"kind":"weapon","upgarde":2}]}"#).contains("upgarde")
        );
        let too_many = format!(
            r#"{{"requirements":[{}]}}"#,
            vec![r#"{"kind":"wand"}"#; 65].join(",")
        );
        assert_eq!(
            decode_query(too_many.as_bytes()),
            Err(WireError::InvalidRequirementCount)
        );
        assert_eq!(decode_query(b"{\xff"), Err(WireError::InvalidUtf8));
        assert_eq!(decode_query(&[0xff]), Err(WireError::InvalidUtf8));
    }

    #[test]
    fn query_requests_carry_the_uncursed_requirement_flag() {
        let cursed_allowed = decode_query(br#"{"requirements":[{"item":"sword"}]}"#).unwrap();
        assert!(!cursed_allowed.requirements[0].require_uncursed);
        let uncursed =
            decode_query(br#"{"requirements":[{"item":"sword","uncursed":true}]}"#).unwrap();
        assert!(uncursed.requirements[0].require_uncursed);
        // The flag is a boolean; serde names the expected type.
        assert!(
            rejection(br#"{"requirements":[{"item":"sword","uncursed":2}]}"#).contains("boolean")
        );
    }

    #[test]
    fn query_requests_select_one_wandmaker_variant_or_none() {
        // An absent filter is "any", which is what every request carries
        // until a user picks a quest, so it must stay the neutral value.
        let any = decode_query(br#"{"requirements":[{"item":"sword"}]}"#).unwrap();
        assert_eq!(any.wandmaker_quest, None);

        for variant in WandmakerQuestType::ALL {
            let query = SearchQuery {
                wandmaker_quest: Some(variant),
                ..any.clone()
            };
            let packet = request(&query);
            assert!(
                std::str::from_utf8(&packet)
                    .unwrap()
                    .contains(variant.document_name())
            );
            assert_eq!(
                decode_query(&packet).unwrap().wandmaker_quest,
                Some(variant)
            );
        }

        assert!(
            rejection(br#"{"requirements":[{"item":"sword"}],"wandmaker_quest":"imp"}"#)
                .contains("imp")
        );
    }

    #[test]
    fn query_requests_honor_the_per_kind_upgrade_ceilings() {
        let upgrade = |item: &str, upgrade: u8| {
            let packet = format!(r#"{{"requirements":[{{"item":"{item}","upgrade":{upgrade}}}]}}"#);
            decode_query(packet.as_bytes())
        };
        assert_eq!(
            upgrade("ring_sharpshooting", 4).unwrap().requirements[0].upgrade,
            UpgradeRequirement::Exact(4)
        );
        // Only the tier-4 weapons reach +5, melee and thrown alike.
        assert_eq!(
            upgrade("battle_axe", 5).unwrap().requirements[0].upgrade,
            UpgradeRequirement::Exact(5)
        );
        assert_eq!(
            upgrade("javelin", 5).unwrap().requirements[0].upgrade,
            UpgradeRequirement::Exact(5)
        );
        assert_eq!(
            upgrade("sword", 4).unwrap().requirements[0].upgrade,
            UpgradeRequirement::Exact(4)
        );
        assert_eq!(
            upgrade("plate_armor", 4).unwrap().requirements[0].upgrade,
            UpgradeRequirement::Exact(4)
        );
        assert!(matches!(
            upgrade("ring_sharpshooting", 5),
            Err(WireError::InvalidQueryDocument(_))
        ));
        assert!(matches!(
            upgrade("sword", 5),
            Err(WireError::InvalidQueryDocument(_))
        ));
        assert!(matches!(
            upgrade("shuriken", 5),
            Err(WireError::InvalidQueryDocument(_))
        ));
        assert!(matches!(
            upgrade("battle_axe", 6),
            Err(WireError::InvalidQueryDocument(_))
        ));
    }

    #[test]
    fn result_packet_matches_android_big_endian_codec() {
        let worlds = vec![
            GeneratedWorld {
                quests: crate::quests::QuestSummary::default(),
                seed: DungeonSeed::MIN,
                items: Vec::new(),
                ring_gems: RingGems::UNSHUFFLED,
            },
            GeneratedWorld {
                quests: crate::quests::QuestSummary::default(),
                seed: DungeonSeed::new(1).unwrap(),
                items: Vec::new(),
                ring_gems: RingGems::UNSHUFFLED,
            },
        ];
        let packet = encode_results(&worlds).unwrap();
        assert_eq!(&packet[..6], b"SSR1\0\x02");
        assert_eq!(packet[6], 11);
        assert_eq!(&packet[7..18], b"AAA-AAA-AAA");
        assert_eq!(packet[18], 11);
        assert_eq!(&packet[19..30], b"AAA-AAA-AAB");
        assert_eq!(empty_results(), b"SSR1\0\0");
    }

    #[test]
    fn scout_request_uses_game_compatible_seed_parser() {
        assert_eq!(
            decode_scout_seed(b"ABC-DEF-GHI").unwrap().to_code(),
            "ABC-DEF-GHI"
        );
        assert_eq!(
            decode_scout_seed(b"abc-def-ghi").unwrap().to_code(),
            "ABC-DEF-GHI"
        );
        assert_eq!(
            decode_scout_seed(b"AAA-AAA-AA0"),
            Err(WireError::InvalidSeedCode)
        );
        assert_eq!(decode_scout_seed(&[0xff]), Err(WireError::InvalidUtf8));
        assert_eq!(decode_scout_seed(b""), Err(WireError::InvalidSeedCode));
    }

    #[test]
    fn ssq2_golden_bytes_decode_challenges_and_legacy_fallback() {
        let request = b"SSQ2\x40\x00AAA-AAA-AAF";
        assert_eq!(
            decode_scout_request(request),
            Ok((
                DungeonSeed::from_code("AAA-AAA-AAF").unwrap(),
                Challenges::NO_SCROLLS,
            ))
        );
        assert_eq!(
            decode_scout_request(b"AAA-AAA-AAF"),
            Ok((
                DungeonSeed::from_code("AAA-AAA-AAF").unwrap(),
                Challenges::NONE,
            ))
        );

        let invalid_mask = b"SSQ2\x00\x02AAA-AAA-AAF";
        assert_eq!(
            decode_scout_request(invalid_mask),
            Err(WireError::InvalidChallenges)
        );
    }

    #[test]
    fn scout_packet_has_a_fixed_android_big_endian_fixture() {
        let world = GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::MIN,
            items: vec![WorldItem {
                item: ItemId::WandFrost,
                upgrade: 2,
                effect: None,
                cursed: true,
                depth: 7,
                source: ItemSource::CrystalChest,
                accessibility: Accessibility::Choice {
                    group: 0x1234,
                    option: 2,
                },
                secret: false,
            }],
            ring_gems: RingGems::UNSHUFFLED,
        };
        let packet = encode_scout_world(&world).unwrap();
        let mut expected = b"SSC3\x0bAAA-AAA-AAA".to_vec();
        // The run's own gem table, unshuffled here, so every ring class
        // keeps the cell the catalog gives it.
        expected.extend_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
        expected.extend_from_slice(b"\0\0\x01\0\x0awand_frost");
        expected.extend_from_slice(&[
            7, 2, 1, // depth, upgrade, cursed flag
            0, 0, // no effect
            3, // crystal chest
            1, 0x12, 0x34, 2, // choice, group, option
        ]);
        assert_eq!(packet, expected);
        assert_eq!(decode_scout_world(&packet), Ok(world));
    }

    #[test]
    fn scout_packet_quest_block_has_a_fixed_big_endian_fixture() {
        use crate::quests::{
            BlacksmithQuestType, GhostQuestType, ImpQuestType, QuestSummary, ScheduledQuest,
            WandmakerQuestType,
        };

        let world = GeneratedWorld {
            quests: QuestSummary {
                ghost: Some(ScheduledQuest {
                    variant: GhostQuestType::GreatCrab,
                    depth: 4,
                }),
                wandmaker: Some(ScheduledQuest {
                    variant: WandmakerQuestType::Rotberry,
                    depth: 8,
                }),
                blacksmith: Some(ScheduledQuest {
                    variant: BlacksmithQuestType::Crystal,
                    depth: 13,
                }),
                imp: Some(ScheduledQuest {
                    variant: ImpQuestType::Vault,
                    depth: 18,
                }),
            },
            seed: DungeonSeed::MIN,
            items: Vec::new(),
            ring_gems: RingGems::UNSHUFFLED,
        };
        let packet = encode_scout_world(&world).unwrap();
        let mut expected = b"SSC3\x0bAAA-AAA-AAA".to_vec();
        // The run's own gem table, unshuffled here, so every ring class
        // keeps the cell the catalog gives it.
        expected.extend_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
        expected.extend_from_slice(&[
            4, // quest count
            1, 3, 4, // ghost: great crab on floor 4
            2, 3, 8, // wandmaker: rotberry on floor 8
            3, 1, 13, // blacksmith: crystal on floor 13
            4, 1, 18, // imp: vault on floor 18
            0, 0, // item count
        ]);
        assert_eq!(packet, expected);
        assert_eq!(decode_scout_world(&packet), Ok(world));
    }

    #[test]
    fn scout_decoder_rejects_malformed_quest_blocks() {
        use crate::quests::{QuestSummary, ScheduledQuest, WandmakerQuestType};

        let world = GeneratedWorld {
            quests: QuestSummary {
                wandmaker: Some(ScheduledQuest {
                    variant: WandmakerQuestType::CorpseDust,
                    depth: 7,
                }),
                ..QuestSummary::default()
            },
            seed: DungeonSeed::MIN,
            items: Vec::new(),
            ring_gems: RingGems::UNSHUFFLED,
        };
        let packet = encode_scout_world(&world).unwrap();
        // The quest count and its one entry (quest, variant, depth), then
        // the empty item count.
        assert_eq!(&packet[SCOUT_HEADER..], &[1, 2, 1, 7, 0, 0]);

        let mut bad_count = packet.clone();
        bad_count[SCOUT_HEADER] = 5;
        assert_eq!(
            decode_scout_world(&bad_count),
            Err(WireError::InvalidQuestCount)
        );

        let mut bad_quest = packet.clone();
        bad_quest[SCOUT_HEADER + 1] = 9;
        assert_eq!(decode_scout_world(&bad_quest), Err(WireError::UnknownQuest));

        let mut bad_variant = packet.clone();
        bad_variant[SCOUT_HEADER + 2] = 4;
        assert_eq!(
            decode_scout_world(&bad_variant),
            Err(WireError::UnknownQuestVariant)
        );

        let mut bad_depth = packet.clone();
        bad_depth[SCOUT_HEADER + 3] = 12;
        assert_eq!(
            decode_scout_world(&bad_depth),
            Err(WireError::InvalidQuestDepth)
        );

        let mut duplicated = packet;
        duplicated[SCOUT_HEADER] = 2;
        let entry_start = SCOUT_HEADER + 1;
        let entry = duplicated[entry_start..entry_start + 3].to_vec();
        duplicated.splice(entry_start + 3..entry_start + 3, entry);
        assert_eq!(
            decode_scout_world(&duplicated),
            Err(WireError::InvalidQuestOrder)
        );

        let mut out_of_range = world;
        out_of_range.quests.wandmaker = Some(ScheduledQuest {
            variant: WandmakerQuestType::CorpseDust,
            depth: 12,
        });
        assert_eq!(
            encode_scout_world(&out_of_range),
            Err(WireError::InvalidQuestDepth)
        );
    }

    #[test]
    fn scout_packet_round_trips_a_plus_four_ring() {
        let world = GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::from_code("AAA-AAA-AAF").unwrap(),
            items: vec![WorldItem {
                item: ItemId::RingSharpshooting,
                upgrade: 4,
                effect: None,
                cursed: true,
                depth: 17,
                source: ItemSource::ImpReward,
                accessibility: Accessibility::Independent,
                secret: true,
            }],
            ring_gems: RingGems::UNSHUFFLED,
        };
        let packet = encode_scout_world(&world).unwrap();
        assert_eq!(decode_scout_world(&packet), Ok(world));
    }

    #[test]
    fn scout_round_trip_covers_every_catalog_item_source_and_accessibility() {
        let items = ITEMS
            .iter()
            .enumerate()
            .map(|(index, definition)| {
                let effect = match definition.kind {
                    ItemKind::Weapon if index % 2 == 0 => {
                        Some(Effect::Weapon(WeaponEffect::Blazing))
                    }
                    ItemKind::Weapon => Some(Effect::Weapon(WeaponEffect::Sacrificial)),
                    ItemKind::Armor if index % 2 == 0 => Some(Effect::Armor(ArmorEffect::Thorns)),
                    ItemKind::Armor => Some(Effect::Armor(ArmorEffect::Stench)),
                    ItemKind::Wand | ItemKind::Ring => None,
                };
                let accessibility = match index % 3 {
                    0 => Accessibility::Independent,
                    1 => Accessibility::Choice {
                        group: u16::try_from(index).unwrap(),
                        option: u8::try_from(index % 64).unwrap(),
                    },
                    _ => Accessibility::Scenarios {
                        group: u16::try_from(index).unwrap(),
                        mask: 1_u64 << (index % 64),
                    },
                };
                WorldItem {
                    item: definition.id,
                    upgrade: u8::try_from(index % 4).unwrap(),
                    effect,
                    cursed: index % 2 != 0,
                    depth: u8::try_from(index % 24 + 1).unwrap(),
                    source: SOURCES[index % SOURCES.len()],
                    accessibility,
                    secret: index % 3 == 0,
                }
            })
            .collect();
        let world = GeneratedWorld {
            quests: crate::quests::QuestSummary {
                ghost: Some(crate::quests::ScheduledQuest {
                    variant: crate::quests::GhostQuestType::FetidRat,
                    depth: 2,
                }),
                imp: Some(crate::quests::ScheduledQuest {
                    variant: crate::quests::ImpQuestType::Vault,
                    depth: 17,
                }),
                ..crate::quests::QuestSummary::default()
            },
            seed: DungeonSeed::MAX,
            items,
            ring_gems: RingGems::UNSHUFFLED,
        };

        let packet = encode_scout_world(&world).unwrap();
        assert_eq!(&packet[..4], b"SSC3");
        assert_eq!(decode_scout_world(&packet), Ok(world));
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One golden packet: quests, then every official item.
    fn canonical_aaa_scout_response_contains_all_official_depth_twenty_four_items() {
        use crate::quests::{
            BlacksmithQuestType, GhostQuestType, ImpQuestType, QuestSummary, ScheduledQuest,
            WandmakerQuestType,
        };

        // Re-pinned from the v4.0.0-BETA-3 oracle (tooling/oracle-4.0): the
        // vault adds fifteen treasure options to the Imp's five prizes.
        let generated = CanonicalMainWorldGenerator.generate(DungeonSeed::MIN, 24);
        assert_eq!(generated.items.len(), 94);
        assert_eq!(
            generated.quests,
            QuestSummary {
                ghost: Some(ScheduledQuest {
                    variant: GhostQuestType::GnollTrickster,
                    depth: 3,
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
            }
        );
        assert_eq!(
            generated
                .items
                .iter()
                .filter(|value| item(value.item).kind == ItemKind::Ring)
                .count(),
            8
        );
        let packet = encode_scout_world(&generated).unwrap();
        let decoded = decode_scout_world(&packet).unwrap();
        assert_eq!(decoded, generated);

        assert!(decoded.items.iter().any(|item| {
            item.depth == 1
                && item.item == ItemId::ThrowingSpear
                && item.upgrade == 2
                && item.source == ItemSource::Chest
        }));
        assert_eq!(decoded.items.iter().filter(|item| item.secret).count(), 5);
        assert!(decoded.items.iter().any(|item| {
            item.depth == 2
                && item.item == ItemId::Kunai
                && item.upgrade == 0
                && item.source == ItemSource::LockedChest
                && item.secret
        }));
        // A v4.0.0 curse travels through the packet by its wire name.
        assert!(decoded.items.iter().any(|item| {
            item.depth == 7
                && item.item == ItemId::Scimitar
                && item.upgrade == 1
                && item.cursed
                && item.effect == Some(Effect::Weapon(WeaponEffect::Wondrous))
        }));

        let blacksmith = decoded
            .items
            .iter()
            .filter(|item| item.depth == 13 && item.source == ItemSource::BlacksmithReward)
            .collect::<Vec<_>>();
        assert_eq!(blacksmith.len(), 4);
        assert!(blacksmith.iter().all(|item| {
            item.upgrade == 2 && matches!(item.accessibility, Accessibility::Choice { .. })
        }));

        let mut depth_twenty = decoded
            .items
            .iter()
            .filter(|item| item.depth == 20 && item.source == ItemSource::Shop)
            .map(|item| item.item)
            .collect::<Vec<_>>();
        depth_twenty.sort_by_key(|item| *item as u8);
        assert_eq!(
            depth_twenty,
            vec![
                ItemId::WarHammer,
                ItemId::ThrowingHammer,
                ItemId::PlateArmor,
                ItemId::IncendiaryDart,
            ]
        );
        // A v4.0.0 enchantment on an animated statue's weapon.
        assert!(decoded.items.iter().any(|item| {
            item.depth == 22
                && item.item == ItemId::Greatsword
                && item.source == ItemSource::Statue
                && item.effect == Some(Effect::Weapon(WeaponEffect::Venomous))
        }));
        assert!(decoded.items.iter().any(|item| {
            item.depth == 24
                && item.item == ItemId::AssassinsBlade
                && item.upgrade == 2
                && item.cursed
                && item.source == ItemSource::SacrificialFire
                && item.effect == Some(Effect::Weapon(WeaponEffect::Polarized))
        }));
        // The Imp's prizes and the vault's treasure share one single-pick group.
        let imp_ring = decoded
            .items
            .iter()
            .find(|item| {
                item.depth == 19
                    && item.item == ItemId::RingHaste
                    && item.upgrade == 2
                    && !item.cursed
                    && item.source == ItemSource::ImpReward
            })
            .expect("the Imp's ring prize");
        let vault_axe = decoded
            .items
            .iter()
            .find(|item| {
                item.depth == 19
                    && item.item == ItemId::BattleAxe
                    && item.upgrade == 4
                    && item.source == ItemSource::VaultTreasure
                    && item.effect == Some(Effect::Weapon(WeaponEffect::Blooming))
            })
            .expect("the vault's +4 battle axe");
        let group = |accessibility: Accessibility| match accessibility {
            Accessibility::Choice { group, .. } => group,
            other => panic!("expected a choice, got {other:?}"),
        };
        assert_eq!(
            group(imp_ring.accessibility),
            group(vault_axe.accessibility)
        );
        assert_ne!(imp_ring.accessibility, vault_axe.accessibility);
    }

    #[test]
    fn every_truncated_scout_fixture_prefix_is_rejected() {
        let world = GeneratedWorld {
            quests: crate::quests::QuestSummary {
                imp: Some(crate::quests::ScheduledQuest {
                    variant: crate::quests::ImpQuestType::Vault,
                    depth: 19,
                }),
                ..crate::quests::QuestSummary::default()
            },
            seed: DungeonSeed::MIN,
            items: vec![WorldItem {
                item: ItemId::Sword,
                upgrade: 3,
                effect: Some(Effect::Weapon(WeaponEffect::Kinetic)),
                cursed: false,
                depth: 19,
                source: ItemSource::ImpReward,
                accessibility: Accessibility::Scenarios {
                    group: 501,
                    mask: 0x8000_0000_0000_0001,
                },
                secret: true,
            }],
            ring_gems: RingGems::UNSHUFFLED,
        };
        let packet = encode_scout_world(&world).unwrap();
        for end in 0..packet.len() {
            assert!(
                decode_scout_world(&packet[..end]).is_err(),
                "accepted truncated prefix of length {end}"
            );
        }
        assert_eq!(decode_scout_world(&packet), Ok(world));
    }

    #[test]
    fn scout_decoder_rejects_reserved_values_and_trailing_data() {
        let world = GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::MIN,
            items: vec![WorldItem {
                item: ItemId::WandFrost,
                upgrade: 0,
                effect: None,
                cursed: false,
                depth: 1,
                source: ItemSource::Heap,
                accessibility: Accessibility::Independent,
                secret: false,
            }],
            ring_gems: RingGems::UNSHUFFLED,
        };
        let packet = encode_scout_world(&world).unwrap();

        let mut bad_flags = packet.clone();
        // Past the header: the empty quest block, the item count, the
        // length-prefixed "wand_frost", then depth and upgrade.
        bad_flags[SCOUT_HEADER + 17] = 4;
        assert_eq!(decode_scout_world(&bad_flags), Err(WireError::InvalidFlags));

        let mut bad_depth = packet.clone();
        bad_depth[SCOUT_HEADER + 15] = 0;
        assert_eq!(
            decode_scout_world(&bad_depth),
            Err(WireError::InvalidItemDepth)
        );

        // Wands reach +4 in v4.0.0; +5 is above every kind's ceiling.
        let mut bad_upgrade = packet.clone();
        bad_upgrade[SCOUT_HEADER + 16] = 5;
        assert_eq!(
            decode_scout_world(&bad_upgrade),
            Err(WireError::InvalidItemUpgrade)
        );

        let mut bad_source = packet.clone();
        bad_source[SCOUT_HEADER + 20] = u8::MAX;
        assert_eq!(
            decode_scout_world(&bad_source),
            Err(WireError::UnknownItemSource)
        );

        let mut bad_accessibility = packet.clone();
        bad_accessibility[SCOUT_HEADER + 21] = u8::MAX;
        assert_eq!(
            decode_scout_world(&bad_accessibility),
            Err(WireError::InvalidAccessibility)
        );

        let mut trailing = packet;
        trailing.push(0);
        assert_eq!(decode_scout_world(&trailing), Err(WireError::TrailingData));

        let mut choice_world = world.clone();
        choice_world.items[0].accessibility = Accessibility::Choice {
            group: 7,
            option: 63,
        };
        let mut bad_choice = encode_scout_world(&choice_world).unwrap();
        *bad_choice.last_mut().unwrap() = 64;
        assert_eq!(
            decode_scout_world(&bad_choice),
            Err(WireError::InvalidAccessibility)
        );
        choice_world.items[0].accessibility = Accessibility::Choice {
            group: 7,
            option: 64,
        };
        assert_eq!(
            encode_scout_world(&choice_world),
            Err(WireError::InvalidAccessibility)
        );

        let mut scenario_world = world;
        scenario_world.items[0].accessibility = Accessibility::Scenarios { group: 9, mask: 1 };
        let mut zero_mask = encode_scout_world(&scenario_world).unwrap();
        let mask_start = zero_mask.len() - 8;
        zero_mask[mask_start..].fill(0);
        assert_eq!(
            decode_scout_world(&zero_mask),
            Err(WireError::InvalidAccessibility)
        );
        scenario_world.items[0].accessibility = Accessibility::Scenarios { group: 9, mask: 0 };
        assert_eq!(
            encode_scout_world(&scenario_world),
            Err(WireError::InvalidAccessibility)
        );
    }

    #[test]
    fn scout_encoder_rejects_more_than_u16_items() {
        let item = WorldItem {
            item: ItemId::WandFrost,
            upgrade: 0,
            effect: None,
            cursed: false,
            depth: 1,
            source: ItemSource::Heap,
            accessibility: Accessibility::Independent,
            secret: false,
        };
        let world = GeneratedWorld {
            quests: crate::quests::QuestSummary::default(),
            seed: DungeonSeed::MIN,
            items: vec![item; usize::from(u16::MAX) + 1],
            ring_gems: RingGems::UNSHUFFLED,
        };
        assert_eq!(
            encode_scout_world(&world),
            Err(WireError::TooManyWorldItems)
        );
    }
}
