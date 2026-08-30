//! Room graph for the v4.0.0 Imp Vault (`levels/rooms/quest/vault/*`).
//!
//! The vault's rooms are a closed family that never mixes with the regular
//! dungeon's room classes, and every one of them has a rigid 11- or 21-cell
//! footprint. They therefore get their own light-weight graph type instead
//! of a new [`crate::room::RoomKind`] variant: [`VaultRoom`] keeps exactly
//! the identity-based `ArrayList`/`LinkedHashMap` state the Java graph
//! carries (neighbours with their upstream duplicate quirks, insertion-ordered
//! connections, shared `Door` objects) and ports the `Room` graph methods
//! plus every `canConnect` override the vault classes declare.

// Direct ports of upstream graph methods keep Java's unchecked invariants.
#![allow(clippy::missing_panics_doc)]

use crate::geometry::{Point, Rect};
use crate::java_math::rem_i32;
use crate::rng::RandomStack;
pub use crate::room::{Direction, Door, DoorType};

/// Every concrete vault room class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum VaultRoomKind {
    Entrance,
    Ring,
    Cross,
    Quadrants,
    Rings,
    EnemyCenter,
    Hallway,
    LongRings,
    Circle,
    AlternatingFire,
    Lasers,
    Tokens,
    SimpleEnemyTreasure,
    FlamePath,
    LaserTreasure,
    CircleScanTreasure,
    SingleEnemyTreasure,
    BookcaseTreasure,
    FlamesTreasure,
    ManyScans,
    MultipleEnemyTreasure,
    HardLaserTreasure,
    Final,
}

impl VaultRoomKind {
    /// `VaultRoom.rooms` in registration order (the `chances` indices).
    pub const STANDARD_TABLE: [Self; 10] = [
        Self::Ring,
        Self::Cross,
        Self::Quadrants,
        Self::Rings,
        Self::EnemyCenter,
        Self::Hallway,
        Self::LongRings,
        Self::Circle,
        Self::AlternatingFire,
        Self::Lasers,
    ];

    /// `VaultTreasureRoom.T1_ROOMS`.
    pub const TIER_ONE_TREASURES: [Self; 3] = [
        Self::FlamePath,
        Self::LaserTreasure,
        Self::CircleScanTreasure,
    ];
    /// `VaultTreasureRoom.T2_ROOMS`.
    pub const TIER_TWO_TREASURES: [Self; 3] = [
        Self::SingleEnemyTreasure,
        Self::BookcaseTreasure,
        Self::FlamesTreasure,
    ];
    /// `VaultTreasureRoom.T3_ROOMS`.
    pub const TIER_THREE_TREASURES: [Self; 3] = [
        Self::ManyScans,
        Self::MultipleEnemyTreasure,
        Self::HardLaserTreasure,
    ];

    /// `VaultLongRoom` subclasses.
    #[must_use]
    pub const fn is_long(self) -> bool {
        matches!(self, Self::Hallway | Self::LongRings | Self::Tokens)
    }

    /// `VaultTreasureRoom` subclasses.
    #[must_use]
    pub const fn is_treasure(self) -> bool {
        matches!(
            self,
            Self::FlamePath
                | Self::LaserTreasure
                | Self::CircleScanTreasure
                | Self::SingleEnemyTreasure
                | Self::BookcaseTreasure
                | Self::FlamesTreasure
                | Self::ManyScans
                | Self::MultipleEnemyTreasure
                | Self::HardLaserTreasure
        )
    }

    /// `StandardRoom.class.isInstance(room)`: everything but the final
    /// `SpecialRoom`.
    #[must_use]
    pub const fn is_standard(self) -> bool {
        !matches!(self, Self::Final)
    }

    /// `VaultRoom.sizeFactor()`.
    #[must_use]
    pub const fn size_factor(self) -> i32 {
        if self.is_long() { 2 } else { 1 }
    }

    /// `Room.canPlaceWater` overrides.
    #[must_use]
    pub const fn can_place_water(self) -> bool {
        !matches!(self, Self::Final)
    }

    /// `Room.canPlaceGrass` overrides.
    #[must_use]
    pub const fn can_place_grass(self) -> bool {
        !matches!(
            self,
            Self::Final
                | Self::CircleScanTreasure
                | Self::FlamesTreasure
                | Self::SingleEnemyTreasure
        )
    }
}

