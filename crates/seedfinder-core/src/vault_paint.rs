//! Exact `paint(Level)` ports for every v4.0.0 Imp Vault room class.
//!
//! Each function reproduces its Java counterpart's terrain writes, heap
//! drops, mob placements, and RNG draws in source order. `Painter.fill`
//! overloads that take the room itself dispatch to the inclusive
//! `Room.width()/height()`, so the room helpers below add one to the plain
//! `Rect` extents; overloads taking a plain `Rect` (`treasure`, `itemPlace`)
//! use the exclusive `Rect.width()` exactly as written upstream.

// Each painter is one long method upstream; splitting them would obscure the
// draw-order audit against the Java source.
#![allow(clippy::missing_panics_doc, clippy::too_many_lines)]

use crate::challenges::Challenges;
use crate::geometry::{Point, Rect, painter as draw, terrain};
use crate::rng::RandomStack;
use crate::vault_floor::{VaultHeapKind, VaultLevelState};
use crate::vault_loot::{PrizeFilter, VaultItem};
use crate::vault_mobs::{VaultMobClass, VaultMobKind, random_tier_two_enemy};
use crate::vault_rooms::{DoorType, VaultRoom, VaultRoomKind, set_shared_door_type};

fn fill_room(state: &mut VaultLevelState, room: &VaultRoom, value: i32) {
    draw::fill(
        &mut state.level.map,
        room.bounds.left,
        room.bounds.top,
        room.width(),
        room.height(),
        value,
    );
}

fn fill_room_margin(state: &mut VaultLevelState, room: &VaultRoom, margin: i32, value: i32) {
    draw::fill(
        &mut state.level.map,
        room.bounds.left + margin,
        room.bounds.top + margin,
        room.width() - margin * 2,
        room.height() - margin * 2,
        value,
    );
}

fn fill_room_margins(
    state: &mut VaultLevelState,
    room: &VaultRoom,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    value: i32,
) {
    draw::fill(
        &mut state.level.map,
        room.bounds.left + left,
        room.bounds.top + top,
        room.width() - (left + right),
        room.height() - (top + bottom),
        value,
    );
}

fn fill_ellipse_room_margin(
    state: &mut VaultLevelState,
    room: &VaultRoom,
    margin: i32,
    value: i32,
) {
    draw::fill_ellipse(
        &mut state.level.map,
        room.bounds.left + margin,
        room.bounds.top + margin,
        room.width() - margin * 2,
        room.height() - margin * 2,
        value,
    );
}

/// `Painter.fill(level, rect, value)` for an `EmptyRoom` helper rectangle,
/// whose virtual `width()`/`height()` are inclusive.
fn fill_inclusive_rect(state: &mut VaultLevelState, rect: Rect, value: i32) {
    draw::fill(
        &mut state.level.map,
        rect.left,
        rect.top,
        rect.width() + 1,
        rect.height() + 1,
        value,
    );
}

fn fill(state: &mut VaultLevelState, x: i32, y: i32, width: i32, height: i32, value: i32) {
    draw::fill(&mut state.level.map, x, y, width, height, value);
}

fn fill_diamond(state: &mut VaultLevelState, x: i32, y: i32, width: i32, height: i32, value: i32) {
    draw::fill_diamond(&mut state.level.map, x, y, width, height, value);
}

fn set_xy(state: &mut VaultLevelState, x: i32, y: i32, value: i32) {
    state.level.map.set(x, y, value);
}

fn set_cell(state: &mut VaultLevelState, cell: i32, value: i32) {
    let cell = usize::try_from(cell).expect("painted cell lies on the map");
    state.level.map.cells[cell] = value;
}

fn set_point(state: &mut VaultLevelState, point: Point, value: i32) {
    state.level.map.set_point(point, value);
}

fn draw_inside(state: &mut VaultLevelState, room: &VaultRoom, from: Point, count: i32, value: i32) {
    draw::draw_inside(&mut state.level.map, room.bounds, from, count, value);
}

fn draw_line(state: &mut VaultLevelState, from: Point, to: Point, value: i32) {
    draw::draw_line(&mut state.level.map, from, to, value);
}

fn cell_of(state: &VaultLevelState, x: i32, y: i32) -> i32 {
    state.cell_index(x, y)
}

fn as_cell(cell: i32) -> usize {
    usize::try_from(cell).expect("cell lies on the map")
}

fn map_at(state: &VaultLevelState, cell: i32) -> i32 {
    state.level.map.cells[as_cell(cell)]
}

fn doors_regular(rooms: &mut [VaultRoom], room: usize) {
    let neighbours: Vec<usize> = rooms[room]
        .connected
        .iter()
        .map(|entry| entry.room)
        .collect();
    for neighbour in neighbours {
        set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
    }
}

fn door_points(rooms: &[VaultRoom], room: usize) -> Vec<Point> {
    rooms[room]
        .connected
        .iter()
        .map(|entry| entry.door.expect("doors are placed before painting").point)
        .collect()
}

fn entrance_door(rooms: &[VaultRoom], room: usize) -> Point {
    rooms[room]
        .entrance_door()
        .expect("treasure and final rooms always have a placed entrance")
        .point
}

fn set_entrance_regular(rooms: &mut [VaultRoom], room: usize) {
    let neighbour = rooms[room]
        .entrance_neighbour()
        .expect("treasure and final rooms are connected");
    set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
}

fn neighbours8(width: i32) -> [i32; 8] {
    [
        -width - 1,
        -width,
        -width + 1,
        -1,
        1,
        width - 1,
        width,
        width + 1,
    ]
}

fn neighbours4(width: i32) -> [i32; 4] {
    [-width, -1, 1, width]
}

fn random_index(count: usize, random: &mut RandomStack) -> usize {
    let bound = i32::try_from(count).expect("collection fits Java int");
    usize::try_from(random.int_bound(bound)).expect("Random.Int is non-negative")
}

/// `Random.element(rect.getPoints())` for a plain `Rect`.
fn random_rect_point(rect: Rect, random: &mut RandomStack) -> Point {
    let points: Vec<Point> = rect.points().collect();
    points[random_index(points.len(), random)]
}

/// `(int) GameMath.gate(min, value, max)` on integral inputs.
fn gate(minimum: i32, value: i32, maximum: i32) -> i32 {
    value.clamp(minimum, maximum)
}

fn drop_chest(state: &mut VaultLevelState, item: VaultItem, cell: i32) {
    let heap = state.drop(item, as_cell(cell));
    state.set_heap_kind(heap, VaultHeapKind::Chest);
}

fn drop_heap(state: &mut VaultLevelState, item: VaultItem, cell: i32) {
    state.drop(item, as_cell(cell));
}

/// Dispatches to the concrete `Room.paint(Level)`.
pub(crate) fn paint_room(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    match rooms[room].kind {
        VaultRoomKind::Entrance => paint_entrance(state, rooms, room, random),
        VaultRoomKind::Ring => paint_ring(state, rooms, room, random),
        VaultRoomKind::Cross => paint_cross(state, rooms, room),
        VaultRoomKind::Quadrants => paint_quadrants(state, rooms, room, random),
        VaultRoomKind::Rings => paint_rings(state, rooms, room, random),
        VaultRoomKind::EnemyCenter => paint_enemy_center(state, rooms, room, random),
        VaultRoomKind::Hallway => paint_hallway(state, rooms, room, random),
        VaultRoomKind::LongRings => paint_long_rings(state, rooms, room, random),
        VaultRoomKind::Circle => paint_circle(state, rooms, room, random),
        VaultRoomKind::AlternatingFire => paint_alternating_fire(state, rooms, room, random),
        VaultRoomKind::Lasers => paint_lasers(state, rooms, room, random),
        VaultRoomKind::Tokens => paint_tokens(state, rooms, room, random),
        VaultRoomKind::SimpleEnemyTreasure => {
            paint_simple_enemy_treasure(state, rooms, room, random);
        }
        VaultRoomKind::FlamePath => paint_flame_path(state, rooms, room, random),
        VaultRoomKind::LaserTreasure => paint_laser_treasure(state, rooms, room, random),
        VaultRoomKind::CircleScanTreasure => paint_circle_scan_treasure(state, rooms, room, random),
        VaultRoomKind::SingleEnemyTreasure => {
            paint_single_enemy_treasure(state, rooms, room, random);
        }
        VaultRoomKind::BookcaseTreasure => paint_bookcase_treasure(state, rooms, room, random),
        VaultRoomKind::FlamesTreasure => paint_flames_treasure(state, rooms, room, random),
        VaultRoomKind::ManyScans => paint_many_scans(state, rooms, room, random),
        VaultRoomKind::MultipleEnemyTreasure => {
            paint_multiple_enemy_treasure(state, rooms, room, random);
        }
        VaultRoomKind::HardLaserTreasure => paint_hard_laser_treasure(state, rooms, room, random),
        VaultRoomKind::Final => paint_final(state, rooms, room, random),
    }
}

