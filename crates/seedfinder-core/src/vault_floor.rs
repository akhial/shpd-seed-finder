//! Exact v4.0.0 generation of the Imp's Vault sub-level (`VaultLevel`,
//! `Dungeon.branch == 1` at the Imp's depth).
//!
//! `Level.create()` pushes `Dungeon.seedForDepth(depth, 1)` and, because the
//! branch is not zero, skips every run-global mutation: no food, no limited
//! drops, no feeling roll, no `Generator` deck use, no mob rotation. The
//! vault is therefore a pure function of the dungeon seed, the Imp's depth,
//! and the challenge mask, and can be generated after the main prefix. The
//! phases are `VaultLevel.build()` (loot queue, `initRooms`, `GridBuilder`,
//! `CityPainter` with no traps and no hidden doors), `buildFlagMaps`, and the
//! overridden `createItems` which scatters `itemsToSpawn` as plain heaps.

// Ports of upstream methods keep their unchecked Java invariants.
#![allow(clippy::missing_panics_doc)]

use std::fmt;

use crate::challenges::Challenges;
use crate::city_rooms::decorate_city;
use crate::geometry::{Point, terrain};
use crate::java_math::{div_i32, rem_i32};
use crate::level::Level;
use crate::level_flags::LevelFlags;
use crate::level_prelude::Feeling;
use crate::model::{Accessibility, ItemSource, WorldItem};
use crate::painter::generate_patch;
use crate::rng::{RandomStack, seed_for_depth};
use crate::room::Room;
use crate::vault_loot::{
    VaultConsumables, VaultEquipmentLoot, VaultItem, VaultSpawnQueue, random_food_using_defaults,
};
use crate::vault_mobs::{VaultMob, VaultMobDeck, VaultMobKind};
use crate::vault_paint::paint_room;
use crate::vault_rooms::{
    Door, DoorType, VaultRoom, VaultRoomChances, VaultRoomKind, force_shared_door_type,
    generate_treasure_room_list, set_shared_door,
};

/// `Heap.Type` values the vault produces.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum VaultHeapKind {
    Heap,
    Chest,
}

/// One heap, with its items in Java `LinkedList` order (newest first).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultHeap {
    pub cell: usize,
    pub kind: VaultHeapKind,
    pub items: Vec<VaultItem>,
}

/// Why the vault could not be generated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VaultError {
    /// The Imp only appears on City depths 17 through 19.
    InvalidDepth(u8),
    /// `RegularPainter.placeDoors` found no legal door point for an edge.
    NoDoorCandidate { room: usize, neighbour: usize },
    /// `randomDropCell` exhausted its hundred attempts.
    NoDropCell,
}

impl fmt::Display for VaultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDepth(depth) => {
                write!(
                    formatter,
                    "the Imp's vault depth must be 17..=19, got {depth}"
                )
            }
            Self::NoDoorCandidate { room, neighbour } => write!(
                formatter,
                "vault rooms {room} and {neighbour} have no valid door point"
            ),
            Self::NoDropCell => formatter.write_str("vault item placement found no drop cell"),
        }
    }
}

impl std::error::Error for VaultError {}

/// Mutable `VaultLevel` state visible to room painting.
#[derive(Clone, Debug, PartialEq)]
pub struct VaultLevelState {
    pub depth: u32,
    pub challenges: Challenges,
    /// Map, `heaps`/`mobs` occupancy, and the shuffled room list order.
    pub level: Level,
    pub equipment: VaultEquipmentLoot,
    pub consumables: VaultConsumables,
    pub mob_deck: VaultMobDeck,
    pub queue: VaultSpawnQueue,
    pub heaps: Vec<VaultHeap>,
    pub mobs: Vec<VaultMob>,
    /// Cells given a `VaultFlameTrap`, in setup order.
    pub flame_traps: Vec<usize>,
    /// The `BRANCH_ENTRANCE` transition cell.
    pub entrance_cell: Option<usize>,
}

impl VaultLevelState {
    fn new(depth: u32, challenges: Challenges) -> Self {
        Self {
            depth,
            challenges,
            level: Level::new(depth, Feeling::None),
            equipment: VaultEquipmentLoot::default(),
            consumables: VaultConsumables::default(),
            mob_deck: VaultMobDeck::default(),
            queue: VaultSpawnQueue::default(),
            heaps: Vec::new(),
            mobs: Vec::new(),
            flame_traps: Vec::new(),
            entrance_cell: None,
        }
    }

    #[must_use]
    pub const fn width(&self) -> i32 {
        self.level.map.width
    }

    #[must_use]
    pub fn point_to_cell(&self, point: Point) -> usize {
        self.level.point_to_cell(point)
    }

    /// Signed `x + y * width` for cells that may sit outside the map.
    #[must_use]
    pub fn cell_index(&self, x: i32, y: i32) -> i32 {
        x.wrapping_add(y.wrapping_mul(self.width()))
    }

    /// `Level.distance(a, b)` with Java's truncating division, which is what
    /// makes `distance(pos, -1)` well defined.
    #[must_use]
    pub fn distance(&self, a: i32, b: i32) -> i32 {
        let width = self.width();
        let ax = rem_i32(a, width);
        let ay = div_i32(a, width);
        let bx = rem_i32(b, width);
        let by = div_i32(b, width);
        (ax - bx).abs().max((ay - by).abs())
    }

    /// `Level.trueDistance(a, b)`.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // Java double-to-float cast.
    pub fn true_distance(&self, a: i32, b: i32) -> f32 {
        let width = self.width();
        let ax = rem_i32(a, width);
        let ay = div_i32(a, width);
        let bx = rem_i32(b, width);
        let by = div_i32(b, width);
        let dx = f64::from((ax - bx).abs());
        let dy = f64::from((ay - by).abs());
        (dx.powi(2) + dy.powi(2)).sqrt() as f32
    }

