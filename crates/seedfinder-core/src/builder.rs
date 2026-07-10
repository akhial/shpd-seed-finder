//! Exact room-placement builders used by v3.3.8 regular Sewer levels.
//!
//! This is a direct, draw-for-draw port of `Builder`, `RegularBuilder`,
//! `LoopBuilder`, and `FigureEightBuilder`.  Java's room list contains object
//! identities and can hold not-yet-positioned initial rooms. During a build we
//! therefore keep an explicit `active` list of [`RoomId`] values. Newly
//! allocated tunnel rooms only enter that list at the same point Java calls
//! `rooms.add`, and successful builds are compacted back into exact Java list
//! order before being returned.

// These routines deliberately retain the shape, sentinel comparisons, and
// integer formulas of their Java sources so operation and RNG ordering remain
// auditable against upstream.
#![allow(
    clippy::float_cmp,
    clippy::manual_div_ceil,
    clippy::manual_range_contains,
    clippy::missing_panics_doc,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]

use crate::geometry::{Point, Rect};
use crate::java_math::{f32_to_i32, round_f32, round_f64};
use crate::rng::RandomStack;
use crate::room::{
    ConnectionRoomKind, Direction, Room, RoomId, add_neighbour, clear_all_connections,
    clear_connections, connect_rooms, create_connection_room,
};

const ANGLE_FACTOR: f64 = 180.0 / std::f64::consts::PI;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct PointF {
    x: f32,
    y: f32,
}

impl PointF {
    const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Shared parameters and branch logic from Java's abstract `RegularBuilder`.
#[derive(Clone, Debug, PartialEq)]
pub struct RegularBuilder {
    pub path_variance: f32,
    pub path_length: f32,
    pub path_length_jitter_chances: Vec<f32>,
    pub path_tunnel_chances: Vec<f32>,
    pub branch_tunnel_chances: Vec<f32>,
    pub extra_connection_chance: f32,
}

impl Default for RegularBuilder {
    fn default() -> Self {
        Self {
            path_variance: 45.0,
            path_length: 0.25,
            path_length_jitter_chances: vec![0.0, 0.0, 0.0, 1.0],
            path_tunnel_chances: vec![2.0, 2.0, 1.0],
            branch_tunnel_chances: vec![1.0, 1.0, 0.0],
            extra_connection_chance: 0.30,
        }
    }
}

impl RegularBuilder {
    #[must_use]
    pub fn set_path_variance(mut self, variance: f32) -> Self {
        self.path_variance = variance;
        self
    }

    #[must_use]
    pub fn set_path_length(mut self, length: f32, jitter: &[f32]) -> Self {
        self.path_length = length;
        self.path_length_jitter_chances = jitter.to_vec();
        self
    }

    #[must_use]
    pub fn set_tunnel_length(mut self, path: &[f32], branch: &[f32]) -> Self {
        self.path_tunnel_chances = path.to_vec();
        self.branch_tunnel_chances = branch.to_vec();
        self
    }

    #[must_use]
    pub fn set_extra_connection_chance(mut self, chance: f32) -> Self {
        self.extra_connection_chance = chance;
        self
    }

    fn setup_rooms(
        &self,
        rooms: &mut [Room],
        active: &[RoomId],
        rng: &mut RandomStack,
    ) -> SetupRooms {
        for &room in active {
            rooms[room].set_empty();
        }

        let mut setup = SetupRooms::default();
        for &room in active {
            if rooms[room].is_entrance() {
                setup.entrance = Some(room);
            } else if rooms[room].is_exit() {
                setup.exit = Some(room);
            } else if rooms[room].is_shop() && rooms[room].max_connections(Direction::All) == 1 {
                setup.shop = Some(room);
            } else if rooms[room].max_connections(Direction::All) > 1 {
                setup.multi_connections.push(room);
            } else if rooms[room].max_connections(Direction::All) == 1 {
                setup.single_connections.push(room);
            }
        }

        weight_rooms(rooms, &mut setup.multi_connections);
        rng.shuffle_list(&mut setup.multi_connections);
        deduplicate_in_place(&mut setup.multi_connections);
        rng.shuffle_list(&mut setup.multi_connections);

        #[allow(clippy::cast_precision_loss)]
        let multi_count = setup.multi_connections.len() as f32;
        let jitter = rng
            .chances(&self.path_length_jitter_chances)
            .map_or(-1, |index| i32::try_from(index).unwrap_or(i32::MAX));
        let mut rooms_on_main_path =
            f32_to_i32(multi_count * self.path_length).wrapping_add(jitter);

        while rooms_on_main_path > 0 && !setup.multi_connections.is_empty() {
            let room = setup.multi_connections.remove(0);
            if rooms[room].is_standard() {
                rooms_on_main_path = rooms_on_main_path.wrapping_sub(rooms[room].size_factor());
            } else {
                rooms_on_main_path = rooms_on_main_path.wrapping_sub(1);
            }
            setup.main_path_rooms.push(room);
        }
        setup
    }
}

#[derive(Debug, Default)]
struct SetupRooms {
    entrance: Option<RoomId>,
    exit: Option<RoomId>,
    shop: Option<RoomId>,
    main_path_rooms: Vec<RoomId>,
    multi_connections: Vec<RoomId>,
    single_connections: Vec<RoomId>,
}

/// Common interface for concrete regular builders.
pub trait Builder {
    /// Positions and connects `rooms`, returning `false` exactly where the
    /// Java builder returns `null`. Callers retry with the same already-drawn
    /// initial room objects and the continuing RNG stream.
    fn build_with_shop_size_hook(
        &mut self,
        rooms: &mut Vec<Room>,
        depth: u32,
        rng: &mut RandomStack,
        shop_size_hook: &mut dyn FnMut(&mut Room, &mut RandomStack),
    ) -> bool;