/// One insertion-ordered `Room.connected` entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VaultConnection {
    pub room: usize,
    pub door: Option<Door>,
}

/// A vault room with its graph state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultRoom {
    pub kind: VaultRoomKind,
    pub bounds: Rect,
    /// `VaultLongRoom.wide`, drawn in the field initializer.
    wide: bool,
    /// Java `ArrayList` order, including upstream's one-sided duplicates.
    pub neighbours: Vec<usize>,
    /// Java `LinkedHashMap` key order.
    pub connected: Vec<VaultConnection>,
}

impl VaultRoom {
    /// Constructs a `StandardRoom` subclass: the instance initializer calls
    /// `setSizeCat()` (one `Random.chances` float against `{0, 1, 0}`), and
    /// long rooms then draw `wide = Random.Int(2) == 0`.
    pub fn standard(kind: VaultRoomKind, random: &mut RandomStack) -> Self {
        assert!(kind.is_standard(), "the final room is a SpecialRoom");
        let category = random.chances(&[0.0, 1.0, 0.0]);
        debug_assert_eq!(category, Some(1));
        let wide = kind.is_long() && random.int_bound(2) == 0;
        Self {
            kind,
            bounds: Rect::default(),
            wide,
            neighbours: Vec::new(),
            connected: Vec::new(),
        }
    }

    /// `new VaultFinalRoom()`: a `SpecialRoom`, so no constructor draw.
    #[must_use]
    pub fn final_room() -> Self {
        Self {
            kind: VaultRoomKind::Final,
            bounds: Rect::default(),
            wide: false,
            neighbours: Vec::new(),
            connected: Vec::new(),
        }
    }

    #[must_use]
    pub const fn is_entrance(&self) -> bool {
        matches!(self.kind, VaultRoomKind::Entrance)
    }

    #[must_use]
    pub const fn is_exit(&self) -> bool {
        matches!(self.kind, VaultRoomKind::Final)
    }

    /// `VaultLongRoom.wide()`: the field until the room has a shape.
    #[must_use]
    pub const fn wide(&self) -> bool {
        if self.width() == self.height() {
            self.wide
        } else {
            self.width() > self.height()
        }
    }

    /// Inclusive `Room.width()`.
    #[must_use]
    pub const fn width(&self) -> i32 {
        self.bounds.width().wrapping_add(1)
    }

    /// Inclusive `Room.height()`.
    #[must_use]
    pub const fn height(&self) -> i32 {
        self.bounds.height().wrapping_add(1)
    }

    #[must_use]
    pub const fn min_width(&self) -> i32 {
        match self.kind {
            VaultRoomKind::Final => 21,
            kind if kind.is_long() && self.wide() => 21,
            _ => 11,
        }
    }

    #[must_use]
    pub const fn max_width(&self) -> i32 {
        self.min_width()
    }

    #[must_use]
    pub const fn min_height(&self) -> i32 {
        match self.kind {
            VaultRoomKind::Final => 21,
            kind if kind.is_long() && !self.wide() => 21,
            _ => 11,
        }
    }

    #[must_use]
    pub const fn max_height(&self) -> i32 {
        self.min_height()
    }

    pub fn set_empty(&mut self) {
        self.bounds.set_empty();
    }

    pub fn set_position(&mut self, x: i32, y: i32) {
        self.bounds.set_pos(x, y);
    }

    pub fn shift(&mut self, x: i32, y: i32) {
        self.bounds.shift(x, y);
    }

    /// `Room.forceSize(w, h)`: the range check happens first, and a
    /// successful call still draws `Random.NormalIntRange` twice.
    pub fn force_size(&mut self, width: i32, height: i32, random: &mut RandomStack) -> bool {
        if width < self.min_width()
            || width > self.max_width()
            || height < self.min_height()
            || height > self.max_height()
        {
            return false;
        }
        let new_width = random.normal_int_range(width, width).wrapping_sub(1);
        let new_height = random.normal_int_range(height, height).wrapping_sub(1);
        self.bounds.resize(new_width, new_height);
        true
    }

