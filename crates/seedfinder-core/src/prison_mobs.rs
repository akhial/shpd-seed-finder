//! Exact v3.3.8 Prison mob construction and `RegularLevel.createMobs()` order.
//!
//! `PrisonLevel.createMobs()` first attempts to place the Wandmaker and, when
//! present, generates both of his wand rewards. Only then does the superclass
//! roll the ordinary mob limit, build and shuffle the weighted standard-room
//! list, and construct/place ordinary mobs. Keeping those phases together is
//! important: moving reward generation after the mob-limit draw changes every
//! subsequent class, position, and item generated on the floor.

use std::fmt;

use crate::geometry::{PathFinder, Point, cast_shadow, terrain};
use crate::level::Level;
use crate::level_flags::LevelFlags;
use crate::mobs::ThiefLootCategory;
use crate::model::WorldItem;
use crate::quests::{QuestError, WandmakerQuest};
use crate::rng::RandomStack;
use crate::room::{Room, RoomId};
use crate::run::GeneratorState;
use crate::sewer_mob_placement::RoomCharacterRules;

/// Every class which can occur in the canonical Prison rotations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum PrisonMobKind {
    Skeleton,
    Thief,
    Swarm,
    Dm100,
    Guard,
    Necromancer,
    /// The 2% replacement for a Thief entry.
    Bandit,
    /// The 2% replacement for a Necromancer entry.
    SpectralNecromancer,
    /// The 2.5% depth-nine out-of-region insertion.
    Bat,
}

impl PrisonMobKind {
    /// None of the v3.3.8 Prison rotation classes has `Property.LARGE` at
    /// construction time. The explicit query keeps the spatial rule visible
    /// and makes accidental table expansion fail review rather than silently
    /// skipping `openSpace`.
    #[must_use]
    pub const fn is_large(self) -> bool {
        false
    }

    const fn has_thief_initializer(self) -> bool {
        matches!(self, Self::Thief | Self::Bandit)
    }
}

/// Generation-visible result of reflection construction followed by the
/// unconditional no-challenge champion-class draw.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConstructedPrisonMob {
    pub kind: PrisonMobKind,
    /// `Thief`'s instance initializer also runs for `Bandit` and selects its
    /// eventual loot category before the champion roll.
    pub thief_loot: Option<ThiefLootCategory>,
    pub discarded_champion_roll: u8,
}

/// Persistent `Level.mobsToSpawn` state for a Prison level instance.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PrisonMobQueue {
    remaining: Vec<PrisonMobKind>,
}

impl PrisonMobQueue {
    #[must_use]
    pub fn remaining(&self) -> &[PrisonMobKind] {
        &self.remaining
    }

    /// Mirrors `Level.createMob()`: refill and shuffle only when empty,
    /// remove index zero, run the concrete constructor, then consume the
    /// mandatory champion-class `Random.Int(6)` draw.
    ///
    /// # Panics
    ///
    /// Panics outside regular Prison depths 6 through 9.
    pub fn create_mob(&mut self, depth: u32, random: &mut RandomStack) -> ConstructedPrisonMob {
        assert!((6..=9).contains(&depth), "Prison depths are 6..=9");
        if self.remaining.is_empty() {
            self.remaining = prison_mob_rotation(depth, random);
        }
        let kind = self.remaining.remove(0);
        let thief_loot = kind.has_thief_initializer().then(|| {
            if random.int_bound(2) == 0 {
                ThiefLootCategory::Ring
            } else {
                ThiefLootCategory::Artifact
            }
        });
        let discarded_champion_roll =
            u8::try_from(random.int_bound(6)).expect("champion ordinal is in 0..6");
        ConstructedPrisonMob {
            kind,
            thief_loot,
            discarded_champion_roll,
        }
    }
}