    #[must_use]
    pub fn heap_at(&self, cell: usize) -> Option<usize> {
        self.heaps.iter().position(|heap| heap.cell == cell)
    }

    #[must_use]
    pub fn find_mob(&self, cell: usize) -> bool {
        self.mobs.iter().any(|mob| mob.cell == cell)
    }

    /// `Level.drop(item, cell)`: a new plain heap, or `Heap.drop` onto the
    /// existing one (stackables merge, everything else goes to the front).
    /// Returns the heap index so callers can set `Heap.type`.
    pub fn drop(&mut self, item: VaultItem, cell: usize) -> usize {
        self.level.mark_heap(cell);
        if let Some(index) = self.heap_at(cell) {
            let heap = &mut self.heaps[index];
            let stackable = matches!(
                item,
                VaultItem::Dart
                    | VaultItem::Consumable(_)
                    | VaultItem::ShuffledConsumable { .. }
                    | VaultItem::Food(_)
                    | VaultItem::DwarfToken
                    | VaultItem::Torch
            );
            if stackable && heap.items.contains(&item) {
                heap.items.retain(|existing| *existing != item);
            }
            heap.items.insert(0, item);
            index
        } else {
            self.heaps.push(VaultHeap {
                cell,
                kind: VaultHeapKind::Heap,
                items: vec![item],
            });
            self.heaps.len() - 1
        }
    }

    pub fn set_heap_kind(&mut self, heap: usize, kind: VaultHeapKind) {
        self.heaps[heap].kind = kind;
    }

    pub fn add_mob(&mut self, kind: VaultMobKind, cell: usize) {
        self.level.mark_mob(cell);
        self.mobs.push(VaultMob { kind, cell });
    }

    /// `VaultLevel.VaultFlameTrap.setupTrap`: the blob bookkeeping draws
    /// nothing; the visible effect is the inactive trap tile.
    pub fn setup_flame_trap(&mut self, cell: usize) {
        self.level.map.cells[cell] = terrain::INACTIVE_TRAP;
        self.flame_traps.push(cell);
    }
}

/// A generated Imp Vault.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedVault {
    pub depth: u8,
    pub challenges: Challenges,
    pub level: Level,
    /// `RegularLevel.rooms` in its final (painter-shuffled) list order; the
    /// later `randomDropCell` shuffles live in `level.room_order`.
    pub rooms: Vec<VaultRoom>,
    pub mobs: Vec<VaultMob>,
    pub heaps: Vec<VaultHeap>,
    pub flame_traps: Vec<usize>,
    pub entrance_cell: usize,
    pub builder_attempts: u32,
}

impl GeneratedVault {
    #[must_use]
    pub fn width(&self) -> i32 {
        self.level.width()
    }

    #[must_use]
    pub fn height(&self) -> i32 {
        self.level.height()
    }

    /// `RegularLevel.room(cell)`: the first listed room whose interior holds
    /// the cell.
    #[must_use]
    pub fn room_at(&self, cell: usize) -> Option<usize> {
        let point = self.level.map.cell_to_point(cell);
        self.rooms.iter().position(|room| room.inside(point))
    }

    /// Every searchable treasure heap item (melee and thrown weapons, armor,
    /// wands, rings) outside the final room, sorted by cell and then heap
    /// order, as one option each of the same choice group as the Imp's
    /// reward options: the Escape Crystal lets exactly one item leave.
    #[must_use]
    pub fn world_items(&self, depth: u8, group: u16, first_option: u8) -> Vec<WorldItem> {
        let mut heaps: Vec<&VaultHeap> = self.heaps.iter().collect();
        heaps.sort_by_key(|heap| heap.cell);
        let mut output = Vec::new();
        let mut option = first_option;
        for heap in heaps {
            if self
                .room_at(heap.cell)
                .is_some_and(|room| self.rooms[room].kind == VaultRoomKind::Final)
            {
                continue;
            }
            for item in &heap.items {
                let Some(equipment) = item.equipment() else {
                    continue;
                };
                output.push(WorldItem {
                    item: equipment.item,
                    upgrade: equipment.upgrade,
                    effect: equipment.effect,
                    cursed: false,
                    depth,
                    source: ItemSource::VaultTreasure,
                    accessibility: Accessibility::Choice { group, option },
                    secret: false,
                });
                option = option.wrapping_add(1);
            }
        }
        output
    }
}

/// Generates the Imp's Vault for a run.
///
/// # Errors
///
/// Returns [`VaultError::InvalidDepth`] outside 17..=19, or a structural
/// failure that upstream would report as an exception.
pub fn generate_vault(
    dungeon_seed: i64,
    depth: u8,
    challenges: Challenges,
) -> Result<GeneratedVault, VaultError> {
    generate_vault_with_trinket(
        dungeon_seed,
        depth,
        challenges,
        &crate::trinkets::TrinketEffects::default(),
    )
}

pub(crate) fn generate_vault_with_trinket(
    dungeon_seed: i64,
    depth: u8,
    challenges: Challenges,
    trinket: &crate::trinkets::TrinketEffects,
) -> Result<GeneratedVault, VaultError> {
    if !(17..=19).contains(&depth) {
        return Err(VaultError::InvalidDepth(depth));
    }
    let mut random = RandomStack::with_base_seed(0);
    random.trinket = trinket.clone();
    random.push(seed_for_depth(dungeon_seed, u32::from(depth), 1));
    let result = generate_vault_with_generator(u32::from(depth), challenges, &mut random);
    random.pop();
    result
}