    /// `Room.center()`. Vault rooms are always an odd number of cells wide
    /// and tall, so the conditional jitter draws never fire.
    #[must_use]
    pub fn center(&self) -> Point {
        debug_assert_eq!(
            rem_i32(self.bounds.right.wrapping_sub(self.bounds.left), 2),
            0
        );
        debug_assert_eq!(
            rem_i32(self.bounds.bottom.wrapping_sub(self.bounds.top), 2),
            0
        );
        Point::new(
            self.bounds.left.wrapping_add(self.bounds.right) / 2,
            self.bounds.top.wrapping_add(self.bounds.bottom) / 2,
        )
    }

    /// `Room.inside(p)`: strictly within the wall ring.
    #[must_use]
    pub const fn inside(&self, point: Point) -> bool {
        point.x > self.bounds.left
            && point.y > self.bounds.top
            && point.x < self.bounds.right
            && point.y < self.bounds.bottom
    }

    /// `Room.random(m)`: the inclusive x range is drawn before y.
    pub fn random_point(&self, margin: i32, random: &mut RandomStack) -> Point {
        let x = random.int_range(
            self.bounds.left.wrapping_add(margin),
            self.bounds.right.wrapping_sub(margin),
        );
        let y = random.int_range(
            self.bounds.top.wrapping_add(margin),
            self.bounds.bottom.wrapping_sub(margin),
        );
        Point::new(x, y)
    }

    /// `Room.maxConnections(direction)` with the treasure/special override.
    #[must_use]
    pub const fn max_connections(&self, direction: Direction) -> i32 {
        if self.kind.is_treasure() || matches!(self.kind, VaultRoomKind::Final) {
            1
        } else if matches!(direction, Direction::All) {
            16
        } else {
            4
        }
    }

    /// `Room.canConnect(Point)` including every vault override. Overrides
    /// that skip `super` (entrance, final) are reproduced as written.
    #[must_use]
    pub fn can_connect_point(&self, point: Point) -> bool {
        let bounds = self.bounds;
        let base = (point.x == bounds.left || point.x == bounds.right)
            != (point.y == bounds.top || point.y == bounds.bottom);
        match self.kind {
            VaultRoomKind::Entrance => {
                (point.x > bounds.left + 1 && point.x < bounds.right - 1)
                    || (point.y > bounds.top + 1 && point.y < bounds.bottom - 1)
            }
            VaultRoomKind::Final => {
                let center = self.center();
                (point.x - center.x).abs() <= 5 || (point.y - center.y).abs() <= 5
            }
            VaultRoomKind::Tokens => {
                base && if self.wide() {
                    point.x < bounds.left + 5 || point.x > bounds.right - 5
                } else {
                    point.y < bounds.top + 5 || point.y > bounds.bottom - 5
                }
            }
            VaultRoomKind::Cross => {
                let center = self.center();
                ((center.x - point.x).abs() <= 1 || (center.y - point.y).abs() <= 1) && base
            }
            VaultRoomKind::Quadrants => {
                let center = self.center();
                center.x != point.x && center.y != point.y && base
            }
            VaultRoomKind::CircleScanTreasure | VaultRoomKind::FlamePath => {
                let center = self.center();
                (center.x - point.x).abs() > 1 && (center.y - point.y).abs() > 1 && base
            }
            VaultRoomKind::FlamesTreasure | VaultRoomKind::SingleEnemyTreasure => {
                let center = self.center();
                ((center.x - point.x).abs() <= 2 || (center.y - point.y).abs() <= 2) && base
            }
            VaultRoomKind::MultipleEnemyTreasure => {
                let center = self.center();
                ((center.x - point.x).abs() <= 3 || (center.y - point.y).abs() <= 3) && base
            }
            _ => base,
        }
    }

    #[must_use]
    pub fn connection_to(&self, room: usize) -> Option<&VaultConnection> {
        self.connected.iter().find(|entry| entry.room == room)
    }

    pub fn connection_to_mut(&mut self, room: usize) -> Option<&mut VaultConnection> {
        self.connected.iter_mut().find(|entry| entry.room == room)
    }

    /// `SpecialRoom.entrance()` / `VaultTreasureRoom.entrance()`: the first
    /// connection's door.
    #[must_use]
    pub fn entrance_door(&self) -> Option<Door> {
        self.connected.first().and_then(|entry| entry.door)
    }

