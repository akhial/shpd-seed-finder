//! Concrete painted-level adapter for [`crate::regular_items::RegularItemPlacement`].
//!
//! `RegularLevel.randomDropCell` is unusually stateful: every attempt shuffles
//! the mutable `rooms` list, even attempts which select the entrance or reject
//! a candidate. [`PaintedRegularPlacement`] mutates [`Level::room_order`] in
//! place and performs all predicates in the exact Java short-circuit order.

use crate::generator::GeneratedItem;
use crate::geometry::{Point, Rect, terrain};
use crate::level::{HeapKind, Level, PaintItem, PlacedTrap, TrapKind};
use crate::level_flags::LevelFlags;
use crate::regular_items::{
    DropCellKind, RegularHeapKind, RegularItem, RegularItemPlacement, RegularMimicKind,
};
use crate::rng::RandomStack;
use crate::room::{QuestRoomKind, Room, RoomId, RoomKind, StandardRoomKind};

/// Class-specific `Room.canPlaceItem` and trap-class behavior unavailable in
/// the generic room graph.
///
/// The adapter already checks `Room.inside(point)` before calling
/// `can_place_item`. Implementations should apply only subclass overrides,
/// such as a bridge's painted `spaceRect`, Plants/Aquarium occupancy, or quest
/// room restrictions. Methods are mutable so fixture and dispatcher adapters
/// can retain exact class-specific paint state or operation traces.
pub trait RoomPlacementRules {
    fn can_place_item(
        &mut self,
        room_id: RoomId,
        room: &Room,
        point: Point,
        cell: usize,
        level: &Level,
    ) -> bool;

    fn trap_destroys_items(&mut self, trap: &PlacedTrap, cell: usize, level: &Level) -> bool;
}

/// Built-in rules for all regular-room overrides represented by the current
/// room graph. Painters can register bridge `spaceRect`s and `RitualSite` center
/// cells after painting without coupling this module to a region dispatcher.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CanonicalRoomPlacementRules {
    bridge_space_rects: Vec<(RoomId, Rect)>,
    ritual_cells: Vec<(RoomId, usize)>,
}

impl CanonicalRoomPlacementRules {
    /// Records the `StandardBridgeRoom.spaceRect` produced while painting.
    pub fn set_bridge_space_rect(&mut self, room: RoomId, rect: Rect) {
        set_room_value(&mut self.bridge_space_rects, room, rect);
    }

    /// Records `CeremonialCandle.ritualPos` for a painted `RitualSiteRoom`.
    pub fn set_ritual_cell(&mut self, room: RoomId, cell: usize) {
        set_room_value(&mut self.ritual_cells, room, cell);
    }
}

impl RoomPlacementRules for CanonicalRoomPlacementRules {
    fn can_place_item(
        &mut self,
        room_id: RoomId,
        room: &Room,
        point: Point,
        cell: usize,
        level: &Level,
    ) -> bool {
        match room.kind {
            RoomKind::Entrance(kind) | RoomKind::Exit(kind) | RoomKind::Standard(kind) => {
                if matches!(
                    kind,
                    StandardRoomKind::WaterBridge
                        | StandardRoomKind::RegionDecoBridge
                        | StandardRoomKind::ChasmBridge
                ) && self
                    .bridge_space_rects
                    .iter()
                    .find_map(|(id, rect)| (*id == room_id).then_some(*rect))
                    .is_some_and(|rect| rect.inside(point))
                {
                    return false;
                }
                match kind {
                    StandardRoomKind::Plants => {
                        !level.plants.iter().any(|plant| plant.cell == cell)
                    }
                    StandardRoomKind::Aquarium => level.map.cells[cell] != terrain::WATER,
                    StandardRoomKind::CavesFissure => level.map.cells[cell] != terrain::EMPTY_SP,
                    _ => true,
                }
            }
            RoomKind::Quest(QuestRoomKind::AmbitiousImp) => false,
            // v4.0.0 BlacksmithRoom only accepts its plain-floor ring.
            RoomKind::Quest(QuestRoomKind::Blacksmith) => level.map.cells[cell] == terrain::EMPTY,
            RoomKind::Quest(QuestRoomKind::RitualSite) => self
                .ritual_cells
                .iter()
                .find_map(|(id, ritual)| (*id == room_id).then_some(*ritual))
                .is_none_or(|ritual| level_distance(level, ritual, cell) >= 2),
            RoomKind::Connection(_)
            | RoomKind::Special(_)
            | RoomKind::Secret(_)
            | RoomKind::Quest(QuestRoomKind::MassGrave | QuestRoomKind::RotGarden) => true,
        }
    }

