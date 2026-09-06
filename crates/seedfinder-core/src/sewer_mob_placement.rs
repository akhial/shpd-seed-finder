//! Exact spatial orchestration for v3.3.8 `SewerLevel.createMobs()`.
//!
//! Mob class rotations and constructor draws live in [`crate::mobs`]. This
//! layer preserves Ghost placement, weighted/shuffled standard-room traversal,
//! entrance exclusion maps, 31-attempt placement loops, pending-mob reuse, and
//! the same-room second-spawn branch.

use std::fmt;

use crate::geometry::{PathFinder, Point, cast_shadow, terrain};
use crate::level::{Level, TrapKind};
use crate::level_flags::LevelFlags;
use crate::mobs::{ConstructedSewerMob, MobQueue};
use crate::quests::{GhostQuest, QuestError};
use crate::rng::RandomStack;
use crate::room::{Room, RoomId};
use crate::run::GeneratorState;

/// Class-specific `Room.canPlaceCharacter` behavior retained by a concrete
/// room dispatcher after painting (notably `StandardBridgeRoom.spaceRect`).
pub trait RoomCharacterRules {
    fn can_place_character(
        &self,
        level: &Level,
        rooms: &[Room],
        room: RoomId,
        point: Point,
    ) -> bool;
}

/// One ordinary mob after successful spatial placement.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlacedSewerMob {
    pub mob: ConstructedSewerMob,
    pub cell: usize,
}

/// Complete generation-visible result of the Sewer mob phase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SewerMobsResult {
    pub requested_count: u32,
    pub ghost_cell: Option<usize>,
    pub mobs: Vec<PlacedSewerMob>,
    /// The queue can remain partially consumed when the requested count is not
    /// an exact multiple of its rotation size; respawns reuse this state.
    pub remaining_rotation: Vec<crate::mobs::SewerMobKind>,
}

/// Typed failure from Ghost reward generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SewerMobsError(pub QuestError);

impl fmt::Display for SewerMobsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for SewerMobsError {}

impl From<QuestError> for SewerMobsError {
    fn from(value: QuestError) -> Self {
        Self(value)
    }
}

/// `RegularLevel.mobLimit()` for ordinary Sewer floors. Depth one bypasses
/// this result and places its fixed eight-mob tutorial population.
///
/// # Panics
///
/// Panics outside Sewer depths 1 through 4.
#[must_use]
pub fn sewer_mob_limit(depth: u32, large: bool, random: &mut RandomStack) -> u32 {
    assert!((1..=4).contains(&depth), "Sewer depths are 1..=4");
    if depth == 1 {
        return 0;
    }
    let base = 3_i32
        .wrapping_add(i32::try_from(depth % 5).unwrap_or_default())
        .wrapping_add(random.int_bound(3));
    if large {
        #[allow(clippy::cast_precision_loss)]
        let scaled = (base as f32 * 1.33_f32).ceil();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let result = scaled as u32;
        result
    } else {
        u32::try_from(base).unwrap_or_default()
    }
}