    /// The other room of the first connection.
    #[must_use]
    pub fn entrance_neighbour(&self) -> Option<usize> {
        self.connected.first().map(|entry| entry.room)
    }
}

fn two_rooms_mut(
    rooms: &mut [VaultRoom],
    first: usize,
    second: usize,
) -> (&mut VaultRoom, &mut VaultRoom) {
    assert_ne!(first, second, "a room cannot connect to itself");
    if first < second {
        let (left, right) = rooms.split_at_mut(second);
        (&mut left[first], &mut right[0])
    } else {
        let (left, right) = rooms.split_at_mut(first);
        (&mut right[0], &mut left[second])
    }
}

/// `Room.addNeigbour`. Only the calling side is checked for an existing
/// entry, so the other side can accumulate duplicates exactly as upstream.
pub fn add_neighbour(rooms: &mut [VaultRoom], this: usize, other: usize) -> bool {
    if rooms[this].neighbours.contains(&other) {
        return true;
    }
    let intersection = rooms[this].bounds.intersect(rooms[other].bounds);
    if (intersection.width() == 0 && intersection.height() >= 2)
        || (intersection.height() == 0 && intersection.width() >= 2)
    {
        let (this_room, other_room) = two_rooms_mut(rooms, this, other);
        this_room.neighbours.push(other);
        other_room.neighbours.push(this);
        true
    } else {
        false
    }
}

/// `Room.curConnections(direction)`.
#[must_use]
pub fn current_connections(rooms: &[VaultRoom], room: usize, direction: Direction) -> i32 {
    if direction == Direction::All {
        return i32::try_from(rooms[room].connected.len()).unwrap_or(i32::MAX);
    }
    let bounds = rooms[room].bounds;
    let mut total = 0_i32;
    for connection in &rooms[room].connected {
        let intersection = bounds.intersect(rooms[connection.room].bounds);
        let matches = match direction {
            Direction::Left => intersection.width() == 0 && intersection.left == bounds.left,
            Direction::Top => intersection.height() == 0 && intersection.top == bounds.top,
            Direction::Right => intersection.width() == 0 && intersection.right == bounds.right,
            Direction::Bottom => intersection.height() == 0 && intersection.bottom == bounds.bottom,
            Direction::All => unreachable!(),
        };
        if matches {
            total = total.wrapping_add(1);
        }
    }
    total
}

/// `Room.remConnections(direction)`.
#[must_use]
pub fn remaining_connections(rooms: &[VaultRoom], room: usize, direction: Direction) -> i32 {
    if current_connections(rooms, room, Direction::All)
        >= rooms[room].max_connections(Direction::All)
    {
        0
    } else {
        rooms[room]
            .max_connections(direction)
            .wrapping_sub(current_connections(rooms, room, direction))
    }
}

fn can_connect_direction(rooms: &[VaultRoom], room: usize, direction: Direction) -> bool {
    remaining_connections(rooms, room, direction) > 0
}

fn touches_entrance_within(rooms: &[VaultRoom], room: usize, hops: u32) -> bool {
    if rooms[room].is_entrance() {
        return true;
    }
    if hops == 0 {
        return false;
    }
    rooms[room]
        .connected
        .iter()
        .any(|entry| touches_entrance_within(rooms, entry.room, hops - 1))
}

/// `this.canConnect(Room r)` with the `VaultTokensRoom` and
/// `VaultFinalRoom` overrides applied on the calling side only.
#[must_use]
pub fn can_connect_rooms(rooms: &[VaultRoom], this: usize, other: usize) -> bool {
    match rooms[this].kind {
        // r itself, its connections, and their connections must avoid the
        // entrance.
        VaultRoomKind::Tokens if touches_entrance_within(rooms, other, 2) => return false,
        VaultRoomKind::Final if touches_entrance_within(rooms, other, 3) => return false,
        _ => {}
    }
    if (rooms[this].is_exit() && rooms[other].is_entrance())
        || (rooms[this].is_entrance() && rooms[other].is_exit())
    {
        return false;
    }

    let this_bounds = rooms[this].bounds;
    let intersection = this_bounds.intersect(rooms[other].bounds);
    let found_point = intersection
        .points()
        .any(|point| rooms[this].can_connect_point(point) && rooms[other].can_connect_point(point));
    if !found_point {
        return false;
    }

    if intersection.width() == 0 && intersection.left == this_bounds.left {
        can_connect_direction(rooms, this, Direction::Left)
            && can_connect_direction(rooms, other, Direction::Right)
    } else if intersection.height() == 0 && intersection.top == this_bounds.top {
        can_connect_direction(rooms, this, Direction::Top)
            && can_connect_direction(rooms, other, Direction::Bottom)
    } else if intersection.width() == 0 && intersection.right == this_bounds.right {
        can_connect_direction(rooms, this, Direction::Right)
            && can_connect_direction(rooms, other, Direction::Left)
    } else if intersection.height() == 0 && intersection.bottom == this_bounds.bottom {
        can_connect_direction(rooms, this, Direction::Bottom)
            && can_connect_direction(rooms, other, Direction::Top)
    } else {
        false
    }
}