    fn trap_destroys_items(&mut self, trap: &PlacedTrap, _cell: usize, _level: &Level) -> bool {
        matches!(
            trap.spec.kind,
            TrapKind::Burning
                | TrapKind::Blazing
                | TrapKind::Chilling
                | TrapKind::Frost
                | TrapKind::Explosive
                | TrapKind::Disintegration
                | TrapKind::Pitfall
        )
    }
}

fn set_room_value<T>(values: &mut Vec<(RoomId, T)>, room: RoomId, value: T) {
    if let Some(existing) = values.iter_mut().find(|(id, _)| *id == room) {
        existing.1 = value;
    } else {
        values.push((room, value));
    }
}

fn level_distance(level: &Level, first: usize, second: usize) -> i32 {
    let first = level.map.cell_to_point(first);
    let second = level.map.cell_to_point(second);
    first
        .x
        .wrapping_sub(second.x)
        .abs()
        .max(first.y.wrapping_sub(second.y).abs())
}

/// Why one `randomDropCell` attempt did not return its candidate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DropCellRejection {
    EntranceRoom,
    NotPassable,
    Solid,
    Exit,
    Heap,
    RoomRule,
    Mob,
    DestructiveTrap,
}

/// Auditable trace of a Java `randomDropCell` attempt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DropCellAttempt {
    NoMatchingRoom {
        kind: DropCellKind,
    },
    Entrance {
        room: RoomId,
    },
    Candidate {
        room: RoomId,
        point: Point,
        cell: usize,
        result: Result<(), DropCellRejection>,
    },
}

/// Item already present in, or added to, an adapter-owned heap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlacementHeapItem {
    Painted(PaintItem),
    Regular(RegularItem),
}

/// Heap type including containers created before `createItems()`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PlacementHeapKind {
    Heap,
    Chest,
    LockedChest,
    Skeleton,
    Tomb,
}

/// Ordered generation-visible heap state maintained by the adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementHeap {
    pub cell: usize,
    pub kind: PlacementHeapKind,
    /// Java `Heap.drop` puts ordinary non-`dropsDownHeap` items at the front.
    pub items: Vec<PlacementHeapItem>,
    pub haunted: bool,
}

/// Mimic occupancy created by regular item placement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementMimic {
    pub cell: usize,
    pub kind: RegularMimicKind,
    pub items: Vec<RegularItem>,
}

/// Stable handle returned from `Level.drop` to the caller.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlacementHeapHandle(usize);

/// Concrete adapter over one painted regular level.
pub struct PaintedRegularPlacement<'a, R: RoomPlacementRules> {
    level: &'a mut Level,
    flags: &'a mut LevelFlags,
    rooms: &'a [Room],
    rules: &'a mut R,
    entrance: RoomId,
    pub heaps: Vec<PlacementHeap>,
    pub mimics: Vec<PlacementMimic>,
    pub attempts: Vec<DropCellAttempt>,
}

