//! Exact v3.3.8 Caves mob construction and `RegularLevel.createMobs()` order.
//!
//! Caves rotations have two RNG boundaries absent from the earlier regions:
//! every `Shaman.random()` call happens while a rotation is built, and every
//! constructed DM-200 (including its DM-201 alternate) selects a weapon/armor
//! loot category before the unconditional champion-class draw.
//!
//! Blacksmith scheduling and reward generation deliberately do not appear in
//! this module. `Blacksmith.Quest.spawn()` runs from `CavesLevel.initRooms()`,
//! before the graph builder and painter; `BlacksmithRoom.paint()` then creates
//! the NPC. The inherited mob phase only sees that NPC as an already occupied
//! cell and includes it in the final high-grass cleanup.

use crate::geometry::{PathFinder, Point, cast_shadow, terrain};
use crate::level::Level;
use crate::level_flags::LevelFlags;
use crate::rng::RandomStack;
use crate::room::{Room, RoomId};
use crate::sewer_mob_placement::RoomCharacterRules;

/// Every class which can occur in the canonical Caves rotations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum CavesMobKind {
    Bat,
    Brute,
    RedShaman,
    BlueShaman,
    PurpleShaman,
    Spinner,
    Dm200,
    /// The 2% replacement for a Brute entry.
    ArmoredBrute,
    /// The 2% replacement for a DM-200 entry.
    Dm201,
    /// The 2.5% depth-fourteen out-of-region insertion.
    Ghoul,
}

impl CavesMobKind {
    /// DM-200 declares `Property.LARGE`; DM-201 inherits it. No other class in
    /// the canonical Caves table is large at construction time.
    #[must_use]
    pub const fn is_large(self) -> bool {
        matches!(self, Self::Dm200 | Self::Dm201)
    }

    const fn has_dm200_initializer(self) -> bool {
        matches!(self, Self::Dm200 | Self::Dm201)
    }
}

/// Category selected by DM-200's instance initializer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Dm200LootCategory {
    Weapon,
    Armor,
}

/// Generation-visible result of reflection construction followed by the
/// unconditional no-challenge champion-class draw.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConstructedCavesMob {
    pub kind: CavesMobKind,
    /// `DM200`'s initializer also runs before the `DM201` initializer.
    pub dm200_loot: Option<Dm200LootCategory>,
    pub discarded_champion_roll: u8,
}

/// Persistent `Level.mobsToSpawn` state for a Caves level instance.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CavesMobQueue {
    remaining: Vec<CavesMobKind>,
}

impl CavesMobQueue {
    #[must_use]
    pub fn remaining(&self) -> &[CavesMobKind] {
        &self.remaining
    }

    /// Mirrors `Level.createMob()`: refill and shuffle only when empty,
    /// remove index zero, run the concrete constructor, then consume the
    /// mandatory champion-class `Random.Int(6)` draw.
    ///
    /// # Panics
    ///
    /// Panics outside regular Caves depths 11 through 14.
    pub fn create_mob(&mut self, depth: u32, random: &mut RandomStack) -> ConstructedCavesMob {
        assert!((11..=14).contains(&depth), "Caves depths are 11..=14");
        if self.remaining.is_empty() {
            self.remaining = caves_mob_rotation(depth, random);
        }
        let kind = self.remaining.remove(0);
        let dm200_loot = kind.has_dm200_initializer().then(|| {
            if random.int_bound(2) == 0 {
                Dm200LootCategory::Weapon
            } else {
                Dm200LootCategory::Armor
            }
        });
        let discarded_champion_roll =
            u8::try_from(random.int_bound(6)).expect("champion ordinal is in 0..6");
        ConstructedCavesMob {
            kind,
            dm200_loot,
            discarded_champion_roll,
        }
    }
}

/// Exact `MobSpawner.getMobRotation` for regular Caves floors, including the
/// eagerly selected Shaman subclasses, depth-fourteen Ghoul insertion,
/// per-entry rare-alt draws, and Java shuffle.
///
/// # Panics
///
/// Panics outside regular Caves depths 11 through 14.
#[must_use]
pub fn caves_mob_rotation(depth: u32, random: &mut RandomStack) -> Vec<CavesMobKind> {
    use CavesMobKind::{Bat, Brute, Dm200, Spinner};

    let mut rotation = match depth {
        11 => vec![Bat, Bat, Bat, Brute, random_shaman(random)],
        12 => vec![Bat, Bat, Brute, Brute, random_shaman(random), Spinner],
        13 => vec![
            Bat,
            Brute,
            Brute,
            random_shaman(random),
            random_shaman(random),
            Spinner,
            Spinner,
            Dm200,
        ],
        14 => vec![
            Bat,
            Brute,
            random_shaman(random),
            random_shaman(random),
            Spinner,
            Spinner,
            Dm200,
            Dm200,
        ],
        _ => panic!("Caves mob rotations are defined only for depths 11..=14"),
    };

    if depth == 14 && random.float() < 0.025_f32 {
        rotation.push(CavesMobKind::Ghoul);
    }
    for kind in &mut rotation {
        if random.float() < 0.02_f32 * random.trinket.exotic_multiplier() {
            *kind = caves_rare_alt(*kind);
        }
    }
    random.shuffle_list(&mut rotation);
    rotation
}

