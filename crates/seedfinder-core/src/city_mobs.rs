//! Exact v3.3.8 City mob construction and `RegularLevel.createMobs()` order.
//!
//! City rotations have three generation-visible RNG boundaries. Every call to
//! `Elemental.random()` eagerly selects a concrete elemental class while the
//! rotation is built, every constructed elemental initializes its ranged
//! cooldown, and every constructed Golem selects its weapon/armor loot
//! category. All of those draws happen before the unconditional no-challenge
//! champion-class draw.
//!
//! Imp quest scheduling and reward generation deliberately do not appear in
//! this module. `Imp.Quest.spawn()` runs from `CityLevel.initRooms()`, before
//! the graph builder and painter; `ImpRoom.paint()` creates the NPC. The
//! inherited mob phase sees that NPC only as an already occupied cell and
//! includes it in the final high-grass cleanup.

use crate::geometry::{PathFinder, Point, cast_shadow, terrain};
use crate::level::Level;
use crate::level_flags::LevelFlags;
use crate::rng::RandomStack;
use crate::room::{Room, RoomId};
use crate::sewer_mob_placement::RoomCharacterRules;

/// Every concrete class which can occur in the canonical City rotations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum CityMobKind {
    Ghoul,
    FireElemental,
    FrostElemental,
    ShockElemental,
    /// Selected inside `Elemental.random()` by its own 2% exotic check.
    ChaosElemental,
    Warlock,
    Monk,
    Golem,
    /// The 2% replacement for a Monk entry.
    Senior,
    /// The 2.5% depth-nineteen out-of-region insertion.
    Succubus,
}

impl CityMobKind {
    /// Golem declares `Property.LARGE`; no other canonical City mob does so at
    /// construction time.
    #[must_use]
    pub const fn is_large(self) -> bool {
        matches!(self, Self::Golem)
    }

    const fn is_elemental(self) -> bool {
        matches!(
            self,
            Self::FireElemental
                | Self::FrostElemental
                | Self::ShockElemental
                | Self::ChaosElemental
        )
    }
}

/// Category selected by Golem's instance initializer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GolemLootCategory {
    Weapon,
    Armor,
}

/// Generation-visible result of reflection construction followed by the
/// unconditional no-challenge champion-class draw.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConstructedCityMob {
    pub kind: CityMobKind,
    /// `Elemental.rangedCooldown = Random.NormalIntRange(3, 5)` executes for
    /// every concrete elemental subclass during construction.
    pub elemental_ranged_cooldown: Option<i32>,
    /// Golem chooses between `Generator.Category.WEAPON` and `.ARMOR` during
    /// construction, before champion selection.
    pub golem_loot: Option<GolemLootCategory>,
    pub discarded_champion_roll: u8,
}

/// Persistent `Level.mobsToSpawn` state for a City level instance.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CityMobQueue {
    remaining: Vec<CityMobKind>,
}

impl CityMobQueue {
    #[must_use]
    pub fn remaining(&self) -> &[CityMobKind] {
        &self.remaining
    }

    /// Mirrors `Level.createMob()`: refill and shuffle only when empty,
    /// remove index zero, run the concrete constructor, then consume the
    /// mandatory champion-class `Random.Int(6)` draw.
    ///
    /// # Panics
    ///
    /// Panics outside regular City depths 16 through 19.
    pub fn create_mob(&mut self, depth: u32, random: &mut RandomStack) -> ConstructedCityMob {
        assert!((16..=19).contains(&depth), "City depths are 16..=19");
        if self.remaining.is_empty() {
            self.remaining = city_mob_rotation(depth, random);
        }
        let kind = self.remaining.remove(0);
        let elemental_ranged_cooldown = kind.is_elemental().then(|| random.normal_int_range(3, 5));
        let golem_loot = (kind == CityMobKind::Golem).then(|| {
            if random.int_bound(2) == 0 {
                GolemLootCategory::Weapon
            } else {
                GolemLootCategory::Armor
            }
        });
        let discarded_champion_roll =
            u8::try_from(random.int_bound(6)).expect("champion ordinal is in 0..6");
        ConstructedCityMob {
            kind,
            elemental_ranged_cooldown,
            golem_loot,
            discarded_champion_roll,
        }
    }
}