fn paint_entrance(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    fill_room(state, &this, terrain::WALL);
    fill_room_margin(state, &this, 2, terrain::EMPTY);
    let c = this.center();
    set_xy(state, c.x - 3, c.y - 3, terrain::WALL);
    set_xy(state, c.x + 3, c.y - 3, terrain::WALL);
    set_xy(state, c.x - 3, c.y + 3, terrain::WALL);
    set_xy(state, c.x + 3, c.y + 3, terrain::WALL);

    let neighbours: Vec<usize> = this.connected.iter().map(|entry| entry.room).collect();
    for neighbour in neighbours {
        set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
        let door = rooms[room]
            .connection_to(neighbour)
            .and_then(|entry| entry.door)
            .expect("door placed")
            .point;
        draw_inside(state, &this, door, 3, terrain::EMPTY);
    }

    set_xy(state, c.x - 2, c.y - 2, terrain::REGION_DECO);
    set_xy(state, c.x + 2, c.y - 2, terrain::REGION_DECO);
    set_xy(state, c.x - 2, c.y + 2, terrain::REGION_DECO);
    set_xy(state, c.x + 2, c.y + 2, terrain::REGION_DECO);
    fill(
        state,
        this.bounds.left + 2,
        this.bounds.top + 4,
        7,
        3,
        terrain::CUSTOM_DECO_EMPTY,
    );
    fill(
        state,
        this.bounds.left + 4,
        this.bounds.top + 2,
        3,
        7,
        terrain::CUSTOM_DECO_EMPTY,
    );

    let candidates = [
        Point::new(this.bounds.left + 2, c.y),
        Point::new(this.bounds.right - 2, c.y),
        Point::new(c.x, this.bounds.top + 2),
        Point::new(c.x, this.bounds.bottom - 2),
    ];
    let doors = door_points(rooms, room);
    let mut furthest: Option<Point> = None;
    let mut furthest_dist = 0.0_f32;
    for candidate in candidates {
        let mut dist = 0.0_f32;
        for door in &doors {
            dist += Point::distance(candidate, *door);
        }
        if furthest.is_none() || dist > furthest_dist {
            furthest = Some(candidate);
            furthest_dist = dist;
        }
    }
    let furthest = furthest.expect("four candidates");
    let offset = if furthest.x == c.x { 1 } else { state.width() };
    let add_torch = state.challenges.contains(Challenges::DARKNESS);
    let center_cell = i32::try_from(state.point_to_cell(furthest)).expect("cell fits Java int");
    match random.int_bound(3) {
        0 => {
            if add_torch {
                drop_heap(state, VaultItem::Torch, center_cell - offset);
            }
            drop_heap(state, VaultItem::VaultBeacon, center_cell);
            drop_heap(state, VaultItem::VaultBeacon, center_cell + offset);
        }
        1 => {
            drop_heap(state, VaultItem::VaultBeacon, center_cell - offset);
            if add_torch {
                drop_heap(state, VaultItem::Torch, center_cell);
            }
            drop_heap(state, VaultItem::VaultBeacon, center_cell + offset);
        }
        _ => {
            drop_heap(state, VaultItem::VaultBeacon, center_cell - offset);
            drop_heap(state, VaultItem::VaultBeacon, center_cell);
            if add_torch {
                drop_heap(state, VaultItem::Torch, center_cell + offset);
            }
        }
    }
    let entrance = state.point_to_cell(this.center());
    assert!(
        !state.find_mob(entrance),
        "a mob never occupies the vault entrance"
    );
    state.entrance_cell = Some(entrance);
}

fn paint_ring(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    fill_room(state, &this, terrain::WALL);
    fill_room_margin(state, &this, 1, terrain::EMPTY);
    fill_room_margin(state, &this, 4, terrain::WALL);
    doors_regular(rooms, room);
    let enemy = state.mob_deck.create_mob(random);
    let b = this.bounds;
    let wander = if random.int_bound(2) == 0 {
        [
            Point::new(b.left + 2, b.top + 2),
            Point::new(b.right - 2, b.top + 2),
            Point::new(b.right - 2, b.bottom - 2),
            Point::new(b.left + 2, b.bottom - 2),
        ]
    } else {
        [
            Point::new(b.left + 2, b.bottom - 2),
            Point::new(b.right - 2, b.bottom - 2),
            Point::new(b.right - 2, b.top + 2),
            Point::new(b.left + 2, b.top + 2),
        ]
    };
    let index = random_index(4, random);
    let cell = state.point_to_cell(wander[index]);
    state.add_mob(enemy, cell);
}

fn paint_cross(state: &mut VaultLevelState, rooms: &mut [VaultRoom], room: usize) {
    let this = rooms[room].clone();
    fill_room(state, &this, terrain::WALL);
    fill_room_margins(state, &this, 4, 1, 4, 1, terrain::EMPTY);
    fill_room_margins(state, &this, 1, 4, 1, 4, terrain::EMPTY);
    set_point(state, this.center(), terrain::PEDESTAL);
    let cell = state.point_to_cell(this.center());
    state.add_mob(VaultMobKind::Sentry, cell);
    doors_regular(rooms, room);
}

fn paint_quadrants(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    fill_room(state, &this, terrain::WALL);
    fill_room_margin(state, &this, 1, terrain::EMPTY);
    let c = this.center();
    let b = this.bounds;
    draw_inside(state, &this, Point::new(b.left, c.y), 3, terrain::WALL);
    draw_inside(state, &this, Point::new(b.right, c.y), 3, terrain::WALL);
    draw_inside(state, &this, Point::new(c.x, b.top), 3, terrain::WALL);
    draw_inside(state, &this, Point::new(c.x, b.bottom), 3, terrain::WALL);
    set_point(state, c, terrain::STATUE);
    doors_regular(rooms, room);

    let mut spawn_positions = vec![
        Point::new(b.left + 2, b.top + 2),
        Point::new(b.right - 2, b.top + 2),
        Point::new(b.right - 2, b.bottom - 2),
        Point::new(b.left + 2, b.bottom - 2),
    ];
    let doors = door_points(rooms, room);
    for candidate in spawn_positions.clone() {
        for door in &doors {
            if Point::distance(candidate, *door) <= 3.0 {
                if let Some(index) = spawn_positions.iter().position(|p| *p == candidate) {
                    spawn_positions.remove(index);
                }
            }
        }
    }
    if spawn_positions.is_empty() {
        return;
    }
    let enemy = state.mob_deck.create_mob(random);
    let corner = spawn_positions[random_index(spawn_positions.len(), random)];
    let pos = i32::try_from(state.point_to_cell(corner)).expect("cell fits Java int");
    state.add_mob(enemy, as_cell(pos));
    let tier = enemy.treasure_tier(1);
    let treasure = state.equipment.create_equipment(usize::from(tier), random);
    let mut treasure_pos = pos;
    if corner.x < c.x {
        treasure_pos -= 1;
    } else {
        treasure_pos += 1;
    }
    if corner.y < c.y {
        treasure_pos -= state.width();
    } else {
        treasure_pos += state.width();
    }
    drop_chest(state, VaultItem::Equipment(treasure), treasure_pos);
}

