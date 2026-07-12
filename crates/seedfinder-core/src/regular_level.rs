//! Exact regular-floor preparation and Sewer room-graph orchestration.
//!
//! This module joins the already-pinned run decks, mandatory-drop rolls,
//! room queues, room factories, and builders at the same boundaries used by
//! v3.3.8 `Level.create()` and `RegularLevel.build()`. Concrete room painting,
//! mobs, and ordinary item placement remain later phases; keeping them out of
//! this layer makes pre-painter RNG and graph parity independently testable.

use crate::builder::{Builder, RegularLevelBuilder};
use crate::challenges::Challenges;
use crate::generator::{GeneratedItem, GeneratorError, random_category};
use crate::level_prelude::{Feeling, LimitedDrops, MandatoryDrops, roll_feeling};
use crate::quests::{QuestError, QuestState, WandmakerQuestType};
use crate::rng::RandomStack;
use crate::room::{
    QuestRoomKind, Room, SecretRoomKind as GraphSecretRoom, SpecialRoomKind as GraphSpecialRoom,
    create_entrance_room, create_exit_room, create_quest_room, create_standard_room,
};
use crate::room_decks::{
    FloorSpecialDeck, SelectedSpecialRoom, begin_special_floor, secrets_for_floor,
    select_secret_room, select_special_room,
};
use crate::run::{
    ConsumableSpecialRoom, EquipmentSpecialRoom, GeneratorCategory, RunState,
    SecretRoomKind as RunSecretRoom, SpecialRoomKind as RunSpecialRoom,
};

/// Non-spatial state prepared before `RegularLevel.build()` begins.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedRegularFloor {
    pub depth: u32,
    pub feeling: Feeling,
    pub mandatory_drops: MandatoryDrops,
    /// `HallsLevel.create()` queues two Torches before delegating to
    /// `Level.create`; other regular regions queue none at this point.
    pub prequeued_torches: u8,
    /// Deck-backed food queued by `Level.create()`. Fixed mandatory potion,
    /// scroll, stone, and catalyst identities are represented by
    /// [`MandatoryDrops`] because they do not touch `Generator`.
    pub queued_generated_items: Vec<GeneratedItem>,
}

/// Performs the ordinary main-branch part of `Level.create()` through the
/// feeling roll. The caller must already have pushed `seedCurDepth()`.
///
/// # Panics
///
/// Panics when called for a boss, branch, or out-of-range main-path depth.
///
/// # Errors
///
/// Returns an error only if the supplied generator state violates a pinned
/// category invariant.
pub fn prepare_regular_floor(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    depth: u32,
    random: &mut RandomStack,
) -> Result<PreparedRegularFloor, GeneratorError> {
    assert!(
        (1..=24).contains(&depth) && !matches!(depth, 5 | 10 | 15 | 20),
        "regular main-path depth must be a non-boss floor in 1..=24"
    );
    let depth_i32 = i32::try_from(depth).expect("main-path depth fits i32");

    let mut queued_generated_items = vec![random_category(
        random,
        &mut run.generator,
        GeneratorCategory::Food,
        depth_i32,
    )?];
    let mut mandatory_drops = limited_drops.roll_for_floor(depth_i32, random);
    if mandatory_drops.upgrade_scroll
        && run.challenges.contains(Challenges::NO_SCROLLS)
        && limited_drops.upgrade_scrolls % 2 == 0
    {
        mandatory_drops.upgrade_scroll = false;
    }
    let feeling = roll_feeling(depth_i32, random);
    if feeling == Feeling::Large {
        queued_generated_items.push(random_category(
            random,
            &mut run.generator,
            GeneratorCategory::Food,
            depth_i32,
        )?);
    }

    Ok(PreparedRegularFloor {
        depth,
        feeling,
        mandatory_drops,
        prequeued_torches: if depth >= 21 { 2 } else { 0 },
        queued_generated_items,
    })
}