    /// Builds without a dynamic shop-content owner. This is exact for floors
    /// without a shop and remains the convenient entry point for isolated
    /// builder tests.
    fn build(&mut self, rooms: &mut Vec<Room>, depth: u32, rng: &mut RandomStack) -> bool {
        self.build_with_shop_size_hook(rooms, depth, rng, &mut |_, _| {})
    }
}

/// `Builder.findNeighbours` over an explicit Java room-list order.
pub fn find_neighbours(rooms: &mut [Room], order: &[RoomId]) {
    for first_index in 0..order.len().saturating_sub(1) {
        for second_index in first_index + 1..order.len() {
            add_neighbour(rooms, order[first_index], order[second_index]);
        }
    }
}

/// `Builder.findFreeSpace`. The cumulative `inside` and `curDiff` variables
/// inside each pass intentionally reproduce quirks in the Java source.
pub fn find_free_space(
    start: Point,
    rooms: &[Room],
    collision: &[RoomId],
    max_size: i32,
    rng: &mut RandomStack,
) -> Rect {
    let space = Rect::new(
        start.x.wrapping_sub(max_size),
        start.y.wrapping_sub(max_size),
        start.x.wrapping_add(max_size),
        start.y.wrapping_add(max_size),
    );
    // The passes below rescan the collision list repeatedly. Copying the
    // rectangles up front keeps those scans on a dense array instead of
    // striding through the much larger `Room` structs; empty bounds stay
    // empty, so filtering them once up front matches retesting every pass.
    // This function runs thousands of times per generated seed, so the copy
    // lives in a per-thread buffer that is allocated once and reused, rather
    // than in per-call stack or heap storage.
    COLLIDING_SCRATCH.with(|scratch| {
        let mut colliding = scratch.borrow_mut();
        colliding.clear();
        colliding.extend(
            collision
                .iter()
                .map(|&room| rooms[room].bounds)
                .filter(|bounds| !bounds.is_empty()),
        );
        free_space_from_collisions(start, &mut colliding, space, rng)
    })
}

thread_local! {
    static COLLIDING_SCRATCH: std::cell::RefCell<Vec<Rect>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

fn free_space_from_collisions(
    start: Point,
    colliding: &mut [Rect],
    mut space: Rect,
    rng: &mut RandomStack,
) -> Rect {
    let mut count = colliding.len();
    loop {
        // One fused pass compacts away rectangles no longer intersecting the
        // shrinking `space` and runs Java's closest-collision scan over the
        // survivors, in their original order.
        let mut kept = 0_usize;
        let mut closest_room = None;
        let mut closest_difference = i32::MAX;
        let mut inside = true;
        let mut current_difference = 0_i32;
        for index in 0..count {
            let bounds = colliding[index];
            // Everything below is predicated on `intersects` with conditional
            // moves rather than branches: which rectangles survive each pass
            // is data-dependent and defeats the branch predictor otherwise.
            #[allow(clippy::needless_bitwise_bool)]
            let intersects = (space.left.max(bounds.left) < space.right.min(bounds.right))
                & (space.top.max(bounds.top) < space.bottom.min(bounds.bottom));
            colliding[kept] = bounds;
            kept += usize::from(intersects);

            // Branch-free form of Java's axis checks: bounds are non-empty
            // (left < right, top < bottom), so at most one term per axis is
            // positive and equals exactly the summand the original branch
            // chain would have added; boundary contact contributes zero but
            // still clears `inside`, as the original `<=`/`>=` tests did.
            // Rectangles the retain pass dropped contribute nothing.
            let excess = bounds
                .left
                .wrapping_sub(start.x)
                .max(0)
                .wrapping_add(start.x.wrapping_sub(bounds.right).max(0))
                .wrapping_add(bounds.top.wrapping_sub(start.y).max(0))
                .wrapping_add(start.y.wrapping_sub(bounds.bottom).max(0));
            current_difference =
                current_difference.wrapping_add(if intersects { excess } else { 0 });
            #[allow(clippy::needless_bitwise_bool)]
            let strictly_inside = (start.x > bounds.left)
                & (start.x < bounds.right)
                & (start.y > bounds.top)
                & (start.y < bounds.bottom);
            #[allow(clippy::needless_bitwise_bool)]
            {
                inside &= strictly_inside | !intersects;
            }

            #[allow(clippy::needless_bitwise_bool)]
            if inside & intersects {
                space.set(start.x, start.y, start.x, start.y);
                return space;
            }
            #[allow(clippy::needless_bitwise_bool)]
            let better = intersects & (current_difference < closest_difference);
            closest_difference = if better {
                current_difference
            } else {
                closest_difference
            };
            closest_room = if better { Some(kept - 1) } else { closest_room };
        }
        count = kept;

        if let Some(closest) = closest_room {
            let bounds = colliding[closest];
            let mut width_difference = i32::MAX;
            if bounds.left >= start.x {
                width_difference = space
                    .right
                    .wrapping_sub(bounds.left)
                    .wrapping_mul(space.height().wrapping_add(1));
            } else if bounds.right <= start.x {
                width_difference = bounds
                    .right
                    .wrapping_sub(space.left)
                    .wrapping_mul(space.height().wrapping_add(1));
            }

            let mut height_difference = i32::MAX;
            if bounds.top >= start.y {
                height_difference = space
                    .bottom
                    .wrapping_sub(bounds.top)
                    .wrapping_mul(space.width().wrapping_add(1));
            } else if bounds.bottom <= start.y {
                height_difference = bounds
                    .bottom
                    .wrapping_sub(space.top)
                    .wrapping_mul(space.width().wrapping_add(1));
            }

            let reduce_width = width_difference < height_difference
                || (width_difference == height_difference && rng.int_bound(2) == 0);
            if reduce_width {
                if bounds.left >= start.x && bounds.left < space.right {
                    space.right = bounds.left;
                }
                if bounds.right <= start.x && bounds.right > space.left {
                    space.left = bounds.right;
                }
            } else {
                if bounds.top >= start.y && bounds.top < space.bottom {
                    space.bottom = bounds.top;
                }
                if bounds.bottom <= start.y && bounds.bottom > space.top {
                    space.top = bounds.bottom;
                }
            }
            colliding.copy_within(closest + 1..count, closest);
            count -= 1;
        } else {
            count = 0;
        }

        if count == 0 {
            return space;
        }
    }
}

/// Exact float/double operation ordering of `Builder.angleBetweenPoints`.
fn angle_between_points(from: PointF, to: PointF) -> f32 {
    // Java first performs this division as float, then widens it to double.
    let slope = f64::from((to.y - from.y) / (to.x - from.x));
    #[allow(clippy::cast_possible_truncation)]
    let mut angle = (ANGLE_FACTOR * (slope.atan() + std::f64::consts::PI / 2.0)) as f32;
    if from.x > to.x {
        angle -= 180.0;
    }
    angle
}

/// Angle between inclusive room centerpoints, with zero pointing upward.
#[must_use]
pub fn angle_between_rooms(rooms: &[Room], from: RoomId, to: RoomId) -> f32 {
    let from_bounds = rooms[from].bounds;
    let to_bounds = rooms[to].bounds;
    #[allow(clippy::cast_precision_loss)]
    let from_center = PointF::new(
        from_bounds.left.wrapping_add(from_bounds.right) as f32 / 2.0,
        from_bounds.top.wrapping_add(from_bounds.bottom) as f32 / 2.0,
    );
    #[allow(clippy::cast_precision_loss)]
    let to_center = PointF::new(
        to_bounds.left.wrapping_add(to_bounds.right) as f32 / 2.0,
        to_bounds.top.wrapping_add(to_bounds.bottom) as f32 / 2.0,
    );
    angle_between_points(from_center, to_center)
}

fn gate(minimum: i32, value: i32, maximum: i32) -> i32 {
    // `GameMath.gate(float,float,float)` followed by `(int)`.
    #[allow(clippy::cast_precision_loss)]
    let minimum = minimum as f32;
    #[allow(clippy::cast_precision_loss)]
    let value = value as f32;
    #[allow(clippy::cast_precision_loss)]
    let maximum = maximum as f32;
    f32_to_i32(if value < minimum {
        minimum
    } else if value > maximum {
        maximum
    } else {
        value
    })
}

/// `Builder.placeRoom`. `collision` is the exact Java list supplied at that
/// call; `next` may be allocated but not yet present in it.
pub fn place_room(
    rooms: &mut [Room],
    collision: &[RoomId],
    previous: RoomId,
    next: RoomId,
    angle: f32,
    rng: &mut RandomStack,
) -> f32 {
    place_room_with_shop_size_hook(rooms, collision, previous, next, angle, rng, &mut |_, _| {})
}

/// `Builder.placeRoom` with the one virtual side effect that cannot live in
/// [`Room`]: the first `ShopRoom.minWidth()` call generates and caches stock.
/// The hook runs after `findFreeSpace` and immediately before
/// `setSizeWithLimit`, exactly where Java first evaluates that minimum.
pub fn place_room_with_shop_size_hook(
    rooms: &mut [Room],
    collision: &[RoomId],
    previous: RoomId,
    next: RoomId,
    mut angle: f32,
    rng: &mut RandomStack,
    shop_size_hook: &mut dyn FnMut(&mut Room, &mut RandomStack),
) -> f32 {
    angle %= 360.0;
    if angle < 0.0 {
        angle += 360.0;
    }

    let previous_bounds = rooms[previous].bounds;
    #[allow(clippy::cast_precision_loss)]
    let previous_center = PointF::new(
        previous_bounds.left.wrapping_add(previous_bounds.right) as f32 / 2.0,
        previous_bounds.top.wrapping_add(previous_bounds.bottom) as f32 / 2.0,
    );

    let slope = (f64::from(angle) / ANGLE_FACTOR + std::f64::consts::PI / 2.0).tan();
    let intercept = f64::from(previous_center.y) - slope * f64::from(previous_center.x);

    let (direction, mut start) = if slope.abs() >= 1.0 {
        if angle < 90.0 || angle > 270.0 {
            #[allow(clippy::cast_possible_truncation)]
            let x = round_f64((f64::from(previous_bounds.top) - intercept) / slope) as i32;
            (Direction::Top, Point::new(x, previous_bounds.top))
        } else {
            #[allow(clippy::cast_possible_truncation)]
            let x = round_f64((f64::from(previous_bounds.bottom) - intercept) / slope) as i32;
            (Direction::Bottom, Point::new(x, previous_bounds.bottom))
        }
    } else if angle < 180.0 {
        #[allow(clippy::cast_possible_truncation)]
        let y = round_f64(slope * f64::from(previous_bounds.right) + intercept) as i32;
        (Direction::Right, Point::new(previous_bounds.right, y))
    } else {
        #[allow(clippy::cast_possible_truncation)]
        let y = round_f64(slope * f64::from(previous_bounds.left) + intercept) as i32;
        (Direction::Left, Point::new(previous_bounds.left, y))
    };

    if matches!(direction, Direction::Top | Direction::Bottom) {
        start.x = gate(
            previous_bounds.left.wrapping_add(1),
            start.x,
            previous_bounds.right.wrapping_sub(1),
        );
    } else {
        start.y = gate(
            previous_bounds.top.wrapping_add(1),
            start.y,
            previous_bounds.bottom.wrapping_sub(1),
        );
    }

    let max_size = rooms[next].max_width().max(rooms[next].max_height());
    let space = find_free_space(start, rooms, collision, max_size, rng);
    if rooms[next].is_shop() {
        shop_size_hook(&mut rooms[next], rng);
    }
    if !rooms[next].set_size_with_limit(
        space.width().wrapping_add(1),
        space.height().wrapping_add(1),
        rng,
    ) {
        return -1.0;
    }

    match direction {
        Direction::Top => {
            #[allow(clippy::cast_precision_loss)]
            let target_y =
                previous_bounds.top as f32 - rooms[next].height().wrapping_sub(1) as f32 / 2.0;
            #[allow(clippy::cast_possible_truncation)]
            let target_x = ((f64::from(target_y) - intercept) / slope) as f32;
            #[allow(clippy::cast_precision_loss)]
            let x = round_f32(target_x - rooms[next].width().wrapping_sub(1) as f32 / 2.0);
            let y = previous_bounds
                .top
                .wrapping_sub(rooms[next].height().wrapping_sub(1));
            rooms[next].set_position(x, y);
        }
        Direction::Bottom => {
            #[allow(clippy::cast_precision_loss)]
            let target_y =
                previous_bounds.bottom as f32 + rooms[next].height().wrapping_sub(1) as f32 / 2.0;
            #[allow(clippy::cast_possible_truncation)]
            let target_x = ((f64::from(target_y) - intercept) / slope) as f32;
            #[allow(clippy::cast_precision_loss)]
            let x = round_f32(target_x - rooms[next].width().wrapping_sub(1) as f32 / 2.0);
            rooms[next].set_position(x, previous_bounds.bottom);
        }
        Direction::Right => {
            #[allow(clippy::cast_precision_loss)]
            let target_x =
                previous_bounds.right as f32 + rooms[next].width().wrapping_sub(1) as f32 / 2.0;
            #[allow(clippy::cast_possible_truncation)]
            let target_y = (slope * f64::from(target_x) + intercept) as f32;
            #[allow(clippy::cast_precision_loss)]
            let y = round_f32(target_y - rooms[next].height().wrapping_sub(1) as f32 / 2.0);
            rooms[next].set_position(previous_bounds.right, y);
        }
        Direction::Left => {
            #[allow(clippy::cast_precision_loss)]
            let target_x =
                previous_bounds.left as f32 - rooms[next].width().wrapping_sub(1) as f32 / 2.0;
            #[allow(clippy::cast_possible_truncation)]
            let target_y = (slope * f64::from(target_x) + intercept) as f32;
            #[allow(clippy::cast_precision_loss)]
            let y = round_f32(target_y - rooms[next].height().wrapping_sub(1) as f32 / 2.0);
            let x = previous_bounds
                .left
                .wrapping_sub(rooms[next].width().wrapping_sub(1));
            rooms[next].set_position(x, y);
        }
        Direction::All => unreachable!(),
    }

    if matches!(direction, Direction::Top | Direction::Bottom) {
        if rooms[next].bounds.right < previous_bounds.left.wrapping_add(2) {
            rooms[next].shift(
                previous_bounds
                    .left
                    .wrapping_add(2)
                    .wrapping_sub(rooms[next].bounds.right),
                0,
            );
        } else if rooms[next].bounds.left > previous_bounds.right.wrapping_sub(2) {
            rooms[next].shift(
                previous_bounds
                    .right
                    .wrapping_sub(2)
                    .wrapping_sub(rooms[next].bounds.left),
                0,
            );
        }

        if rooms[next].bounds.right > space.right {
            rooms[next].shift(space.right.wrapping_sub(rooms[next].bounds.right), 0);
        } else if rooms[next].bounds.left < space.left {
            rooms[next].shift(space.left.wrapping_sub(rooms[next].bounds.left), 0);
        }
    } else {
        if rooms[next].bounds.bottom < previous_bounds.top.wrapping_add(2) {
            rooms[next].shift(
                0,
                previous_bounds
                    .top
                    .wrapping_add(2)
                    .wrapping_sub(rooms[next].bounds.bottom),
            );
        } else if rooms[next].bounds.top > previous_bounds.bottom.wrapping_sub(2) {
            rooms[next].shift(
                0,
                previous_bounds
                    .bottom
                    .wrapping_sub(2)
                    .wrapping_sub(rooms[next].bounds.top),
            );
        }

        if rooms[next].bounds.bottom > space.bottom {
            rooms[next].shift(0, space.bottom.wrapping_sub(rooms[next].bounds.bottom));
        } else if rooms[next].bounds.top < space.top {
            rooms[next].shift(0, space.top.wrapping_sub(rooms[next].bounds.top));
        }
    }

    if connect_rooms(rooms, next, previous, rng) {
        angle_between_rooms(rooms, previous, next)
    } else {
        -1.0
    }
}

fn selected_chance(chances: &mut Vec<f32>, defaults: &[f32], rng: &mut RandomStack) -> usize {
    if let Some(index) = rng.chances(chances) {
        index
    } else {
        *chances = defaults.to_vec();
        rng.chances(chances)
            .expect("builder chance deck must contain positive weight")
    }
}

fn weight_rooms(rooms: &[Room], weighted: &mut Vec<RoomId>) {
    let snapshot = weighted.clone();
    for room in snapshot {
        if rooms[room].is_standard() {
            for _ in 1..rooms[room].connection_weight() {
                weighted.push(room);
            }
        }
    }
}

fn deduplicate_in_place(values: &mut Vec<RoomId>) {
    let mut unique = Vec::with_capacity(values.len());
    for &value in values.iter() {
        if !unique.contains(&value) {
            unique.push(value);
        }
    }
    *values = unique;
}

fn remove_first(values: &mut Vec<RoomId>, value: RoomId) -> bool {
    if let Some(index) = values.iter().position(|&candidate| candidate == value) {
        values.remove(index);
        true
    } else {
        false
    }
}

fn append_pending(rooms: &mut Vec<Room>, room: Room) -> RoomId {
    let id = rooms.len();
    rooms.push(room);
    id
}

fn add_if_new(active: &mut Vec<RoomId>, room: RoomId) {
    if !active.contains(&room) {
        active.push(room);
    }
}

fn compact_to_active_order(rooms: &mut Vec<Room>, active: &[RoomId]) {
    let mut remap = vec![usize::MAX; rooms.len()];
    for (new, &old) in active.iter().enumerate() {
        remap[old] = new;
    }
    let mut compacted: Vec<Room> = active.iter().map(|&room| rooms[room].clone()).collect();
    for room in &mut compacted {
        for neighbour in &mut room.neighbours {
            *neighbour = remap[*neighbour];
            debug_assert_ne!(*neighbour, usize::MAX);
        }
        for connection in &mut room.connected {
            connection.room = remap[connection.room];
            debug_assert_ne!(connection.room, usize::MAX);
        }
    }
    *rooms = compacted;
}

fn add_extra_connections(
    rooms: &mut [Room],
    active: &[RoomId],
    chance: f32,
    rng: &mut RandomStack,
) {
    for &room in active {
        let neighbours = rooms[room].neighbours.clone();
        for neighbour in neighbours {
            if rooms[neighbour].connection_to(room).is_none() && rng.float() < chance {
                connect_rooms(rooms, room, neighbour, rng);
            }
        }
    }
}

fn create_branches<F>(
    regular: &RegularBuilder,
    rooms: &mut Vec<Room>,
    active: &mut Vec<RoomId>,
    branchable: &mut Vec<RoomId>,
    rooms_to_branch: &[RoomId],
    connection_chances: &[f32],
    depth: u32,
    rng: &mut RandomStack,
    mut random_branch_angle: F,
) -> bool
where
    F: FnMut(RoomId, &[Room], &mut RandomStack) -> f32,
{
    let mut room_index = 0_usize;
    let mut failed_branch_attempts = 0_i32;
    let mut chance_deck = connection_chances.to_vec();
    let mut connecting_this_branch = Vec::new();

    while room_index < rooms_to_branch.len() {
        if failed_branch_attempts > 100 {
            return false;
        }
        let room = rooms_to_branch[room_index];
        connecting_this_branch.clear();

        let current = loop {
            let index = usize::try_from(rng.int_bound(
                i32::try_from(branchable.len()).expect("branchable list exceeds Java int"),
            ))
            .expect("Random.Int is non-negative");
            let candidate = branchable[index];
            if !(rooms[room].is_secret() && rooms[candidate].is_connection()) {
                break candidate;
            }
        };
        let mut current = current;

        let connecting_rooms = selected_chance(&mut chance_deck, connection_chances, rng);
        chance_deck[connecting_rooms] -= 1.0;

        for _ in 0..connecting_rooms {
            let connection = if rooms[room].is_secret() {
                Room::connection(ConnectionRoomKind::Maze)
            } else {
                create_connection_room(depth, rng)
            };
            let tunnel = append_pending(rooms, connection);
            let mut tries = 3;
            let angle = loop {
                let target = random_branch_angle(current, rooms, rng);
                let placed = place_room(rooms, active, current, tunnel, target, rng);
                tries -= 1;
                if placed != -1.0 || tries <= 0 {
                    break placed;
                }
            };
            if angle == -1.0 {
                clear_connections(rooms, tunnel);
                for &connection in &connecting_this_branch {
                    clear_connections(rooms, connection);
                    remove_first(active, connection);
                }
                connecting_this_branch.clear();
                break;
            }
            connecting_this_branch.push(tunnel);
            active.push(tunnel);
            current = tunnel;
        }

        if connecting_this_branch.len() != connecting_rooms {
            failed_branch_attempts = failed_branch_attempts.wrapping_add(1);
            continue;
        }

        let mut tries = 10;
        let angle = loop {
            let target = random_branch_angle(current, rooms, rng);
            let placed = place_room(rooms, active, current, room, target, rng);
            tries -= 1;
            if placed != -1.0 || tries <= 0 {
                break placed;
            }
        };
        if angle == -1.0 {
            clear_connections(rooms, room);
            for &connection in &connecting_this_branch {
                clear_connections(rooms, connection);
                remove_first(active, connection);
            }
            connecting_this_branch.clear();
            failed_branch_attempts = failed_branch_attempts.wrapping_add(1);
            continue;
        }

        for &connection in &connecting_this_branch {
            if rng.int_bound(3) <= 1 {
                branchable.push(connection);
            }
        }
        if rooms[room].max_connections(Direction::All) > 1 && rng.int_bound(3) == 0 {
            if rooms[room].is_standard() {
                for _ in 0..rooms[room].connection_weight() {
                    branchable.push(room);
                }
            } else {
                branchable.push(room);
            }
        }
        room_index += 1;
    }

    // Kept as a parameter for source-level correspondence and future regular
    // builders; Java's current Loop/FigureEight overrides do not read it.
    let _ = regular.path_variance;
    true
}

/// The regular-level builder with one core loop.
#[derive(Clone, Debug, PartialEq)]
pub struct LoopBuilder {
    pub regular: RegularBuilder,
    curve_exponent: i32,
    curve_intensity: f32,
    curve_offset: f32,
    loop_center: Option<PointF>,
}

impl Default for LoopBuilder {
    fn default() -> Self {
        Self {
            regular: RegularBuilder::default(),
            curve_exponent: 0,
            curve_intensity: 1.0,
            curve_offset: 0.0,
            loop_center: None,
        }
    }
}

impl LoopBuilder {
    #[must_use]
    pub fn set_loop_shape(mut self, exponent: i32, intensity: f32, offset: f32) -> Self {
        self.curve_exponent = exponent.wrapping_abs();
        self.curve_intensity = intensity % 1.0;
        self.curve_offset = offset % 0.5;
        self
    }

    #[must_use]
    pub fn set_path_variance(mut self, variance: f32) -> Self {
        self.regular = self.regular.set_path_variance(variance);
        self
    }

    #[must_use]
    pub fn set_path_length(mut self, length: f32, jitter: &[f32]) -> Self {
        self.regular = self.regular.set_path_length(length, jitter);
        self
    }

    #[must_use]
    pub fn set_tunnel_length(mut self, path: &[f32], branch: &[f32]) -> Self {
        self.regular = self.regular.set_tunnel_length(path, branch);
        self
    }

    #[must_use]
    pub fn set_extra_connection_chance(mut self, chance: f32) -> Self {
        self.regular = self.regular.set_extra_connection_chance(chance);
        self
    }

    fn curve_equation(&self, x: f64) -> f64 {
        let doubled = self.curve_exponent.wrapping_mul(2);
        4_f64.powf(f64::from(doubled)) * ((x % 0.5) - 0.25).powf(f64::from(doubled.wrapping_add(1)))
            + 0.25
            + 0.5 * (2.0 * x).floor()
    }

    fn target_angle(&self, mut percent_along: f32) -> f32 {
        percent_along += self.curve_offset;
        // The second product is evaluated as float in Java before the whole
        // expression is promoted to double by curveEquation's result.
        let linear = (1.0_f32 - self.curve_intensity) * percent_along;
        #[allow(clippy::cast_possible_truncation)]
        let curved = (f64::from(self.curve_intensity)
            * self.curve_equation(f64::from(percent_along))
            + f64::from(linear)
            - f64::from(self.curve_offset)) as f32;
        360.0_f32 * curved
    }
}

impl Builder for LoopBuilder {
    fn build_with_shop_size_hook(
        &mut self,
        rooms: &mut Vec<Room>,
        depth: u32,
        rng: &mut RandomStack,
        shop_size_hook: &mut dyn FnMut(&mut Room, &mut RandomStack),
    ) -> bool {
        // Tunnel rooms pushed mid-build would otherwise regrow the room list
        // (and memcpy the large `Room` structs) several times per attempt.
        rooms.reserve(64);
        clear_all_connections(rooms);
        let mut active: Vec<RoomId> = (0..rooms.len()).collect();
        let mut setup = self.regular.setup_rooms(rooms, &active, rng);
        let Some(entrance) = setup.entrance else {
            return false;
        };

        rooms[entrance].set_size(rng);
        rooms[entrance].set_position(0, 0);
        let start_angle = rng.float_between(0.0, 360.0);

        setup.main_path_rooms.insert(0, entrance);
        if let Some(exit) = setup.exit {
            let index = (setup.main_path_rooms.len() + 1) / 2;
            setup.main_path_rooms.insert(index, exit);
        }

        let mut loop_rooms = Vec::new();
        let mut tunnel_deck = self.regular.path_tunnel_chances.clone();
        for room in setup.main_path_rooms.iter().copied() {
            loop_rooms.push(room);
            let tunnels = selected_chance(&mut tunnel_deck, &self.regular.path_tunnel_chances, rng);
            tunnel_deck[tunnels] -= 1.0;
            for _ in 0..tunnels {
                let tunnel = append_pending(rooms, create_connection_room(depth, rng));
                loop_rooms.push(tunnel);
            }
        }

        let mut previous = entrance;
        for index in 1..loop_rooms.len() {
            let room = loop_rooms[index];
            #[allow(clippy::cast_precision_loss)]
            let percent = index as f32 / loop_rooms.len() as f32;
            let target = start_angle + self.target_angle(percent);
            let placed = place_room(rooms, &active, previous, room, target, rng);
            if placed == -1.0 {
                return false;
            }
            previous = room;
            add_if_new(&mut active, previous);
        }

        while !connect_rooms(rooms, previous, entrance, rng) {
            let tunnel = append_pending(rooms, create_connection_room(depth, rng));
            let target = angle_between_rooms(rooms, previous, entrance);
            if place_room(rooms, &loop_rooms, previous, tunnel, target, rng) == -1.0 {
                return false;
            }
            loop_rooms.push(tunnel);
            active.push(tunnel);
            previous = tunnel;
        }

        if let Some(shop) = setup.shop {
            let mut tries = 10;
            let angle = loop {
                let target = rng.float_bound(360.0);
                let placed = place_room_with_shop_size_hook(
                    rooms,
                    &loop_rooms,
                    entrance,
                    shop,
                    target,
                    rng,
                    shop_size_hook,
                );
                tries -= 1;
                if placed != -1.0 || tries < 0 {
                    break placed;
                }
            };
            if angle == -1.0 {
                return false;
            }
        }

        let mut center = PointF::default();
        for &room in &loop_rooms {
            let bounds = rooms[room].bounds;
            #[allow(clippy::cast_precision_loss)]
            {
                center.x += bounds.left.wrapping_add(bounds.right) as f32 / 2.0;
                center.y += bounds.top.wrapping_add(bounds.bottom) as f32 / 2.0;
            }
        }
        #[allow(clippy::cast_precision_loss)]
        {
            center.x /= loop_rooms.len() as f32;
            center.y /= loop_rooms.len() as f32;
        }
        self.loop_center = Some(center);

        let mut branchable = loop_rooms.clone();
        let mut rooms_to_branch = setup.multi_connections;
        rooms_to_branch.extend(setup.single_connections);
        weight_rooms(rooms, &mut branchable);

        let center = self.loop_center;
        if !create_branches(
            &self.regular,
            rooms,
            &mut active,
            &mut branchable,
            &rooms_to_branch,
            &self.regular.branch_tunnel_chances,
            depth,
            rng,
            move |room, rooms, rng| random_angle_toward_center(center, room, rooms, rng),
        ) {
            return false;
        }

        find_neighbours(rooms, &active);
        add_extra_connections(rooms, &active, self.regular.extra_connection_chance, rng);
        compact_to_active_order(rooms, &active);
        true
    }
}

fn random_angle_toward_center(
    center: Option<PointF>,
    room: RoomId,
    rooms: &[Room],
    rng: &mut RandomStack,
) -> f32 {
    let Some(center) = center else {
        return rng.float_bound(360.0);
    };
    let bounds = rooms[room].bounds;
    #[allow(clippy::cast_precision_loss)]
    let room_center = PointF::new(
        bounds.left.wrapping_add(bounds.right) as f32 / 2.0,
        bounds.top.wrapping_add(bounds.bottom) as f32 / 2.0,
    );
    let mut to_center = angle_between_points(room_center, center);
    if to_center < 0.0 {
        to_center += 360.0;
    }
    let mut current = rng.float_bound(360.0);
    for _ in 0..4 {
        let new_angle = rng.float_bound(360.0);
        if (to_center - new_angle).abs() < (to_center - current).abs() {
            current = new_angle;
        }
    }
    current
}

/// The regular-level two-loop/figure-eight builder.
#[derive(Clone, Debug, PartialEq)]
pub struct FigureEightBuilder {
    pub regular: RegularBuilder,
    curve_exponent: i32,
    curve_intensity: f32,
    curve_offset: f32,
    landmark_room: Option<RoomId>,
    first_loop: Vec<RoomId>,
    second_loop: Vec<RoomId>,
    first_loop_center: Option<PointF>,
    second_loop_center: Option<PointF>,
}

impl Default for FigureEightBuilder {
    fn default() -> Self {
        Self {
            regular: RegularBuilder::default(),
            curve_exponent: 0,
            curve_intensity: 1.0,
            curve_offset: 0.0,
            landmark_room: None,
            first_loop: Vec::new(),
            second_loop: Vec::new(),
            first_loop_center: None,
            second_loop_center: None,
        }
    }
}

impl FigureEightBuilder {
    #[must_use]
    pub fn set_loop_shape(mut self, exponent: i32, intensity: f32, offset: f32) -> Self {
        self.curve_exponent = exponent.wrapping_abs();
        self.curve_intensity = intensity % 1.0;
        self.curve_offset = offset % 0.5;
        self
    }

    #[must_use]
    pub fn set_landmark_room(mut self, room: RoomId) -> Self {
        self.landmark_room = Some(room);
        self
    }

    #[must_use]
    pub fn set_path_variance(mut self, variance: f32) -> Self {
        self.regular = self.regular.set_path_variance(variance);
        self
    }

    #[must_use]
    pub fn set_path_length(mut self, length: f32, jitter: &[f32]) -> Self {
        self.regular = self.regular.set_path_length(length, jitter);
        self
    }

    #[must_use]
    pub fn set_tunnel_length(mut self, path: &[f32], branch: &[f32]) -> Self {
        self.regular = self.regular.set_tunnel_length(path, branch);
        self
    }

    #[must_use]
    pub fn set_extra_connection_chance(mut self, chance: f32) -> Self {
        self.regular = self.regular.set_extra_connection_chance(chance);
        self
    }

    fn curve_equation(&self, x: f64) -> f64 {
        let doubled = self.curve_exponent.wrapping_mul(2);
        4_f64.powf(f64::from(doubled)) * ((x % 0.5) - 0.25).powf(f64::from(doubled.wrapping_add(1)))
            + 0.25
            + 0.5 * (2.0 * x).floor()
    }

    fn target_angle(&self, mut percent_along: f32) -> f32 {
        percent_along += self.curve_offset;
        let linear = (1.0_f32 - self.curve_intensity) * percent_along;
        #[allow(clippy::cast_possible_truncation)]
        let curved = (f64::from(self.curve_intensity)
            * self.curve_equation(f64::from(percent_along))
            + f64::from(linear)
            - f64::from(self.curve_offset)) as f32;
        360.0_f32 * curved
    }
}

impl Builder for FigureEightBuilder {
    fn build_with_shop_size_hook(
        &mut self,
        rooms: &mut Vec<Room>,
        depth: u32,
        rng: &mut RandomStack,
        shop_size_hook: &mut dyn FnMut(&mut Room, &mut RandomStack),
    ) -> bool {
        // Tunnel rooms pushed mid-build would otherwise regrow the room list
        // (and memcpy the large `Room` structs) several times per attempt.
        rooms.reserve(64);
        clear_all_connections(rooms);
        let mut active: Vec<RoomId> = (0..rooms.len()).collect();
        let mut setup = self.regular.setup_rooms(rooms, &active, rng);
        let Some(entrance) = setup.entrance else {
            return false;
        };

        if self.landmark_room.is_none() {
            for &room in &setup.main_path_rooms {
                let replace = if let Some(landmark) = self.landmark_room {
                    rooms[landmark]
                        .min_width()
                        .wrapping_mul(rooms[landmark].min_height())
                        < rooms[room]
                            .min_width()
                            .wrapping_mul(rooms[room].min_height())
                } else {
                    true
                };
                if rooms[room].max_connections(Direction::All) >= 4 && replace {
                    self.landmark_room = Some(room);
                }
            }
            if !setup.multi_connections.is_empty() {
                setup
                    .main_path_rooms
                    .push(setup.multi_connections.remove(0));
            }
        }
        let Some(landmark) = self.landmark_room else {
            return false;
        };
        remove_first(&mut setup.main_path_rooms, landmark);
        remove_first(&mut setup.multi_connections, landmark);

        let mut start_angle = rng.float_between(0.0, 360.0);
        let mut rooms_on_first_loop = setup.main_path_rooms.len() / 2;
        if setup.main_path_rooms.len() % 2 == 1 {
            rooms_on_first_loop +=
                usize::try_from(rng.int_bound(2)).expect("Random.Int is non-negative");
        }

        let mut rooms_to_loop = setup.main_path_rooms.clone();
        let mut first_temp = vec![landmark];
        for _ in 0..rooms_on_first_loop {
            first_temp.push(rooms_to_loop.remove(0));
        }
        let entrance_index = (first_temp.len() + 1) / 2;
        first_temp.insert(entrance_index, entrance);

        let mut tunnel_deck = self.regular.path_tunnel_chances.clone();
        self.first_loop.clear();
        for room in first_temp {
            self.first_loop.push(room);
            let tunnels = selected_chance(&mut tunnel_deck, &self.regular.path_tunnel_chances, rng);
            tunnel_deck[tunnels] -= 1.0;
            for _ in 0..tunnels {
                let tunnel = append_pending(rooms, create_connection_room(depth, rng));
                self.first_loop.push(tunnel);
            }
        }

        let mut second_temp = vec![landmark];
        second_temp.extend(rooms_to_loop);
        if let Some(exit) = setup.exit {
            let exit_index = (second_temp.len() + 1) / 2;
            second_temp.insert(exit_index, exit);
        }
        self.second_loop.clear();
        for room in second_temp {
            self.second_loop.push(room);
            let tunnels = selected_chance(&mut tunnel_deck, &self.regular.path_tunnel_chances, rng);
            tunnel_deck[tunnels] -= 1.0;
            for _ in 0..tunnels {
                let tunnel = append_pending(rooms, create_connection_room(depth, rng));
                self.second_loop.push(tunnel);
            }
        }

        rooms[landmark].set_size(rng);
        rooms[landmark].set_position(0, 0);

        let mut previous = landmark;
        for index in 1..self.first_loop.len() {
            let room = self.first_loop[index];
            #[allow(clippy::cast_precision_loss)]
            let percent = index as f32 / self.first_loop.len() as f32;
            let target = start_angle + self.target_angle(percent);
            if place_room(rooms, &active, previous, room, target, rng) == -1.0 {
                return false;
            }
            previous = room;
            add_if_new(&mut active, previous);
        }
        while !connect_rooms(rooms, previous, landmark, rng) {
            let tunnel = append_pending(rooms, create_connection_room(depth, rng));
            let target = angle_between_rooms(rooms, previous, landmark);
            if place_room(rooms, &active, previous, tunnel, target, rng) == -1.0 {
                return false;
            }
            self.first_loop.push(tunnel);
            active.push(tunnel);
            previous = tunnel;
        }

        previous = landmark;
        start_angle += 180.0;
        for index in 1..self.second_loop.len() {
            let room = self.second_loop[index];
            #[allow(clippy::cast_precision_loss)]
            let percent = index as f32 / self.second_loop.len() as f32;
            let target = start_angle + self.target_angle(percent);
            if place_room(rooms, &active, previous, room, target, rng) == -1.0 {
                return false;
            }
            previous = room;
            add_if_new(&mut active, previous);
        }
        while !connect_rooms(rooms, previous, landmark, rng) {
            let tunnel = append_pending(rooms, create_connection_room(depth, rng));
            let target = angle_between_rooms(rooms, previous, landmark);
            if place_room(rooms, &active, previous, tunnel, target, rng) == -1.0 {
                return false;
            }
            self.second_loop.push(tunnel);
            active.push(tunnel);
            previous = tunnel;
        }

        if let Some(shop) = setup.shop {
            let mut tries = 10;
            let angle = loop {
                let target = rng.float_bound(360.0);
                let placed = place_room_with_shop_size_hook(
                    rooms,
                    &active,
                    entrance,
                    shop,
                    target,
                    rng,
                    shop_size_hook,
                );
                tries -= 1;
                if placed != -1.0 || tries < 0 {
                    break placed;
                }
            };
            if angle == -1.0 {
                return false;
            }
        }

        self.first_loop_center = Some(loop_center(rooms, &self.first_loop));
        self.second_loop_center = Some(loop_center(rooms, &self.second_loop));

        let mut branchable = self.first_loop.clone();
        branchable.extend(self.second_loop.iter().copied());
        remove_first(&mut branchable, landmark);
        let mut rooms_to_branch = setup.multi_connections;
        rooms_to_branch.extend(setup.single_connections);
        weight_rooms(rooms, &mut branchable);

        let first_loop = self.first_loop.clone();
        let first_center = self.first_loop_center;
        let second_center = self.second_loop_center;
        if !create_branches(
            &self.regular,
            rooms,
            &mut active,
            &mut branchable,
            &rooms_to_branch,
            &self.regular.branch_tunnel_chances,
            depth,
            rng,
            move |room, rooms, rng| {
                let center = if first_loop.contains(&room) {
                    first_center
                } else {
                    second_center
                };
                random_angle_toward_center(center, room, rooms, rng)
            },
        ) {
            return false;
        }

        find_neighbours(rooms, &active);
        add_extra_connections(rooms, &active, self.regular.extra_connection_chance, rng);
        compact_to_active_order(rooms, &active);
        true
    }
}

fn loop_center(rooms: &[Room], loop_rooms: &[RoomId]) -> PointF {
    let mut center = PointF::default();
    for &room in loop_rooms {
        let bounds = rooms[room].bounds;
        #[allow(clippy::cast_precision_loss)]
        {
            center.x += bounds.left.wrapping_add(bounds.right) as f32 / 2.0;
            center.y += bounds.top.wrapping_add(bounds.bottom) as f32 / 2.0;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    {
        center.x /= loop_rooms.len() as f32;
        center.y /= loop_rooms.len() as f32;
    }
    center
}

/// `RegularLevel.builder()` for v3.3.8. The builder choice and its two shape
/// draws happen before initial rooms are constructed.
#[derive(Clone, Debug, PartialEq)]
pub enum RegularLevelBuilder {
    Loop(LoopBuilder),
    FigureEight(FigureEightBuilder),
}

impl RegularLevelBuilder {
    /// The shared `RegularLevel.builder()` implementation used by all five
    /// main-dungeon regions.
    #[must_use]
    pub fn for_regular_level(rng: &mut RandomStack) -> Self {
        Self::for_sewer_level(rng)
    }

    /// Backward-compatible name retained for the original Sewer graph slice.
    #[must_use]
    pub fn for_sewer_level(rng: &mut RandomStack) -> Self {
        if rng.int_bound(2) == 0 {
            Self::Loop(LoopBuilder::default().set_loop_shape(
                2,
                rng.float_between(0.0, 0.65),
                rng.float_between(0.0, 0.50),
            ))
        } else {
            Self::FigureEight(FigureEightBuilder::default().set_loop_shape(
                2,
                rng.float_between(0.3, 0.8),
                0.0,
            ))
        }
    }
}

impl Builder for RegularLevelBuilder {
    fn build_with_shop_size_hook(
        &mut self,
        rooms: &mut Vec<Room>,
        depth: u32,
        rng: &mut RandomStack,
        shop_size_hook: &mut dyn FnMut(&mut Room, &mut RandomStack),
    ) -> bool {
        match self {
            Self::Loop(builder) => {
                builder.build_with_shop_size_hook(rooms, depth, rng, shop_size_hook)
            }
            Self::FigureEight(builder) => {
                builder.build_with_shop_size_hook(rooms, depth, rng, shop_size_hook)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::room::{
        RoomConnection, RoomKind, SizeCategory, StandardRoomKind, create_entrance_room,
        create_exit_room, create_standard_room,
    };

    fn basic_rooms(depth: u32, rng: &mut RandomStack) -> Vec<Room> {
        let mut rooms = vec![
            create_entrance_room(depth, rng),
            create_exit_room(depth, rng),
        ];
        for remaining_value in (1..=5).rev() {
            let mut room = create_standard_room(depth, rng);
            assert!(room.set_size_category_for_value(remaining_value.min(3), rng));
            rooms.push(room);
        }
        rooms
    }

    #[test]
    fn free_space_clips_against_closest_collision() {
        let mut rng = RandomStack::with_base_seed(1);
        let mut rooms = vec![
            Room::connection(ConnectionRoomKind::Tunnel),
            Room::connection(ConnectionRoomKind::Tunnel),
        ];
        rooms[0].bounds = Rect::new(-8, -2, -4, 2);
        rooms[1].bounds = Rect::new(4, -2, 8, 2);
        let space = find_free_space(Point::new(0, 0), &rooms, &[0, 1], 10, &mut rng);
        assert_eq!(space, Rect::new(-4, -10, 4, 10));
    }

    #[test]
    fn place_room_positions_and_connects_inclusive_rectangles() {
        let mut rng = RandomStack::with_base_seed(0x55aa);
        let mut rooms = vec![
            Room::connection(ConnectionRoomKind::Tunnel),
            Room::connection(ConnectionRoomKind::Tunnel),
        ];
        assert!(rooms[0].force_size(5, 5, &mut rng));
        rooms[0].set_position(0, 0);
        let angle = place_room(&mut rooms, &[0], 0, 1, 90.0, &mut rng);
        assert_ne!(angle, -1.0);
        assert_eq!(rooms[1].bounds.left, rooms[0].bounds.right);
        assert_eq!(rooms[0].connected[0].room, 1);
        assert_eq!(rooms[1].connected[0].room, 0);
    }

    #[test]
    fn loop_builder_returns_a_connected_positioned_graph() {
        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x1020_3040_5060_7080);
        let mut rooms = basic_rooms(2, &mut rng);
        rng.shuffle_list(&mut rooms);
        let mut builder = LoopBuilder::default();
        let mut attempts = 0;
        while !builder.build(&mut rooms, 2, &mut rng) {
            // A Java retry starts from clones of the initial room identities.
            rooms.retain(|room| !matches!(room.kind, RoomKind::Connection(_)));
            attempts += 1;
            assert!(attempts < 200);
        }
        assert!(rooms.iter().all(|room| !room.bounds.is_empty()));
        assert!(rooms.iter().all(|room| !room.connected.is_empty()));
        for (id, room) in rooms.iter().enumerate() {
            for RoomConnection { room: other, .. } in &room.connected {
                assert!(rooms[*other].connection_to(id).is_some());
            }
        }
    }

    #[test]
    fn loop_builder_matches_pinned_java_room_graph() {
        // Captured by running the v3.3.8 classes at commit 7b8b845a with the
        // same initRooms subset. This covers failed-build retries, Java list
        // shuffle order, tunnel decks, placement, closure, neighbours, and
        // connection insertion order.
        let mut rng = RandomStack::with_base_seed(0);
        rng.push(0x1020_3040_5060_7080);
        let mut rooms = vec![
            create_entrance_room(2, &mut rng),
            create_exit_room(2, &mut rng),
        ];
        let mut used_value = 0_i32;
        while used_value < 5 {
            let room = loop {
                let mut candidate = create_standard_room(2, &mut rng);
                if candidate.set_size_category_for_value(5 - used_value, &mut rng) {
                    break candidate;
                }
            };
            used_value += room.size_factor();
            rooms.push(room);
        }
        rng.shuffle_list(&mut rooms);
        let initial_count = rooms.len();
        let mut builder = LoopBuilder::default();
        let mut attempts = 1;
        while !builder.build(&mut rooms, 2, &mut rng) {
            rooms.truncate(initial_count);
            attempts += 1;
            assert!(attempts < 20, "builder did not converge");
        }
        assert_eq!(attempts, 3);

        let actual: Vec<_> = rooms
            .iter()
            .map(|room| {
                (
                    room.kind,
                    room.size_category,
                    room.bounds,
                    room.connected
                        .iter()
                        .map(|connection| connection.room)
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        assert_eq!(
            actual,
            vec![
                (
                    RoomKind::Exit(StandardRoomKind::CircleBasin),
                    Some(SizeCategory::Large),
                    Rect::new(-21, 6, -9, 16),
                    vec![6, 7, 10],
                ),
                (
                    RoomKind::Entrance(StandardRoomKind::WaterBridge),
                    Some(SizeCategory::Normal),
                    Rect::new(0, 0, 7, 8),
                    vec![5, 2],
                ),
                (
                    RoomKind::Standard(StandardRoomKind::Ring),
                    Some(SizeCategory::Large),
                    Rect::new(0, -10, 10, 0),
                    vec![9, 1],
                ),
                (
                    RoomKind::Standard(StandardRoomKind::SewerPipe),
                    Some(SizeCategory::Normal),
                    Rect::new(-13, -1, -5, 5),
                    vec![10],
                ),
                (
                    RoomKind::Standard(StandardRoomKind::Burned),
                    Some(SizeCategory::Normal),
                    Rect::new(-20, -5, -15, 0),
                    vec![7, 8],
                ),
                (
                    RoomKind::Standard(StandardRoomKind::SewerPipe),
                    Some(SizeCategory::Normal),
                    Rect::new(-5, 8, 3, 16),
                    vec![1, 6, 10],
                ),
                (
                    RoomKind::Connection(ConnectionRoomKind::Walkway),
                    None,
                    Rect::new(-9, 13, -5, 16),
                    vec![5, 0],
                ),
                (
                    RoomKind::Connection(ConnectionRoomKind::Tunnel),
                    None,
                    Rect::new(-21, 0, -17, 6),
                    vec![0, 4],
                ),
                (
                    RoomKind::Connection(ConnectionRoomKind::Tunnel),
                    None,
                    Rect::new(-15, -8, -6, -3),
                    vec![4, 9],
                ),
                (
                    RoomKind::Connection(ConnectionRoomKind::Tunnel),
                    None,
                    Rect::new(-6, -9, 0, -1),
                    vec![8, 2],
                ),
                (
                    RoomKind::Connection(ConnectionRoomKind::Tunnel),
                    None,
                    Rect::new(-9, 5, -5, 12),
                    vec![5, 3, 0],
                ),
            ]
        );
        assert_eq!(rng.int(), -405_108_799);
    }
}