impl<'a, R: RoomPlacementRules> PaintedRegularPlacement<'a, R> {
    /// Attaches to a fully painted level after `buildFlagMaps`.
    ///
    /// # Panics
    ///
    /// Panics unless `room_order` is a complete permutation of `rooms`, an
    /// entrance exists, and level/flag array lengths agree.
    #[must_use]
    pub fn new(
        level: &'a mut Level,
        flags: &'a mut LevelFlags,
        rooms: &'a [Room],
        rules: &'a mut R,
    ) -> Self {
        assert_eq!(level.len(), flags.passable.len(), "flag length differs");
        assert_eq!(level.len(), flags.solid.len(), "flag length differs");
        assert_eq!(
            level.room_order.len(),
            rooms.len(),
            "room order is incomplete"
        );
        let mut sorted_order = level.room_order.clone();
        sorted_order.sort_unstable();
        assert_eq!(sorted_order, (0..rooms.len()).collect::<Vec<_>>());
        let entrance = rooms
            .iter()
            .position(Room::is_entrance)
            .expect("painted regular level has an entrance");

        let heaps = level
            .heaps
            .iter()
            .map(|heap| PlacementHeap {
                cell: heap.cell,
                kind: match heap.kind {
                    HeapKind::Heap => PlacementHeapKind::Heap,
                    HeapKind::Chest => PlacementHeapKind::Chest,
                    HeapKind::Tomb => PlacementHeapKind::Tomb,
                },
                items: heap
                    .items
                    .iter()
                    .copied()
                    .map(PlacementHeapItem::Painted)
                    .collect(),
                haunted: false,
            })
            .collect();

        // Reconcile painter records with the occupancy arrays used by Java's
        // `heaps.get`/`findMob` checks.
        for heap in &level.heaps {
            level.heap_cells[heap.cell] = true;
        }
        for mob in &level.mobs {
            level.mob_cells[mob.cell()] = true;
        }

        Self {
            level,
            flags,
            rooms,
            rules,
            entrance,
            heaps,
            mimics: Vec::new(),
            attempts: Vec::new(),
        }
    }

    /// Immutable access to the mutated level, including final room order.
    #[must_use]
    pub const fn level(&self) -> &Level {
        self.level
    }

    /// Mutable access after placement when a caller needs later level phases.
    pub fn level_mut(&mut self) -> &mut Level {
        self.level
    }

    fn select_room(&mut self, random: &mut RandomStack, kind: DropCellKind) -> Option<RoomId> {
        random.shuffle_list(&mut self.level.room_order);
        self.level
            .room_order
            .iter()
            .copied()
            .find(|&room| room_matches(self.rooms[room].kind, kind))
    }

    fn rejection_for(
        &mut self,
        room_id: RoomId,
        point: Point,
        cell: usize,
    ) -> Option<DropCellRejection> {
        if !self.flags.passable[cell] {
            return Some(DropCellRejection::NotPassable);
        }
        if self.flags.solid[cell] {
            return Some(DropCellRejection::Solid);
        }
        if self.level.exit().unwrap_or(0) == cell {
            return Some(DropCellRejection::Exit);
        }
        if self.level.heap_cells[cell] {
            return Some(DropCellRejection::Heap);
        }
        let room = &self.rooms[room_id];
        if !room.inside(point)
            || !self
                .rules
                .can_place_item(room_id, room, point, cell, self.level)
        {
            return Some(DropCellRejection::RoomRule);
        }
        if self.level.mob_cells[cell] {
            return Some(DropCellRejection::Mob);
        }
        if let Some(trap) = self.level.traps.iter().find(|trap| trap.cell == cell) {
            if self.rules.trap_destroys_items(trap, cell, self.level) {
                return Some(DropCellRejection::DestructiveTrap);
            }
        }
        None
    }

