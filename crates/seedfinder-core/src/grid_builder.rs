//! Draw-for-draw port of v4.0.0 `levels/builders/GridBuilder.java`, the
//! builder used by the Imp Vault.
//!
//! Rooms snap to an `11 x 11` grid (`ROOM_SIZE - 1` apart), large rooms take
//! `2 x 1`, `1 x 2`, or `2 x 2` cells, and each room is attached next to an
//! already placed room. That neighbour is picked with
//! `keys[Random.Int(keys.length)]` over `SparseArray.keyArray()`, whose
//! order is the iteration order of libgdx 1.14's `IntMap` hash table, so the
//! table is emulated here slot for slot.

// Sentinel arithmetic and the 1000-stride grid keys are kept verbatim.
#![allow(clippy::missing_panics_doc)]

use crate::geometry::{Point, Rect};
use crate::java_math::{div_i32, rem_i32};
use crate::rng::RandomStack;
use crate::vault_rooms::{Direction, VaultRoom, connect_rooms, find_neighbours};

/// `GridBuilder.ROOM_SIZE`.
pub const ROOM_SIZE: i32 = 11;
const EXTRA_CONNECTION_CHANCE: f32 = 0.55;

/// libgdx `IntMap<Room>` (via `SparseArray`) with the default capacity 51
/// and load factor 0.8: a 64-slot open-addressing table whose iteration
/// order is table order.
#[derive(Clone, Debug)]
struct GridCells {
    keys: Vec<i32>,
    values: Vec<usize>,
    size: usize,
    threshold: usize,
    shift: u32,
    mask: usize,
}

impl GridCells {
    fn new() -> Self {
        // ObjectSet.tableSize(51, 0.8f) == 64.
        Self::with_table_size(64, 0.8)
    }

    fn with_table_size(table_size: usize, load_factor: f32) -> Self {
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let threshold = (table_size as f32 * load_factor) as usize;
        let mask = table_size - 1;
        Self {
            keys: vec![0; table_size],
            values: vec![usize::MAX; table_size],
            size: 0,
            threshold,
            shift: (mask as u64).leading_zeros(),
            mask,
        }
    }

    /// `IntMap.place(item)`: the top bits of a Fibonacci-hashed key.
    fn place(&self, key: i32) -> usize {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let hashed =
            (i64::from(key).wrapping_mul(-7_046_029_254_386_353_131_i64) as u64) >> self.shift;
        usize::try_from(hashed).expect("a six-bit table index fits usize")
    }

    fn locate(&self, key: i32) -> Result<usize, usize> {
        let mut index = self.place(key);
        loop {
            let other = self.keys[index];
            if other == 0 {
                return Err(index);
            }
            if other == key {
                return Ok(index);
            }
            index = (index + 1) & self.mask;
        }
    }

    fn contains_key(&self, key: i32) -> bool {
        assert_ne!(key, 0, "grid keys are always offset away from zero");
        self.locate(key).is_ok()
    }

    fn get(&self, key: i32) -> Option<usize> {
        self.locate(key).ok().map(|index| self.values[index])
    }

    fn put(&mut self, key: i32, value: usize) {
        assert_ne!(key, 0, "grid keys are always offset away from zero");
        match self.locate(key) {
            Ok(index) => self.values[index] = value,
            Err(index) => {
                self.keys[index] = key;
                self.values[index] = value;
                self.size += 1;
                if self.size >= self.threshold {
                    self.resize(self.keys.len() << 1);
                }
            }
        }
    }

    fn resize(&mut self, new_size: usize) {
        let old_keys = std::mem::take(&mut self.keys);
        let old_values = std::mem::take(&mut self.values);
        let mut grown = Self::with_table_size(new_size, 0.8);
        grown.size = self.size;
        for (key, value) in old_keys.into_iter().zip(old_values) {
            if key != 0 {
                let mut index = grown.place(key);
                while grown.keys[index] != 0 {
                    index = (index + 1) & grown.mask;
                }
                grown.keys[index] = key;
                grown.values[index] = value;
            }
        }
        *self = grown;
    }

    /// `SparseArray.keyArray()`: table iteration order.
    fn key_array(&self) -> Vec<i32> {
        self.keys.iter().copied().filter(|&key| key != 0).collect()
    }
}

const fn grid_index(x: i32, y: i32) -> i32 {
    x + 100 + 1000 * (y + 100)
}