/// `VaultLevel.create()` against an already pushed depth generator.
///
/// # Errors
///
/// Returns a structural failure that upstream would report as an exception.
pub fn generate_vault_with_generator(
    depth: u32,
    challenges: Challenges,
    random: &mut RandomStack,
) -> Result<GeneratedVault, VaultError> {
    let mut state = VaultLevelState::new(depth, challenges);
    let (rooms, attempts) = build(&mut state, random)?;
    let flags = LevelFlags::build_for_generation(&state.level.map);
    create_items(&mut state, &rooms, &flags, random)?;
    let entrance_cell = state
        .entrance_cell
        .expect("the entrance room registers its transition");
    Ok(GeneratedVault {
        depth: u8::try_from(depth).expect("vault depth fits u8"),
        challenges,
        level: state.level,
        rooms,
        mobs: state.mobs,
        heaps: state.heaps,
        flame_traps: state.flame_traps,
        entrance_cell,
        builder_attempts: attempts,
    })
}

/// `VaultLevel.build()` followed by `RegularLevel.build()`.
fn build(
    state: &mut VaultLevelState,
    random: &mut RandomStack,
) -> Result<(Vec<VaultRoom>, u32), VaultError> {
    state.queue.items.clear();
    for _ in 0..4 {
        let item = state.equipment.create_equipment(0, random);
        state.queue.add(VaultItem::Equipment(item));
    }
    state.queue.add(VaultItem::Dart);
    for _ in 0..5 {
        let item = state.consumables.create_consumable(0, random);
        state.queue.add(item);
    }
    for _ in 0..3 {
        state
            .queue
            .add(VaultItem::Food(random_food_using_defaults(random)));
    }

    // RegularLevel.build(): builder(), initRooms(), shuffle, retry loop.
    let mut init_rooms = init_rooms(random);
    random.shuffle_list(&mut init_rooms);
    let mut attempts = 0_u32;
    // Nearly half of all vaults need a second builder pass and a few need ten,
    // so the candidate list is kept across attempts: `clone_from` reuses both
    // its own buffer and each room's neighbour and connection vectors.
    let mut candidate: Vec<VaultRoom> = Vec::new();
    let mut rooms = loop {
        attempts = attempts.wrapping_add(1);
        for room in &mut init_rooms {
            room.neighbours.clear();
            room.connected.clear();
        }
        candidate.clone_from(&init_rooms);
        if crate::grid_builder::build_grid(&mut candidate, random) {
            break candidate;
        }
        // Java keeps the same room objects: failed placement leaves their
        // sizes behind, which setEmpty() resets on the next attempt, and
        // the long rooms' `wide` fields persist through the clone.
        std::mem::swap(&mut init_rooms, &mut candidate);
    };
    paint(state, &mut rooms, random)?;
    Ok((rooms, attempts))
}

/// `VaultLevel.initRooms()`.
fn init_rooms(random: &mut RandomStack) -> Vec<VaultRoom> {
    let mut rooms = vec![VaultRoom::standard(VaultRoomKind::Entrance, random)];
    let mut chances = VaultRoomChances::setup();
    let mut size = 0_i32;
    while size < 9 {
        let room = chances.create_room(random);
        size += room.kind.size_factor();
        rooms.push(room);
    }
    rooms.push(VaultRoom::standard(VaultRoomKind::Tokens, random));
    rooms.push(VaultRoom::standard(
        VaultRoomKind::SimpleEnemyTreasure,
        random,
    ));
    let treasures = generate_treasure_room_list(random);
    for kind in treasures.into_iter().take(7) {
        rooms.push(VaultRoom::standard(kind, random));
    }
    rooms.push(VaultRoom::final_room());
    rooms
}

/// `CityPainter` with `hiddenDoorChance` 0, water 0.15/12, grass 0.3/3 and
/// no traps: `RegularPainter.paint`.
fn paint(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    random: &mut RandomStack,
) -> Result<(), VaultError> {
    normalize_rooms(state, rooms);

    let mut order: Vec<usize> = (0..rooms.len()).collect();
    random.shuffle_list(&mut order);
    state.level.room_order.clone_from(&order);

    for &room in &order {
        place_doors(rooms, room, random)?;
        paint_room(state, rooms, room, random);
    }

    paint_doors(state, rooms, &order, random);

    let child_seed = random.long();
    random.push(child_seed);
    paint_water(state, rooms, &order, random);
    paint_grass(state, rooms, &order, random);
    let no_rooms: &[Room] = &[];
    decorate_city(&mut state.level, no_rooms, &[], random);
    random.pop();
    Ok(())
}

fn normalize_rooms(state: &mut VaultLevelState, rooms: &mut [VaultRoom]) {
    let padding = 1;
    let mut left_most = i32::MAX;
    let mut top_most = i32::MAX;
    for room in rooms.iter() {
        left_most = left_most.min(room.bounds.left);
        top_most = top_most.min(room.bounds.top);
    }
    left_most -= padding;
    top_most -= padding;
    let mut right_most = 0_i32;
    let mut bottom_most = 0_i32;
    for room in rooms.iter_mut() {
        room.shift(-left_most, -top_most);
        right_most = right_most.max(room.bounds.right);
        bottom_most = bottom_most.max(room.bounds.bottom);
    }
    right_most += padding;
    bottom_most += padding;
    state.level.set_size(right_most + 1, bottom_most + 1);
}