fn paint_rings(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    let b = this.bounds;
    fill_room(state, &this, terrain::WALL);
    fill_room_margin(state, &this, 1, terrain::EMPTY);
    fill(state, b.left + 2, b.top + 2, 3, 3, terrain::WALL);
    fill(state, b.right - 4, b.top + 2, 3, 3, terrain::WALL);
    fill(state, b.left + 2, b.bottom - 4, 3, 3, terrain::WALL);
    fill(state, b.right - 4, b.bottom - 4, 3, 3, terrain::WALL);
    doors_regular(rooms, room);

    let mut to_return = Vec::new();
    let enemy = loop {
        let enemy = state.mob_deck.create_mob(random);
        if enemy.is_large() {
            to_return.push(enemy.class().expect("deck mobs have classes"));
        } else {
            break enemy;
        }
    };
    let pos = loop {
        let pos = state.point_to_cell(this.random_point(1, random));
        if state.level.map.cells[pos] != terrain::WALL {
            break pos;
        }
    };
    state.add_mob(enemy, pos);
    let mut wander = [0_i32; 9];
    for (index, (dx, dy)) in [
        (1, 1),
        (1, 5),
        (1, 9),
        (5, 1),
        (5, 5),
        (5, 9),
        (9, 1),
        (9, 5),
        (9, 9),
    ]
    .into_iter()
    .enumerate()
    {
        wander[index] =
            i32::try_from(state.point_to_cell(Point::new(b.left + dx, b.top + dy))).unwrap();
    }
    random.shuffle_array(&mut wander);
    for class in to_return {
        state.mob_deck.return_mob(class);
    }
}

fn paint_enemy_center(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    let b = this.bounds;
    fill_room(state, &this, terrain::WALL);
    fill_room_margin(state, &this, 1, terrain::EMPTY);
    fill_room_margin(state, &this, 2, terrain::WALL);
    fill_room_margin(state, &this, 3, terrain::EMPTY);
    draw_line(
        state,
        Point::new(b.left + 1, b.top + 3),
        Point::new(b.right - 1, b.top + 3),
        terrain::EMPTY,
    );
    draw_line(
        state,
        Point::new(b.left + 1, b.bottom - 3),
        Point::new(b.right - 1, b.bottom - 3),
        terrain::EMPTY,
    );
    draw_line(
        state,
        Point::new(b.left + 3, b.top + 1),
        Point::new(b.left + 3, b.bottom - 1),
        terrain::EMPTY,
    );
    draw_line(
        state,
        Point::new(b.right - 3, b.top + 1),
        Point::new(b.right - 3, b.bottom - 1),
        terrain::EMPTY,
    );
    doors_regular(rooms, room);

    let enemy = state.mob_deck.create_mob(random);
    let c = this.center();
    let wander = if random.int_bound(2) == 0 {
        [
            Point::new(c.x - 1, c.y - 1),
            Point::new(c.x + 1, c.y - 1),
            Point::new(c.x + 1, c.y + 1),
            Point::new(c.x - 1, c.y + 1),
        ]
    } else {
        [
            Point::new(c.x - 1, c.y - 1),
            Point::new(c.x - 1, c.y + 1),
            Point::new(c.x + 1, c.y + 1),
            Point::new(c.x + 1, c.y - 1),
        ]
    };
    let index = random_index(4, random);
    let cell = state.point_to_cell(wander[index]);
    state.add_mob(enemy, cell);
    let tier = enemy.treasure_tier(1);
    let treasure = state.equipment.create_equipment(usize::from(tier), random);
    let center_cell = i32::try_from(state.point_to_cell(c)).unwrap();
    drop_chest(state, VaultItem::Equipment(treasure), center_cell);
}

fn paint_hallway(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    let b = this.bounds;
    let wide = this.wide();
    fill_room(state, &this, terrain::WALL);
    if wide {
        fill(
            state,
            b.left + 1,
            b.top + 4,
            this.width() - 2,
            3,
            terrain::EMPTY,
        );
        fill(
            state,
            b.left + 6,
            b.top + 1,
            1,
            this.height() - 2,
            terrain::EMPTY,
        );
        fill(
            state,
            b.right - 7,
            b.top + 1,
            1,
            this.height() - 2,
            terrain::EMPTY,
        );
    } else {
        fill(
            state,
            b.left + 4,
            b.top + 1,
            3,
            this.height() - 2,
            terrain::EMPTY,
        );
        fill(
            state,
            b.left + 1,
            b.top + 6,
            this.width() - 2,
            1,
            terrain::EMPTY,
        );
        fill(
            state,
            b.left + 1,
            b.bottom - 7,
            this.width() - 2,
            1,
            terrain::EMPTY,
        );
    }

    let c = this.center();
    let width = state.width();
    let mut loot_positions = if wide {
        vec![b.left + 2 + c.y * width, b.right - 2 + c.y * width]
    } else {
        vec![c.x + (b.top + 2) * width, c.x + (b.bottom - 2) * width]
    };
    for door in door_points(rooms, room) {
        let door_cell = i32::try_from(state.point_to_cell(door)).unwrap();
        for candidate in loot_positions.clone() {
            if state.distance(candidate, door_cell) <= 6 {
                if let Some(index) = loot_positions.iter().position(|p| *p == candidate) {
                    loot_positions.remove(index);
                }
            }
        }
    }
    if loot_positions.is_empty() {
        loot_positions.push(i32::try_from(state.point_to_cell(c)).unwrap());
    }
    if let Some(item) = state.queue.find_prize_item(PrizeFilter::Equipable) {
        drop_heap(state, item, loot_positions[0]);
    }

    let enemy = state.mob_deck.create_mob(random);
    // setupStealthGameplayWanderPositions(..., Random.Int(2)) only stores
    // the starting index; the draw itself is what matters here.
    let _ = random.int_bound(2);
    let center_cell = state.point_to_cell(c);
    state.add_mob(enemy, center_cell);

    let neighbours: Vec<usize> = this.connected.iter().map(|entry| entry.room).collect();
    for neighbour in neighbours {
        set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
        let door = rooms[room]
            .connection_to(neighbour)
            .and_then(|entry| entry.door)
            .expect("door placed")
            .point;
        if wide {
            if door.x == b.left {
                draw_line(
                    state,
                    Point::new(door.x + 1, door.y),
                    Point::new(b.left + 1, c.y),
                    terrain::EMPTY,
                );
            } else if door.x == b.right {
                draw_line(
                    state,
                    Point::new(door.x - 1, door.y),
                    Point::new(b.right - 1, c.y),
                    terrain::EMPTY,
                );
            } else if door.x > b.left + 3 && door.x < b.right - 3 {
                let closest_x = if door.x >= c.x && (door.x != c.x || random.int_bound(2) != 0) {
                    b.right - 7
                } else {
                    b.left + 6
                };
                if door.y == b.top {
                    draw_line(
                        state,
                        Point::new(door.x, door.y + 1),
                        Point::new(closest_x, door.y + 1),
                        terrain::EMPTY,
                    );
                } else {
                    draw_line(
                        state,
                        Point::new(door.x, door.y - 1),
                        Point::new(closest_x, door.y - 1),
                        terrain::EMPTY,
                    );
                }
            } else {
                draw_inside(state, &this, door, 5, terrain::EMPTY);
            }
        } else if door.y == b.top {
            draw_line(
                state,
                Point::new(door.x, door.y + 1),
                Point::new(c.x, b.top + 1),
                terrain::EMPTY,
            );
        } else if door.y == b.bottom {
            draw_line(
                state,
                Point::new(door.x, door.y - 1),
                Point::new(c.x, b.bottom - 1),
                terrain::EMPTY,
            );
        } else if door.y > b.top + 3 && door.y < b.bottom - 3 {
            let closest_y = if door.y >= c.y && (door.y != c.y || random.int_bound(2) != 0) {
                b.bottom - 7
            } else {
                b.top + 6
            };
            if door.x == b.left {
                draw_line(
                    state,
                    Point::new(door.x + 1, door.y),
                    Point::new(door.x + 1, closest_y),
                    terrain::EMPTY,
                );
            } else {
                draw_line(
                    state,
                    Point::new(door.x - 1, door.y),
                    Point::new(door.x - 1, closest_y),
                    terrain::EMPTY,
                );
            }
        } else {
            draw_inside(state, &this, door, 5, terrain::EMPTY);
        }
    }
}

fn paint_long_rings(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    fill_room(state, &this, terrain::WALL);
    fill_room_margin(state, &this, 1, terrain::EMPTY);
    fill_room_margin(state, &this, 4, terrain::WALL);
    if this.wide() {
        fill_room_margins(state, &this, 8, 4, 8, 4, terrain::EMPTY);
    } else {
        fill_room_margins(state, &this, 4, 8, 4, 8, terrain::EMPTY);
    }
    let c = this.center();
    if let Some(item) = state.queue.find_prize_item(PrizeFilter::Equipable) {
        let cell = i32::try_from(state.point_to_cell(c)).unwrap();
        drop_heap(state, item, cell);
    }
    for _ in 0..2 {
        let enemy = state.mob_deck.create_mob(random);
        let mut wander = [0_i32; 20];
        wander[0] = random_wander(state, &this, -1, random);
        for index in 1..wander.len() {
            wander[index] = random_wander(state, &this, wander[index - 1], random);
        }
        state.add_mob(enemy, as_cell(wander[0]));
    }
    doors_regular(rooms, room);
}