/// `GridBuilder.findFreeGridSpace`.
fn find_free_grid_space(
    start: Point,
    collision: &GridCells,
    max_width: i32,
    max_height: i32,
) -> Rect {
    let mut space = Rect::new(start.x, start.y, start.x, start.y);
    let mut expanded = true;
    while expanded {
        expanded = false;
        if space.left > start.x - (max_width - 1) {
            let valid = (space.top..=space.bottom)
                .all(|y| !collision.contains_key(grid_index(space.left - 1, y)));
            if valid {
                space.left -= 1;
                expanded = true;
            }
        }
        if space.top > start.y - (max_height - 1) {
            let valid = (space.left..=space.right)
                .all(|x| !collision.contains_key(grid_index(x, space.top - 1)));
            if valid {
                space.top -= 1;
                expanded = true;
            }
        }
        if space.right < start.x + (max_width - 1) {
            let valid = (space.top..=space.bottom)
                .all(|y| !collision.contains_key(grid_index(space.right + 1, y)));
            if valid {
                space.right += 1;
                expanded = true;
            }
        }
        if space.bottom < start.y + (max_height - 1) {
            let valid = (space.left..=space.right)
                .all(|x| !collision.contains_key(grid_index(x, space.bottom + 1)));
            if valid {
                space.bottom += 1;
                expanded = true;
            }
        }
    }
    space
}