/// `RegularPainter.placeDoors(r)`.
fn place_doors(
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) -> Result<(), VaultError> {
    let neighbours: Vec<usize> = rooms[room]
        .connected
        .iter()
        .map(|entry| entry.room)
        .collect();
    for neighbour in neighbours {
        if rooms[room]
            .connection_to(neighbour)
            .and_then(|entry| entry.door)
            .is_some()
        {
            continue;
        }
        let intersection = rooms[room].bounds.intersect(rooms[neighbour].bounds);
        let candidates: Vec<Point> = intersection
            .points()
            .filter(|&point| {
                rooms[room].can_connect_point(point) && rooms[neighbour].can_connect_point(point)
            })
            .collect();
        if candidates.is_empty() {
            return Err(VaultError::NoDoorCandidate { room, neighbour });
        }
        let bound = i32::try_from(candidates.len()).expect("door candidates fit Java int");
        let index = usize::try_from(random.int_bound(bound)).expect("Random.Int is non-negative");
        set_shared_door(rooms, room, neighbour, Door::new(candidates[index]));
    }
    Ok(())
}

/// `RegularPainter.paintDoors` with `hiddenDoorChance == 0`: no vault room
/// can merge, every `REGULAR` door still costs one float, and all of them
/// become unlocked doors.
fn paint_doors(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    order: &[usize],
    random: &mut RandomStack,
) {
    for &room in order {
        let neighbours: Vec<usize> = rooms[room]
            .connected
            .iter()
            .map(|entry| entry.room)
            .collect();
        for neighbour in neighbours {
            let door = rooms[room]
                .connection_to(neighbour)
                .and_then(|entry| entry.door)
                .expect("doors are placed before painting");
            if door.door_type == DoorType::Regular {
                let _ = random.float();
                force_shared_door_type(rooms, room, neighbour, DoorType::Unlocked);
            }
            let door = rooms[room]
                .connection_to(neighbour)
                .and_then(|entry| entry.door)
                .expect("doors are placed before painting");
            let cell = state.point_to_cell(door.point);
            let tile = match door.door_type {
                DoorType::Empty | DoorType::Tunnel => Some(terrain::EMPTY),
                DoorType::Water => Some(terrain::WATER),
                DoorType::Regular => None,
                DoorType::Unlocked => Some(terrain::DOOR),
                DoorType::Hidden => Some(terrain::SECRET_DOOR),
                DoorType::Barricade => Some(terrain::BARRICADE),
                DoorType::Locked => Some(terrain::LOCKED_DOOR),
                DoorType::Crystal => Some(terrain::CRYSTAL_DOOR),
                DoorType::Wall => Some(terrain::WALL),
            };
            if let Some(tile) = tile {
                state.level.map.cells[cell] = tile;
            }
        }
    }
}

fn paint_water(
    state: &mut VaultLevelState,
    rooms: &[VaultRoom],
    order: &[usize],
    random: &mut RandomStack,
) {
    let lake = generate_patch(
        state.level.width(),
        state.level.height(),
        0.15,
        12,
        true,
        random,
    );
    for &room in order {
        if !rooms[room].kind.can_place_water() {
            continue;
        }
        for point in rooms[room].bounds.points() {
            let cell = state.point_to_cell(point);
            if lake[cell] && state.level.map.cells[cell] == terrain::EMPTY {
                state.level.map.cells[cell] = terrain::WATER;
            }
        }
    }
}

