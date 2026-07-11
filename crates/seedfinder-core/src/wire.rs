//! Dependency-free binary protocol shared with the Android JNI adapter.

use std::fmt;

use crate::catalog::{Effect, ItemKind, item, item_by_stable_id};
use crate::model::{Accessibility, GeneratedWorld, ItemSource, WorldItem};
use crate::query::{Requirement, SearchQuery, UpgradeRequirement};
use crate::seed::DungeonSeed;

const REQUEST_MAGIC_V1: &[u8; 4] = b"SSF1";
const REQUEST_MAGIC_V2: &[u8; 4] = b"SSF2";
const REQUEST_MAGIC_V3: &[u8; 4] = b"SSF3";
const RESULT_MAGIC: &[u8; 4] = b"SSR1";
const SCOUT_RESULT_MAGIC: &[u8; 4] = b"SSC1";
const MAX_REQUIREMENTS: usize = 64;

/// Decodes Android `SSF1`, `SSF2`, and `SSF3` packets. Older versions are
/// retained for compatibility; V2 adds the overall floor limit,
/// source/identity constraints, upgrade predicates, and
/// three flag bits: bit 0 requires blacksmith availability, bit 1 enables the
/// lossy fast search mode described on [`SearchQuery::fast_mode`], and bit 2
/// prevents Blacksmith "Smith" rewards from satisfying item requirements. V3
/// adds an optional floor limit to every individual requirement.
///
/// # Errors
///
/// Returns [`WireError`] for malformed lengths/UTF-8, unknown IDs or effects,
/// invalid upgrades, trailing data, or an inconsistent query.
pub fn decode_query(packet: &[u8]) -> Result<SearchQuery, WireError> {
    let mut input = Input::new(packet);
    let magic = input.take(4)?;
    let query = if magic == REQUEST_MAGIC_V1 {
        decode_query_v1(&mut input)?
    } else if magic == REQUEST_MAGIC_V2 || magic == REQUEST_MAGIC_V3 {
        decode_query_v2(&mut input, magic == REQUEST_MAGIC_V3)?
    } else {
        return Err(WireError::BadMagic);
    };
    if !input.is_empty() {
        return Err(WireError::TrailingData);
    }
    query.validate().map_err(|_| WireError::InvalidQuery)?;
    Ok(query)
}

fn decode_query_v1(input: &mut Input<'_>) -> Result<SearchQuery, WireError> {
    let count = usize::from(input.u16()?);
    if count == 0 || count > MAX_REQUIREMENTS {
        return Err(WireError::InvalidRequirementCount);
    }
    let mut requirements = Vec::with_capacity(count);
    for _ in 0..count {
        let stable_id = input.utf8_u16()?;
        let definition = item_by_stable_id(stable_id).ok_or(WireError::UnknownItem)?;
        let upgrade = input.u8()?;
        let modifier = input.utf8_u16()?;
        let effect = if modifier.is_empty() {
            None
        } else {
            Some(
                Effect::from_wire_name(definition.kind, modifier)
                    .ok_or(WireError::UnknownModifier)?,
            )
        };
        requirements.push(Requirement {
            kind: definition.kind,
            item: Some(definition.id),
            upgrade: UpgradeRequirement::Exact(upgrade),
            effect,
            source: None,
            identity_group: None,
            max_depth: None,
        });
    }
    Ok(SearchQuery {
        requirements,
        max_depth: 24,
        require_blacksmith: false,
        exclude_blacksmith_rewards: false,
        fast_mode: false,
    })
}