fn random_wander(
    state: &VaultLevelState,
    room: &VaultRoom,
    previous: i32,
    random: &mut RandomStack,
) -> i32 {
    loop {
        let pos = i32::try_from(state.point_to_cell(room.random_point(1, random))).unwrap();
        let tile = map_at(state, pos);
        if tile != terrain::WALL && tile != terrain::WALL_DECO && state.distance(pos, previous) >= 6
        {
            return pos;
        }
    }
}

fn paint_circle(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    fill_room(state, &this, terrain::WALL);
    fill_room_margin(state, &this, 2, terrain::EMPTY);
    fill_room_margins(state, &this, 4, 1, 4, 1, terrain::EMPTY);
    fill_room_margins(state, &this, 1, 4, 1, 4, terrain::EMPTY);
    set_point(state, this.center(), terrain::PEDESTAL);
    // The sentry's scan pattern is chosen with Random.Int(4).
    let _ = random.int_bound(4);
    let cell = state.point_to_cell(this.center());
    state.add_mob(VaultMobKind::Sentry, cell);
    let neighbours: Vec<usize> = this.connected.iter().map(|entry| entry.room).collect();
    for neighbour in neighbours {
        set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
        let door = rooms[room]
            .connection_to(neighbour)
            .and_then(|entry| entry.door)
            .expect("door placed")
            .point;
        draw_inside(state, &this, door, 4, terrain::EMPTY);
    }
}

fn paint_alternating_fire(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    let b = this.bounds;
    fill_room(state, &this, terrain::WALL);
    fill_room_margin(state, &this, 1, terrain::EMPTY);
    doors_regular(rooms, room);
    let c = this.center();
    set_point(state, c, terrain::PEDESTAL);
    let item = state.equipment.create_equipment(0, random);
    let cell = i32::try_from(state.point_to_cell(c)).unwrap();
    drop_heap(state, VaultItem::Equipment(item), cell);
    for x in b.left + 1..b.right {
        for y in b.top + 1..b.bottom {
            let cell = cell_of(state, x, y);
            if map_at(state, cell) != terrain::PEDESTAL {
                state.setup_flame_trap(as_cell(cell));
            }
        }
    }
}

fn paint_lasers(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    let b = this.bounds;
    fill_room(state, &this, terrain::WALL);
    fill_room_margin(state, &this, 2, terrain::EMPTY);
    let neighbours: Vec<usize> = this.connected.iter().map(|entry| entry.room).collect();
    for neighbour in neighbours {
        let door = rooms[room]
            .connection_to(neighbour)
            .and_then(|entry| entry.door)
            .expect("door placed")
            .point;
        draw_inside(state, &this, door, 2, terrain::EMPTY);
        set_shared_door_type(rooms, room, neighbour, DoorType::Regular);
    }
    let width = state.width();
    for x in b.left + 2..=b.right - 2 {
        if map_at(state, x + (b.top + 1) * width) == terrain::WALL
            && map_at(state, x + (b.bottom - 1) * width) == terrain::WALL
        {
            let cell = if random.int_bound(2) == 0 {
                x + width * (b.top + 1)
            } else {
                x + width * (b.bottom - 1)
            };
            set_cell(state, cell, terrain::PEDESTAL);
            let after_shot = random.int_range(3, 7);
            let _ = random.int_range(1, after_shot);
            state.add_mob(VaultMobKind::Laser, as_cell(cell));
        }
    }
    for y in b.top + 2..=b.bottom - 2 {
        if map_at(state, b.left + 1 + y * width) == terrain::WALL
            && map_at(state, b.right - 1 + y * width) == terrain::WALL
        {
            let cell = if random.int_bound(2) == 0 {
                b.left + 1 + width * y
            } else {
                b.right - 1 + width * y
            };
            set_cell(state, cell, terrain::PEDESTAL);
            let after_shot = random.int_range(3, 7);
            let _ = random.int_range(1, after_shot);
            state.add_mob(VaultMobKind::Laser, as_cell(cell));
        }
    }
}

fn paint_tokens(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    let b = this.bounds;
    fill_room(state, &this, terrain::WALL);
    let c = this.center();
    let doors = door_points(rooms, room);
    if this.wide() {
        let mut left_door = 0;
        let mut right_door = 0;
        for door in &doors {
            if door.x < c.x {
                left_door = 1;
                fill(
                    state,
                    b.left + 1,
                    b.top + 1,
                    (this.width() - 2) / 2,
                    this.height() - 2,
                    terrain::EMPTY,
                );
            } else {
                right_door = 1;
                fill(
                    state,
                    c.x + 1,
                    b.top + 1,
                    (this.width() - 2) / 2,
                    this.height() - 2,
                    terrain::EMPTY,
                );
            }
        }
        fill_diamond(state, b.left + 3, b.top + 1, 9, 9, terrain::WALL);
        fill_diamond(state, b.left + 9, b.top + 1, 9, 9, terrain::WALL);
        fill_diamond(state, b.left + 5, b.top + 1, 9, 9, terrain::EMPTY_SP);
        fill_diamond(state, b.left + 7, b.top + 1, 9, 9, terrain::EMPTY_SP);
        fill(
            state,
            b.left + 4 - left_door,
            c.y,
            13 + left_door + right_door,
            1,
            terrain::EMPTY,
        );
        fill(state, b.left + 4, c.y, 13, 1, terrain::EMPTY_SP);
    } else {
        let mut top_door = 0;
        let mut bottom_door = 0;
        for door in &doors {
            if door.y < c.y {
                top_door = 1;
                fill(
                    state,
                    b.left + 1,
                    b.top + 1,
                    this.width() - 2,
                    (this.height() - 2) / 2,
                    terrain::EMPTY,
                );
            } else {
                bottom_door = 1;
                fill(
                    state,
                    b.left + 1,
                    c.y + 1,
                    this.width() - 2,
                    (this.height() - 2) / 2,
                    terrain::EMPTY,
                );
            }
        }
        fill_diamond(state, b.left + 1, b.top + 3, 9, 9, terrain::WALL);
        fill_diamond(state, b.left + 1, b.top + 9, 9, 9, terrain::WALL);
        fill_diamond(state, b.left + 1, b.top + 5, 9, 9, terrain::EMPTY_SP);
        fill_diamond(state, b.left + 1, b.top + 7, 9, 9, terrain::EMPTY_SP);
        fill(
            state,
            c.x,
            b.top + 4 - top_door,
            1,
            13 + top_door + bottom_door,
            terrain::EMPTY,
        );
        fill(state, c.x, b.top + 4, 1, 13, terrain::EMPTY_SP);
    }
    fill_diamond(state, c.x - 3, c.y - 3, 7, 7, terrain::WALL);
    fill(state, c.x - 1, c.y - 1, 3, 3, terrain::EMPTY_SP);
    fill(state, c.x - 2, c.y, 5, 1, terrain::EMPTY_SP);
    fill(state, c.x, c.y - 1, 1, 5, terrain::EMPTY_SP);
    set_xy(state, c.x - 1, c.y - 1, terrain::REGION_DECO_ALT);
    set_xy(state, c.x + 1, c.y - 1, terrain::REGION_DECO_ALT);
    set_xy(state, c.x, c.y + 3, terrain::LOCKED_DOOR);
    let width = state.width();
    state.add_mob(VaultMobKind::TokenDoor, as_cell(c.x + (c.y + 3) * width));
    // VaultMirror.createReward: pushGenerator(Random.Long()), then the
    // class-specific reward is rolled inside the child generator.
    let _ = random.long();
    state.add_mob(VaultMobKind::Mirror, as_cell(c.x + (c.y - 1) * width));

    let left_cell = c.x - 2 + c.y * width;
    let right_cell = c.x + 2 + c.y * width;
    if random.int_bound(2) == 0 {
        let equipment = state.equipment.create_equipment(3, random);
        drop_heap(state, VaultItem::Equipment(equipment), left_cell);
        let consumable = state.consumables.create_consumable(3, random);
        drop_heap(state, consumable, right_cell);
    } else {
        let equipment = state.equipment.create_equipment(3, random);
        drop_heap(state, VaultItem::Equipment(equipment), right_cell);
        let consumable = state.consumables.create_consumable(3, random);
        drop_heap(state, consumable, left_cell);
    }

    let mut to_return: Vec<VaultMobClass> = Vec::new();
    let enemy = loop {
        let enemy = state.mob_deck.create_mob(random);
        if enemy.is_large() {
            to_return.push(enemy.class().expect("deck mobs have classes"));
        } else {
            break enemy;
        }
    };
    // Both branches build the same wander list; only the draw matters.
    let _ = random.int_bound(2);
    let wander = [
        Point::new(c.x - 4, c.y),
        Point::new(c.x, c.y - 4),
        Point::new(c.x + 4, c.y),
        Point::new(c.x, c.y + 4),
    ];
    let index = random_index(4, random);
    let cell = state.point_to_cell(wander[index]);
    state.add_mob(enemy, cell);
    for class in to_return {
        state.mob_deck.return_mob(class);
    }
}