/// `GridBuilder.build(rooms)`. Returns `false` where Java returns `null`;
/// the caller retries with cleared graph state and the continuing stream.
#[allow(clippy::too_many_lines)] // One auditable port of a single Java method.
pub fn build_grid(rooms: &mut [VaultRoom], random: &mut RandomStack) -> bool {
    for room in rooms.iter_mut() {
        room.set_empty();
    }

    let mut entrance = None;
    let mut exit = None;
    for (id, room) in rooms.iter().enumerate() {
        if room.is_entrance() {
            entrance = Some(id);
        } else if room.is_exit() {
            exit = Some(id);
        }
    }
    let entrance = entrance.expect("the vault always has an entrance room");
    assert!(
        rooms[entrance].force_size(ROOM_SIZE, ROOM_SIZE, random),
        "rigid room sizes for now!"
    );
    rooms[entrance].set_position(0, 0);

    let mut to_place: Vec<usize> = Vec::new();
    let mut multis: Vec<usize> = Vec::new();
    let mut singles: Vec<usize> = Vec::new();
    for (id, room) in rooms.iter().enumerate() {
        if room.max_connections(Direction::All) == 1 {
            singles.push(id);
        } else {
            multis.push(id);
        }
    }

    let mut max_width = 0_i32;
    let mut max_height = 0_i32;
    #[allow(clippy::cast_precision_loss)]
    let target = rooms.len() as f32 * 1.25_f32;
    #[allow(clippy::cast_precision_loss)]
    while ((max_width * max_height) as f32) < target {
        if max_width >= max_height && (max_width != max_height || random.int_bound(2) != 0) {
            max_height += 1;
        } else {
            max_width += 1;
        }
    }

    let mut placed: Vec<usize> = Vec::new();
    if !multis.is_empty() {
        to_place.push(multis.remove(0));
    }
    if !multis.is_empty() {
        to_place.push(multis.remove(0));
    }
    while !multis.is_empty() || !singles.is_empty() {
        for _ in 0..3 {
            if !multis.is_empty() {
                to_place.push(multis.remove(0));
            }
        }
        if !singles.is_empty() {
            to_place.push(singles.remove(0));
        }
    }
    to_place.retain(|&room| room != entrance);

    let (entry_x, entry_y) = match random.int_bound(4) {
        1 => (random.int_between(0, max_width), 0),
        2 => (max_width - 1, random.int_between(0, max_height)),
        3 => (random.int_between(0, max_width), max_height - 1),
        _ => (0, random.int_between(0, max_height)),
    };
    rooms[entrance].set_position(entry_x * (ROOM_SIZE - 1), entry_y * (ROOM_SIZE - 1));
    placed.push(entrance);
    if let Some(exit) = exit {
        to_place.retain(|&room| room != exit);
        to_place.push(exit);
    }

    #[allow(clippy::cast_precision_loss)]
    let aim_center = (max_width as f32 / 2.0_f32, max_height as f32 / 2.0_f32);
    let mut grid_cells = GridCells::new();
    let entry_index = grid_index(entry_x, entry_y);
    grid_cells.put(entry_index, entrance);
    let mut room_placement_failures = 0_i32;

    while !to_place.is_empty() {
        let room = to_place.remove(0);
        let (cell_width, cell_height) = if rooms[room].force_size(ROOM_SIZE, ROOM_SIZE, random) {
            (1, 1)
        } else if rooms[room].force_size(2 * ROOM_SIZE - 1, 2 * ROOM_SIZE - 1, random) {
            (2, 2)
        } else if rooms[room].force_size(ROOM_SIZE, 2 * ROOM_SIZE - 1, random) {
            (1, 2)
        } else if rooms[room].force_size(2 * ROOM_SIZE - 1, ROOM_SIZE, random) {
            (2, 1)
        } else {
            panic!("rigid room sizes for now!");
        };

        let mut tries = 0_i32;
        loop {
            rooms[room].neighbours.clear();
            tries += 1;
            if tries > 100 {
                let insert_at = to_place.len().min(2);
                to_place.insert(insert_at, room);
                room_placement_failures += 1;
                if room_placement_failures > 100 {
                    return false;
                }
                break;
            }

            let (neighbour, neighbour_index) = if placed.len() < 3 {
                (entrance, entry_index)
            } else {
                let keys = grid_cells.key_array();
                let bound = i32::try_from(keys.len()).expect("grid key count fits Java int");
                let key = keys
                    [usize::try_from(random.int_bound(bound)).expect("Random.Int is non-negative")];
                (
                    grid_cells.get(key).expect("key array entries are live"),
                    key,
                )
            };

            #[allow(clippy::cast_precision_loss)]
            let x_diff = aim_center.0 - (rem_i32(neighbour_index, 1000) - 100) as f32;
            #[allow(clippy::cast_precision_loss)]
            let y_diff = aim_center.1 - (div_i32(neighbour_index, 1000) - 100) as f32;
            #[allow(clippy::cast_possible_truncation)]
            let dist = (f64::from(x_diff).powi(2) + f64::from(y_diff).powi(2)).sqrt() as f32;
            let toward_center = random.float_bound(12.0) < 8.0 + dist;
            let mut room_index = if !toward_center && placed.len() != 1 {
                match random.int_bound(4) {
                    0 => neighbour_index + 1,
                    1 => neighbour_index - 1000,
                    2 => neighbour_index - 1,
                    3 => neighbour_index + 1000,
                    _ => unreachable!(),
                }
            } else if x_diff.abs() >= y_diff.abs() {
                if x_diff > 0.0 {
                    neighbour_index + 1
                } else {
                    neighbour_index - 1
                }
            } else if y_diff > 0.0 {
                neighbour_index + 1000
            } else {
                neighbour_index - 1000
            };

            let mut x = rem_i32(room_index, 1000) - 100;
            let mut y = div_i32(room_index, 1000) - 100;
            // Upstream checks x against maxHeight; keep the quirk.
            let mut valid = if x < 0
                || x >= max_height
                || y < 0
                || y >= max_height
                || grid_cells.contains_key(room_index)
            {
                false
            } else if cell_width <= 1 && cell_height <= 1 {
                true
            } else {
                let mut space =
                    find_free_grid_space(Point::new(x, y), &grid_cells, cell_width, cell_height);
                if cell_width * cell_height <= 2 {
                    space.left = space.left.max(0);
                    space.top = space.top.max(0);
                    space.right = space.right.min(max_width - 1);
                    space.bottom = space.bottom.min(max_height - 1);
                }
                let excess_width = space.width() + 1 - cell_width;
                let excess_height = space.height() + 1 - cell_height;
                let valid = excess_width >= 0 && excess_height >= 0;
                if valid {
                    x = space.left + random.int_bound(excess_width + 1);
                    y = space.top + random.int_bound(excess_height + 1);
                    room_index = grid_index(x, y);
                }
                valid
            };

            if valid {
                rooms[room].set_position(x * (ROOM_SIZE - 1), y * (ROOM_SIZE - 1));
                for &other in &placed {
                    if rooms[other].kind == rooms[room].kind {
                        let intersection = rooms[room].bounds.intersect(rooms[other].bounds);
                        if intersection.width() > 0 || intersection.height() > 0 {
                            valid = false;
                        }
                    }
                }
            }

            if valid && connect_rooms(rooms, room, neighbour) {
                placed.push(room);
                for i in 0..cell_width {
                    for j in 0..cell_height {
                        grid_cells.put(room_index + i + j * 1000, room);
                    }
                }
            }
            if placed.contains(&room) {
                break;
            }
        }
    }

    find_neighbours(rooms);
    for room in 0..rooms.len() {
        let neighbours = rooms[room].neighbours.clone();
        for other in neighbours {
            if rooms[other].connection_to(room).is_none()
                && random.float() < EXTRA_CONNECTION_CHANCE
            {
                connect_rooms(rooms, room, other);
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_map_emulation_iterates_in_table_order() {
        let mut cells = GridCells::new();
        for (index, key) in [grid_index(0, 0), grid_index(1, 0), grid_index(0, 1)]
            .into_iter()
            .enumerate()
        {
            cells.put(key, index);
        }
        let keys = cells.key_array();
        assert_eq!(keys.len(), 3);
        let mut slots: Vec<usize> = keys.iter().map(|&key| cells.place(key)).collect();
        // Keys are listed by slot, so their placements must be ascending
        // unless probing wrapped them.
        let mut sorted = slots.clone();
        sorted.sort_unstable();
        slots.dedup();
        assert!(slots.len() == keys.len() || sorted == slots);
        assert_eq!(cells.get(grid_index(1, 0)), Some(1));
        assert!(!cells.contains_key(grid_index(5, 5)));
    }

    #[test]
    fn int_map_place_matches_libgdx_fibonacci_hash() {
        let cells = GridCells::new();
        // 100100 * -7046029254386353131 >>> 58, computed independently.
        let expected = ((100_100_i64.wrapping_mul(-7_046_029_254_386_353_131_i64)) as u64) >> 58;
        assert_eq!(cells.place(100_100), usize::try_from(expected).unwrap());
        assert_eq!(cells.shift, 58);
        assert_eq!(cells.mask, 63);
        assert_eq!(cells.threshold, 51);
    }
}