/// Exact `MobSpawner.getMobRotation` for regular City floors, including eager
/// concrete elemental selection, the depth-nineteen Succubus insertion,
/// per-entry rare-alt draws, and Java shuffle.
///
/// # Panics
///
/// Panics outside regular City depths 16 through 19.
#[must_use]
pub fn city_mob_rotation(depth: u32, random: &mut RandomStack) -> Vec<CityMobKind> {
    use CityMobKind::{Ghoul, Golem, Monk, Warlock};

    let mut rotation = match depth {
        16 => vec![Ghoul, Ghoul, Ghoul, random_elemental(random), Warlock],
        17 => vec![
            Ghoul,
            random_elemental(random),
            random_elemental(random),
            Warlock,
            Monk,
        ],
        18 => vec![
            Ghoul,
            random_elemental(random),
            Warlock,
            Warlock,
            Monk,
            Monk,
            Golem,
        ],
        19 => vec![
            random_elemental(random),
            Warlock,
            Warlock,
            Monk,
            Monk,
            Golem,
            Golem,
            Golem,
        ],
        _ => panic!("City mob rotations are defined only for depths 16..=19"),
    };

    if depth == 19 && random.float() < 0.025_f32 {
        rotation.push(CityMobKind::Succubus);
    }
    for kind in &mut rotation {
        if random.float() < 0.02_f32 * random.trinket.exotic_multiplier() {
            *kind = city_rare_alt(*kind);
        }
    }
    random.shuffle_list(&mut rotation);
    rotation
}

fn random_elemental(random: &mut RandomStack) -> CityMobKind {
    // The chaos check is inside Elemental.random(), before the ordinary
    // subtype roll. A successful chaos check therefore skips the second draw.
    if random.float() < 0.02_f32 * random.trinket.exotic_multiplier() {
        return CityMobKind::ChaosElemental;
    }
    let roll = random.float();
    if roll < 0.4_f32 {
        CityMobKind::FireElemental
    } else if roll < 0.8_f32 {
        CityMobKind::FrostElemental
    } else {
        CityMobKind::ShockElemental
    }
}

const fn city_rare_alt(kind: CityMobKind) -> CityMobKind {
    match kind {
        CityMobKind::Monk => CityMobKind::Senior,
        // MobSpawner's Elemental.class mapping cannot match the concrete class
        // returned eagerly by Elemental.random(). Its Float draw still occurs.
        _ => kind,
    }
}

/// `RegularLevel.mobLimit()` on depths 16 through 19.
///
/// # Panics
///
/// Panics outside regular City depths 16 through 19.
#[must_use]
pub fn city_mob_limit(depth: u32, large: bool, random: &mut RandomStack) -> u32 {
    assert!((16..=19).contains(&depth), "City depths are 16..=19");
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
        u32::try_from(base).expect("canonical City mob limit is positive")
    }
}

/// One ordinary City mob after successful spatial placement.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlacedCityMob {
    pub mob: ConstructedCityMob,
    pub cell: usize,
}

/// Complete generation-visible result of inherited
/// `RegularLevel.createMobs()` for a City floor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CityMobsResult {
    pub requested_count: u32,
    pub mobs: Vec<PlacedCityMob>,
    pub remaining_rotation: Vec<CityMobKind>,
}