/// Sewer `initRooms()` output after Java's final `Random.shuffle(initRooms)`.
#[derive(Clone, Debug, PartialEq)]
pub struct InitializedRegularRooms {
    pub rooms: Vec<Room>,
    pub entrance: usize,
    pub exit: usize,
    /// Mutable `SpecialRoom.floorSpecials` after this floor selected all of
    /// its special rooms. The official oracle exposes this queue as metadata.
    pub floor_specials: FloorSpecialDeck,
}

/// Successfully positioned room graph, before painter normalization.
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltRegularGraph {
    pub rooms: Vec<Room>,
    pub entrance: usize,
    pub exit: usize,
    pub floor_specials: FloorSpecialDeck,
    pub builder_attempts: u32,
}

/// Main-dungeon region owning one regular floor.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MainRegion {
    Sewers,
    Prison,
    Caves,
    City,
    Halls,
}

impl MainRegion {
    /// Region for a non-boss main-path depth.
    ///
    /// # Panics
    ///
    /// Panics for boss or out-of-range depths.
    #[must_use]
    pub const fn for_regular_depth(depth: u32) -> Self {
        match depth {
            1..=4 => Self::Sewers,
            6..=9 => Self::Prison,
            11..=14 => Self::Caves,
            16..=19 => Self::City,
            21..=24 => Self::Halls,
            _ => panic!("regular main-path depth is outside 1..=24"),
        }
    }
}

/// Region-generic `RegularLevel.build()` through a successful room graph.
/// Quest schedulers run after base `initRooms()` and before its final shuffle,
/// matching each region subclass.
///
/// # Panics
///
/// Panics for boss/out-of-range depths or a canonical builder invariant.
///
/// # Errors
///
/// Returns an error only if Blacksmith or Imp reward generation sees corrupt
/// generator state.
pub fn build_regular_room_graph(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    quests: &mut QuestState,
    depth: u32,
    feeling: Feeling,
    random: &mut RandomStack,
) -> Result<BuiltRegularGraph, QuestError> {
    let mut builder = RegularLevelBuilder::for_regular_level(random);
    let initialized = init_regular_rooms(run, limited_drops, quests, depth, feeling, random)?;
    let mut attempts = 0_u32;
    loop {
        attempts = attempts.wrapping_add(1);
        let mut rooms = initialized.rooms.clone();
        for room in &mut rooms {
            room.neighbours.clear();
            room.connected.clear();
        }
        if builder.build(&mut rooms, depth, random) {
            let entrance = rooms
                .iter()
                .position(Room::is_entrance)
                .expect("a regular graph retains its entrance");
            let exit = rooms
                .iter()
                .position(Room::is_exit)
                .expect("a regular graph retains its exit");
            return Ok(BuiltRegularGraph {
                rooms,
                entrance,
                exit,
                floor_specials: initialized.floor_specials,
                builder_attempts: attempts,
            });
        }
    }
}