fn paint_simple_enemy_treasure(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    let b = this.bounds;
    fill_room(state, &this, terrain::WALL);
    fill_room_margin(state, &this, 1, terrain::EMPTY);
    let (enemy_pos, treasure_pos) = match random.int_bound(4) {
        0 => {
            fill(state, b.left + 2, b.top + 2, 6, 6, terrain::WALL);
            fill(state, b.left + 3, b.top + 3, 4, 4, terrain::EMPTY_SP);
            fill(state, b.left + 4, b.top + 7, 2, 1, terrain::EMPTY_SP);
            fill(state, b.left + 7, b.top + 4, 1, 2, terrain::EMPTY_SP);
            (
                Point::new(b.left + 4, b.top + 4),
                Point::new(b.left + 3, b.top + 3),
            )
        }
        1 => {
            fill(state, b.left + 3, b.top + 2, 6, 6, terrain::WALL);
            fill(state, b.left + 4, b.top + 3, 4, 4, terrain::EMPTY_SP);
            fill(state, b.left + 5, b.top + 7, 2, 1, terrain::EMPTY_SP);
            fill(state, b.left + 3, b.top + 4, 1, 2, terrain::EMPTY_SP);
            (
                Point::new(b.right - 4, b.top + 4),
                Point::new(b.right - 3, b.top + 3),
            )
        }
        2 => {
            fill(state, b.left + 3, b.top + 3, 6, 6, terrain::WALL);
            fill(state, b.left + 4, b.top + 4, 4, 4, terrain::EMPTY_SP);
            fill(state, b.left + 5, b.top + 3, 2, 1, terrain::EMPTY_SP);
            fill(state, b.left + 3, b.top + 5, 1, 2, terrain::EMPTY_SP);
            (
                Point::new(b.right - 4, b.bottom - 4),
                Point::new(b.right - 3, b.bottom - 3),
            )
        }
        _ => {
            fill(state, b.left + 2, b.top + 3, 6, 6, terrain::WALL);
            fill(state, b.left + 3, b.top + 4, 4, 4, terrain::EMPTY_SP);
            fill(state, b.left + 4, b.top + 3, 2, 1, terrain::EMPTY_SP);
            fill(state, b.left + 7, b.top + 5, 1, 2, terrain::EMPTY_SP);
            (
                Point::new(b.left + 4, b.bottom - 4),
                Point::new(b.left + 3, b.bottom - 3),
            )
        }
    };
    let mut to_return = Vec::new();
    let enemy = loop {
        let enemy = state.mob_deck.create_mob(random);
        if enemy.is_tier_one() {
            to_return.push(enemy.class().expect("deck mobs have classes"));
        } else {
            break enemy;
        }
    };
    for class in to_return {
        state.mob_deck.return_mob(class);
    }
    let tier = enemy.treasure_tier(2);
    let treasure = state.equipment.create_equipment(usize::from(tier), random);
    let treasure_cell = i32::try_from(state.point_to_cell(treasure_pos)).unwrap();
    drop_chest(state, VaultItem::Equipment(treasure), treasure_cell);
    doors_regular(rooms, room);
    let enemy_cell = state.point_to_cell(enemy_pos);
    state.add_mob(enemy, enemy_cell);
}

fn fill_flame_group(
    state: &mut VaultLevelState,
    space: Rect,
    direction: u8,
    random: &mut RandomStack,
) {
    let width = state.width();
    let mut prior_offsets: Vec<i32> = Vec::new();
    let rows_first = matches!(direction, 0 | 2);
    let outer: Vec<i32> = if rows_first {
        (space.top..=space.bottom).collect()
    } else {
        (space.left..=space.right).collect()
    };
    for line in outer {
        let offset = loop {
            let offset = random.int_bound(5);
            if !prior_offsets.contains(&offset) {
                break offset;
            }
        };
        let mut delay = 0;
        let inner: Vec<i32> = match direction {
            0 => (space.left..=space.right).rev().collect(),
            2 => (space.left..=space.right).collect(),
            1 => (space.top..=space.bottom).rev().collect(),
            _ => (space.top..=space.bottom).collect(),
        };
        for step in inner {
            let cell = if rows_first {
                step + width * line
            } else {
                line + width * step
            };
            // setupTrap(level, cell, delay + ofs, 5, 2)
            state.setup_flame_trap(as_cell(cell));
            delay += 1;
        }
        let _ = delay;
        prior_offsets.push(offset);
    }
}

fn paint_flame_path(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    let b = this.bounds;
    fill_room(state, &this, terrain::WALL);
    let entrance = entrance_door(rooms, room);
    set_entrance_regular(rooms, room);
    let c = this.center();
    let width = state.width();
    let left_side = Rect::new(b.left + 1, b.top + 1, b.left + 3, b.bottom - 1);
    let top_side = Rect::new(b.left + 1, b.top + 1, b.right - 1, b.top + 3);
    let right_side = Rect::new(b.right - 3, b.top + 1, b.right - 1, b.bottom - 1);
    let bottom_side = Rect::new(b.left + 1, b.bottom - 3, b.right - 1, b.bottom - 1);
    let treasure = if entrance.x == b.left {
        if entrance.y < c.y {
            fill_flame_group(state, bottom_side, 2, random);
            fill_flame_group(state, right_side, 1, random);
            fill_flame_group(state, top_side, 0, random);
            set_cell(state, b.left + 1 + width * (b.top + 6), terrain::EMPTY_SP);
        } else {
            fill_flame_group(state, top_side, 2, random);
            fill_flame_group(state, right_side, 3, random);
            fill_flame_group(state, bottom_side, 0, random);
            set_cell(state, b.left + 1 + width * (b.top + 4), terrain::EMPTY_SP);
        }
        Rect::new(b.left + 1, b.top + 5, b.left + 5, b.top + 5)
    } else if entrance.y == b.top {
        if entrance.x < c.x {
            fill_flame_group(state, right_side, 3, random);
            fill_flame_group(state, bottom_side, 0, random);
            fill_flame_group(state, left_side, 1, random);
            set_cell(state, b.left + 6 + width * (b.top + 1), terrain::EMPTY_SP);
        } else {
            fill_flame_group(state, left_side, 3, random);
            fill_flame_group(state, bottom_side, 2, random);
            fill_flame_group(state, right_side, 1, random);
            set_cell(state, b.left + 4 + width * (b.top + 1), terrain::EMPTY_SP);
        }
        Rect::new(b.left + 5, b.top + 1, b.left + 5, b.top + 5)
    } else if entrance.x == b.right {
        if entrance.y < c.y {
            fill_flame_group(state, bottom_side, 0, random);
            fill_flame_group(state, left_side, 1, random);
            fill_flame_group(state, top_side, 2, random);
            set_cell(state, b.right - 1 + width * (b.top + 6), terrain::EMPTY_SP);
        } else {
            fill_flame_group(state, top_side, 0, random);
            fill_flame_group(state, left_side, 3, random);
            fill_flame_group(state, bottom_side, 2, random);
            set_cell(state, b.right - 1 + width * (b.top + 4), terrain::EMPTY_SP);
        }
        Rect::new(b.right - 5, b.top + 5, b.right - 1, b.top + 5)
    } else {
        if entrance.x < c.x {
            fill_flame_group(state, right_side, 1, random);
            fill_flame_group(state, top_side, 0, random);
            fill_flame_group(state, left_side, 3, random);
            set_cell(
                state,
                b.left + 6 + width * (b.bottom - 1),
                terrain::EMPTY_SP,
            );
        } else {
            fill_flame_group(state, left_side, 1, random);
            fill_flame_group(state, top_side, 2, random);
            fill_flame_group(state, right_side, 3, random);
            set_cell(
                state,
                b.left + 4 + width * (b.bottom - 1),
                terrain::EMPTY_SP,
            );
        }
        Rect::new(b.left + 5, b.bottom - 5, b.left + 5, b.bottom - 1)
    };
    fill(
        state,
        treasure.left,
        treasure.top,
        treasure.width() + 1,
        treasure.height() + 1,
        terrain::EMPTY_SP,
    );
    let treasure_pos = state.point_to_cell(random_rect_point(treasure, random));
    let equipment = state.equipment.create_equipment(1, random);
    drop_chest(
        state,
        VaultItem::Equipment(equipment),
        i32::try_from(treasure_pos).unwrap(),
    );
    let item = match state.queue.find_t2_solve_item(random) {
        Some(item) => item,
        None => state.consumables.create_consumable(1, random),
    };
    let pos = loop {
        let pos = state.point_to_cell(random_rect_point(treasure, random));
        if state.heap_at(pos).is_none() {
            break pos;
        }
    };
    state.drop(item, pos);
    let pos = loop {
        let pos = state.point_to_cell(random_rect_point(treasure, random));
        if state.heap_at(pos).is_none() {
            break pos;
        }
    };
    state.drop(VaultItem::DwarfToken, pos);
}