fn decode_query_v2(
    input: &mut Input<'_>,
    has_requirement_depths: bool,
) -> Result<SearchQuery, WireError> {
    let max_depth = input.u8()?;
    let flags = input.u8()?;
    if flags & !0b111 != 0 {
        return Err(WireError::InvalidFlags);
    }
    let count = usize::from(input.u16()?);
    if count == 0 || count > MAX_REQUIREMENTS {
        return Err(WireError::InvalidRequirementCount);
    }
    let mut requirements = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = item_kind_from_wire_id(input.u8()?).ok_or(WireError::UnknownItemKind)?;
        let stable_id = input.utf8_u16()?;
        let item = if stable_id.is_empty() {
            None
        } else {
            Some(
                item_by_stable_id(stable_id)
                    .ok_or(WireError::UnknownItem)?
                    .id,
            )
        };
        let upgrade_mode = input.u8()?;
        let upgrade_value = input.u8()?;
        let upgrade = match upgrade_mode {
            0 if upgrade_value == 0 => UpgradeRequirement::Any,
            1 => UpgradeRequirement::Exact(upgrade_value),
            2 => UpgradeRequirement::AtLeast(upgrade_value),
            _ => return Err(WireError::InvalidUpgradeMode),
        };
        let modifier = input.utf8_u16()?;
        let effect = if modifier.is_empty() {
            None
        } else {
            Some(Effect::from_wire_name(kind, modifier).ok_or(WireError::UnknownModifier)?)
        };
        let source = match input.u8()? {
            0 => None,
            value => Some(source_from_wire_id(value - 1).ok_or(WireError::UnknownItemSource)?),
        };
        let identity_group = match input.u8()? {
            0 => None,
            value => Some(value),
        };
        let requirement_max_depth = if has_requirement_depths {
            match input.u8()? {
                0 => None,
                value => Some(value),
            }
        } else {
            None
        };
        requirements.push(Requirement {
            kind,
            item,
            upgrade,
            effect,
            source,
            identity_group,
            max_depth: requirement_max_depth,
        });
    }
    Ok(SearchQuery {
        requirements,
        max_depth,
        require_blacksmith: flags & 1 != 0,
        exclude_blacksmith_rewards: flags & 0b100 != 0,
        fast_mode: flags & 0b10 != 0,
    })
}