/// `this.connect(room)`.
pub fn connect_rooms(rooms: &mut [VaultRoom], this: usize, other: usize) -> bool {
    let neighbours = rooms[this].neighbours.contains(&other) || add_neighbour(rooms, this, other);
    if !neighbours
        || rooms[this].connection_to(other).is_some()
        || !can_connect_rooms(rooms, this, other)
    {
        return false;
    }
    let (this_room, other_room) = two_rooms_mut(rooms, this, other);
    this_room.connected.push(VaultConnection {
        room: other,
        door: None,
    });
    other_room.connected.push(VaultConnection {
        room: this,
        door: None,
    });
    true
}

/// `Builder.findNeighbours(rooms)` over the list order.
pub fn find_neighbours(rooms: &mut [VaultRoom]) {
    for first in 0..rooms.len().saturating_sub(1) {
        for second in first + 1..rooms.len() {
            add_neighbour(rooms, first, second);
        }
    }
}

/// `Room.Door.set(type)` on the door object shared by both connection
/// records.
pub fn set_shared_door_type(
    rooms: &mut [VaultRoom],
    first: usize,
    second: usize,
    door_type: DoorType,
) {
    let (first_room, second_room) = two_rooms_mut(rooms, first, second);
    first_room
        .connection_to_mut(second)
        .expect("forward connection is missing")
        .door
        .as_mut()
        .expect("door must be placed before painting")
        .set_type(door_type);
    second_room
        .connection_to_mut(first)
        .expect("reverse connection is missing")
        .door
        .as_mut()
        .expect("reverse door must be placed before painting")
        .set_type(door_type);
}

/// Sets the shared door on both records unconditionally (`d.type = ...`).
pub fn force_shared_door_type(
    rooms: &mut [VaultRoom],
    first: usize,
    second: usize,
    door_type: DoorType,
) {
    let (first_room, second_room) = two_rooms_mut(rooms, first, second);
    first_room
        .connection_to_mut(second)
        .expect("forward connection is missing")
        .door
        .as_mut()
        .expect("door must be placed")
        .door_type = door_type;
    second_room
        .connection_to_mut(first)
        .expect("reverse connection is missing")
        .door
        .as_mut()
        .expect("reverse door must be placed")
        .door_type = door_type;
}

/// Stores one placed door on both records.
pub fn set_shared_door(rooms: &mut [VaultRoom], first: usize, second: usize, door: Door) {
    let (first_room, second_room) = two_rooms_mut(rooms, first, second);
    first_room
        .connection_to_mut(second)
        .expect("forward connection is missing")
        .door = Some(door);
    second_room
        .connection_to_mut(first)
        .expect("reverse connection is missing")
        .door = Some(door);
}

/// `VaultRoom.chances` and `createRoom()`.
#[derive(Clone, Debug, PartialEq)]
pub struct VaultRoomChances {
    chances: [f32; 10],
}

impl Default for VaultRoomChances {
    fn default() -> Self {
        Self::setup()
    }
}