/// Shared body of `VaultLaserTreasureRoom` (`hard == false`) and
/// `VaultHardLaserTreasureRoom` (`hard == true`).
#[allow(clippy::too_many_lines)]
fn paint_laser_treasure_common(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
    hard: bool,
) {
    let this = rooms[room].clone();
    let b = this.bounds;
    fill_room(state, &this, terrain::WALL);
    let entrance = entrance_door(rooms, room);
    set_entrance_regular(rooms, room);
    let width = state.width();
    let (inset, area_terrain) = if hard {
        (1, terrain::EMPTY_SP)
    } else {
        (2, terrain::EMPTY)
    };
    let laser_offset = i32::from(!hard);
    let item_place = if entrance.x != b.left && entrance.x != b.right {
        let area_left = gate(b.left + inset, entrance.x - 2, b.right - 4 - inset);
        fill(state, area_left, b.top + 1, 5, 9, area_terrain);
        let (rows, item_place): (Vec<i32>, Rect) = if entrance.y == b.top {
            (
                (b.top + 2..b.bottom - 1).rev().collect(),
                Rect::new(area_left, b.bottom - 1, area_left + 4, b.bottom - 1),
            )
        } else {
            (
                (b.top + 2..b.bottom - 1).collect(),
                Rect::new(area_left, b.top + 1, area_left + 4, b.top + 1),
            )
        };
        for y in rows {
            let first = area_left - laser_offset + width * y;
            set_cell(state, first, terrain::PEDESTAL);
            state.add_mob(VaultMobKind::Laser, as_cell(first));
            let second = area_left + 4 + laser_offset + width * y;
            set_cell(state, second, terrain::PEDESTAL);
            state.add_mob(VaultMobKind::Laser, as_cell(second));
        }
        item_place
    } else {
        let area_top = gate(b.top + inset, entrance.y - 2, b.bottom - 4 - inset);
        fill(state, b.left + 1, area_top, 9, 5, area_terrain);
        let (columns, item_place): (Vec<i32>, Rect) = if entrance.x == b.left {
            (
                (b.left + 2..b.right - 1).rev().collect(),
                Rect::new(b.right - 1, area_top, b.right - 1, area_top + 4),
            )
        } else {
            (
                (b.left + 2..b.right - 1).collect(),
                Rect::new(b.left + 1, area_top, b.left + 1, area_top + 4),
            )
        };
        for x in columns {
            let first = x + width * (area_top - laser_offset);
            set_cell(state, first, terrain::PEDESTAL);
            state.add_mob(VaultMobKind::Laser, as_cell(first));
            let second = x + width * (area_top + 4 + laser_offset);
            set_cell(state, second, terrain::PEDESTAL);
            state.add_mob(VaultMobKind::Laser, as_cell(second));
        }
        item_place
    };
    if !hard {
        draw_inside(state, &this, entrance, 1, terrain::EMPTY);
    }
    fill(
        state,
        item_place.left,
        item_place.top,
        item_place.width() + 1,
        item_place.height() + 1,
        terrain::EMPTY_SP,
    );
    let treasure_pos = state.point_to_cell(random_rect_point(item_place, random));
    let tier = if hard { 3 } else { 1 };
    let equipment = state.equipment.create_equipment(tier, random);
    drop_chest(
        state,
        VaultItem::Equipment(equipment),
        i32::try_from(treasure_pos).unwrap(),
    );
    let item = if hard {
        state.consumables.create_consumable(3, random)
    } else {
        match state.queue.find_t2_solve_item(random) {
            Some(item) => item,
            None => state.consumables.create_consumable(1, random),
        }
    };
    let pos = loop {
        let pos = state.point_to_cell(random_rect_point(item_place, random));
        if state.heap_at(pos).is_none() {
            break pos;
        }
    };
    state.drop(item, pos);
    let pos = loop {
        let pos = state.point_to_cell(random_rect_point(item_place, random));
        if state.heap_at(pos).is_none() {
            break pos;
        }
    };
    state.drop(VaultItem::DwarfToken, pos);
    if hard {
        state.queue.add(VaultItem::Consumable(
            crate::vault_loot::VaultConsumable::Stone(crate::generator::StoneKind::Blink),
        ));
    }
}

fn paint_laser_treasure(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    paint_laser_treasure_common(state, rooms, room, random, false);
}

fn paint_hard_laser_treasure(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    paint_laser_treasure_common(state, rooms, room, random, true);
}

fn paint_circle_scan_treasure(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    let b = this.bounds;
    fill_room(state, &this, terrain::WALL);
    fill_room_margin(state, &this, 2, terrain::EMPTY);
    fill_room_margins(state, &this, 4, 1, 4, 1, terrain::EMPTY);
    fill_room_margins(state, &this, 1, 4, 1, 4, terrain::EMPTY);
    let entrance = entrance_door(rooms, room);
    set_entrance_regular(rooms, room);
    draw_inside(state, &this, entrance, 3, terrain::EMPTY);
    let c = this.center();
    set_point(state, c, terrain::PEDESTAL);
    let mut treasure;
    if entrance.x == b.left {
        set_xy(state, c.x + 2, c.y, terrain::WALL);
        set_xy(state, c.x + 1, c.y, terrain::STATUE);
        set_xy(state, c.x - 1, c.y, terrain::STATUE);
        treasure = Rect::new(b.left + 1, c.y - 1, c.x - 2, c.y + 1);
        fill(
            state,
            treasure.left,
            treasure.top,
            treasure.width() + 1,
            treasure.height() + 1,
            terrain::WALL,
        );
        treasure.right -= 1;
        if entrance.y < c.y {
            treasure.top += 1;
        } else {
            treasure.bottom -= 1;
        }
    } else if entrance.y == b.top {
        set_xy(state, c.x, c.y + 2, terrain::WALL);
        set_xy(state, c.x, c.y + 1, terrain::STATUE);
        set_xy(state, c.x, c.y - 1, terrain::STATUE);
        treasure = Rect::new(c.x - 1, b.top + 1, c.x + 1, b.top + 3);
        fill(
            state,
            treasure.left,
            treasure.top,
            treasure.width() + 1,
            treasure.height() + 1,
            terrain::WALL,
        );
        treasure.bottom -= 1;
        if entrance.x < c.x {
            treasure.left += 1;
        } else {
            treasure.right -= 1;
        }
    } else if entrance.x == b.right {
        set_xy(state, c.x - 2, c.y, terrain::WALL);
        set_xy(state, c.x - 1, c.y, terrain::STATUE);
        set_xy(state, c.x + 1, c.y, terrain::STATUE);
        treasure = Rect::new(b.right - 3, c.y - 1, b.right - 1, c.y + 1);
        fill(
            state,
            treasure.left,
            treasure.top,
            treasure.width() + 1,
            treasure.height() + 1,
            terrain::WALL,
        );
        treasure.left += 1;
        if entrance.y < c.y {
            treasure.top += 1;
        } else {
            treasure.bottom -= 1;
        }
    } else {
        set_xy(state, c.x, c.y - 2, terrain::WALL);
        set_xy(state, c.x, c.y - 1, terrain::STATUE);
        set_xy(state, c.x, c.y + 1, terrain::STATUE);
        treasure = Rect::new(c.x - 1, b.bottom - 3, c.x + 1, b.bottom - 1);
        fill(
            state,
            treasure.left,
            treasure.top,
            treasure.width() + 1,
            treasure.height() + 1,
            terrain::WALL,
        );
        treasure.top += 1;
        if entrance.x < c.x {
            treasure.left += 1;
        } else {
            treasure.right -= 1;
        }
    }
    fill(
        state,
        treasure.left,
        treasure.top,
        treasure.width() + 1,
        treasure.height() + 1,
        terrain::EMPTY_SP,
    );
    let center_cell = state.point_to_cell(c);
    state.add_mob(VaultMobKind::Sentry, center_cell);
    let treasure_pos = state.point_to_cell(random_rect_point(treasure, random));
    let equipment = state.equipment.create_equipment(1, random);
    drop_chest(
        state,
        VaultItem::Equipment(equipment),
        i32::try_from(treasure_pos).unwrap(),
    );
    let item = match state.queue.find_t2_solve_item(random) {
        Some(item) => item,
        None => state.consumables.create_consumable(1, random),
    };
    let pos = loop {
        let pos = state.point_to_cell(random_rect_point(treasure, random));
        if state.heap_at(pos).is_none() {
            break pos;
        }
    };
    state.drop(item, pos);
    let pos = loop {
        let pos = state.point_to_cell(random_rect_point(treasure, random));
        if state.heap_at(pos).is_none() {
            break pos;
        }
    };
    state.drop(VaultItem::DwarfToken, pos);
}