const fn item_kind_from_wire_id(id: u8) -> Option<ItemKind> {
    Some(match id {
        0 => ItemKind::Weapon,
        1 => ItemKind::Armor,
        2 => ItemKind::Wand,
        3 => ItemKind::Ring,
        _ => return None,
    })
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

/// Parses the UTF-8 seed-code request accepted by `JniBindings.scoutSeed`.
///
/// The request deliberately contains no duplicated envelope: the JNI method
/// identifies its schema, while [`DungeonSeed::from_code`] supplies the exact
/// game-compatible syntax and range validation.
///
/// # Errors
///
/// Returns [`WireError::InvalidUtf8`] or [`WireError::InvalidSeedCode`] when
/// the request is not one user-enterable dungeon seed.
pub fn decode_scout_seed(request: &[u8]) -> Result<DungeonSeed, WireError> {
    let code = std::str::from_utf8(request).map_err(|_| WireError::InvalidUtf8)?;
    DungeonSeed::from_code(code).map_err(|_| WireError::InvalidSeedCode)
}

/// Encodes every searchable item in one generated world for scouting mode.
///
/// `SSC1` is big-endian and self-delimiting:
///
/// ```text
/// magic[4], seed:utf8_u8, item_count:u16,
/// repeated {
///   stable_item_id:utf8_u16, depth:u8, exact_upgrade:u8,
///   flags:u8 (bit 0 = cursed), effect_wire_name:utf8_u16,
///   source:u8, accessibility_tag:u8, accessibility_payload
/// }
/// ```
///
/// Accessibility payloads are empty for independent items, `group:u16,
/// option:u8` for choices, and `group:u16, mask:u64` for explicit scenarios.
/// Item order is preserved exactly from [`GeneratedWorld::items`].
///
/// # Errors
///
/// Returns an error if the item count or a UTF-8 field exceeds its declared
/// protocol width. Catalog fields in the pinned game version always fit.
pub fn encode_scout_world(world: &GeneratedWorld) -> Result<Vec<u8>, WireError> {
    let count = u16::try_from(world.items.len()).map_err(|_| WireError::TooManyWorldItems)?;
    let seed = world.seed.to_code();
    let seed_length = u8::try_from(seed.len()).map_err(|_| WireError::FieldTooLong)?;

    let mut output = Vec::with_capacity(7 + seed.len() + world.items.len() * 32);
    output.extend_from_slice(SCOUT_RESULT_MAGIC);
    output.push(seed_length);
    output.extend_from_slice(seed.as_bytes());
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
        output.push(u8::from(world_item.cursed));
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

/// Decodes an `SSC1` scouting response.
///
/// This is primarily the executable protocol specification and makes native
/// round-trip tests cover every source/accessibility branch. Android uses the
/// same field layout directly to attach catalog display metadata.
///
/// # Errors
///
/// Returns [`WireError`] for malformed lengths, identifiers, flags, enum
/// values, accessibility constraints, or trailing bytes.
pub fn decode_scout_world(packet: &[u8]) -> Result<GeneratedWorld, WireError> {
    let mut input = Input::new(packet);
    if input.take(4)? != SCOUT_RESULT_MAGIC {
        return Err(WireError::BadMagic);
    }
    let seed = DungeonSeed::from_code(input.utf8_u8()?).map_err(|_| WireError::InvalidSeedCode)?;
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
        if flags & !1 != 0 {
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
        });
    }
    if !input.is_empty() {
        return Err(WireError::TrailingData);
    }
    Ok(GeneratedWorld { seed, items })
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WireError {
    BadMagic,
    Truncated,
    InvalidUtf8,
    InvalidSeedCode,
    InvalidRequirementCount,
    UnknownItemKind,
    UnknownItem,
    UnknownModifier,
    InvalidUpgradeMode,
    InvalidQuery,
    TrailingData,
    TooManyResults,
    TooManyWorldItems,
    FieldTooLong,
    InvalidFlags,
    InvalidItemDepth,
    InvalidItemUpgrade,
    UnknownItemSource,
    InvalidAccessibility,
}

impl fmt::Display for WireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::BadMagic => "unexpected packet magic or schema version",
            Self::Truncated => "packet ended before a declared field",
            Self::InvalidUtf8 => "packet contains invalid UTF-8",
            Self::InvalidSeedCode => "seed code must contain nine A-Z characters",
            Self::InvalidRequirementCount => "requirement count must be 1..=64",
            Self::UnknownItemKind => "packet names an unknown item category",
            Self::UnknownItem => "packet names an unknown item ID",
            Self::UnknownModifier => "packet names an unknown enchantment or glyph",
            Self::InvalidUpgradeMode => "packet contains an invalid upgrade predicate",
            Self::InvalidQuery => "packet describes an inconsistent search query",
            Self::TrailingData => "packet has trailing bytes",
            Self::TooManyResults => "result batch exceeds the protocol limit",
            Self::TooManyWorldItems => "scouted world exceeds the protocol item limit",
            Self::FieldTooLong => "packet string exceeds its declared field width",
            Self::InvalidFlags => "packet contains unknown flag bits",
            Self::InvalidItemDepth => "scouted item depth must be in 1..=24",
            Self::InvalidItemUpgrade => "scouted item upgrade must be in 0..=3",
            Self::UnknownItemSource => "packet names an unknown item source",
            Self::InvalidAccessibility => "packet contains an invalid accessibility constraint",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for WireError {}

#[cfg(test)]
mod tests {
    use crate::catalog::{ArmorEffect, Effect, ITEMS, ItemId, ItemKind, WeaponEffect, item};
    use crate::main_world::CanonicalMainWorldGenerator;
    use crate::model::{Accessibility, GeneratedWorld, ItemSource, WorldItem};
    use crate::query::UpgradeRequirement;
    use crate::search::WorldGenerator;
    use crate::seed::DungeonSeed;

    use super::{
        WireError, decode_query, decode_scout_seed, decode_scout_world, empty_results,
        encode_results, encode_scout_world,
    };

    const SOURCES: [ItemSource; 17] = [
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

    fn field(output: &mut Vec<u8>, value: &str) {
        output.extend_from_slice(&u16::try_from(value.len()).unwrap().to_be_bytes());
        output.extend_from_slice(value.as_bytes());
    }

    #[test]
    fn kotlin_ssf1_shape_decodes_to_exact_and_query() {
        let mut packet = b"SSF1".to_vec();
        packet.extend_from_slice(&2_u16.to_be_bytes());
        field(&mut packet, "wand_frost");
        packet.push(2);
        field(&mut packet, "");
        field(&mut packet, "mail_armor");
        packet.push(3);
        field(&mut packet, "Stench");

        let query = decode_query(&packet).unwrap();
        assert_eq!(query.requirements.len(), 2);
        assert_eq!(query.requirements[0].item, Some(ItemId::WandFrost));
        assert_eq!(query.requirements[0].upgrade, UpgradeRequirement::Exact(2));
        assert_eq!(
            query.requirements[1].effect,
            Some(Effect::Armor(ArmorEffect::Stench))
        );
        assert_eq!(query.max_depth, 24);
    }

    #[test]
    fn decoder_rejects_truncation_unknown_ids_and_trailing_data() {
        assert_eq!(decode_query(b"SSF1\0"), Err(WireError::Truncated));

        let mut packet = b"SSF1\0\x01\0\x07unknown\x01\0\0".to_vec();
        assert_eq!(decode_query(&packet), Err(WireError::UnknownItem));
        packet = b"SSF1\0\x01\0\x05sword\x01\0\0x".to_vec();
        assert_eq!(decode_query(&packet), Err(WireError::TrailingData));
    }

    #[test]
    fn appended_tipped_dart_ids_decode_without_changing_v1_wire_shape() {
        let mut packet = b"SSF1".to_vec();
        packet.extend_from_slice(&1_u16.to_be_bytes());
        field(&mut packet, "cleansing_dart");
        packet.push(1);
        field(&mut packet, "");

        let query = decode_query(&packet).unwrap();
        assert_eq!(query.requirements[0].item, Some(ItemId::CleansingDart));
        assert_eq!(query.requirements[0].upgrade, UpgradeRequirement::Exact(1));
        assert_eq!(query.requirements[0].effect, None);
    }

    #[test]
    fn query_wire_accepts_plus_four_only_for_rings() {
        let mut ring = b"SSF1".to_vec();
        ring.extend_from_slice(&1_u16.to_be_bytes());
        field(&mut ring, "ring_sharpshooting");
        ring.push(4);
        field(&mut ring, "");
        let query = decode_query(&ring).unwrap();
        assert_eq!(query.requirements[0].kind, ItemKind::Ring);
        assert_eq!(query.requirements[0].upgrade, UpgradeRequirement::Exact(4));

        let mut sword = b"SSF1".to_vec();
        sword.extend_from_slice(&1_u16.to_be_bytes());
        field(&mut sword, "sword");
        sword.push(4);
        field(&mut sword, "");
        assert_eq!(decode_query(&sword), Err(WireError::InvalidQuery));
    }

    #[test]
    fn ssf2_decodes_general_linked_source_and_floor_constraints() {
        let mut packet = b"SSF2".to_vec();
        packet.extend_from_slice(&[14, 1, 0, 4]); // floor, blacksmith flag, count
        for (mode, value, source, group) in [
            (1, 3, 15, 1), // exact +3 from Wandmaker, linked group 1
            (2, 0, 0, 1),  // at least +0, linked group 1
            (2, 0, 0, 1),
            (1, 1, 0, 0), // any +1 wand
        ] {
            packet.push(2); // wand
            field(&mut packet, ""); // any wand type
            packet.extend_from_slice(&[mode, value]);
            field(&mut packet, "");
            packet.extend_from_slice(&[source, group]);
        }

        let query = decode_query(&packet).unwrap();
        assert_eq!(query.max_depth, 14);
        assert!(query.require_blacksmith);
        assert_eq!(query.requirements.len(), 4);
        assert_eq!(query.requirements[0].item, None);
        assert_eq!(query.requirements[0].upgrade, UpgradeRequirement::Exact(3));
        assert_eq!(
            query.requirements[0].source,
            Some(ItemSource::WandmakerReward)
        );
        assert_eq!(query.requirements[0].identity_group, Some(1));
        assert_eq!(
            query.requirements[1].upgrade,
            UpgradeRequirement::AtLeast(0)
        );
        assert_eq!(query.requirements[3].identity_group, None);
    }

    #[test]
    fn ssf3_decodes_per_requirement_floor_limit() {
        let mut packet = b"SSF3".to_vec();
        packet.extend_from_slice(&[24, 0, 0, 1]);
        packet.push(0); // weapon
        field(&mut packet, "sword");
        packet.extend_from_slice(&[1, 2]);
        field(&mut packet, "");
        packet.extend_from_slice(&[0, 0, 5]);

        let query = decode_query(&packet).unwrap();
        assert_eq!(query.requirements[0].max_depth, Some(5));
    }

    #[test]
    fn ssf2_flag_bit_one_selects_fast_mode() {
        let mut packet = b"SSF2".to_vec();
        packet.extend_from_slice(&[24, 0b10, 0, 1]); // floor, fast-mode flag, count
        packet.push(1); // armor
        field(&mut packet, "");
        packet.extend_from_slice(&[1, 3]); // exact +3
        field(&mut packet, "");
        packet.extend_from_slice(&[0, 0]);

        let query = decode_query(&packet).unwrap();
        assert!(query.fast_mode);
        assert!(!query.require_blacksmith);
    }

    #[test]
    fn ssf2_flag_bit_two_excludes_blacksmith_rewards() {
        let mut packet = b"SSF2".to_vec();
        packet.extend_from_slice(&[24, 0b100, 0, 1]);
        packet.push(0); // weapon
        field(&mut packet, "sword");
        packet.extend_from_slice(&[1, 2]);
        field(&mut packet, "");
        packet.extend_from_slice(&[0, 0]);

        let query = decode_query(&packet).unwrap();
        assert!(query.exclude_blacksmith_rewards);
        assert!(!query.require_blacksmith);
    }

    #[test]
    fn ssf2_rejects_reserved_flags_kinds_and_upgrade_modes() {
        assert_eq!(
            decode_query(b"SSF2\x18\x08\0\x01"),
            Err(WireError::InvalidFlags)
        );

        let mut bad_kind = b"SSF2\x18\0\0\x01".to_vec();
        bad_kind.push(4);
        assert_eq!(decode_query(&bad_kind), Err(WireError::UnknownItemKind));

        let mut bad_mode = b"SSF2\x18\0\0\x01\x02\0\0".to_vec();
        bad_mode.extend_from_slice(&[3, 0]);
        assert_eq!(decode_query(&bad_mode), Err(WireError::InvalidUpgradeMode));
    }

    #[test]
    fn result_packet_matches_android_big_endian_codec() {
        let worlds = vec![
            GeneratedWorld {
                seed: DungeonSeed::MIN,
                items: Vec::new(),
            },
            GeneratedWorld {
                seed: DungeonSeed::new(1).unwrap(),
                items: Vec::new(),
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
    fn scout_packet_has_a_fixed_android_big_endian_fixture() {
        let world = GeneratedWorld {
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
            }],
        };
        let packet = encode_scout_world(&world).unwrap();
        let mut expected = b"SSC1\x0bAAA-AAA-AAA\0\x01\0\x0awand_frost".to_vec();
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
    fn scout_packet_round_trips_a_plus_four_ring() {
        let world = GeneratedWorld {
            seed: DungeonSeed::from_code("AAA-AAA-AAF").unwrap(),
            items: vec![WorldItem {
                item: ItemId::RingSharpshooting,
                upgrade: 4,
                effect: None,
                cursed: true,
                depth: 17,
                source: ItemSource::ImpReward,
                accessibility: Accessibility::Independent,
            }],
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
                }
            })
            .collect();
        let world = GeneratedWorld {
            seed: DungeonSeed::MAX,
            items,
        };

        let packet = encode_scout_world(&world).unwrap();
        assert_eq!(&packet[..4], b"SSC1");
        assert_eq!(decode_scout_world(&packet), Ok(world));
    }

    #[test]
    fn canonical_aaa_scout_response_contains_all_official_depth_twenty_four_items() {
        let generated = CanonicalMainWorldGenerator.generate(DungeonSeed::MIN, 24);
        assert_eq!(generated.items.len(), 67);
        assert_eq!(
            generated
                .items
                .iter()
                .filter(|value| item(value.item).kind == ItemKind::Ring)
                .count(),
            5
        );
        let packet = encode_scout_world(&generated).unwrap();
        let decoded = decode_scout_world(&packet).unwrap();
        assert_eq!(decoded, generated);

        assert!(decoded.items.iter().any(|item| {
            item.depth == 1
                && item.item == ItemId::ScaleArmor
                && item.upgrade == 0
                && item.source == ItemSource::Chest
        }));
        assert!(decoded.items.iter().any(|item| {
            item.depth == 7
                && item.item == ItemId::ThrowingSpear
                && item.upgrade == 1
                && item.cursed
                && item.effect == Some(Effect::Weapon(WeaponEffect::Polarized))
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

        let depth_twenty = decoded
            .items
            .iter()
            .filter(|item| item.depth == 20 && item.source == ItemSource::Shop)
            .map(|item| item.item)
            .collect::<Vec<_>>();
        assert_eq!(
            depth_twenty,
            vec![
                ItemId::PlateArmor,
                ItemId::ThrowingHammer,
                ItemId::Greatshield,
                ItemId::IncendiaryDart,
            ]
        );
        assert!(decoded.items.iter().any(|item| {
            item.depth == 22
                && item.item == ItemId::PlateArmor
                && item.upgrade == 2
                && item.effect == Some(Effect::Armor(ArmorEffect::Swiftness))
        }));
        assert!(decoded.items.iter().any(|item| {
            item.depth == 24
                && item.item == ItemId::RunicBlade
                && item.cursed
                && item.effect == Some(Effect::Weapon(WeaponEffect::Displacing))
        }));
        assert!(decoded.items.iter().any(|item| {
            item.depth == 19
                && item.item == ItemId::RingHaste
                && item.upgrade == 3
                && item.cursed
                && item.source == ItemSource::ImpReward
        }));
    }

    #[test]
    fn every_truncated_scout_fixture_prefix_is_rejected() {
        let world = GeneratedWorld {
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
            }],
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
            seed: DungeonSeed::MIN,
            items: vec![WorldItem {
                item: ItemId::WandFrost,
                upgrade: 0,
                effect: None,
                cursed: false,
                depth: 1,
                source: ItemSource::Heap,
                accessibility: Accessibility::Independent,
            }],
        };
        let packet = encode_scout_world(&world).unwrap();

        let mut bad_flags = packet.clone();
        // Header (18), ID length (2), "wand_frost" (10), depth, upgrade.
        bad_flags[32] = 2;
        assert_eq!(decode_scout_world(&bad_flags), Err(WireError::InvalidFlags));

        let mut bad_depth = packet.clone();
        bad_depth[30] = 0;
        assert_eq!(
            decode_scout_world(&bad_depth),
            Err(WireError::InvalidItemDepth)
        );

        let mut bad_upgrade = packet.clone();
        bad_upgrade[31] = 4;
        assert_eq!(
            decode_scout_world(&bad_upgrade),
            Err(WireError::InvalidItemUpgrade)
        );

        let mut bad_source = packet.clone();
        bad_source[35] = u8::MAX;
        assert_eq!(
            decode_scout_world(&bad_source),
            Err(WireError::UnknownItemSource)
        );

        let mut bad_accessibility = packet.clone();
        bad_accessibility[36] = u8::MAX;
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
        };
        let world = GeneratedWorld {
            seed: DungeonSeed::MIN,
            items: vec![item; usize::from(u16::MAX) + 1],
        };
        assert_eq!(
            encode_scout_world(&world),
            Err(WireError::TooManyWorldItems)
        );
    }
}