fn paint_grass(
    state: &mut VaultLevelState,
    rooms: &[VaultRoom],
    order: &[usize],
    random: &mut RandomStack,
) {
    let grass = generate_patch(
        state.level.width(),
        state.level.height(),
        0.3,
        3,
        true,
        random,
    );
    let mut grass_cells = Vec::new();
    for &room in order {
        if !rooms[room].kind.can_place_grass() {
            continue;
        }
        for point in rooms[room].bounds.points() {
            let cell = state.point_to_cell(point);
            if grass[cell] && state.level.map.cells[cell] == terrain::EMPTY {
                grass_cells.push(cell);
            }
        }
    }
    let width = state.level.width();
    let neighbours = [
        -width - 1,
        -width,
        -width + 1,
        -1,
        1,
        width - 1,
        width,
        width + 1,
    ];
    for cell in grass_cells {
        if state.heap_at(cell).is_some() || state.find_mob(cell) {
            state.level.map.cells[cell] = terrain::GRASS;
            continue;
        }
        let cell_i32 = i32::try_from(cell).expect("level map fits Java int");
        let mut count = 1_i32;
        for offset in neighbours {
            let neighbour =
                usize::try_from(cell_i32.wrapping_add(offset)).expect("grass cell is interior");
            if grass[neighbour] {
                count += 1;
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let high_grass_chance = count as f32 / 12.0_f32;
        state.level.map.cells[cell] = if random.float() < high_grass_chance {
            terrain::HIGH_GRASS
        } else {
            terrain::GRASS
        };
    }
}

/// `Room.canPlaceItem` with the vault overrides. `VaultAlternatingFireRoom`
/// compares `p == this.center()` by reference, which is never true.
fn can_place_item(state: &VaultLevelState, room: &VaultRoom, point: Point) -> bool {
    match room.kind {
        VaultRoomKind::Final | VaultRoomKind::AlternatingFire => false,
        kind if kind.is_treasure() => false,
        VaultRoomKind::Tokens => {
            room.inside(point)
                && state.level.map.cells[state.point_to_cell(point)] != terrain::EMPTY_SP
        }
        VaultRoomKind::SimpleEnemyTreasure | VaultRoomKind::EnemyCenter => {
            let center = room.center();
            if (center.x - point.x).abs() <= 2 || (center.y - point.y).abs() <= 2 {
                false
            } else {
                room.inside(point)
            }
        }
        _ => room.inside(point),
    }
}

/// `RegularLevel.randomDropCell(StandardRoom.class)`.
fn random_drop_cell(
    state: &mut VaultLevelState,
    rooms: &[VaultRoom],
    flags: &LevelFlags,
    random: &mut RandomStack,
) -> Option<usize> {
    for _ in 0..100 {
        random.shuffle_list(&mut state.level.room_order);
        let room = state
            .level
            .room_order
            .iter()
            .copied()
            .find(|&room| rooms[room].kind.is_standard())?;
        if rooms[room].is_entrance() {
            continue;
        }
        let point = rooms[room].random_point(1, random);
        let cell = state.point_to_cell(point);
        if flags.passable[cell]
            && !flags.solid[cell]
            && cell != 0
            && state.heap_at(cell).is_none()
            && can_place_item(state, &rooms[room], point)
            && !state.find_mob(cell)
        {
            return Some(cell);
        }
    }
    None
}

/// `VaultLevel.createItems()`.
fn create_items(
    state: &mut VaultLevelState,
    rooms: &[VaultRoom],
    flags: &LevelFlags,
    random: &mut RandomStack,
) -> Result<(), VaultError> {
    let items = std::mem::take(&mut state.queue.items);
    for item in items {
        let cell = random_drop_cell(state, rooms, flags, random).ok_or(VaultError::NoDropCell)?;
        let heap = state.drop(item, cell);
        state.set_heap_kind(heap, VaultHeapKind::Heap);
        clear_grass(state, cell);
    }
    if state.challenges.contains(Challenges::DARKNESS) {
        let child_seed = random.long();
        random.push(child_seed);
        for _ in 0..2 {
            let cell = random_drop_cell(state, rooms, flags, random);
            let Some(cell) = cell else {
                random.pop();
                return Err(VaultError::NoDropCell);
            };
            let heap = state.drop(VaultItem::Torch, cell);
            state.set_heap_kind(heap, VaultHeapKind::Heap);
            clear_grass(state, cell);
        }
        random.pop();
    }
    Ok(())
}

fn clear_grass(state: &mut VaultLevelState, cell: usize) {
    if matches!(
        state.level.map.cells[cell],
        terrain::HIGH_GRASS | terrain::FURROWED_GRASS
    ) {
        state.level.map.cells[cell] = terrain::GRASS;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{ArmorEffect, ItemId, WeaponEffect};
    use crate::seed::DungeonSeed;
    use crate::vault_loot::VaultItem;

    fn seed(code: &str) -> i64 {
        i64::try_from(DungeonSeed::from_code(code).unwrap().value()).unwrap()
    }

    /// One official v4.0.0-BETA-3 vault, captured with the headless probe
    /// (`tooling/oracle-4.0/.work/probe/VaultProbe.java` extended to print
    /// mobs) after generating floors 1 through the Imp's depth and then
    /// `Dungeon.branch = 1; Dungeon.newLevel()`.
    struct VaultFixture {
        code: &'static str,
        depth: u8,
        size: (i32, i32),
        map_hash: i32,
        heaps: usize,
        entrance: usize,
        /// Every mob, sorted by cell.
        mobs: &'static [(usize, VaultMobKind)],
        /// Every weapon/armor/wand/ring heap outside the final room, sorted
        /// by cell: `(cell, heap type, class, level, enchantment/glyph)`.
        treasure: &'static [(usize, VaultHeapKind, ItemId, u8, Option<Effect>)],
    }

    use crate::catalog::Effect;

    const FIXTURES: &[VaultFixture] = &[
        VaultFixture {
            code: "AAA-AAA-AAA",
            depth: 19,
            size: (63, 53),
            map_hash: -743_826_665,
            heaps: 51,
            entrance: 2_914,
            mobs: &[
                (875, VaultMobKind::Laser),
                (879, VaultMobKind::Laser),
                (938, VaultMobKind::Laser),
                (942, VaultMobKind::Laser),
                (976, VaultMobKind::Mirror),
                (1_001, VaultMobKind::Laser),
                (1_005, VaultMobKind::Laser),
                (1_035, VaultMobKind::Dm100),
                (1_064, VaultMobKind::Laser),
                (1_068, VaultMobKind::Laser),
                (1_127, VaultMobKind::Laser),
                (1_131, VaultMobKind::Laser),
                (1_190, VaultMobKind::Laser),
                (1_194, VaultMobKind::Laser),
                (1_228, VaultMobKind::TokenDoor),
                (1_253, VaultMobKind::Laser),
                (1_257, VaultMobKind::Laser),
                (1_600, VaultMobKind::Shaman),
                (1_632, VaultMobKind::Dm200),
                (1_678, VaultMobKind::Ghoul),
                (2_038, VaultMobKind::Sentry),
                (2_042, VaultMobKind::Sentry),
                (2_046, VaultMobKind::Sentry),
                (2_063, VaultMobKind::FrostElemental),
                (2_098, VaultMobKind::Golem),
                (2_255, VaultMobKind::Dm100),
                (2_290, VaultMobKind::Sentry),
                (2_298, VaultMobKind::Sentry),
                (2_304, VaultMobKind::Sentry),
                (2_324, VaultMobKind::Sentry),
                (2_546, VaultMobKind::Sentry),
                (2_550, VaultMobKind::Sentry),
                (2_699, VaultMobKind::Laser),
                (2_700, VaultMobKind::Laser),
                (2_701, VaultMobKind::Laser),
                (2_702, VaultMobKind::Laser),
                (2_703, VaultMobKind::Laser),
                (2_704, VaultMobKind::Laser),
                (2_705, VaultMobKind::Laser),
                (2_939, VaultMobKind::Golem),
                (3_077, VaultMobKind::Laser),
                (3_078, VaultMobKind::Laser),
                (3_079, VaultMobKind::Laser),
                (3_080, VaultMobKind::Laser),
                (3_081, VaultMobKind::Laser),
                (3_082, VaultMobKind::Laser),
                (3_083, VaultMobKind::Laser),
                (3_117, VaultMobKind::Skeleton),
            ],
            treasure: &[
                (225, VaultHeapKind::Chest, ItemId::Katana, 2, None),
                (
                    814,
                    VaultHeapKind::Chest,
                    ItemId::BattleAxe,
                    4,
                    Some(Effect::Weapon(WeaponEffect::Blooming)),
                ),
                (930, VaultHeapKind::Chest, ItemId::Javelin, 2, None),
                (1_041, VaultHeapKind::Heap, ItemId::WandLivingEarth, 3, None),
                (
                    1_570,
                    VaultHeapKind::Chest,
                    ItemId::Whip,
                    3,
                    Some(Effect::Weapon(WeaponEffect::Kinetic)),
                ),
                (1_657, VaultHeapKind::Chest, ItemId::RingEvasion, 1, None),
                (1_664, VaultHeapKind::Chest, ItemId::RingArcana, 2, None),
                (1_670, VaultHeapKind::Heap, ItemId::LeatherArmor, 0, None),
                (1_999, VaultHeapKind::Heap, ItemId::Sickle, 0, None),
                (
                    2_036,
                    VaultHeapKind::Chest,
                    ItemId::Greatsword,
                    3,
                    Some(Effect::Weapon(WeaponEffect::Grim)),
                ),
                (2_072, VaultHeapKind::Chest, ItemId::WandFireblast, 1, None),
                (
                    2_294,
                    VaultHeapKind::Chest,
                    ItemId::PlateArmor,
                    3,
                    Some(Effect::Armor(ArmorEffect::Entanglement)),
                ),
                (
                    2_769,
                    VaultHeapKind::Chest,
                    ItemId::Spear,
                    2,
                    Some(Effect::Weapon(WeaponEffect::Corrupting)),
                ),
                (2_928, VaultHeapKind::Heap, ItemId::FishingSpear, 0, None),
                (2_939, VaultHeapKind::Heap, ItemId::HandAxe, 0, None),
            ],
        },
        VaultFixture {
            code: "AAA-AAA-AAB",
            depth: 18,
            size: (63, 53),
            map_hash: 1_059_305_718,
            heaps: 49,
            entrance: 1_694,
            mobs: &[
                (370, VaultMobKind::Dm200),
                (676, VaultMobKind::Mirror),
                (729, VaultMobKind::Shaman),
                (743, VaultMobKind::Skeleton),
                (778, VaultMobKind::FireElemental),
                (928, VaultMobKind::TokenDoor),
                (1_285, VaultMobKind::Dm100),
                (1_423, VaultMobKind::Laser),
                (1_425, VaultMobKind::Laser),
                (1_489, VaultMobKind::Laser),
                (1_544, VaultMobKind::Laser),
                (1_607, VaultMobKind::Laser),
                (1_664, VaultMobKind::Sentry),
                (1_670, VaultMobKind::Laser),
                (1_733, VaultMobKind::Laser),
                (1_867, VaultMobKind::Laser),
                (1_870, VaultMobKind::Skeleton),
                (1_926, VaultMobKind::Laser),
                (1_928, VaultMobKind::Laser),
                (2_038, VaultMobKind::Sentry),
                (2_042, VaultMobKind::Sentry),
                (2_046, VaultMobKind::Sentry),
                (2_138, VaultMobKind::ShockElemental),
                (2_242, VaultMobKind::Golem),
                (2_252, VaultMobKind::Ghoul),
                (2_290, VaultMobKind::Sentry),
                (2_298, VaultMobKind::Sentry),
                (2_542, VaultMobKind::Sentry),
                (2_546, VaultMobKind::Sentry),
                (2_761, VaultMobKind::Laser),
                (2_767, VaultMobKind::Laser),
                (2_824, VaultMobKind::Laser),
                (2_830, VaultMobKind::Laser),
                (2_887, VaultMobKind::Laser),
                (2_893, VaultMobKind::Laser),
                (2_934, VaultMobKind::Ghoul),
                (2_938, VaultMobKind::Ghoul),
                (2_950, VaultMobKind::Laser),
                (2_956, VaultMobKind::Laser),
                (2_999, VaultMobKind::Ghoul),
                (3_013, VaultMobKind::Laser),
                (3_019, VaultMobKind::Laser),
                (3_076, VaultMobKind::Laser),
                (3_082, VaultMobKind::Laser),
                (3_139, VaultMobKind::Laser),
                (3_145, VaultMobKind::Laser),
            ],
            treasure: &[
                (175, VaultHeapKind::Heap, ItemId::LeatherArmor, 0, None),
                (
                    391,
                    VaultHeapKind::Chest,
                    ItemId::AssassinsBlade,
                    2,
                    Some(Effect::Weapon(WeaponEffect::Blazing)),
                ),
                (
                    434,
                    VaultHeapKind::Chest,
                    ItemId::ScaleArmor,
                    2,
                    Some(Effect::Armor(ArmorEffect::Entanglement)),
                ),
                (719, VaultHeapKind::Heap, ItemId::Shortsword, 0, None),
                (729, VaultHeapKind::Heap, ItemId::Sickle, 0, None),
                (
                    737,
                    VaultHeapKind::Heap,
                    ItemId::Trident,
                    3,
                    Some(Effect::Weapon(WeaponEffect::Kinetic)),
                ),
                (1_129, VaultHeapKind::Chest, ItemId::Javelin, 2, None),
                (1_731, VaultHeapKind::Chest, ItemId::RingFuror, 1, None),
                (1_744, VaultHeapKind::Heap, ItemId::ThrowingClub, 0, None),
                (2_076, VaultHeapKind::Chest, ItemId::RingEvasion, 3, None),
                (
                    2_190,
                    VaultHeapKind::Chest,
                    ItemId::Mace,
                    3,
                    Some(Effect::Weapon(WeaponEffect::Unstable)),
                ),
                (
                    2_294,
                    VaultHeapKind::Chest,
                    ItemId::Longsword,
                    4,
                    Some(Effect::Weapon(WeaponEffect::Vorpal)),
                ),
                (
                    2_304,
                    VaultHeapKind::Chest,
                    ItemId::Glaive,
                    3,
                    Some(Effect::Weapon(WeaponEffect::Corrupting)),
                ),
                (
                    2_881,
                    VaultHeapKind::Chest,
                    ItemId::Spear,
                    2,
                    Some(Effect::Weapon(WeaponEffect::Blazing)),
                ),
                (3_125, VaultHeapKind::Chest, ItemId::WandCorrosion, 3, None),
                (3_205, VaultHeapKind::Chest, ItemId::WandFireblast, 1, None),
            ],
        },
        VaultFixture {
            code: "AAA-AAA-ABG",
            depth: 17,
            size: (53, 63),
            map_hash: -1_802_270_593,
            heaps: 46,
            entrance: 2_974,
            mobs: &[
                (1_233, VaultMobKind::Dm100),
                (1_278, VaultMobKind::Ghoul),
                (1_383, VaultMobKind::Ghoul),
                (1_490, VaultMobKind::Ghoul),
                (1_617, VaultMobKind::Ghoul),
                (1_698, VaultMobKind::Sentry),
                (1_702, VaultMobKind::Sentry),
                (1_910, VaultMobKind::Sentry),
                (1_918, VaultMobKind::Sentry),
                (1_924, VaultMobKind::Sentry),
                (1_945, VaultMobKind::Ghoul),
                (1_954, VaultMobKind::Sentry),
                (2_090, VaultMobKind::Golem),
                (2_122, VaultMobKind::Sentry),
                (2_126, VaultMobKind::Sentry),
                (2_130, VaultMobKind::Sentry),
                (2_259, VaultMobKind::Laser),
                (2_263, VaultMobKind::Laser),
                (2_264, VaultMobKind::Laser),
                (2_364, VaultMobKind::Laser),
                (2_392, VaultMobKind::FrostElemental),
                (2_417, VaultMobKind::Laser),
                (2_508, VaultMobKind::Dm200),
                (2_531, VaultMobKind::Laser),
                (2_584, VaultMobKind::Laser),
                (2_629, VaultMobKind::Laser),
                (2_684, VaultMobKind::Laser),
                (2_686, VaultMobKind::Laser),
                (2_689, VaultMobKind::Laser),
                (2_696, VaultMobKind::Mirror),
                (2_729, VaultMobKind::Shaman),
                (2_753, VaultMobKind::Skeleton),
                (2_841, VaultMobKind::Laser),
                (2_847, VaultMobKind::Laser),
                (2_894, VaultMobKind::Laser),
                (2_900, VaultMobKind::Laser),
                (2_908, VaultMobKind::TokenDoor),
                (2_947, VaultMobKind::Laser),
                (2_953, VaultMobKind::Laser),
                (2_984, VaultMobKind::Sentry),
                (3_000, VaultMobKind::Laser),
                (3_006, VaultMobKind::Laser),
                (3_053, VaultMobKind::Laser),
                (3_059, VaultMobKind::Laser),
                (3_106, VaultMobKind::Laser),
                (3_112, VaultMobKind::Laser),
                (3_159, VaultMobKind::Laser),
                (3_165, VaultMobKind::Laser),
            ],
            treasure: &[
                (980, VaultHeapKind::Chest, ItemId::WandMagicMissile, 1, None),
                (1_381, VaultHeapKind::Chest, ItemId::RingFuror, 3, None),
                (1_399, VaultHeapKind::Heap, ItemId::Quarterstaff, 0, None),
                (
                    1_416,
                    VaultHeapKind::Chest,
                    ItemId::HeavyBoomerang,
                    2,
                    Some(Effect::Weapon(WeaponEffect::Blazing)),
                ),
                (1_603, VaultHeapKind::Heap, ItemId::ThrowingClub, 0, None),
                (
                    1_914,
                    VaultHeapKind::Chest,
                    ItemId::Katana,
                    4,
                    Some(Effect::Weapon(WeaponEffect::Venomous)),
                ),
                (
                    1_946,
                    VaultHeapKind::Chest,
                    ItemId::RunicBlade,
                    2,
                    Some(Effect::Weapon(WeaponEffect::Lucky)),
                ),
                (2_082, VaultHeapKind::Heap, ItemId::LeatherArmor, 0, None),
                (2_113, VaultHeapKind::Chest, ItemId::Sickle, 2, None),
                (
                    2_444,
                    VaultHeapKind::Chest,
                    ItemId::Glaive,
                    3,
                    Some(Effect::Weapon(WeaponEffect::Blocking)),
                ),
                (
                    2_562,
                    VaultHeapKind::Chest,
                    ItemId::ScaleArmor,
                    2,
                    Some(Effect::Armor(ArmorEffect::Potential)),
                ),
                (2_729, VaultHeapKind::Heap, ItemId::Dirk, 0, None),
                (2_751, VaultHeapKind::Heap, ItemId::WandCorrosion, 3, None),
                (3_216, VaultHeapKind::Chest, ItemId::RingTenacity, 1, None),
            ],
        },
    ];

    fn treasure_records(
        vault: &GeneratedVault,
    ) -> Vec<(usize, VaultHeapKind, ItemId, u8, Option<Effect>)> {
        let mut records = Vec::new();
        let mut heaps: Vec<&VaultHeap> = vault.heaps.iter().collect();
        heaps.sort_by_key(|heap| heap.cell);
        for heap in heaps {
            let room = vault
                .room_at(heap.cell)
                .expect("vault heaps lie inside rooms");
            if vault.rooms[room].kind == VaultRoomKind::Final {
                continue;
            }
            for item in &heap.items {
                if let Some(equipment) = item.equipment() {
                    records.push((
                        heap.cell,
                        heap.kind,
                        equipment.item,
                        equipment.upgrade,
                        equipment.effect,
                    ));
                }
            }
        }
        records
    }

    #[test]
    fn rejects_depths_without_an_imp() {
        assert_eq!(
            generate_vault(0, 16, Challenges::NONE).map(|_| ()),
            Err(VaultError::InvalidDepth(16))
        );
        assert_eq!(
            generate_vault(0, 20, Challenges::NONE).map(|_| ()),
            Err(VaultError::InvalidDepth(20))
        );
    }

    #[test]
    fn official_vaults_match_map_mobs_and_treasure_heaps() {
        for fixture in FIXTURES {
            let vault = generate_vault(seed(fixture.code), fixture.depth, Challenges::NONE)
                .unwrap_or_else(|error| panic!("{}: {error}", fixture.code));
            assert_eq!(vault.depth, fixture.depth, "{}", fixture.code);
            assert_eq!(
                (vault.width(), vault.height()),
                fixture.size,
                "{}",
                fixture.code
            );
            assert_eq!(
                vault.level.java_map_hash(),
                fixture.map_hash,
                "{}",
                fixture.code
            );
            assert_eq!(vault.heaps.len(), fixture.heaps, "{}", fixture.code);
            assert_eq!(vault.entrance_cell, fixture.entrance, "{}", fixture.code);
            assert_eq!(vault.rooms.len(), 18, "{}", fixture.code);

            let mut mobs: Vec<(usize, VaultMobKind)> =
                vault.mobs.iter().map(|mob| (mob.cell, mob.kind)).collect();
            mobs.sort_unstable();
            assert_eq!(mobs, fixture.mobs, "{}", fixture.code);

            assert_eq!(
                treasure_records(&vault),
                fixture.treasure,
                "{}",
                fixture.code
            );

            // The final room holds the Imp statue plus the six reward
            // options and nothing else.
            let final_room = vault
                .rooms
                .iter()
                .position(|room| room.kind == VaultRoomKind::Final)
                .unwrap();
            let final_items: Vec<VaultItem> = vault
                .heaps
                .iter()
                .filter(|heap| vault.room_at(heap.cell) == Some(final_room))
                .flat_map(|heap| heap.items.iter().copied())
                .collect();
            assert_eq!(final_items.len(), 7, "{}", fixture.code);
            assert_eq!(
                final_items
                    .iter()
                    .filter(|item| **item == VaultItem::ImpStatue)
                    .count(),
                1
            );
            for option in 0..6_u8 {
                assert!(final_items.contains(&VaultItem::ImpRewardOption(option)));
            }
        }
    }

    #[test]
    fn world_items_are_one_choice_group_sorted_by_cell() {
        for fixture in FIXTURES {
            let vault =
                generate_vault(seed(fixture.code), fixture.depth, Challenges::NONE).unwrap();
            let items = vault.world_items(fixture.depth, 7, 6);
            assert_eq!(items.len(), fixture.treasure.len(), "{}", fixture.code);
            for (index, (item, expected)) in items.iter().zip(fixture.treasure).enumerate() {
                assert_eq!(item.item, expected.2, "{}", fixture.code);
                assert_eq!(item.upgrade, expected.3, "{}", fixture.code);
                assert_eq!(item.effect, expected.4, "{}", fixture.code);
                assert!(!item.cursed);
                assert!(!item.secret);
                assert_eq!(item.depth, fixture.depth);
                assert_eq!(item.source, ItemSource::VaultTreasure);
                assert_eq!(
                    item.accessibility,
                    Accessibility::Choice {
                        group: 7,
                        option: 6 + u8::try_from(index).unwrap(),
                    },
                    "{}",
                    fixture.code
                );
            }
        }
    }

    #[test]
    fn into_darkness_adds_the_entrance_torch_and_two_isolated_torch_drops() {
        // Official oracle with challenges = 32 (Into Darkness) for AAA-AAA-AAA:
        // the map hash is unchanged because the extra drops come from an
        // isolated child generator and the torches land on plain floor.
        let vault = generate_vault(seed("AAA-AAA-AAA"), 19, Challenges::DARKNESS).unwrap();
        assert_eq!(vault.level.java_map_hash(), -743_826_665);
        assert_eq!(vault.heaps.len(), 54);
        assert_eq!(vault.mobs.len(), 48);
        let mut torches: Vec<usize> = vault
            .heaps
            .iter()
            .filter(|heap| heap.items.contains(&VaultItem::Torch))
            .map(|heap| heap.cell)
            .collect();
        torches.sort_unstable();
        assert_eq!(torches, [857, 2_430, 2_911]);
        assert_eq!(
            treasure_records(&vault),
            treasure_records(&generate_vault(seed("AAA-AAA-AAA"), 19, Challenges::NONE).unwrap())
        );
    }

    #[test]
    fn generator_is_left_balanced_after_generation() {
        let mut random = RandomStack::with_base_seed(0);
        random.push(seed_for_depth(seed("AAA-AAA-AAB"), 18, 1));
        let vault = generate_vault_with_generator(18, Challenges::NONE, &mut random).unwrap();
        assert_eq!(vault.level.java_map_hash(), 1_059_305_718);
        // Only the depth generator remains pushed; popping it must succeed.
        random.pop();
    }
}