/// Exact region-generic `initRooms()` including Prison/Caves/City quests and
/// the mandatory Halls Demon Spawner room.
///
/// # Panics
///
/// Panics for boss/out-of-range depths or an exhausted pinned room table.
///
/// # Errors
///
/// Returns an error only from reward generation performed while scheduling a
/// Blacksmith or Imp room.
pub fn init_regular_rooms(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    quests: &mut QuestState,
    depth: u32,
    feeling: Feeling,
    random: &mut RandomStack,
) -> Result<InitializedRegularRooms, QuestError> {
    let region = MainRegion::for_regular_depth(depth);
    let depth_u8 = u8::try_from(depth).expect("main-path depth fits u8");
    let depth_i32 = i32::try_from(depth).expect("main-path depth fits i32");
    let mut rooms = vec![
        create_entrance_room(depth, random),
        create_exit_room(depth, random),
    ];

    let base_standard_count = if feeling == Feeling::Large {
        region.maximum_standard_rooms()
    } else {
        region.draw_standard_rooms(random)
    };
    let standards = if feeling == Feeling::Large {
        // Java evaluates this in float and truncates ceil back to int.
        #[allow(clippy::cast_precision_loss)]
        let scaled = (base_standard_count as f32 * 1.5_f32).ceil();
        #[allow(clippy::cast_possible_truncation)]
        let result = scaled as i32;
        result
    } else {
        base_standard_count
    };
    append_standard_rooms(&mut rooms, standards, depth, random);

    if matches!(depth, 6 | 11 | 16) {
        rooms.push(Room::special(GraphSpecialRoom::Shop));
    }

    let mut special_count = if feeling == Feeling::Large {
        region.maximum_special_rooms().wrapping_add(1)
    } else {
        region.draw_special_rooms(random)
    };
    let mut floor_specials =
        begin_special_floor(&run.special_rooms, limited_drops, depth_i32, random);
    let mut selected_specials = 0_i32;
    while selected_specials < special_count {
        let selected = select_special_room(
            &mut run.special_rooms,
            &mut floor_specials,
            depth_i32,
            random,
        );
        if selected == SelectedSpecialRoom::Pit {
            special_count = special_count.wrapping_add(1);
        }
        rooms.push(Room::special(graph_special_kind(selected)));
        selected_specials = selected_specials.wrapping_add(1);
    }

    let mut secret_count =
        secrets_for_floor(&mut run.secret_rooms.region_secrets, depth_i32, random);
    if feeling == Feeling::Secrets {
        secret_count = secret_count.wrapping_add(1);
    }
    for _ in 0..secret_count {
        let selected = select_secret_room(&mut run.secret_rooms.run_secrets, random);
        rooms.push(Room::secret(graph_secret_kind(selected)));
    }

    match region {
        MainRegion::Sewers => {}
        MainRegion::Prison => {
            if let Some(kind) = quests.wandmaker.schedule_room(random, depth_u8) {
                let graph_kind = match kind {
                    WandmakerQuestType::CorpseDust => QuestRoomKind::MassGrave,
                    WandmakerQuestType::ElementalEmbers => QuestRoomKind::RitualSite,
                    WandmakerQuestType::Rotberry => QuestRoomKind::RotGarden,
                };
                rooms.push(create_quest_room(graph_kind, random));
            }
        }
        MainRegion::Caves => {
            if quests.blacksmith.begin_room_schedule(random, depth_u8) {
                rooms.push(create_quest_room(QuestRoomKind::Blacksmith, random));
                quests
                    .blacksmith
                    .finish_room_schedule(random, &mut run.generator)?;
            }
        }
        MainRegion::City => {
            if quests.imp.begin_room_schedule(random, depth_u8) {
                rooms.push(create_quest_room(QuestRoomKind::AmbitiousImp, random));
                quests
                    .imp
                    .finish_room_schedule(random, &mut run.generator)?;
            }
        }
        MainRegion::Halls => rooms.push(Room::special(GraphSpecialRoom::DemonSpawner)),
    }

    random.shuffle_list(&mut rooms);
    let entrance = rooms
        .iter()
        .position(Room::is_entrance)
        .expect("initRooms always includes an entrance");
    let exit = rooms
        .iter()
        .position(Room::is_exit)
        .expect("initRooms always includes an exit");
    Ok(InitializedRegularRooms {
        rooms,
        entrance,
        exit,
        floor_specials,
    })
}

impl MainRegion {
    const fn maximum_standard_rooms(self) -> i32 {
        match self {
            Self::Sewers | Self::Prison => 6,
            Self::Caves => 7,
            Self::City => 8,
            Self::Halls => 9,
        }
    }

    fn draw_standard_rooms(self, random: &mut RandomStack) -> i32 {
        let (base, weights): (i32, &[f32]) = match self {
            Self::Sewers => (4, &[1.0, 3.0, 1.0]),
            Self::Prison => (5, &[1.0, 1.0]),
            Self::Caves => (6, &[2.0, 1.0]),
            Self::City => (6, &[1.0, 3.0, 1.0]),
            Self::Halls => (8, &[2.0, 1.0]),
        };
        base.wrapping_add(
            i32::try_from(
                random
                    .chances(weights)
                    .expect("regional standard-room weights are positive"),
            )
            .expect("fixed chance index fits i32"),
        )
    }