    fn random_cell(&mut self, random: &mut RandomStack, kind: DropCellKind) -> i32 {
        for _ in 0..100 {
            let Some(room_id) = self.select_room(random, kind) else {
                self.attempts.push(DropCellAttempt::NoMatchingRoom { kind });
                return -1;
            };
            if room_id == self.entrance {
                self.attempts
                    .push(DropCellAttempt::Entrance { room: room_id });
                continue;
            }

            let room = &self.rooms[room_id];
            // Room.random(): the inclusive x IntRange is evaluated before y.
            let point = Point::new(
                random.int_range(room.bounds.left + 1, room.bounds.right - 1),
                random.int_range(room.bounds.top + 1, room.bounds.bottom - 1),
            );
            let cell = self.level.point_to_cell(point);
            let rejection = self.rejection_for(room_id, point, cell);
            self.attempts.push(DropCellAttempt::Candidate {
                room: room_id,
                point,
                cell,
                result: rejection.map_or(Ok(()), Err),
            });
            if rejection.is_none() {
                return i32::try_from(cell).expect("generated level cell fits Java int");
            }
        }
        -1
    }
}

impl<R: RoomPlacementRules> RegularItemPlacement for PaintedRegularPlacement<'_, R> {
    type HeapHandle = PlacementHeapHandle;

    fn ebony_mimic_cell(&mut self, random: &mut RandomStack) -> i32 {
        let mut cells = Vec::new();
        if random.int_bound(2) == 0 {
            for heap in &self.heaps {
                let point = self.level.map.cell_to_point(heap.cell);
                let special = self
                    .rooms
                    .iter()
                    .any(|r| r.inside(point) && matches!(r.kind, RoomKind::Special(_)));
                if heap.kind == PlacementHeapKind::Heap
                    && !special
                    && !self.level.mob_cells[heap.cell]
                {
                    cells.push(heap.cell);
                }
            }
            cells.sort_unstable();
        }
        if cells.is_empty() {
            let exit = self.level.exit().expect("regular floor exit");
            if random.int_bound(5) == 0 && !self.level.mob_cells[exit] {
                cells.push(exit);
            } else {
                cells.extend(self.level.map.cells.iter().enumerate().filter_map(
                    |(cell, &tile)| {
                        (tile == terrain::DOOR && !self.level.mob_cells[cell]).then_some(cell)
                    },
                ));
            }
        }
        let index =
            usize::try_from(random.int_bound(i32::try_from(cells.len()).expect("map fits i32")))
                .expect("nonnegative index");
        i32::try_from(cells[index]).expect("map fits i32")
    }

    fn random_drop_cell(&mut self, random: &mut RandomStack, kind: DropCellKind) -> i32 {
        self.random_cell(random, kind)
    }

    fn clear_grass_if_needed(&mut self, cell: i32) {
        let cell = usize::try_from(cell).expect("randomDropCell returned a valid cell");
        if matches!(
            self.level.map.cells[cell],
            terrain::HIGH_GRASS | terrain::FURROWED_GRASS
        ) {
            self.level.map.cells[cell] = terrain::GRASS;
            self.flags.los_blocking[cell] = false;
        }
    }

    fn has_mob(&mut self, cell: i32) -> bool {
        usize::try_from(cell).is_ok_and(|cell| self.level.mob_cells[cell])
    }

    fn drop_item(&mut self, cell: i32, item: RegularItem) -> Self::HeapHandle {
        let cell = usize::try_from(cell).expect("randomDropCell returned a valid cell");
        let handle = if let Some((index, heap)) = self
            .heaps
            .iter_mut()
            .enumerate()
            .find(|(_, heap)| heap.cell == cell)
        {
            // `randomDropCell` rejects all existing heaps, so locked/crystal
            // relocation in Level.drop is unreachable for this adapter's
            // public workflow. Ordinary merging inserts at the heap front.
            heap.items.insert(0, PlacementHeapItem::Regular(item));
            PlacementHeapHandle(index)
        } else {
            let index = self.heaps.len();
            self.heaps.push(PlacementHeap {
                cell,
                kind: PlacementHeapKind::Heap,
                items: vec![PlacementHeapItem::Regular(item)],
                haunted: false,
            });
            PlacementHeapHandle(index)
        };
        self.level.heap_cells[cell] = true;
        handle
    }

    fn is_primary_heap(&self, cell: i32, heap: &Self::HeapHandle) -> bool {
        let Ok(cell) = usize::try_from(cell) else {
            return false;
        };
        self.heaps
            .iter()
            .position(|candidate| candidate.cell == cell)
            == Some(heap.0)
    }

    fn set_heap_type(&mut self, heap: &Self::HeapHandle, kind: RegularHeapKind) {
        self.heaps[heap.0].kind = match kind {
            RegularHeapKind::Heap => PlacementHeapKind::Heap,
            RegularHeapKind::Chest => PlacementHeapKind::Chest,
            RegularHeapKind::LockedChest => PlacementHeapKind::LockedChest,
            RegularHeapKind::Skeleton => PlacementHeapKind::Skeleton,
        };
    }

    fn set_haunted_if_cursed(&mut self, heap: &Self::HeapHandle) {
        self.heaps[heap.0].haunted = self.heaps[heap.0].items.iter().any(heap_item_is_cursed);
    }

    fn spawn_mimic(&mut self, cell: i32, kind: RegularMimicKind, items: &[RegularItem]) {
        let cell = usize::try_from(cell).expect("randomDropCell returned a valid cell");
        self.level.mob_cells[cell] = true;
        self.mimics.push(PlacementMimic {
            cell,
            kind,
            items: items.to_vec(),
        });
    }
}