fn paint_single_enemy_treasure(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    let b = this.bounds;
    fill_room(state, &this, terrain::WALL);
    fill_ellipse_room_margin(state, &this, 3, terrain::EMPTY);
    let entrance = entrance_door(rooms, room);
    draw_inside(state, &this, entrance, 3, terrain::EMPTY);
    let enemy = random_tier_two_enemy(random);
    let width = state.width();
    let mut enemy_pos = i32::try_from(state.point_to_cell(this.center())).unwrap();
    let treasure_pos;
    if entrance.x == b.left {
        treasure_pos = enemy_pos + 2;
        enemy_pos += 1;
    } else if entrance.y == b.top {
        treasure_pos = enemy_pos + 2 * width;
        enemy_pos += width;
    } else if entrance.x == b.right {
        treasure_pos = enemy_pos - 2;
        enemy_pos -= 1;
    } else {
        treasure_pos = enemy_pos - 2 * width;
        enemy_pos -= width;
    }
    state.add_mob(enemy, as_cell(enemy_pos));
    let equipment = state.equipment.create_equipment(2, random);
    drop_chest(state, VaultItem::Equipment(equipment), treasure_pos);
    let offsets = neighbours4(width);
    let offset = loop {
        let offset = offsets[random_index(4, random)];
        if map_at(state, treasure_pos + offset) != terrain::WALL
            && treasure_pos + offset != enemy_pos
        {
            break offset;
        }
    };
    let consumable = state.consumables.create_consumable(2, random);
    drop_heap(state, consumable, treasure_pos + offset);
    set_entrance_regular(rooms, room);
}

fn paint_bookcase_treasure(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    let b = this.bounds;
    fill_room(state, &this, terrain::WALL);
    let entrance = entrance_door(rooms, room);
    set_entrance_regular(rooms, room);
    let width = state.width();
    let (first_item, second_item) = if entrance.x != b.left && entrance.x != b.right {
        let book_left = gate(b.left + 1, entrance.x - 2, b.right - 5);
        fill(state, book_left, b.top + 1, 5, 9, terrain::BOOKSHELF);
        fill(state, book_left + 1, b.top + 2, 3, 3, terrain::EMPTY_SP);
        fill(state, book_left + 1, b.top + 6, 3, 3, terrain::EMPTY_SP);
        if entrance.y == b.top {
            (
                book_left + 2 + width * (b.top + 3),
                book_left + 2 + width * (b.bottom - 3),
            )
        } else {
            (
                book_left + 2 + width * (b.bottom - 3),
                book_left + 2 + width * (b.top + 3),
            )
        }
    } else {
        let book_top = gate(b.top + 1, entrance.y - 2, b.bottom - 5);
        fill(state, b.left + 1, book_top, 9, 5, terrain::BOOKSHELF);
        fill(state, b.left + 2, book_top + 1, 3, 3, terrain::EMPTY_SP);
        fill(state, b.left + 6, book_top + 1, 3, 3, terrain::EMPTY_SP);
        if entrance.x == b.left {
            (
                b.left + 3 + width * (book_top + 2),
                b.right - 3 + width * (book_top + 2),
            )
        } else {
            (
                b.right - 3 + width * (book_top + 2),
                b.left + 3 + width * (book_top + 2),
            )
        }
    };
    set_cell(state, first_item, terrain::PEDESTAL);
    set_cell(state, second_item, terrain::PEDESTAL);
    if let Some(item) = state.queue.find_random_prize_item(random) {
        drop_heap(state, item, first_item);
    }
    let equipment = state.equipment.create_equipment(2, random);
    drop_chest(state, VaultItem::Equipment(equipment), second_item);
    let item = match state.queue.find_t3_solve_item(random) {
        Some(item) => item,
        None => state.consumables.create_consumable(2, random),
    };
    let offsets = neighbours8(width);
    let offset = offsets[random_index(8, random)];
    drop_heap(state, item, second_item + offset);
    let offset = offsets[random_index(8, random)];
    drop_heap(state, VaultItem::DwarfToken, second_item + offset);
    state.queue.add(VaultItem::Consumable(
        crate::vault_loot::VaultConsumable::Potion(crate::run::PotionKind::LiquidFlame),
    ));
    draw_inside(state, &this, entrance, 2, terrain::EMPTY_SP);
}

fn paint_flames_treasure(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    let b = this.bounds;
    fill_room(state, &this, terrain::WALL);
    fill_ellipse_room_margin(state, &this, 2, terrain::EMPTY);
    let c = this.center();
    let width = state.width();
    let center_cell = i32::try_from(state.point_to_cell(c)).unwrap();
    let positions = [
        center_cell - 3 * width,
        center_cell + 3,
        center_cell + 3 * width,
        center_cell - 3,
        center_cell - 3 * width,
        center_cell + 3,
    ];
    let entrance = entrance_door(rooms, room);
    let treasure_index = if entrance.x == b.left {
        1
    } else if entrance.y == b.top {
        2
    } else if entrance.x == b.right {
        3
    } else {
        4
    };
    set_cell(state, positions[treasure_index - 1], terrain::PEDESTAL);
    set_cell(state, positions[treasure_index], terrain::PEDESTAL);
    set_cell(state, positions[treasure_index + 1], terrain::PEDESTAL);
    let equipment = state.equipment.create_equipment(2, random);
    drop_chest(
        state,
        VaultItem::Equipment(equipment),
        positions[treasure_index],
    );
    for x in b.left + 2..=b.right - 2 {
        for y in b.top + 2..=b.bottom - 2 {
            let cell = x + width * y;
            if map_at(state, cell) == terrain::EMPTY {
                state.setup_flame_trap(as_cell(cell));
            }
        }
    }
    if (c.x - entrance.x).abs() > 1 && (c.y - entrance.y).abs() > 1 {
        draw_inside(state, &this, entrance, 2, terrain::EMPTY);
    } else {
        draw_inside(state, &this, entrance, 1, terrain::EMPTY);
    }
    let item = match state.queue.find_t3_solve_item(random) {
        Some(item) => item,
        None => state.consumables.create_consumable(2, random),
    };
    if random.int_bound(2) == 0 {
        drop_heap(state, item, positions[treasure_index - 1]);
        drop_heap(state, VaultItem::DwarfToken, positions[treasure_index + 1]);
    } else {
        drop_heap(state, VaultItem::DwarfToken, positions[treasure_index - 1]);
        drop_heap(state, item, positions[treasure_index + 1]);
    }
    state.queue.add(VaultItem::Consumable(
        crate::vault_loot::VaultConsumable::Potion(crate::run::PotionKind::Purity),
    ));
    set_entrance_regular(rooms, room);
}