    const fn maximum_special_rooms(self) -> i32 {
        match self {
            Self::Sewers => 2,
            Self::Prison | Self::Caves | Self::City | Self::Halls => 3,
        }
    }

    fn draw_special_rooms(self, random: &mut RandomStack) -> i32 {
        let (base, weights): (i32, &[f32]) = match self {
            Self::Sewers => (1, &[1.0, 4.0]),
            Self::Prison => (1, &[1.0, 3.0, 1.0]),
            Self::Caves => (2, &[4.0, 1.0]),
            Self::City => (2, &[2.0, 1.0]),
            Self::Halls => (2, &[1.0, 1.0]),
        };
        base.wrapping_add(
            i32::try_from(
                random
                    .chances(weights)
                    .expect("regional special-room weights are positive"),
            )
            .expect("fixed chance index fits i32"),
        )
    }
}

fn append_standard_rooms(
    rooms: &mut Vec<Room>,
    standards: i32,
    depth: u32,
    random: &mut RandomStack,
) {
    let mut consumed_value = 0_i32;
    while consumed_value < standards {
        let remaining = standards.wrapping_sub(consumed_value);
        let room = loop {
            let mut candidate = create_standard_room(depth, random);
            if candidate.set_size_category_for_value(remaining, random) {
                break candidate;
            }
        };
        consumed_value = consumed_value.wrapping_add(room.size_factor());
        rooms.push(room);
    }
}

/// Runs the exact Sewer `RegularLevel.builder()` selection followed by
/// `initRooms()` and builder retry semantics. The caller's depth generator
/// must be active and floor preparation must already be complete.
///
/// # Panics
///
/// Panics when called outside regular Sewer depths 1 through 4 or if a
/// canonical builder invariant is violated.
#[must_use]
pub fn build_sewer_room_graph(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    depth: u32,
    feeling: Feeling,
    random: &mut RandomStack,
) -> BuiltRegularGraph {
    assert!((1..=4).contains(&depth), "Sewer regular depths are 1..=4");

    // `RegularLevel.build()` chooses and configures its builder before it
    // calls the virtual `initRooms()` hook.
    let mut builder = RegularLevelBuilder::for_sewer_level(random);
    let initialized = init_sewer_rooms(run, limited_drops, depth, feeling, random);
    let mut attempts = 0_u32;
    loop {
        attempts = attempts.wrapping_add(1);
        // Java clones the ArrayList but reuses the room objects. Every builder
        // begins with setEmpty() and cleared graph edges, so cloning this
        // immutable pre-build snapshot is observationally equivalent while
        // excluding tunnel rooms allocated by failed attempts.
        let mut rooms = initialized.rooms.clone();
        for room in &mut rooms {
            room.neighbours.clear();
            room.connected.clear();
        }
        if builder.build(&mut rooms, depth, random) {
            let entrance = rooms
                .iter()
                .position(Room::is_entrance)
                .expect("a regular graph retains its entrance");
            let exit = rooms
                .iter()
                .position(Room::is_exit)
                .expect("a regular graph retains its exit");
            return BuiltRegularGraph {
                rooms,
                entrance,
                exit,
                floor_specials: initialized.floor_specials,
                builder_attempts: attempts,
            };
        }
    }
}