/// Exact `MobSpawner.getMobRotation` for regular Prison floors, including the
/// depth-nine Bat insertion, per-entry rare-alt draws, and Java shuffle.
///
/// # Panics
///
/// Panics outside regular Prison depths 6 through 9.
#[must_use]
pub fn prison_mob_rotation(depth: u32, random: &mut RandomStack) -> Vec<PrisonMobKind> {
    use PrisonMobKind::{Dm100, Guard, Necromancer, Skeleton, Swarm, Thief};

    let mut rotation = match depth {
        6 => vec![Skeleton, Skeleton, Skeleton, Thief, Swarm],
        7 => vec![Skeleton, Skeleton, Skeleton, Thief, Dm100, Guard],
        8 => vec![
            Skeleton,
            Skeleton,
            Thief,
            Dm100,
            Dm100,
            Guard,
            Guard,
            Necromancer,
        ],
        9 => vec![
            Skeleton,
            Thief,
            Dm100,
            Dm100,
            Guard,
            Guard,
            Necromancer,
            Necromancer,
        ],
        _ => panic!("Prison mob rotations are defined only for depths 6..=9"),
    };

    if depth == 9 && random.float() < 0.025_f32 {
        rotation.push(PrisonMobKind::Bat);
    }
    for kind in &mut rotation {
        if random.float() < 0.02_f32 * random.trinket.exotic_multiplier() {
            *kind = prison_rare_alt(*kind);
        }
    }
    random.shuffle_list(&mut rotation);
    rotation
}

const fn prison_rare_alt(kind: PrisonMobKind) -> PrisonMobKind {
    match kind {
        PrisonMobKind::Thief => PrisonMobKind::Bandit,
        PrisonMobKind::Necromancer => PrisonMobKind::SpectralNecromancer,
        _ => kind,
    }
}

/// `RegularLevel.mobLimit()` on depths 6 through 9.
///
/// # Panics
///
/// Panics outside regular Prison depths 6 through 9.
#[must_use]
pub fn prison_mob_limit(depth: u32, large: bool, random: &mut RandomStack) -> u32 {
    assert!((6..=9).contains(&depth), "Prison depths are 6..=9");
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
        u32::try_from(base).expect("canonical Prison mob limit is positive")
    }
}

/// One ordinary Prison mob after successful spatial placement.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlacedPrisonMob {
    pub mob: ConstructedPrisonMob,
    pub cell: usize,
}

/// Complete generation-visible result of `PrisonLevel.createMobs()`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrisonMobsResult {
    pub requested_count: u32,
    pub wandmaker_cell: Option<usize>,
    pub mobs: Vec<PlacedPrisonMob>,
    pub remaining_rotation: Vec<PrisonMobKind>,
    /// The two mutually exclusive Wandmaker choices, empty when no quest room
    /// was present in the successful build attempt.
    pub quest_rewards: Vec<WorldItem>,
}

/// Typed failure from Wandmaker reward generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrisonMobsError(pub QuestError);

impl fmt::Display for PrisonMobsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for PrisonMobsError {}

impl From<QuestError> for PrisonMobsError {
    fn from(value: QuestError) -> Self {
        Self(value)
    }
}