const fn room_matches(room: RoomKind, requested: DropCellKind) -> bool {
    match requested {
        DropCellKind::StandardRoom => matches!(
            room,
            RoomKind::Entrance(_)
                | RoomKind::Exit(_)
                | RoomKind::Standard(_)
                | RoomKind::Quest(QuestRoomKind::RitualSite | QuestRoomKind::Blacksmith)
        ),
        DropCellKind::SpecialRoom => matches!(
            room,
            RoomKind::Special(_)
                | RoomKind::Secret(_)
                | RoomKind::Quest(
                    QuestRoomKind::MassGrave
                        | QuestRoomKind::RotGarden
                        | QuestRoomKind::AmbitiousImp
                )
        ),
    }
}

fn heap_item_is_cursed(item: &PlacementHeapItem) -> bool {
    match item {
        PlacementHeapItem::Painted(PaintItem::Generated(item))
        | PlacementHeapItem::Regular(RegularItem::Generated(item)) => {
            generated_item_is_cursed(*item)
        }
        PlacementHeapItem::Painted(
            PaintItem::ArcaneStylus | PaintItem::TrinketCatalyst | PaintItem::Direct(_),
        )
        | PlacementHeapItem::Regular(RegularItem::Queued(_)) => false,
    }
}

const fn generated_item_is_cursed(item: GeneratedItem) -> bool {
    match item {
        GeneratedItem::Equipment(equipment) => equipment.roll.cursed,
        GeneratedItem::Missile(missile) => missile.roll.cursed,
        GeneratedItem::Ring(ring) => ring.roll.cursed,
        GeneratedItem::Artifact(artifact) => artifact.cursed,
        GeneratedItem::Food(_)
        | GeneratedItem::Potion { .. }
        | GeneratedItem::Seed(_)
        | GeneratedItem::Scroll { .. }
        | GeneratedItem::Stone(_)
        | GeneratedItem::Gold { .. }
        | GeneratedItem::Trinket(_)
        | GeneratedItem::Bomb(_)
        | GeneratedItem::TippedDart { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CanonicalRoomPlacementRules, DropCellAttempt, DropCellRejection, PaintedRegularPlacement,
        PlacementHeapItem, PlacementHeapKind, RoomPlacementRules,
    };
    use crate::equipment::EquipmentRoll;
    use crate::generator::{GeneratedEquipment, GeneratedItem};
    use crate::geometry::{Point, Rect, terrain};
    use crate::level::{Level, PlacedTrap, TrapKind, TrapSpec};
    use crate::level_flags::LevelFlags;
    use crate::level_prelude::Feeling;
    use crate::regular_items::{
        DropCellKind, QueuedItemKind, RegularHeapKind, RegularItem, RegularItemPlacement,
        RegularMimicKind,
    };
    use crate::rng::RandomStack;
    use crate::room::{ConnectionRoomKind, Room, StandardRoomKind};

    struct RejectRules {
        rejects: [u8; 5],
        calls: Vec<(usize, usize)>,
    }

    impl RejectRules {
        fn new(reject_a: u8, reject_b: u8) -> Self {
            Self {
                rejects: [0, reject_a, reject_b, 0, 0],
                calls: Vec::new(),
            }
        }
    }

    impl RoomPlacementRules for RejectRules {
        fn can_place_item(
            &mut self,
            room_id: usize,
            _room: &Room,
            _point: Point,
            cell: usize,
            _level: &Level,
        ) -> bool {
            self.calls.push((room_id, cell));
            if self.rejects[room_id] > 0 {
                self.rejects[room_id] -= 1;
                false
            } else {
                true
            }
        }

        fn trap_destroys_items(&mut self, trap: &PlacedTrap, _cell: usize, _level: &Level) -> bool {
            matches!(
                trap.spec.kind,
                TrapKind::Burning | TrapKind::Chilling | TrapKind::Explosive
            )
        }
    }

    fn fixture_rooms(random: &mut RandomStack) -> Vec<Room> {
        let mut entrance = crate::room::create_entrance_room(1, random);
        entrance.bounds = Rect::new(1, 1, 5, 5);
        let mut first = Room::standard(StandardRoomKind::SewerPipe, random);
        first.bounds = Rect::new(7, 1, 13, 8);
        let mut second = Room::standard(StandardRoomKind::SewerPipe, random);
        second.bounds = Rect::new(14, 1, 20, 8);
        let mut special = Room::special(crate::room::SpecialRoomKind::Garden);
        special.bounds = Rect::new(7, 8, 13, 11);
        let mut tunnel = Room::connection(ConnectionRoomKind::Tunnel);
        tunnel.bounds = Rect::new(1, 7, 5, 10);
        vec![entrance, first, second, special, tunnel]
    }

    fn fixture_level(rooms: &[Room]) -> (Level, LevelFlags) {
        let mut level = Level::new(1, Feeling::None);
        level.set_size(24, 12);
        level.map.cells.fill(terrain::EMPTY);
        level.room_order = (0..rooms.len()).collect();
        let flags = LevelFlags::build(&level.map, false);
        (level, flags)
    }

    #[test]
    fn official_first_attempt_and_room_mutation_fixture_matches() {
        let mut setup = RandomStack::with_base_seed(0);
        let rooms = fixture_rooms(&mut setup);
        let (mut level, mut flags) = fixture_level(&rooms);
        let mut rules = RejectRules::new(0, 0);
        let mut placement =
            PaintedRegularPlacement::new(&mut level, &mut flags, &rooms, &mut rules);
        let mut random = RandomStack::with_base_seed(0);
        random.push(123);
        assert_eq!(
            placement.random_drop_cell(&mut random, DropCellKind::StandardRoom),
            183
        );
        assert_eq!(placement.level().room_order, [3, 2, 0, 1, 4]);
        assert_eq!(random.long(), -2_619_718_845_790_709_464);
        assert_eq!(
            placement.attempts,
            [DropCellAttempt::Candidate {
                room: 2,
                point: Point::new(15, 7),
                cell: 183,
                result: Ok(()),
            }]
        );
        random.pop();
    }

    #[test]
    fn official_four_retry_fixture_matches_room_list_and_coordinates() {
        let mut setup = RandomStack::with_base_seed(0);
        let rooms = fixture_rooms(&mut setup);
        let (mut level, mut flags) = fixture_level(&rooms);
        let mut rules = RejectRules::new(2, 2);
        let mut placement =
            PaintedRegularPlacement::new(&mut level, &mut flags, &rooms, &mut rules);
        let mut random = RandomStack::with_base_seed(0);
        random.push(123);
        assert_eq!(
            placement.random_drop_cell(&mut random, DropCellKind::StandardRoom),
            89
        );
        assert_eq!(placement.level().room_order, [3, 2, 4, 1, 0]);
        assert_eq!(random.long(), -8_981_052_000_858_401_101);
        let points: Vec<_> = placement
            .attempts
            .iter()
            .filter_map(|attempt| match attempt {
                DropCellAttempt::Candidate {
                    room,
                    point,
                    result,
                    ..
                } => Some((*room, *point, *result)),
                _ => None,
            })
            .collect();
        assert_eq!(
            points,
            [
                (2, Point::new(15, 7), Err(DropCellRejection::RoomRule)),
                (2, Point::new(16, 3), Err(DropCellRejection::RoomRule)),
                (1, Point::new(10, 6), Err(DropCellRejection::RoomRule)),
                (2, Point::new(17, 3), Ok(())),
            ]
        );
        random.pop();
    }

    #[test]
    fn official_entrance_skip_and_special_room_fixtures_match() {
        let mut setup = RandomStack::with_base_seed(0);
        let rooms = fixture_rooms(&mut setup);

        let (mut level, mut flags) = fixture_level(&rooms);
        let mut rules = RejectRules::new(0, 0);
        let mut placement =
            PaintedRegularPlacement::new(&mut level, &mut flags, &rooms, &mut rules);
        let mut random = RandomStack::with_base_seed(0);
        random.push(4);
        assert_eq!(
            placement.random_drop_cell(&mut random, DropCellKind::StandardRoom),
            177
        );
        assert_eq!(placement.level().room_order, [3, 4, 1, 0, 2]);
        assert!(matches!(
            placement.attempts[0],
            DropCellAttempt::Entrance { .. }
        ));
        assert!(matches!(
            placement.attempts[1],
            DropCellAttempt::Entrance { .. }
        ));
        assert_eq!(random.long(), -3_742_375_144_186_711_875);
        random.pop();

        let (mut level, mut flags) = fixture_level(&rooms);
        let mut rules = RejectRules::new(0, 0);
        let mut placement =
            PaintedRegularPlacement::new(&mut level, &mut flags, &rooms, &mut rules);
        let mut random = RandomStack::with_base_seed(0);
        random.push(9_876);
        assert_eq!(
            placement.random_drop_cell(&mut random, DropCellKind::SpecialRoom),
            251
        );
        assert_eq!(placement.level().room_order, [4, 3, 2, 1, 0]);
        assert_eq!(random.long(), -1_630_606_503_456_447_255);
        random.pop();
    }

    #[test]
    fn predicate_rejections_follow_java_short_circuit_order() {
        let mut setup = RandomStack::with_base_seed(0);
        let rooms = fixture_rooms(&mut setup);
        let (mut level, mut flags) = fixture_level(&rooms);
        // Seed 123 first selects room B and point (15,7), cell 183.
        flags.passable[183] = false;
        level.heap_cells[183] = true;
        level.mob_cells[183] = true;
        level.set_trap(PlacedTrap {
            spec: TrapSpec::new(TrapKind::Explosive),
            cell: 183,
            visible: false,
            active: true,
        });
        let mut rules = RejectRules::new(0, 0);
        let mut placement =
            PaintedRegularPlacement::new(&mut level, &mut flags, &rooms, &mut rules);
        let mut random = RandomStack::with_base_seed(0);
        random.push(123);
        let _ = placement.random_drop_cell(&mut random, DropCellKind::StandardRoom);
        assert!(matches!(
            placement.attempts[0],
            DropCellAttempt::Candidate {
                result: Err(DropCellRejection::NotPassable),
                ..
            }
        ));
        assert!(placement.rules.calls.iter().all(|(_, cell)| *cell != 183));
        random.pop();
    }

    #[test]
    fn grass_heaps_haunting_and_mimic_occupancy_mutate_spatial_state() {
        let mut setup = RandomStack::with_base_seed(0);
        let rooms = fixture_rooms(&mut setup);
        let (mut level, mut flags) = fixture_level(&rooms);
        level.map.cells[183] = terrain::HIGH_GRASS;
        flags.los_blocking[183] = true;
        let mut rules = CanonicalRoomPlacementRules::default();
        let mut placement =
            PaintedRegularPlacement::new(&mut level, &mut flags, &rooms, &mut rules);
        let cursed = RegularItem::Generated(GeneratedItem::Equipment(GeneratedEquipment {
            item: crate::catalog::ItemId::Sai,
            roll: EquipmentRoll {
                upgrade: 0,
                effect: None,
                cursed: true,
            },
        }));
        placement.clear_grass_if_needed(183);
        let heap = placement.drop_item(183, cursed);
        placement.set_heap_type(&heap, RegularHeapKind::Skeleton);
        placement.set_haunted_if_cursed(&heap);
        placement.spawn_mimic(
            184,
            RegularMimicKind::Mimic,
            &[RegularItem::Queued(QueuedItemKind::Stylus)],
        );
        assert_eq!(placement.level().map.cells[183], terrain::GRASS);
        assert!(!placement.flags.los_blocking[183]);
        assert_eq!(placement.heaps[0].kind, PlacementHeapKind::Skeleton);
        assert!(placement.heaps[0].haunted);
        assert_eq!(
            placement.heaps[0].items,
            [PlacementHeapItem::Regular(cursed)]
        );
        assert!(placement.has_mob(184));
    }

    #[test]
    fn canonical_rules_cover_bridge_plants_aquarium_and_destructive_traps() {
        let mut setup = RandomStack::with_base_seed(0);
        let mut bridge = Room::standard(StandardRoomKind::WaterBridge, &mut setup);
        bridge.bounds = Rect::new(1, 1, 9, 9);
        let mut level = Level::new(6, Feeling::None);
        level.set_size(12, 12);
        level.map.cells.fill(terrain::EMPTY);
        let mut rules = CanonicalRoomPlacementRules::default();
        rules.set_bridge_space_rect(0, Rect::new(3, 3, 7, 7));
        assert!(!rules.can_place_item(0, &bridge, Point::new(4, 4), 52, &level));
        assert!(rules.can_place_item(0, &bridge, Point::new(2, 2), 26, &level));

        let mut plants = Room::standard(StandardRoomKind::Plants, &mut setup);
        plants.bounds = Rect::new(1, 1, 9, 9);
        level.plant(crate::generator::SeedKind::Sungrass, 26);
        assert!(!rules.can_place_item(1, &plants, Point::new(2, 2), 26, &level));

        let mut aquarium = Room::standard(StandardRoomKind::Aquarium, &mut setup);
        aquarium.bounds = Rect::new(1, 1, 9, 9);
        level.map.cells[27] = terrain::WATER;
        assert!(!rules.can_place_item(2, &aquarium, Point::new(3, 2), 27, &level));

        for kind in [
            TrapKind::Burning,
            TrapKind::Blazing,
            TrapKind::Chilling,
            TrapKind::Frost,
            TrapKind::Explosive,
            TrapKind::Disintegration,
            TrapKind::Pitfall,
        ] {
            let trap = PlacedTrap {
                spec: TrapSpec::new(kind),
                cell: 26,
                visible: false,
                active: true,
            };
            assert!(rules.trap_destroys_items(&trap, 26, &level), "{kind:?}");
        }
        let harmless = PlacedTrap {
            spec: TrapSpec::new(TrapKind::Storm),
            cell: 26,
            visible: false,
            active: true,
        };
        assert!(!rules.trap_destroys_items(&harmless, 26, &level));
    }
}