/// Runs exact ordinary-mob generation for a painted City floor.
/// `level.room_order` must be the painter-mutated Java room order and `flags`
/// must have been built from the final painted map. Pre-painted actors such as
/// Imp are read from `level.mob_cells`; they neither trigger a constructor or
/// champion draw here nor count against `requested_count`.
///
/// # Panics
///
/// Panics for malformed painted levels (invalid transition cells or missing
/// standard rooms) or outside regular City depths 16 through 19.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn create_city_mobs<R: RoomCharacterRules>(
    level: &mut Level,
    flags: &mut LevelFlags,
    rooms: &[Room],
    entrance_room: RoomId,
    exit_room: RoomId,
    entrance_cell: usize,
    exit_cell: usize,
    rules: &R,
    random: &mut RandomStack,
) -> CityMobsResult {
    assert!((16..=19).contains(&level.depth), "City depths are 16..=19");
    assert_eq!(flags.passable.len(), level.len(), "flag/map size mismatch");
    assert!(entrance_room < rooms.len() && exit_room < rooms.len());
    assert!(entrance_cell < level.len() && exit_cell < level.len());

    let mut occupied = level.mob_cells.clone();
    let requested_count = city_mob_limit(
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

    let mut queue = CityMobQueue::default();
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
            placed.push(PlacedCityMob { mob, cell });

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
                    placed.push(PlacedCityMob {
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
    // Imp and actors from special rooms.
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

    CityMobsResult {
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
    mob: ConstructedCityMob,
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

    struct SyntheticCityRules;

    impl RoomCharacterRules for SyntheticCityRules {
        fn can_place_character(
            &self,
            _level: &Level,
            rooms: &[Room],
            room: RoomId,
            point: Point,
        ) -> bool {
            rooms[room].inside(point)
        }
    }

    fn sequence(depth: u32, seed: i64, count: usize) -> (Vec<ConstructedCityMob>, i64) {
        let mut random = RandomStack::with_base_seed(0);
        random.push(seed);
        let mut queue = CityMobQueue::default();
        let mobs = (0..count)
            .map(|_| queue.create_mob(depth, &mut random))
            .collect();
        (mobs, random.long())
    }

    fn short(mob: ConstructedCityMob) -> String {
        format!(
            "{:?}:{}:{:?}:{:?}",
            mob.kind, mob.discarded_champion_roll, mob.elemental_ranged_cooldown, mob.golem_loot
        )
    }

    fn synthetic_floor(depth: u32, large: bool, imp: bool) -> (Level, LevelFlags, Vec<Room>) {
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
        if imp {
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
        imp: bool,
    ) -> (CityMobsResult, i64, Level, LevelFlags) {
        let (mut level, mut flags, rooms) = synthetic_floor(depth, large, imp);
        let mut random = RandomStack::with_base_seed(0);
        random.push(outer_seed);
        let result = create_city_mobs(
            &mut level,
            &mut flags,
            &rooms,
            0,
            3,
            105,
            539,
            &SyntheticCityRules,
            &mut random,
        );
        let next = random.long();
        (result, next, level, flags)
    }

    fn sorted_spatial(result: &CityMobsResult) -> Vec<String> {
        let mut mobs = result.mobs.clone();
        mobs.sort_by_key(|placed| placed.cell);
        mobs.into_iter()
            .map(|placed| {
                format!(
                    "{:?}@{}:{:?}:{:?}",
                    placed.mob.kind,
                    placed.cell,
                    placed.mob.elemental_ranged_cooldown,
                    placed.mob.golem_loot
                )
            })
            .collect()
    }

    #[test]
    fn mob_limits_match_regular_level_formula() {
        let mut random = RandomStack::with_base_seed(0);
        random.push(71);
        assert_eq!(city_mob_limit(16, false, &mut random), 4);
        random.pop();
        random.push(71);
        assert_eq!(city_mob_limit(16, true, &mut random), 6);
    }

    #[test]
    fn refill_sequences_and_constructor_draws_match_official_java_oracle() {
        let fixtures = [
            (
                16,
                0_i64,
                7_577_852_396_602_278_602_i64,
                [
                    "Warlock:5:None:None",
                    "Ghoul:5:None:None",
                    "ShockElemental:2:Some(5):None",
                    "Ghoul:4:None:None",
                    "Ghoul:3:None:None",
                    "FireElemental:5:Some(4):None",
                    "Ghoul:5:None:None",
                    "Ghoul:5:None:None",
                    "Warlock:2:None:None",
                    "Ghoul:2:None:None",
                ]
                .as_slice(),
            ),
            (
                18,
                1_i64,
                -707_056_198_636_690_087_i64,
                [
                    "Ghoul:3:None:None",
                    "Golem:0:None:Some(Armor)",
                    "Warlock:5:None:None",
                    "FrostElemental:4:Some(5):None",
                    "Warlock:2:None:None",
                    "Monk:5:None:None",
                    "Monk:1:None:None",
                    "Ghoul:3:None:None",
                    "Monk:3:None:None",
                    "Monk:3:None:None",
                    "FireElemental:2:Some(5):None",
                    "Warlock:3:None:None",
                    "Warlock:1:None:None",
                    "Golem:4:None:Some(Weapon)",
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
    fn chaos_senior_and_succubus_paths_match_official_java_oracle() {
        let fixtures = [
            (
                16,
                40_i64,
                -5_520_734_485_615_104_544_i64,
                [
                    "Warlock:2:None:None",
                    "Ghoul:4:None:None",
                    "Ghoul:4:None:None",
                    "Ghoul:0:None:None",
                    "ChaosElemental:5:Some(4):None",
                ]
                .as_slice(),
            ),
            (
                17,
                12_i64,
                414_227_122_574_413_980_i64,
                [
                    "ShockElemental:4:Some(4):None",
                    "Ghoul:2:None:None",
                    "Warlock:1:None:None",
                    "Senior:4:None:None",
                    "ShockElemental:3:Some(4):None",
                ]
                .as_slice(),
            ),
            (
                19,
                15_i64,
                -453_669_280_535_156_498_i64,
                [
                    "Golem:0:None:Some(Armor)",
                    "Monk:5:None:None",
                    "ShockElemental:1:Some(3):None",
                    "Monk:4:None:None",
                    "Succubus:5:None:None",
                    "Warlock:3:None:None",
                    "Golem:4:None:Some(Weapon)",
                    "Golem:0:None:Some(Weapon)",
                    "Warlock:2:None:None",
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
    fn full_spatial_orchestration_matches_official_city_level_oracle() {
        let (result, next, _, _) = spatial_fixture(16, 123, false, false);
        assert_eq!(result.requested_count, 5);
        assert_eq!(
            sorted_spatial(&result),
            [
                "Ghoul@115:None:None",
                "Ghoul@184:None:None",
                "Warlock@224:None:None",
                "ShockElemental@261:Some(3):None",
                "Ghoul@471:None:None",
            ]
        );
        assert_eq!(next, -4_066_461_022_830_478_158);

        let (result, next, _, _) = spatial_fixture(18, 9_876, true, false);
        assert_eq!(result.requested_count, 8);
        assert_eq!(
            sorted_spatial(&result),
            [
                "Ghoul@90:None:None",
                "Monk@116:None:None",
                "Warlock@146:None:None",
                "ShockElemental@286:Some(4):None",
                "Warlock@295:None:None",
                "Ghoul@435:None:None",
                "Monk@437:None:None",
                "Golem@505:None:Some(Weapon)",
            ]
        );
        assert_eq!(next, 2_590_809_587_052_442_488);
    }

    #[test]
    fn imp_is_preexisting_occupancy_and_participates_in_grass_cleanup() {
        let (result, next, level, flags) = spatial_fixture(19, 54_321, false, true);
        assert_eq!(result.requested_count, 7);
        assert_eq!(
            sorted_spatial(&result),
            [
                "Succubus@81:None:None",
                "Golem@123:None:Some(Weapon)",
                "FrostElemental@252:Some(4):None",
                "Golem@292:None:Some(Armor)",
                "Golem@293:None:Some(Armor)",
                "Monk@439:None:None",
                "Warlock@572:None:None",
            ]
        );
        assert!(level.mob_cells[182]);
        assert_eq!(level.map.cells[182], terrain::GRASS);
        assert!(!flags.los_blocking[182]);
        assert_eq!(next, -1_157_033_700_101_758_983);
    }
}