/// Mirrors `SewerLevel.createMobs()` and its `RegularLevel` superclass.
/// `flags` must have been built after painting and `level.room_order` must be
/// the painter-mutated Java room-list order.
///
/// # Panics
///
/// Panics for malformed painted levels (missing room order, invalid cells, or
/// no standard room capable of accepting the canonical finite population).
///
/// # Errors
///
/// Returns a typed error only if Ghost reward generation sees corrupted
/// generator state or an invalid scheduling boundary.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn create_sewer_mobs<R: RoomCharacterRules>(
    level: &mut Level,
    flags: &mut LevelFlags,
    rooms: &[Room],
    entrance_room: RoomId,
    exit_room: RoomId,
    entrance_cell: usize,
    exit_cell: usize,
    rules: &R,
    ghost: &mut GhostQuest,
    generator: &mut GeneratorState,
    random: &mut RandomStack,
) -> Result<SewerMobsResult, SewerMobsError> {
    assert!((1..=4).contains(&level.depth), "Sewer depths are 1..=4");
    assert_eq!(flags.passable.len(), level.len(), "flag/map size mismatch");
    assert!(entrance_room < rooms.len() && exit_room < rooms.len());
    assert!(entrance_cell < level.len() && exit_cell < level.len());

    let mut occupied = level.mob_cells.clone();
    let mut ghost_cell = None;
    let depth = u8::try_from(level.depth).expect("Sewer depth fits u8");
    if ghost.try_begin_spawn(random, depth) {
        let cell = loop {
            let point = random_room_point(&rooms[exit_room], 1, random);
            let cell = level.point_to_cell(point);
            if !flags.solid[cell] && flags.open_space[cell] && cell != exit_cell {
                break cell;
            }
        };
        occupied[cell] = true;
        level.mark_mob(cell);
        ghost_cell = Some(cell);
        ghost.finish_spawn(random, generator)?;
    }

    let requested_count = if level.depth == 1 {
        8
    } else {
        sewer_mob_limit(
            level.depth,
            level.feeling == crate::level_prelude::Feeling::Large,
            random,
        )
    };

    let mut standard_rooms = Vec::new();
    for &room in &level.room_order {
        if rooms[room].is_standard() {
            let weight = if rooms[room].is_entrance() {
                1
            } else {
                rooms[room].size_factor()
            };
            for _ in 0..weight {
                standard_rooms.push(room);
            }
        }
    }
    assert!(
        !standard_rooms.is_empty(),
        "regular level has no standard rooms"
    );
    random.shuffle_list(&mut standard_rooms);

    let entrance_point = cell_to_point(entrance_cell, level.width());
    let mut entrance_fov = vec![false; level.len()];
    cast_shadow(
        entrance_point.x,
        entrance_point.y,
        level.width(),
        &mut entrance_fov,
        &flags.los_blocking,
        8,
    );
    let mut entrance_walkable: Vec<bool> = flags.solid.iter().map(|solid| !solid).collect();
    let entrance_bounds = rooms[entrance_room].bounds;
    for y in entrance_bounds.top + 1..entrance_bounds.bottom {
        for x in entrance_bounds.left + 1..entrance_bounds.right {
            let cell = level.point_to_cell(Point::new(x, y));
            if flags.passable[cell] {
                entrance_walkable[cell] = true;
            }
        }
    }
    let mut finder = PathFinder::new(level.width(), level.height());
    finder.build_distance_map_limited(entrance_cell, &entrance_walkable, 8);

    let mut queue = MobQueue::default();
    let mut pending = None;
    let mut remaining = requested_count;
    let mut room_index = 0_usize;
    let mut placed = Vec::with_capacity(usize::try_from(requested_count).unwrap_or_default());
    while remaining > 0 {
        let mob = pending
            .take()
            .unwrap_or_else(|| queue.create_mob(level.depth, random));
        if room_index == standard_rooms.len() {
            room_index = 0;
        }
        let room = standard_rooms[room_index];
        room_index += 1;

        if let Some(cell) = try_place_mob(
            level,
            flags,
            rooms,
            room,
            exit_cell,
            rules,
            &entrance_fov,
            &finder.distance,
            &occupied,
            random,
        ) {
            remaining -= 1;
            occupied[cell] = true;
            level.mark_mob(cell);
            placed.push(PlacedSewerMob { mob, cell });

            if level.depth > 1 && remaining > 0 && random.int_bound(4) == 0 {
                let second = queue.create_mob(level.depth, random);
                if let Some(second_cell) = try_place_mob(
                    level,
                    flags,
                    rooms,
                    room,
                    exit_cell,
                    rules,
                    &entrance_fov,
                    &finder.distance,
                    &occupied,
                    random,
                ) {
                    remaining -= 1;
                    occupied[second_cell] = true;
                    level.mark_mob(second_cell);
                    placed.push(PlacedSewerMob {
                        mob: second,
                        cell: second_cell,
                    });
                } else {
                    pending = Some(second);
                }
            }
        } else {
            pending = Some(mob);
        }
    }

    // Java iterates the complete Level.mobs set here, not just mobs created by
    // this loop. This therefore includes room-painted mobs and the Ghost.
    for (cell, &is_occupied) in occupied.iter().enumerate() {
        if is_occupied
            && matches!(
                level.map.cells[cell],
                terrain::HIGH_GRASS | terrain::FURROWED_GRASS
            )
        {
            level.map.cells[cell] = terrain::GRASS;
            flags.los_blocking[cell] = false;
        }
    }

    Ok(SewerMobsResult {
        requested_count,
        ghost_cell,
        mobs: placed,
        remaining_rotation: queue.remaining().to_vec(),
    })
}

#[allow(clippy::too_many_arguments)]
fn try_place_mob<R: RoomCharacterRules>(
    level: &Level,
    flags: &LevelFlags,
    rooms: &[Room],
    room: RoomId,
    exit_cell: usize,
    rules: &R,
    entrance_fov: &[bool],
    entrance_distance: &[i32],
    occupied: &[bool],
    random: &mut RandomStack,
) -> Option<usize> {
    let mut tries = 30_i32;
    loop {
        let point = random_room_point(&rooms[room], 1, random);
        let cell = level.point_to_cell(point);
        tries -= 1;
        let invalid = occupied[cell]
            || entrance_fov[cell]
            || entrance_distance[cell] != i32::MAX
            || !flags.passable[cell]
            || flags.solid[cell]
            || !rules.can_place_character(level, rooms, room, point)
            || cell == exit_cell
            || level.traps.iter().any(|trap| trap.cell == cell)
            || level.plants.iter().any(|plant| plant.cell == cell);
        // Java consumes a 31st candidate but rejects it after the retry budget expires.
        if !invalid && tries >= 0 {
            return Some(cell);
        }
        if tries < 0 {
            return None;
        }
    }
}

fn random_room_point(room: &Room, margin: i32, random: &mut RandomStack) -> Point {
    Point::new(
        random.int_range(room.bounds.left + margin, room.bounds.right - margin),
        random.int_range(room.bounds.top + margin, room.bounds.bottom - margin),
    )
}

fn cell_to_point(cell: usize, width: i32) -> Point {
    let cell = i32::try_from(cell).expect("generated level cell fits Java int");
    Point::new(cell % width, cell / width)
}

/// Whether a trap destroys items. Exported for the item-placement adapter so
/// both spatial phases share the pinned Sewer subset.
#[must_use]
pub const fn destructive_item_trap(kind: TrapKind) -> bool {
    matches!(
        kind,
        TrapKind::Burning | TrapKind::Chilling | TrapKind::Explosive
    )
}

#[cfg(test)]
mod tests {
    use crate::rng::RandomStack;

    use super::sewer_mob_limit;

    #[test]
    fn mob_limit_draw_and_large_scaling_match_java_float_math() {
        let mut normal = RandomStack::with_base_seed(0);
        normal.push(123);
        let mut large = normal.clone();
        assert_eq!(sewer_mob_limit(2, false, &mut normal), 6);
        assert_eq!(sewer_mob_limit(2, true, &mut large), 8);
        assert_eq!(normal.long(), large.long());
    }
}