/// Exact Sewer implementation of `RegularLevel.initRooms()`.
///
/// # Panics
///
/// Panics when called outside regular Sewer depths 1 through 4 or if a
/// version-pinned room table has no selectable entry.
#[must_use]
pub fn init_sewer_rooms(
    run: &mut RunState,
    limited_drops: &mut LimitedDrops,
    depth: u32,
    feeling: Feeling,
    random: &mut RandomStack,
) -> InitializedRegularRooms {
    assert!((1..=4).contains(&depth), "Sewer regular depths are 1..=4");
    let depth_i32 = i32::try_from(depth).expect("Sewer depth fits i32");
    let mut rooms = Vec::new();

    rooms.push(create_entrance_room(depth, random));
    rooms.push(create_exit_room(depth, random));

    let standards = if feeling == Feeling::Large {
        // standardRooms(true) == 6, then ceil(6 * 1.5f).
        9
    } else {
        4 + i32::try_from(
            random
                .chances(&[1.0, 3.0, 1.0])
                .expect("Sewer standard-room weights are positive"),
        )
        .expect("fixed chance index fits i32")
    };
    let mut consumed_value = 0_i32;
    while consumed_value < standards {
        let remaining = standards.wrapping_sub(consumed_value);
        let room = loop {
            let mut candidate = create_standard_room(depth, random);
            if candidate.set_size_category_for_value(remaining, random) {
                break candidate;
            }
        };
        consumed_value = consumed_value.wrapping_add(room.size_factor());
        rooms.push(room);
    }

    // Dungeon.shopOnLevel() is false on Sewer floors.
    let mut special_count = if feeling == Feeling::Large {
        // specialRooms(true) == 2, plus one for LARGE.
        3
    } else {
        1 + i32::try_from(
            random
                .chances(&[1.0, 4.0])
                .expect("Sewer special-room weights are positive"),
        )
        .expect("fixed chance index fits i32")
    };
    let mut floor_specials =
        begin_special_floor(&run.special_rooms, limited_drops, depth_i32, random);
    let mut selected_specials = 0_i32;
    while selected_specials < special_count {
        let selected = select_special_room(
            &mut run.special_rooms,
            &mut floor_specials,
            depth_i32,
            random,
        );
        if selected == SelectedSpecialRoom::Pit {
            special_count = special_count.wrapping_add(1);
        }
        rooms.push(Room::special(graph_special_kind(selected)));
        selected_specials = selected_specials.wrapping_add(1);
    }

    let mut secret_count =
        secrets_for_floor(&mut run.secret_rooms.region_secrets, depth_i32, random);
    if feeling == Feeling::Secrets {
        secret_count = secret_count.wrapping_add(1);
    }
    for _ in 0..secret_count {
        let selected = select_secret_room(&mut run.secret_rooms.run_secrets, random);
        rooms.push(Room::secret(graph_secret_kind(selected)));
    }

    random.shuffle_list(&mut rooms);
    let entrance = rooms
        .iter()
        .position(Room::is_entrance)
        .expect("initRooms always includes an entrance");
    let exit = rooms
        .iter()
        .position(Room::is_exit)
        .expect("initRooms always includes an exit");

    InitializedRegularRooms {
        rooms,
        entrance,
        exit,
        floor_specials,
    }
}

const fn graph_special_kind(selected: SelectedSpecialRoom) -> GraphSpecialRoom {
    match selected {
        SelectedSpecialRoom::Laboratory => GraphSpecialRoom::Laboratory,
        SelectedSpecialRoom::Pit => GraphSpecialRoom::Pit,
        SelectedSpecialRoom::Scheduled(kind) => match kind {
            RunSpecialRoom::Equipment(kind) => match kind {
                EquipmentSpecialRoom::WeakFloor => GraphSpecialRoom::WeakFloor,
                EquipmentSpecialRoom::Crypt => GraphSpecialRoom::Crypt,
                EquipmentSpecialRoom::Pool => GraphSpecialRoom::Pool,
                EquipmentSpecialRoom::Armory => GraphSpecialRoom::Armory,
                EquipmentSpecialRoom::Sentry => GraphSpecialRoom::Sentry,
                EquipmentSpecialRoom::Statue => GraphSpecialRoom::Statue,
                EquipmentSpecialRoom::CrystalVault => GraphSpecialRoom::CrystalVault,
                EquipmentSpecialRoom::CrystalChoice => GraphSpecialRoom::CrystalChoice,
                EquipmentSpecialRoom::Sacrifice => GraphSpecialRoom::Sacrifice,
            },
            RunSpecialRoom::Consumable(kind) => match kind {
                ConsumableSpecialRoom::Runestone => GraphSpecialRoom::Runestone,
                ConsumableSpecialRoom::Garden => GraphSpecialRoom::Garden,
                ConsumableSpecialRoom::Library => GraphSpecialRoom::Library,
                ConsumableSpecialRoom::Storage => GraphSpecialRoom::Storage,
                ConsumableSpecialRoom::Treasury => GraphSpecialRoom::Treasury,
                ConsumableSpecialRoom::MagicWell => GraphSpecialRoom::MagicWell,
                ConsumableSpecialRoom::ToxicGas => GraphSpecialRoom::ToxicGas,
                ConsumableSpecialRoom::MagicalFire => GraphSpecialRoom::MagicalFire,
                ConsumableSpecialRoom::Traps => GraphSpecialRoom::Traps,
                ConsumableSpecialRoom::CrystalPath => GraphSpecialRoom::CrystalPath,
            },
        },
    }
}