fn random_shaman(random: &mut RandomStack) -> CavesMobKind {
    let roll = random.float();
    if roll < 0.4_f32 {
        CavesMobKind::RedShaman
    } else if roll < 0.8_f32 {
        CavesMobKind::BlueShaman
    } else {
        CavesMobKind::PurpleShaman
    }
}

const fn caves_rare_alt(kind: CavesMobKind) -> CavesMobKind {
    match kind {
        CavesMobKind::Brute => CavesMobKind::ArmoredBrute,
        CavesMobKind::Dm200 => CavesMobKind::Dm201,
        _ => kind,
    }
}

/// `RegularLevel.mobLimit()` on depths 11 through 14.
///
/// # Panics
///
/// Panics outside regular Caves depths 11 through 14.
#[must_use]
pub fn caves_mob_limit(depth: u32, large: bool, random: &mut RandomStack) -> u32 {
    assert!((11..=14).contains(&depth), "Caves depths are 11..=14");
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
        u32::try_from(base).expect("canonical Caves mob limit is positive")
    }
}

/// One ordinary Caves mob after successful spatial placement.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlacedCavesMob {
    pub mob: ConstructedCavesMob,
    pub cell: usize,
}

/// Complete generation-visible result of inherited
/// `RegularLevel.createMobs()` for a Caves floor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CavesMobsResult {
    pub requested_count: u32,
    pub mobs: Vec<PlacedCavesMob>,
    pub remaining_rotation: Vec<CavesMobKind>,
}

