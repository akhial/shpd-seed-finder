//! Exact v3.3.8 Halls mob construction and `RegularLevel.createMobs()` order.
//!
//! Halls has no eager subclass selector and no out-of-region rare insertion.
//! Rotation generation therefore consists only of one rare-alternate float
//! per entry followed by Java list shuffle. Succubus, Eye, Scorpio, and Acidic
//! constructors consume no random values; the next draw is the unconditional
//! no-challenge champion-class roll.
//!
//! `DemonSpawnerRoom.paint()` creates its spawner before this inherited mob
//! phase. The spawner is pre-existing occupancy: it does not reduce the number
//! of ordinary mobs requested, but does block placement and participates in
//! the final high-grass cleanup.

use crate::geometry::{PathFinder, Point, cast_shadow, terrain};
use crate::level::Level;
use crate::level_flags::LevelFlags;
use crate::rng::RandomStack;
use crate::room::{Room, RoomId};
use crate::sewer_mob_placement::RoomCharacterRules;

/// Every concrete class which can occur in the canonical Halls rotations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum HallsMobKind {
    Succubus,
    Eye,
    Scorpio,
    /// The 2% replacement for a Scorpio entry.
    Acidic,
}

impl HallsMobKind {
    /// No class in the canonical Halls rotation declares `Property.LARGE`.
    #[must_use]
    pub const fn is_large(self) -> bool {
        let _ = self;
        false
    }
}

/// Generation-visible result of reflection construction followed by the
/// unconditional no-challenge champion-class draw.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConstructedHallsMob {
    pub kind: HallsMobKind,
    pub discarded_champion_roll: u8,
}

/// Persistent `Level.mobsToSpawn` state for a Halls level instance.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HallsMobQueue {
    remaining: Vec<HallsMobKind>,
}

impl HallsMobQueue {
    #[must_use]
    pub fn remaining(&self) -> &[HallsMobKind] {
        &self.remaining
    }

    /// Mirrors `Level.createMob()`: refill and shuffle only when empty,
    /// remove index zero, run the concrete constructor, then consume the
    /// mandatory champion-class `Random.Int(6)` draw.
    ///
    /// # Panics
    ///
    /// Panics outside regular Halls depths 21 through 24.
    pub fn create_mob(&mut self, depth: u32, random: &mut RandomStack) -> ConstructedHallsMob {
        assert!((21..=24).contains(&depth), "Halls depths are 21..=24");
        if self.remaining.is_empty() {
            self.remaining = halls_mob_rotation(depth, random);
        }
        let kind = self.remaining.remove(0);
        let discarded_champion_roll =
            u8::try_from(random.int_bound(6)).expect("champion ordinal is in 0..6");
        ConstructedHallsMob {
            kind,
            discarded_champion_roll,
        }
    }
}

/// Exact `MobSpawner.getMobRotation` for regular Halls floors: no rare-mob
/// insertion, one rare-alt draw per entry, and Java list shuffle.
///
/// # Panics
///
/// Panics outside regular Halls depths 21 through 24.
#[must_use]
pub fn halls_mob_rotation(depth: u32, random: &mut RandomStack) -> Vec<HallsMobKind> {
    use HallsMobKind::{Eye, Scorpio, Succubus};

    let mut rotation = match depth {
        21 => vec![Succubus, Succubus, Eye],
        22 => vec![Succubus, Eye],
        23 => vec![Succubus, Eye, Eye, Scorpio],
        24 => vec![Succubus, Eye, Eye, Scorpio, Scorpio, Scorpio],
        _ => panic!("Halls mob rotations are defined only for depths 21..=24"),
    };

    for kind in &mut rotation {
        if random.float() < 0.02_f32 * random.trinket.exotic_multiplier() {
            *kind = halls_rare_alt(*kind);
        }
    }
    random.shuffle_list(&mut rotation);
    rotation
}

const fn halls_rare_alt(kind: HallsMobKind) -> HallsMobKind {
    match kind {
        HallsMobKind::Scorpio => HallsMobKind::Acidic,
        _ => kind,
    }
}