/// Runs exact Wandmaker and ordinary-mob generation for a painted Prison
/// floor. `level.room_order` must be the painter-mutated Java room order and
/// `flags` must have been built from the final painted map.
///
/// `wandmaker_choice_group` is the run-unique choice-group identifier used by
/// the query matcher for the two mutually exclusive quest rewards.
///
/// # Panics
///
/// Panics for malformed painted levels (invalid transition cells, missing
/// standard rooms, or an entrance room with no valid Wandmaker position).
///
/// # Errors
///
/// Returns an error only when quest/generator state is inconsistent with the
/// room-scheduling phase.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn create_prison_mobs<R: RoomCharacterRules>(
    level: &mut Level,
    flags: &mut LevelFlags,
    rooms: &[Room],
    entrance_room: RoomId,
    exit_room: RoomId,
    entrance_cell: usize,
    exit_cell: usize,
    rules: &R,
    wandmaker: &mut WandmakerQuest,
    generator: &mut GeneratorState,
    wandmaker_choice_group: u16,
    random: &mut RandomStack,
) -> Result<PrisonMobsResult, PrisonMobsError> {
    assert!((6..=9).contains(&level.depth), "Prison depths are 6..=9");
    assert_eq!(flags.passable.len(), level.len(), "flag/map size mismatch");
    assert!(entrance_room < rooms.len() && exit_room < rooms.len());
    assert!(entrance_cell < level.len() && exit_cell < level.len());

    let mut occupied = level.mob_cells.clone();
    let mut wandmaker_cell = None;
    let depth = u8::try_from(level.depth).expect("Prison depth fits u8");
    if wandmaker.begin_mob_spawn(depth) {
        let cell = place_wandmaker(level, flags, &rooms[entrance_room], entrance_cell, random);
        occupied[cell] = true;
        level.mark_mob(cell);
        wandmaker_cell = Some(cell);
        // This must precede mobLimit(): Generator item constructors and wand
        // upgrade curse-clearing draws are part of the level RNG stream.
        wandmaker.finish_spawn(random, generator)?;
    }

    let requested_count = prison_mob_limit(
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

    let mut queue = PrisonMobQueue::default();
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
            placed.push(PlacedPrisonMob { mob, cell });

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
                    placed.push(PlacedPrisonMob {
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
    // this loop. This therefore includes room-painted mobs and Wandmaker.
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

    let mut quest_rewards = Vec::with_capacity(2);
    if wandmaker_cell.is_some() {
        wandmaker.append_world_items(wandmaker_choice_group, &mut quest_rewards);
    }
    Ok(PrisonMobsResult {
        requested_count,
        wandmaker_cell,
        mobs: placed,
        remaining_rotation: queue.remaining().to_vec(),
        quest_rewards,
    })
}

fn place_wandmaker(
    level: &Level,
    flags: &LevelFlags,
    entrance_room: &Room,
    entrance_cell: usize,
    random: &mut RandomStack,
) -> usize {
    let mut tries = 0_i32;
    let mut distance = 2_i32;
    loop {
        if tries > 30 && distance > 0 {
            tries = 0;
            distance -= 1;
        }
        let point = random_room_point(entrance_room, distance, random);
        let cell = level.point_to_cell(point);
        let x = i32::try_from(cell).expect("generated cell fits Java int");
        let width = level.width();
        let adjacent_to_door = [-width, -1, 1, width].iter().any(|offset| {
            let neighbour = usize::try_from(x.wrapping_add(*offset))
                .expect("Wandmaker candidate has four in-bounds neighbours");
            level.map.cells[neighbour] == terrain::DOOR
        });
        let invalid = cell == entrance_cell
            || flags.solid[cell]
            || adjacent_to_door
            || level.traps.iter().any(|trap| trap.cell == cell)
            || !flags.passable[cell]
            || level.map.cells[cell] == terrain::EMPTY_SP;
        tries += 1;
        if !invalid {
            return cell;
        }
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
    mob: ConstructedPrisonMob,
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
        // `tries >= 0` before accepting. Its 31st draw is therefore consumed
        // but cannot succeed.
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
    use crate::catalog::ItemId;
    use crate::geometry::Rect;
    use crate::level_prelude::Feeling;
    use crate::room::{RoomKind, SizeCategory, StandardRoomKind};
    use crate::run::initialize_run;

    struct SyntheticPrisonRules;

    impl RoomCharacterRules for SyntheticPrisonRules {
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

    fn sequence(depth: u32, seed: i64, count: usize) -> (Vec<ConstructedPrisonMob>, i64) {
        let mut random = RandomStack::with_base_seed(0);
        random.push(seed);
        let mut queue = PrisonMobQueue::default();
        let mobs = (0..count)
            .map(|_| queue.create_mob(depth, &mut random))
            .collect();
        (mobs, random.long())
    }

    fn short(mob: ConstructedPrisonMob) -> String {
        format!(
            "{:?}:{}:{:?}",
            mob.kind, mob.discarded_champion_roll, mob.thief_loot
        )
    }

    fn synthetic_floor(
        depth: u32,
        large: bool,
        force_wandmaker_fallback: bool,
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
        if force_wandmaker_fallback {
            for y in 3..=6 {
                for x in 3..=5 {
                    level.map.set(x, y, terrain::EMPTY_SP);
                }
            }
        }
        level.map.cells[105] = terrain::ENTRANCE;
        level.map.cells[539] = terrain::EXIT;
        level.room_order = vec![0, 1, 2, 3];
        let flags = LevelFlags::build(&level.map, false);
        (level, flags, rooms)
    }

    fn spatial_fixture(
        depth: u32,
        outer_seed: i64,
        large: bool,
        with_wandmaker: bool,
        force_wandmaker_fallback: bool,
    ) -> (PrisonMobsResult, i64, i32) {
        let (mut level, mut flags, rooms) = synthetic_floor(depth, large, force_wandmaker_fallback);
        let mut wandmaker = WandmakerQuest::default();
        if with_wandmaker {
            let mut schedule_random = RandomStack::with_base_seed(0);
            schedule_random.push(17);
            assert!(
                wandmaker
                    .schedule_room(
                        &mut schedule_random,
                        u8::try_from(depth).expect("fixture depth fits u8"),
                    )
                    .is_some()
            );
        }
        let mut generator = initialize_run(0).generator;
        let mut random = RandomStack::with_base_seed(0);
        random.push(outer_seed);
        let result = create_prison_mobs(
            &mut level,
            &mut flags,
            &rooms,
            0,
            3,
            105,
            539,
            &SyntheticPrisonRules,
            &mut wandmaker,
            &mut generator,
            71,
            &mut random,
        )
        .expect("official fixture has valid quest/generator state");
        let next = random.long();
        let wand_dropped = generator
            .category(crate::run::GeneratorCategory::Wand)
            .dropped;
        (result, next, wand_dropped)
    }

    fn sorted_spatial(result: &PrisonMobsResult) -> Vec<String> {
        let mut mobs = result.mobs.clone();
        mobs.sort_by_key(|placed| placed.cell);
        mobs.into_iter()
            .map(|placed| {
                format!(
                    "{:?}@{}:{:?}",
                    placed.mob.kind, placed.cell, placed.mob.thief_loot
                )
            })
            .collect()
    }

    #[test]
    fn mob_limits_match_regular_level_formula() {
        let mut random = RandomStack::with_base_seed(0);
        random.push(71);
        assert_eq!(prison_mob_limit(6, false, &mut random), 4);
        random.pop();
        random.push(71);
        assert_eq!(prison_mob_limit(6, true, &mut random), 6);
    }

    #[test]
    fn refill_sequences_match_official_java_oracle() {
        let fixtures = [
            (
                6,
                0_i64,
                -655_101_936_082_782_086_i64,
                [
                    "Swarm:2:None",
                    "Skeleton:5:None",
                    "Skeleton:5:None",
                    "Skeleton:5:None",
                    "Thief:5:Some(Artifact)",
                    "Swarm:2:None",
                    "Skeleton:4:None",
                    "Skeleton:3:None",
                    "Skeleton:4:None",
                    "Thief:5:Some(Artifact)",
                ]
                .as_slice(),
            ),
            (
                8,
                1_i64,
                2_526_686_697_193_033_577_i64,
                [
                    "Skeleton:3:None",
                    "Guard:4:None",
                    "Necromancer:0:None",
                    "Skeleton:5:None",
                    "Thief:4:Some(Artifact)",
                    "Dm100:4:None",
                    "Guard:2:None",
                    "Dm100:5:None",
                    "Guard:3:None",
                    "Dm100:3:None",
                    "Guard:3:None",
                    "Dm100:3:None",
                    "Necromancer:0:None",
                    "Thief:2:Some(Artifact)",
                    "Skeleton:3:None",
                    "Skeleton:1:None",
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
    fn rare_alt_and_bat_constructor_paths_match_official_java_oracle() {
        let fixtures = [
            (
                6,
                3_i64,
                -7_543_070_019_192_949_867_i64,
                [
                    "Skeleton:2:None",
                    "Swarm:3:None",
                    "Skeleton:2:None",
                    "Skeleton:3:None",
                    "Bandit:0:Some(Ring)",
                ]
                .as_slice(),
            ),
            (
                9,
                12_i64,
                8_328_652_621_051_227_890_i64,
                [
                    "Dm100:2:None",
                    "Necromancer:1:None",
                    "Dm100:4:None",
                    "SpectralNecromancer:1:None",
                    "Guard:2:None",
                    "Thief:0:Some(Ring)",
                    "Skeleton:2:None",
                    "Guard:3:None",
                    "Guard:0:None",
                ]
                .as_slice(),
            ),
            (
                9,
                40_i64,
                -380_375_425_674_373_895_i64,
                [
                    "Bat:4:None",
                    "Necromancer:2:None",
                    "Necromancer:2:None",
                    "Dm100:3:None",
                    "Guard:1:None",
                    "Skeleton:0:None",
                    "Thief:1:Some(Ring)",
                    "Dm100:4:None",
                    "Guard:4:None",
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
    fn full_spatial_orchestration_matches_official_prison_level_oracle() {
        let (result, next, wand_dropped) = spatial_fixture(6, 123, false, false, false);
        assert_eq!(result.requested_count, 5);
        assert_eq!(
            sorted_spatial(&result),
            [
                "Skeleton@215:None",
                "Skeleton@250:None",
                "Skeleton@261:None",
                "Swarm@296:None",
                "Thief@472:Some(Ring)",
            ]
        );
        assert_eq!(wand_dropped, 0);
        assert_eq!(next, 6_762_457_678_382_412_045);

        let (result, next, wand_dropped) = spatial_fixture(8, 9_876, true, false, false);
        assert_eq!(result.requested_count, 8);
        assert_eq!(
            sorted_spatial(&result),
            [
                "Necromancer@148:None",
                "Dm100@185:None",
                "Thief@260:Some(Artifact)",
                "Skeleton@284:None",
                "Dm100@295:None",
                "Guard@437:None",
                "Guard@439:None",
                "Skeleton@469:None",
            ]
        );
        assert_eq!(wand_dropped, 0);
        assert_eq!(next, 8_403_689_058_473_592_863);
    }

    #[test]
    fn wandmaker_precedes_mob_limit_and_emits_mutually_exclusive_rewards() {
        let (result, next, wand_dropped) = spatial_fixture(9, 54_321, false, true, false);
        assert_eq!(result.requested_count, 7);
        assert_eq!(result.wandmaker_cell, Some(173));
        assert_eq!(
            sorted_spatial(&result),
            [
                "Dm100@92:None",
                "Thief@158:Some(Artifact)",
                "Guard@185:None",
                "Necromancer@252:None",
                "Necromancer@287:None",
                "Skeleton@435:None",
                "Dm100@609:None",
            ]
        );
        assert_eq!(result.quest_rewards.len(), 2);
        assert_eq!(result.quest_rewards[0].item, ItemId::WandFrost);
        assert_eq!(result.quest_rewards[0].upgrade, 2);
        assert_eq!(result.quest_rewards[1].item, ItemId::WandBlastWave);
        assert_eq!(result.quest_rewards[1].upgrade, 1);
        assert_eq!(wand_dropped, 2);
        assert_eq!(
            result.quest_rewards[0].accessibility,
            crate::model::Accessibility::Choice {
                group: 71,
                option: 0,
            }
        );
        assert_eq!(
            result.quest_rewards[1].accessibility,
            crate::model::Accessibility::Choice {
                group: 71,
                option: 1,
            }
        );
        assert_eq!(next, 6_315_323_137_450_710_079);
    }

    #[test]
    fn wandmaker_relaxes_room_margin_only_after_thirty_one_failed_draws() {
        let (result, next, wand_dropped) = spatial_fixture(9, 24_680, false, true, true);
        assert_eq!(result.requested_count, 9);
        assert_eq!(result.wandmaker_cell, Some(70));
        assert_eq!(
            sorted_spatial(&result),
            [
                "Dm100@115:None",
                "Necromancer@117:None",
                "Skeleton@122:None",
                "Guard@149:None",
                "Necromancer@217:None",
                "Thief@437:Some(Ring)",
                "Thief@471:Some(Artifact)",
                "Dm100@473:None",
                "Guard@575:None",
            ]
        );
        assert_eq!(result.quest_rewards[0].item, ItemId::WandFrost);
        assert_eq!(result.quest_rewards[0].upgrade, 3);
        assert_eq!(result.quest_rewards[1].item, ItemId::WandBlastWave);
        assert_eq!(result.quest_rewards[1].upgrade, 1);
        assert_eq!(wand_dropped, 2);
        assert_eq!(next, -6_938_640_669_918_653_400);
    }
}