const fn graph_secret_kind(selected: RunSecretRoom) -> GraphSecretRoom {
    match selected {
        RunSecretRoom::Garden => GraphSecretRoom::Garden,
        RunSecretRoom::Laboratory => GraphSecretRoom::Laboratory,
        RunSecretRoom::Library => GraphSecretRoom::Library,
        RunSecretRoom::Larder => GraphSecretRoom::Larder,
        RunSecretRoom::Well => GraphSecretRoom::Well,
        RunSecretRoom::Runestone => GraphSecretRoom::Runestone,
        RunSecretRoom::Artillery => GraphSecretRoom::Artillery,
        RunSecretRoom::ChestChasm => GraphSecretRoom::ChestChasm,
        RunSecretRoom::Honeypot => GraphSecretRoom::Honeypot,
        RunSecretRoom::Hoard => GraphSecretRoom::Hoard,
        RunSecretRoom::Maze => GraphSecretRoom::Maze,
        RunSecretRoom::Summoning => GraphSecretRoom::Summoning,
    }
}

#[cfg(test)]
mod tests {
    use crate::generator::{FoodKind, GeneratedItem};
    use crate::level::Level;
    use crate::painter::normalize_rooms;
    use crate::rng::{RandomStack, seed_for_depth};
    use crate::room::RoomKind;
    use crate::run::initialize_run;
    use crate::seed::DungeonSeed;

    use super::*;

    fn normalized_floor_one_graph(seed: u64) -> ((i32, i32), Vec<String>) {
        let mut run = initialize_run(i64::try_from(seed).unwrap());
        let mut limited = LimitedDrops::default();
        let mut random = RandomStack::with_base_seed(0);
        random.push(seed_for_depth(i64::try_from(seed).unwrap(), 1, 0));
        let prepared = prepare_regular_floor(&mut run, &mut limited, 1, &mut random).unwrap();
        let mut graph =
            build_sewer_room_graph(&mut run, &mut limited, 1, prepared.feeling, &mut random);
        let mut level = Level::new(1, prepared.feeling);
        normalize_rooms(&mut level, &mut graph.rooms);
        let mut signatures: Vec<_> = graph
            .rooms
            .iter()
            .map(|room| {
                format!(
                    "{:?}@{},{},{},{}",
                    room.kind,
                    room.bounds.left,
                    room.bounds.top,
                    room.bounds.right,
                    room.bounds.bottom
                )
            })
            .collect();
        signatures.sort();
        ((level.width(), level.height()), signatures)
    }