/// `RegularLevel.mobLimit()` on depths 21 through 24.
///
/// # Panics
///
/// Panics outside regular Halls depths 21 through 24.
#[must_use]
pub fn halls_mob_limit(depth: u32, large: bool, random: &mut RandomStack) -> u32 {
    assert!((21..=24).contains(&depth), "Halls depths are 21..=24");
    let base = 3_i32
        .wrapping_add(i32::try_from(depth % 5).expect("remainder fits Java int"))
        .wrapping_add(random.int_bound(3));
    if large {
        #[allow(clippy::cast_precision_loss)]
        let scaled = (base as f32 * 1.33_f32).ceil();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            scaled as u32
        }
    } else {
        u32::try_from(base).expect("canonical Halls mob limit is positive")
    }
}

/// One ordinary Halls mob after successful spatial placement.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlacedHallsMob {
    pub mob: ConstructedHallsMob,
    pub cell: usize,
}

/// Complete generation-visible result of inherited
/// `RegularLevel.createMobs()` for a Halls floor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HallsMobsResult {
    pub requested_count: u32,
    pub mobs: Vec<PlacedHallsMob>,
    pub remaining_rotation: Vec<HallsMobKind>,
}

/// Runs exact ordinary-mob generation for a painted Halls floor.
/// `level.room_order` must be the painter-mutated Java room order and `flags`
/// must have been built from the final painted map. Pre-painted actors such as
/// the Demon Spawner are read from `level.mob_cells`; they neither trigger a
/// constructor/champion draw here nor count against `requested_count`.
///
/// # Panics
///
/// Panics for malformed painted levels (invalid transition cells or missing
/// standard rooms) or outside regular Halls depths 21 through 24.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn create_halls_mobs<R: RoomCharacterRules>(
    level: &mut Level,
    flags: &mut LevelFlags,
    rooms: &[Room],
    entrance_room: RoomId,
    exit_room: RoomId,
    entrance_cell: usize,
    exit_cell: usize,
    rules: &R,
    random: &mut RandomStack,
) -> HallsMobsResult {
    assert!((21..=24).contains(&level.depth), "Halls depths are 21..=24");
    assert_eq!(flags.passable.len(), level.len(), "flag/map size mismatch");
    assert!(entrance_room < rooms.len() && exit_room < rooms.len());
    assert!(entrance_cell < level.len() && exit_cell < level.len());

    let mut occupied = level.mob_cells.clone();
    let requested_count = halls_mob_limit(
        level.depth,
        level.feeling == crate::level_prelude::Feeling::Large,
        random,
    );

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

    let mut queue = HallsMobQueue::default();
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
            mob,
            &entrance_fov,
            &finder.distance,
            &occupied,
            random,
        ) {
            remaining -= 1;
            occupied[cell] = true;
            level.mark_mob(cell);
            placed.push(PlacedHallsMob { mob, cell });

            if remaining > 0 && random.int_bound(4) == 0 {
                let second = queue.create_mob(level.depth, random);
                if let Some(second_cell) = try_place_mob(
                    level,
                    flags,
                    rooms,
                    room,
                    exit_cell,
                    rules,
                    second,
                    &entrance_fov,
                    &finder.distance,
                    &occupied,
                    random,
                ) {
                    remaining -= 1;
                    occupied[second_cell] = true;
                    level.mark_mob(second_cell);
                    placed.push(PlacedHallsMob {
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

    // Java iterates the complete Level.mobs set here, including the painted
    // Demon Spawner and actors from other special rooms.
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

    HallsMobsResult {
        requested_count,
        mobs: placed,
        remaining_rotation: queue.remaining().to_vec(),
    }
}

#[allow(clippy::too_many_arguments)]
fn try_place_mob<R: RoomCharacterRules>(
    level: &Level,
    flags: &LevelFlags,
    rooms: &[Room],
    room: RoomId,
    exit_cell: usize,
    rules: &R,
    mob: ConstructedHallsMob,
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
            || level.plants.iter().any(|plant| plant.cell == cell)
            || (!flags.open_space[cell] && mob.kind.is_large());
        // Java decrements before the loop condition and then also requires
        // `tries >= 0` before accepting. Its 31st draw is consumed but cannot
        // succeed.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;
    use crate::level_prelude::Feeling;
    use crate::room::{RoomKind, SizeCategory, StandardRoomKind};

    struct SyntheticHallsRules;

    impl RoomCharacterRules for SyntheticHallsRules {
        fn can_place_character(
            &self,
            _level: &Level,
            rooms: &[Room],
            room: RoomId,
            point: Point,
        ) -> bool {
            rooms[room].inside(point) && (room != 1 || point.x != rooms[room].bounds.left + 1)
        }
    }

    fn sequence(depth: u32, seed: i64, count: usize) -> (Vec<ConstructedHallsMob>, i64) {
        let mut random = RandomStack::with_base_seed(0);
        random.push(seed);
        let mut queue = HallsMobQueue::default();
        let mobs = (0..count)
            .map(|_| queue.create_mob(depth, &mut random))
            .collect();
        (mobs, random.long())
    }

    fn short(mob: ConstructedHallsMob) -> String {
        format!("{:?}:{}", mob.kind, mob.discarded_champion_roll)
    }

    fn synthetic_floor(depth: u32, large: bool, spawner: bool) -> (Level, LevelFlags, Vec<Room>) {
        let mut setup_random = RandomStack::with_base_seed(0);
        setup_random.push(999);
        let mut rooms = Vec::new();
        for _ in 0..4 {
            rooms.push(Room::standard(StandardRoomKind::Ruins, &mut setup_random));
        }
        setup_random.pop();

        rooms[0].kind = RoomKind::Entrance(StandardRoomKind::Ruins);
        rooms[0].size_category = Some(SizeCategory::Normal);
        rooms[0].bounds = Rect::new(1, 1, 7, 8);
        rooms[1].size_category = Some(SizeCategory::Large);
        rooms[1].bounds = Rect::new(9, 1, 16, 9);
        rooms[2].size_category = Some(SizeCategory::Normal);
        rooms[2].bounds = Rect::new(18, 1, 25, 9);
        rooms[3].size_category = Some(SizeCategory::Normal);
        rooms[3].bounds = Rect::new(26, 11, 32, 18);

        let mut level = Level::new(depth, if large { Feeling::Large } else { Feeling::None });
        level.set_size(34, 20);
        for room in &rooms {
            for y in room.bounds.top + 1..room.bounds.bottom {
                for x in room.bounds.left + 1..room.bounds.right {
                    level.map.set(x, y, terrain::EMPTY);
                }
            }
        }
        level.map.cells[105] = terrain::ENTRANCE;
        level.map.cells[539] = terrain::EXIT;
        if spawner {
            level.map.cells[182] = terrain::HIGH_GRASS;
            level.mark_mob(182);
        }
        level.room_order = vec![0, 1, 2, 3];
        let flags = LevelFlags::build(&level.map, false);
        (level, flags, rooms)
    }

    fn spatial_fixture(
        depth: u32,
        outer_seed: i64,
        large: bool,
        spawner: bool,
    ) -> (HallsMobsResult, i64, Level, LevelFlags) {
        let (mut level, mut flags, rooms) = synthetic_floor(depth, large, spawner);
        let mut random = RandomStack::with_base_seed(0);
        random.push(outer_seed);
        let result = create_halls_mobs(
            &mut level,
            &mut flags,
            &rooms,
            0,
            3,
            105,
            539,
            &SyntheticHallsRules,
            &mut random,
        );
        let next = random.long();
        (result, next, level, flags)
    }

    fn sorted_spatial(result: &HallsMobsResult) -> Vec<String> {
        let mut mobs = result.mobs.clone();
        mobs.sort_by_key(|placed| placed.cell);
        mobs.into_iter()
            .map(|placed| format!("{:?}@{}", placed.mob.kind, placed.cell))
            .collect()
    }

    #[test]
    fn mob_limits_match_regular_level_formula() {
        let mut random = RandomStack::with_base_seed(0);
        random.push(71);
        assert_eq!(halls_mob_limit(21, false, &mut random), 4);
        random.pop();
        random.push(71);
        assert_eq!(halls_mob_limit(21, true, &mut random), 6);
    }

    #[test]
    fn every_halls_refill_sequence_matches_jdk_oracle() {
        let fixtures = [
            (
                21,
                0_i64,
                2_704_323_167_362_897_208_i64,
                [
                    "Succubus:5",
                    "Succubus:5",
                    "Eye:3",
                    "Succubus:4",
                    "Succubus:5",
                    "Eye:2",
                    "Succubus:0",
                    "Succubus:2",
                    "Eye:5",
                ]
                .as_slice(),
            ),
            (
                22,
                2_i64,
                7_089_749_952_462_770_244_i64,
                [
                    "Eye:0",
                    "Succubus:2",
                    "Succubus:3",
                    "Eye:2",
                    "Eye:3",
                    "Succubus:2",
                    "Eye:5",
                    "Succubus:4",
                ]
                .as_slice(),
            ),
            (
                23,
                1_i64,
                -7_278_947_584_151_701_952_i64,
                [
                    "Scorpio:3",
                    "Eye:4",
                    "Succubus:3",
                    "Eye:4",
                    "Succubus:5",
                    "Scorpio:2",
                    "Eye:4",
                    "Eye:4",
                    "Eye:3",
                    "Eye:4",
                    "Scorpio:0",
                    "Succubus:2",
                ]
                .as_slice(),
            ),
        ];
        for (depth, seed, expected_next, expected) in fixtures {
            let (actual, next) = sequence(depth, seed, expected.len());
            assert_eq!(actual.into_iter().map(short).collect::<Vec<_>>(), expected);
            assert_eq!(next, expected_next);
        }
    }

    #[test]
    fn acidic_alt_and_constructor_boundary_match_jdk_oracle() {
        let (actual, next) = sequence(24, 3, 6);
        assert_eq!(
            actual.into_iter().map(short).collect::<Vec<_>>(),
            [
                "Scorpio:2",
                "Succubus:3",
                "Eye:5",
                "Acidic:0",
                "Eye:4",
                "Scorpio:0",
            ]
        );
        assert_eq!(next, 6_772_372_407_602_667_399);
    }

    #[test]
    fn full_spatial_orchestration_matches_official_halls_oracle() {
        let (result, next, _, _) = spatial_fixture(21, 123, false, false);
        assert_eq!(result.requested_count, 5);
        assert_eq!(
            sorted_spatial(&result),
            [
                "Eye@115",
                "Succubus@122",
                "Succubus@296",
                "Succubus@572",
                "Eye@575",
            ]
        );
        assert_eq!(next, -1_640_229_100_867_826_990);

        let (result, next, _, _) = spatial_fixture(23, 9_876, true, false);
        assert_eq!(result.requested_count, 8);
        assert_eq!(
            sorted_spatial(&result),
            [
                "Eye@88",
                "Scorpio@125",
                "Eye@155",
                "Eye@185",
                "Eye@215",
                "Scorpio@251",
                "Succubus@471",
                "Succubus@473",
            ]
        );
        assert_eq!(next, -7_223_020_374_371_760_863);
    }

    #[test]
    fn demon_spawner_is_preexisting_occupancy_and_is_cleaned_up() {
        let (result, next, level, flags) = spatial_fixture(24, 54_321, false, true);
        assert_eq!(result.requested_count, 7);
        assert_eq!(
            sorted_spatial(&result),
            [
                "Succubus@126",
                "Scorpio@147",
                "Eye@150",
                "Eye@160",
                "Scorpio@435",
                "Scorpio@438",
                "Succubus@608",
            ]
        );
        assert!(level.mob_cells[182]);
        assert!(result.mobs.iter().all(|mob| mob.cell != 182));
        assert_eq!(level.map.cells[182], terrain::GRASS);
        assert!(!flags.los_blocking[182]);
        assert_eq!(next, 6_231_585_805_767_925_542);
    }
}