fn paint_many_scans(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    let b = this.bounds;
    fill_room(state, &this, terrain::WALL);
    fill_room_margin(state, &this, 1, terrain::EMPTY_SP);
    let width = state.width();
    let c = this.center();
    let corners = [
        b.left + 1 + width * (b.top + 1),
        c.x + width * (b.top + 1),
        b.right - 1 + width * (b.top + 1),
        b.left + 1 + width * c.y,
        b.right - 1 + width * c.y,
        b.left + 1 + width * (b.bottom - 1),
        c.x + width * (b.bottom - 1),
        b.right - 1 + width * (b.bottom - 1),
    ];
    let entrance = entrance_door(rooms, room);
    set_entrance_regular(rooms, room);
    let entrance_cell = i32::try_from(state.point_to_cell(entrance)).unwrap();
    for cell in corners {
        if state.true_distance(cell, entrance_cell) >= 2.0 {
            state.add_mob(VaultMobKind::Sentry, as_cell(cell));
        }
    }
    set_point(state, c, terrain::PEDESTAL);
    let center_cell = c.x + width * c.y;
    let equipment = state.equipment.create_equipment(3, random);
    drop_chest(state, VaultItem::Equipment(equipment), center_cell);
    let consumable = state.consumables.create_consumable(3, random);
    let offsets = neighbours8(width);
    let offset = offsets[random_index(8, random)];
    drop_heap(state, consumable, center_cell + offset);
    let offset = offsets[random_index(8, random)];
    drop_heap(state, VaultItem::DwarfToken, center_cell + offset);
    state.queue.add(VaultItem::Consumable(
        crate::vault_loot::VaultConsumable::Potion(crate::run::PotionKind::Invisibility),
    ));
}

fn paint_multiple_enemy_treasure(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    let b = this.bounds;
    fill_room(state, &this, terrain::WALL);
    let entrance = entrance_door(rooms, room);
    set_entrance_regular(rooms, room);
    let c = this.center();
    let width = state.width();
    let treasure_pos = if entrance.x != b.left && entrance.x != b.right {
        let area_left = gate(b.left + 1, entrance.x - 2, b.right - 5);
        fill(state, area_left + 1, b.top + 1, 3, 9, terrain::EMPTY_SP);
        fill(state, area_left, b.top + 4, 5, 3, terrain::EMPTY_SP);
        state.add_mob(VaultMobKind::Ghoul, as_cell(area_left + c.y * width));
        state.add_mob(VaultMobKind::Ghoul, as_cell(area_left + 4 + c.y * width));
        let (ghoul, treasure) = if entrance.y == b.top {
            let ghoul = area_left + 2 + (c.y + 1) * width;
            (ghoul, ghoul + 2 * width)
        } else {
            let ghoul = area_left + 2 + (c.y - 1) * width;
            (ghoul, ghoul - 2 * width)
        };
        state.add_mob(VaultMobKind::Ghoul, as_cell(ghoul));
        treasure
    } else {
        let area_top = gate(b.top + 1, entrance.y - 2, b.bottom - 5);
        fill(state, b.left + 1, area_top + 1, 9, 3, terrain::EMPTY_SP);
        fill(state, b.left + 4, area_top, 3, 5, terrain::EMPTY_SP);
        state.add_mob(VaultMobKind::Ghoul, as_cell(c.x + area_top * width));
        state.add_mob(VaultMobKind::Ghoul, as_cell(c.x + (area_top + 4) * width));
        let (ghoul, treasure) = if entrance.x == b.left {
            let ghoul = c.x + 1 + (area_top + 2) * width;
            (ghoul, ghoul + 2)
        } else {
            let ghoul = c.x - 1 + (area_top + 2) * width;
            (ghoul, ghoul - 2)
        };
        state.add_mob(VaultMobKind::Ghoul, as_cell(ghoul));
        treasure
    };
    set_cell(state, treasure_pos, terrain::PEDESTAL);
    let equipment = state.equipment.create_equipment(3, random);
    drop_chest(state, VaultItem::Equipment(equipment), treasure_pos);
    let offsets = neighbours8(width);
    let offset = loop {
        let offset = offsets[random_index(8, random)];
        if map_at(state, treasure_pos + offset) != terrain::WALL {
            break offset;
        }
    };
    let consumable = state.consumables.create_consumable(3, random);
    drop_heap(state, consumable, treasure_pos + offset);
}

fn paint_final(
    state: &mut VaultLevelState,
    rooms: &mut [VaultRoom],
    room: usize,
    random: &mut RandomStack,
) {
    let this = rooms[room].clone();
    let b = this.bounds;
    fill_room(state, &this, terrain::WALL);
    fill_ellipse_room_margin(state, &this, 5, terrain::EMPTY_SP);
    let c = this.center();
    let entrance = entrance_door(rooms, room);
    set_entrance_regular(rooms, room);
    // `new EmptyRoom()` twice: each StandardRoom constructor calls
    // setSizeCat(), one Random.chances float against {1, 0, 0}.
    let _ = random.chances(&[1.0, 0.0, 0.0]);
    let _ = random.chances(&[1.0, 0.0, 0.0]);
    let (entry, entry_door, treasure, locked_door) = if entrance.x == b.left {
        (
            Rect::new(b.left + 1, b.top + 5, b.left + 3, b.bottom - 5),
            Point::new(b.left + 4, c.y),
            Rect::new(b.right - 3, b.top + 3, b.right - 1, b.bottom - 3),
            Point::new(b.right - 4, c.y),
        )
    } else if entrance.x == b.right {
        (
            Rect::new(b.right - 3, b.top + 5, b.right - 1, b.bottom - 5),
            Point::new(b.right - 4, c.y),
            Rect::new(b.left + 1, b.top + 3, b.left + 3, b.bottom - 3),
            Point::new(b.left + 4, c.y),
        )
    } else if entrance.y == b.top {
        (
            Rect::new(b.left + 5, b.top + 1, b.right - 5, b.top + 3),
            Point::new(c.x, b.top + 4),
            Rect::new(b.left + 3, b.bottom - 3, b.right - 3, b.bottom - 1),
            Point::new(c.x, b.bottom - 4),
        )
    } else {
        (
            Rect::new(b.left + 5, b.bottom - 3, b.right - 5, b.bottom - 1),
            Point::new(c.x, b.bottom - 4),
            Rect::new(b.left + 3, b.top + 1, b.right - 3, b.top + 3),
            Point::new(c.x, b.top + 4),
        )
    };
    set_point(state, entry_door, terrain::DOOR);
    set_point(state, locked_door, terrain::LOCKED_DOOR);
    fill_inclusive_rect(state, entry, terrain::CUSTOM_DECO_EMPTY);
    let width = state.width();
    let mut treasure_spots: Vec<i32> = Vec::with_capacity(7);
    // Room.width()/height() are inclusive on these EmptyRoom helpers.
    if entry.width() + 1 > entry.height() + 1 {
        for dx in [1, 3, 7, 9] {
            set_xy(state, entry.left + dx, entry.top + 1, terrain::REGION_DECO);
        }
        for dx in [1, 3, 5, 7, 9, 11, 13] {
            treasure_spots.push(treasure.left + dx + (treasure.top + 1) * width);
        }
    } else {
        for dy in [1, 3, 7, 9] {
            set_xy(state, entry.left + 1, entry.top + dy, terrain::REGION_DECO);
        }
        for dy in [1, 3, 5, 7, 9, 11, 13] {
            treasure_spots.push(treasure.left + 1 + (treasure.top + dy) * width);
        }
    }
    fill_inclusive_rect(state, treasure, terrain::EMPTY_SP);
    for &cell in &treasure_spots {
        set_cell(state, cell, terrain::PEDESTAL);
    }
    let statue_cell = treasure_spots.remove(3);
    drop_heap(state, VaultItem::ImpStatue, statue_cell);
    random.shuffle_list(&mut treasure_spots);
    for index in 0..6_u8 {
        let cell = treasure_spots.remove(0);
        drop_heap(state, VaultItem::ImpRewardOption(index), cell);
    }
}