    #[test]
    fn aaa_floor_one_preparation_and_graph_match_official_oracle() {
        let mut run = initialize_run(0);
        let mut limited = LimitedDrops::default();
        let mut random = RandomStack::with_base_seed(0);
        random.push(seed_for_depth(0, 1, 0));

        let prepared = prepare_regular_floor(&mut run, &mut limited, 1, &mut random).unwrap();
        assert_eq!(prepared.feeling, Feeling::None);
        assert_eq!(prepared.mandatory_drops, MandatoryDrops::default());
        assert_eq!(
            prepared.queued_generated_items,
            [GeneratedItem::Food(FoodKind::Ration)]
        );

        let mut graph =
            build_sewer_room_graph(&mut run, &mut limited, 1, prepared.feeling, &mut random);
        let mut level = Level::new(1, prepared.feeling);
        normalize_rooms(&mut level, &mut graph.rooms);
        assert_eq!((level.width(), level.height()), (40, 30));

        let mut signatures: Vec<_> = graph
            .rooms
            .iter()
            .map(|room| {
                format!(
                    "{:?}@{},{},{},{}",
                    room.kind,
                    room.bounds.left,
                    room.bounds.top,
                    room.bounds.right,
                    room.bounds.bottom
                )
            })
            .collect();
        signatures.sort();
        assert_eq!(
            signatures,
            [
                "Connection(Tunnel)@24,20,32,28",
                "Connection(Tunnel)@24,9,31,15",
                "Connection(Tunnel)@32,19,35,24",
                "Connection(Tunnel)@7,9,11,17",
                "Entrance(RegionDecoPatch)@2,1,11,9",
                "Exit(WaterBridge)@31,12,38,19",
                "Special(Pool)@31,6,38,12",
                "Special(Runestone)@3,17,10,23",
                "Standard(RegionDecoPatch)@11,1,16,9",
                "Standard(SewerPipe)@1,9,7,17",
                "Standard(SewerPipe)@11,9,24,20",
                "Standard(SewerPipe)@16,20,24,27",
            ]
        );
        assert_eq!(
            graph.rooms[graph.entrance].kind,
            RoomKind::Entrance(crate::room::StandardRoomKind::RegionDecoPatch)
        );
        assert_eq!(
            graph.rooms[graph.exit].kind,
            RoomKind::Exit(crate::room::StandardRoomKind::WaterBridge)
        );
    }

    #[test]
    fn forbidden_runes_omits_every_second_scheduled_upgrade_scroll() {
        let mut normal = RunState::new(0);
        let mut forbidden = RunState::with_challenges(0, crate::challenges::Challenges::NO_SCROLLS);
        let mut normal_limited = LimitedDrops::default();
        let mut forbidden_limited = LimitedDrops::default();
        let mut normal_presence = Vec::new();
        let mut forbidden_presence = Vec::new();
        for depth in 1..=4 {
            let root = seed_for_depth(0, depth, 0);
            let mut normal_random = RandomStack::with_base_seed(0);
            let mut forbidden_random = RandomStack::with_base_seed(0);
            normal_random.push(root);
            forbidden_random.push(root);
            normal_presence.push(
                prepare_regular_floor(&mut normal, &mut normal_limited, depth, &mut normal_random)
                    .unwrap()
                    .mandatory_drops
                    .upgrade_scroll,
            );
            forbidden_presence.push(
                prepare_regular_floor(
                    &mut forbidden,
                    &mut forbidden_limited,
                    depth,
                    &mut forbidden_random,
                )
                .unwrap()
                .mandatory_drops
                .upgrade_scroll,
            );
        }
        assert_eq!(normal_presence, [false, true, true, true]);
        assert_eq!(forbidden_presence, [false, true, false, true]);
        assert_eq!(normal_limited.upgrade_scrolls, 3);
        assert_eq!(forbidden_limited.upgrade_scrolls, 3);
    }