/// Runs exact ordinary-mob generation for a painted Caves floor.
/// `level.room_order` must be the painter-mutated Java room order and `flags`
/// must have been built from the final painted map. Pre-painted actors such as
/// Blacksmith are read from `level.mob_cells`; they neither trigger a
/// constructor/champion draw here nor count against `requested_count`.
///
/// # Panics
///
/// Panics for malformed painted levels (invalid transition cells or missing
/// standard rooms) or outside regular Caves depths 11 through 14.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn create_caves_mobs<R: RoomCharacterRules>(
    level: &mut Level,
    flags: &mut LevelFlags,
    rooms: &[Room],
    entrance_room: RoomId,
    exit_room: RoomId,
    entrance_cell: usize,
    exit_cell: usize,
    rules: &R,
    random: &mut RandomStack,
) -> CavesMobsResult {
    assert!((11..=14).contains(&level.depth), "Caves depths are 11..=14");
    assert_eq!(flags.passable.len(), level.len(), "flag/map size mismatch");
    assert!(entrance_room < rooms.len() && exit_room < rooms.len());
    assert!(entrance_cell < level.len() && exit_cell < level.len());

    let mut occupied = level.mob_cells.clone();
    let requested_count = caves_mob_limit(
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

    let mut queue = CavesMobQueue::default();
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
            placed.push(PlacedCavesMob { mob, cell });

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
                    placed.push(PlacedCavesMob {
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

    // Java iterates the complete Level.mobs set here, including room-painted
    // actors such as Blacksmith and any mobs created by special rooms.
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

    CavesMobsResult {
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
    mob: ConstructedCavesMob,
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

    struct SyntheticCavesRules;

    impl RoomCharacterRules for SyntheticCavesRules {
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

    fn sequence(depth: u32, seed: i64, count: usize) -> (Vec<ConstructedCavesMob>, i64) {
        let mut random = RandomStack::with_base_seed(0);
        random.push(seed);
        let mut queue = CavesMobQueue::default();
        let mobs = (0..count)
            .map(|_| queue.create_mob(depth, &mut random))
            .collect();
        (mobs, random.long())
    }

    fn short(mob: ConstructedCavesMob) -> String {
        format!(
            "{:?}:{}:{:?}",
            mob.kind, mob.discarded_champion_roll, mob.dm200_loot
        )
    }

    fn synthetic_floor(
        depth: u32,
        large: bool,
        blacksmith: bool,
    ) -> (Level, LevelFlags, Vec<Room>) {
        let mut setup_random = RandomStack::with_base_seed(0);
        setup_random.push(999);
        let mut rooms = Vec::new();
        for _ in 0..4 {
            rooms.push(Room::standard(
                StandardRoomKind::RegionDecoLine,
                &mut setup_random,
            ));
        }
        setup_random.pop();

        rooms[0].kind = RoomKind::Entrance(StandardRoomKind::RegionDecoLine);
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
        if blacksmith {
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
        blacksmith: bool,
    ) -> (CavesMobsResult, i64, Level, LevelFlags) {
        let (mut level, mut flags, rooms) = synthetic_floor(depth, large, blacksmith);
        let mut random = RandomStack::with_base_seed(0);
        random.push(outer_seed);
        let result = create_caves_mobs(
            &mut level,
            &mut flags,
            &rooms,
            0,
            3,
            105,
            539,
            &SyntheticCavesRules,
            &mut random,
        );
        let next = random.long();
        (result, next, level, flags)
    }

    fn sorted_spatial(result: &CavesMobsResult) -> Vec<String> {
        let mut mobs = result.mobs.clone();
        mobs.sort_by_key(|placed| placed.cell);
        mobs.into_iter()
            .map(|placed| {
                format!(
                    "{:?}@{}:{:?}",
                    placed.mob.kind, placed.cell, placed.mob.dm200_loot
                )
            })
            .collect()
    }

    #[test]
    fn mob_limits_match_regular_level_formula() {
        let mut random = RandomStack::with_base_seed(0);
        random.push(71);
        assert_eq!(caves_mob_limit(11, false, &mut random), 4);
        random.pop();
        random.push(71);
        assert_eq!(caves_mob_limit(11, true, &mut random), 6);
    }

    #[test]
    fn refill_sequences_match_official_java_oracle() {
        let fixtures = [
            (
                11,
                0_i64,
                -655_101_936_082_782_086_i64,
                [
                    "Bat:5:None",
                    "BlueShaman:5:None",
                    "Brute:5:None",
                    "Bat:4:None",
                    "Bat:5:None",
                    "Bat:4:None",
                    "PurpleShaman:3:None",
                    "Bat:4:None",
                    "Bat:0:None",
                    "Brute:5:None",
                ]
                .as_slice(),
            ),
            (
                13,
                1_i64,
                -6_457_567_885_663_813_036_i64,
                [
                    "Spinner:0:None",
                    "Brute:5:None",
                    "Spinner:2:None",
                    "BlueShaman:4:None",
                    "Dm200:2:Some(Armor)",
                    "Bat:5:None",
                    "Brute:1:None",
                    "BlueShaman:5:None",
                    "Spinner:0:None",
                    "Brute:2:None",
                    "Dm200:3:Some(Armor)",
                    "PurpleShaman:1:None",
                    "Spinner:1:None",
                    "Brute:4:None",
                    "Bat:3:None",
                    "RedShaman:4:None",
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
    fn rare_alts_ghoul_and_dm_initializer_match_official_java_oracle() {
        let fixtures = [
            (
                11,
                15_i64,
                -1_488_372_955_361_611_420_i64,
                [
                    "ArmoredBrute:5:None",
                    "Bat:0:None",
                    "Bat:5:None",
                    "Bat:3:None",
                    "PurpleShaman:2:None",
                ]
                .as_slice(),
            ),
            (
                13,
                5_i64,
                -7_824_830_389_292_077_913_i64,
                [
                    "Spinner:4:None",
                    "BlueShaman:4:None",
                    "Bat:2:None",
                    "BlueShaman:1:None",
                    "Spinner:0:None",
                    "Brute:4:None",
                    "Dm201:1:Some(Armor)",
                    "Brute:0:None",
                ]
                .as_slice(),
            ),
            (
                14,
                15_i64,
                6_267_276_159_466_020_352_i64,
                [
                    "Spinner:3:None",
                    "Spinner:0:None",
                    "Bat:5:None",
                    "PurpleShaman:4:None",
                    "Ghoul:3:None",
                    "ArmoredBrute:1:None",
                    "Dm200:5:Some(Armor)",
                    "Dm201:5:Some(Armor)",
                    "PurpleShaman:4:None",
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
    fn full_spatial_orchestration_matches_official_caves_level_oracle() {
        let (result, next, _, _) = spatial_fixture(11, 123, false, false);
        assert_eq!(result.requested_count, 5);
        assert_eq!(
            sorted_spatial(&result),
            [
                "RedShaman@79:None",
                "Bat@149:None",
                "Brute@185:None",
                "Bat@225:None",
                "Bat@571:None",
            ]
        );
        assert_eq!(next, -1_109_942_638_033_487_682);

        let (result, next, _, _) = spatial_fixture(13, 9_876, true, false);
        assert_eq!(result.requested_count, 8);
        assert_eq!(
            sorted_spatial(&result),
            [
                "BlueShaman@92:None",
                "Dm200@148:Some(Weapon)",
                "Bat@185:None",
                "Spinner@215:None",
                "Brute@250:None",
                "Brute@291:None",
                "Spinner@507:None",
                "PurpleShaman@573:None",
            ]
        );
        assert_eq!(next, -3_724_060_226_511_313_812);
    }

    #[test]
    fn blacksmith_is_preexisting_occupancy_and_participates_in_grass_cleanup() {
        let (result, next, level, flags) = spatial_fixture(14, 54_321, false, true);
        assert_eq!(result.requested_count, 7);
        assert_eq!(
            sorted_spatial(&result),
            [
                "Ghoul@81:None",
                "Dm200@83:Some(Weapon)",
                "Spinner@121:None",
                "Dm200@296:Some(Armor)",
                "Spinner@439:None",
                "RedShaman@572:None",
                "Bat@608:None",
            ]
        );
        assert!(level.mob_cells[182]);
        assert_eq!(level.map.cells[182], terrain::GRASS);
        assert!(!flags.los_blocking[182]);
        assert_eq!(next, -1_682_492_321_092_770_821);
    }
}
