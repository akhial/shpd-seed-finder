//! Shared bit-column fast path for the `MazeRoom.generate` wall-growing
//! loop, used by both the sewer connection maze and secret maze rooms.
//!
//! The maze is one `u64` per column with bit `y` set for a wall, so a
//! valid-move probe collapses from six bounds-checked byte loads into two
//! masked bit tests. Every RNG draw — the cell picks, the three direction
//! draws, and the per-step continuation draw — happens in exactly the
//! canonical order; only the draw-free wall probes changed representation.

use crate::rng::{FastBound, JavaRandom, RandomStack};

/// Tallest maze the bit-column representation can hold. Generated rooms are
/// far smaller; callers fall back to the scalar walker beyond this. The
/// branch-free [`valid_move`] clamps probes into `1..=dim-2`, so callers
/// must also require both dimensions to be at least [`MIN_BIT_MAZE_SIDE`].
pub(crate) const MAX_BIT_MAZE_HEIGHT: i32 = 64;

/// Smallest width and height the bit-column walker accepts.
pub(crate) const MIN_BIT_MAZE_SIDE: i32 = 3;

/// Runs the canonical wall-growing loop — grow from a random existing wall
/// in a random direction until 2,500 consecutive rounds fail — over bit
/// columns prepared by the caller (borders set, doorways cleared).
pub(crate) fn grow_maze(cols: &mut [u64], width: i32, height: i32, random: &mut RandomStack) {
    let generator = random.current_generator();
    let x_bound = FastBound::new(width);
    let y_bound = FastBound::new(height);
    let mut fails = 0_i32;
    while fails < 2_500 {
        let (mut x, mut y) = loop {
            let x = generator.next_i32_fast_bound(&x_bound);
            let y = generator.next_i32_fast_bound(&y_bound);
            let column = cols[usize::try_from(x).expect("maze pick is in bounds")];
            if (column >> y) & 1 != 0 {
                break (x, y);
            }
        };
        let Some((dx, dy)) = direction(cols, width, height, x, y, generator) else {
            fails += 1;
            continue;
        };
        fails = 0;
        let mut moves = 0_i32;
        loop {
            x += dx;
            y += dy;
            cols[usize::try_from(x).expect("maze carve is in bounds")] |= 1 << y;
            moves += 1;
            if generator.next_i32_bound(moves) != 0
                || !valid_move(cols, width, height, x, y, dx, dy)
            {
                break;
            }
        }
    }
}

fn direction(
    cols: &[u64],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    generator: &mut JavaRandom,
) -> Option<(i32, i32)> {
    if generator.next_i32_bound(4) == 0 && valid_move(cols, width, height, x, y, 0, -1) {
        return Some((0, -1));
    }
    if generator.next_i32_bound(3) == 0 && valid_move(cols, width, height, x, y, 1, 0) {
        return Some((1, 0));
    }
    if generator.next_i32_bound(2) == 0 && valid_move(cols, width, height, x, y, 0, 1) {
        return Some((0, 1));
    }
    valid_move(cols, width, height, x, y, -1, 0).then_some((-1, 0))
}

/// Two steps along `(dx, dy)`, each requiring the stepped cell and both side
/// cells clear: for a horizontal move the three cells share one column
/// (a 3-bit mask), for a vertical move they share one row (one bit of three
/// columns). Whether a probe passes is close to a coin flip, so the whole
/// test is evaluated branch-free — out-of-bounds steps clamp their probe
/// coordinates to a valid cell, do the (now meaningless) loads anyway, and
/// are vetoed by the bounds bit. The probes draw nothing, so the missing
/// early exits are unobservable.
fn valid_move(
    cols: &[u64],
    width: i32,
    height: i32,
    mut x: i32,
    mut y: i32,
    dx: i32,
    dy: i32,
) -> bool {
    let vertical = dy != 0;
    let mut ok = true;
    for _ in 0..2_u32 {
        x += dx;
        y += dy;
        #[allow(clippy::needless_bitwise_bool)]
        let in_bounds = (x > 0) & (x < width - 1) & (y > 0) & (y < height - 1);
        let column = usize::try_from(x.clamp(1, width - 2))
            .expect("clamped maze probe column is non-negative");
        let row = y.clamp(1, height - 2);
        let horizontal_occupied = (cols[column] >> (row - 1)) & 0b111;
        let vertical_occupied = ((cols[column - 1] | cols[column] | cols[column + 1]) >> row) & 1;
        let occupied = if vertical {
            vertical_occupied
        } else {
            horizontal_occupied
        };
        #[allow(clippy::needless_bitwise_bool)]
        {
            ok &= in_bounds & (occupied == 0);
        }
    }
    ok
}

/// Bit columns for a `width` x `height` maze with all four borders walled,
/// ready for the caller to clear doorways. Requires `1 <= height <= 64`.
pub(crate) fn walled_border_cols(width: i32, height: i32) -> Vec<u64> {
    let width = usize::try_from(width).expect("positive maze width");
    let full = if height >= 64 {
        u64::MAX
    } else {
        (1_u64 << height) - 1
    };
    let edges = 1 | (1_u64 << (height - 1));
    let mut cols = vec![edges; width];
    cols[0] = full;
    cols[width - 1] = full;
    cols
}

/// Expands bit columns into the column-major `Vec<bool>` layout the maze
/// painters consume (`cell = x * height + y`).
pub(crate) fn cols_to_column_major(cols: &[u64], height: i32) -> Vec<bool> {
    let height = usize::try_from(height).expect("positive maze height");
    let mut maze = vec![false; cols.len() * height];
    for (column, cells) in cols.iter().zip(maze.chunks_exact_mut(height)) {
        for (y, cell) in cells.iter_mut().enumerate() {
            *cell = (column >> y) & 1 != 0;
        }
    }
    maze
}