    #[test]
    #[allow(clippy::too_many_lines)] // Three complete official room-graph fixtures.
    fn additional_floor_one_graphs_match_official_oracle() {
        let fixtures: [(&str, (i32, i32), &[&str]); 3] = [
            (
                "AAA-AAA-AAB",
                (37, 38),
                &[
                    "Connection(RingBridge)@13,11,20,15",
                    "Connection(Tunnel)@2,26,9,29",
                    "Connection(Tunnel)@20,15,23,20",
                    "Connection(Tunnel)@21,1,26,6",
                    "Connection(Tunnel)@23,13,27,21",
                    "Entrance(RegionDecoPatch)@15,2,21,11",
                    "Exit(WaterBridge)@27,8,35,15",
                    "Special(CrystalPath)@20,28,28,36",
                    "Special(Runestone)@1,17,9,26",
                    "Standard(Ring)@20,21,28,28",
                    "Standard(Ring)@9,15,20,28",
                    "Standard(WaterBridge)@26,1,33,8",
                ],
            ),
            (
                "ABC-DEF-GHI",
                (38, 28),
                &[
                    "Connection(RingTunnel)@9,1,14,9",
                    "Connection(Tunnel)@18,18,20,24",
                    "Connection(Tunnel)@24,6,28,15",
                    "Connection(Tunnel)@28,10,34,15",
                    "Connection(Tunnel)@7,12,14,18",
                    "Entrance(WaterBridge)@1,4,9,10",
                    "Exit(RegionDecoPatch)@28,15,36,23",
                    "Special(MagicalFire)@10,18,18,26",
                    "Standard(RegionDecoPatch)@14,2,23,7",
                    "Standard(SewerPipe)@14,7,24,18",
                    "Standard(SewerPipe)@20,18,28,26",
                    "Standard(WaterBridge)@3,10,7,16",
                ],
            ),
            (
                "ZZZ-ZZZ-ZZZ",
                (32, 42),
                &[
                    "Connection(Tunnel)@11,22,17,31",
                    "Connection(Tunnel)@13,2,17,6",
                    "Connection(Tunnel)@17,1,21,6",
                    "Connection(Tunnel)@23,11,30,15",
                    "Connection(Tunnel)@6,20,11,22",
                    "Connection(Tunnel)@6,27,11,32",
                    "Entrance(RegionDecoPatch)@8,11,15,20",
                    "Exit(WaterBridge)@21,4,29,11",
                    "Special(Armory)@15,11,21,15",
                    "Special(Storage)@4,32,11,40",
                    "Standard(CircleBasin)@17,15,29,27",
                    "Standard(Platform)@4,22,11,27",
                    "Standard(SewerPipe)@1,12,8,20",
                    "Standard(WaterBridge)@10,6,15,11",
                ],
            ),
        ];

        for (code, expected_size, expected_rooms) in fixtures {
            let seed = DungeonSeed::from_code(code).unwrap().value();
            let (actual_size, actual_rooms) = normalized_floor_one_graph(seed);
            assert_eq!(actual_size, expected_size, "{code}");
            assert_eq!(actual_rooms, expected_rooms, "{code}");
        }
    }

    #[test]
    fn regional_init_hooks_add_shops_quests_and_halls_spawner() {
        for (depth, expected_region) in [
            (1, MainRegion::Sewers),
            (6, MainRegion::Prison),
            (14, MainRegion::Caves),
            (19, MainRegion::City),
            (21, MainRegion::Halls),
        ] {
            let mut run = initialize_run(321);
            let mut limited = LimitedDrops::default();
            let mut quests = QuestState::new();
            if depth == 6 {
                quests.wandmaker.quest_type = Some(WandmakerQuestType::ElementalEmbers);
            }
            let mut random = RandomStack::with_base_seed(0);
            random.push(700 + i64::from(depth));
            let initialized = init_regular_rooms(
                &mut run,
                &mut limited,
                &mut quests,
                depth,
                Feeling::None,
                &mut random,
            )
            .unwrap();

            assert_eq!(MainRegion::for_regular_depth(depth), expected_region);
            assert_eq!(
                initialized
                    .rooms
                    .iter()
                    .filter(|room| room.is_entrance())
                    .count(),
                1
            );
            assert_eq!(
                initialized
                    .rooms
                    .iter()
                    .filter(|room| room.is_exit())
                    .count(),
                1
            );
            assert_eq!(
                initialized
                    .rooms
                    .iter()
                    .any(|room| room.kind == RoomKind::Special(GraphSpecialRoom::Shop)),
                matches!(depth, 6 | 11 | 16)
            );

            let expected_quest = match depth {
                6 => Some(QuestRoomKind::RitualSite),
                14 => Some(QuestRoomKind::Blacksmith),
                19 => Some(QuestRoomKind::AmbitiousImp),
                _ => None,
            };
            assert_eq!(
                initialized.rooms.iter().find_map(|room| match room.kind {
                    RoomKind::Quest(kind) => Some(kind),
                    _ => None,
                }),
                expected_quest
            );
            assert_eq!(
                initialized
                    .rooms
                    .iter()
                    .any(|room| { room.kind == RoomKind::Special(GraphSpecialRoom::DemonSpawner) }),
                depth == 21
            );
        }
    }
}