impl VaultRoomChances {
    /// `VaultRoom.setupChances()`.
    #[must_use]
    pub const fn setup() -> Self {
        Self {
            chances: [2.0, 2.0, 2.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        }
    }

    /// `VaultRoom.createRoom()`.
    pub fn create_room(&mut self, random: &mut RandomStack) -> VaultRoom {
        let mut index = random.chances(&self.chances);
        if index.is_none() {
            *self = Self::setup();
            index = random.chances(&self.chances);
        }
        let index = index.expect("a fresh vault room table has positive weight");
        self.chances[index] -= 1.0;
        VaultRoom::standard(VaultRoomKind::STANDARD_TABLE[index], random)
    }
}

/// `VaultTreasureRoom.generateRoomList()`: three list shuffles, then the
/// tiers are interleaved one room at a time.
pub fn generate_treasure_room_list(random: &mut RandomStack) -> Vec<VaultRoomKind> {
    let mut tier_one = VaultRoomKind::TIER_ONE_TREASURES.to_vec();
    random.shuffle_list(&mut tier_one);
    let mut tier_two = VaultRoomKind::TIER_TWO_TREASURES.to_vec();
    random.shuffle_list(&mut tier_two);
    let mut tier_three = VaultRoomKind::TIER_THREE_TREASURES.to_vec();
    random.shuffle_list(&mut tier_three);
    let mut full_list = vec![tier_one, tier_two, tier_three];
    let mut output = Vec::with_capacity(9);
    while !full_list.is_empty() {
        let mut current = full_list.remove(0);
        output.push(current.remove(0));
        if !current.is_empty() {
            full_list.push(current);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_rooms_draw_wide_after_the_size_category_float() {
        let mut random = RandomStack::with_base_seed(0);
        random.push(1);
        let mut reference = random.clone();
        let room = VaultRoom::standard(VaultRoomKind::Hallway, &mut random);
        let _ = reference.float();
        let expected_wide = reference.int_bound(2) == 0;
        assert_eq!(room.wide(), expected_wide);
        assert_eq!(random.int(), reference.int());
        if expected_wide {
            assert_eq!((room.min_width(), room.min_height()), (21, 11));
        } else {
            assert_eq!((room.min_width(), room.min_height()), (11, 21));
        }
    }

    #[test]
    fn force_size_checks_the_range_before_drawing() {
        let mut random = RandomStack::with_base_seed(0);
        random.push(2);
        let mut room = VaultRoom::final_room();
        let before = random.clone();
        assert!(!room.force_size(11, 11, &mut random));
        assert_eq!(random.clone().int(), before.clone().int());
        assert!(room.force_size(21, 21, &mut random));
        assert_eq!((room.width(), room.height()), (21, 21));
        let mut reference = before;
        for _ in 0..4 {
            let _ = reference.float();
        }
        assert_eq!(random.int(), reference.int());
    }

    #[test]
    fn treasure_room_list_interleaves_tiers() {
        let mut random = RandomStack::with_base_seed(0);
        random.push(3);
        let list = generate_treasure_room_list(&mut random);
        assert_eq!(list.len(), 9);
        for (index, kind) in list.iter().enumerate() {
            let expected: &[VaultRoomKind] = match index % 3 {
                0 => &VaultRoomKind::TIER_ONE_TREASURES,
                1 => &VaultRoomKind::TIER_TWO_TREASURES,
                _ => &VaultRoomKind::TIER_THREE_TREASURES,
            };
            assert!(expected.contains(kind));
        }
    }

    #[test]
    fn tokens_room_refuses_rooms_near_the_entrance() {
        let mut random = RandomStack::with_base_seed(0);
        random.push(4);
        let mut rooms = vec![
            VaultRoom::standard(VaultRoomKind::Entrance, &mut random),
            VaultRoom::standard(VaultRoomKind::Ring, &mut random),
            VaultRoom::standard(VaultRoomKind::Tokens, &mut random),
        ];
        assert!(rooms[0].force_size(11, 11, &mut random));
        assert!(rooms[1].force_size(11, 11, &mut random));
        rooms[0].set_position(0, 0);
        rooms[1].set_position(10, 0);
        assert!(connect_rooms(&mut rooms, 1, 0));
        let wide = rooms[2].wide();
        assert!(rooms[2].force_size(
            if wide { 21 } else { 11 },
            if wide { 11 } else { 21 },
            &mut random
        ));
        rooms[2].set_position(20, 0);
        assert!(!connect_rooms(&mut rooms, 2, 1));
        assert!(connect_rooms(&mut rooms, 1, 2));
    }
}
