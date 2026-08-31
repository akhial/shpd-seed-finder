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
    if generator.next_i32_bound(4) == 0 && valid_move_vertical(cols, width, height, x, y, -1) {
        return Some((0, -1));
    }
    if generator.next_i32_bound(3) == 0 && valid_move_horizontal(cols, width, height, x, y, 1) {
        return Some((1, 0));
    }
    if generator.next_i32_bound(2) == 0 && valid_move_vertical(cols, width, height, x, y, 1) {
        return Some((0, 1));
    }
    valid_move_horizontal(cols, width, height, x, y, -1).then_some((-1, 0))
}

/// Two steps along `(dx, dy)`, each requiring the stepped cell and both side
/// cells clear. Whether a probe passes is close to a coin flip, so each test
/// is evaluated branch-free — out-of-bounds steps clamp their probe
/// coordinates to a valid cell, do the (now meaningless) loads anyway, and
/// are vetoed by the bounds bit. The probes draw nothing, so the missing
/// early exits are unobservable.
///
/// The two axes are separate functions because the axis is a constant at
/// every call site and each one only needs its own side cells: the shared
/// form loaded four columns per step to pick one of two answers, and this
/// runs tens of thousands of times per generated seed.
fn valid_move(cols: &[u64], width: i32, height: i32, x: i32, y: i32, dx: i32, dy: i32) -> bool {
    if dy == 0 {
        valid_move_horizontal(cols, width, height, x, y, dx)
    } else {
        valid_move_vertical(cols, width, height, x, y, dy)
    }
}

/// A vertical step's three cells share one row of three columns, and `x` is
/// the same for both steps, so the columns merge once.
fn valid_move_vertical(cols: &[u64], width: i32, height: i32, x: i32, mut y: i32, dy: i32) -> bool {
    let column =
        usize::try_from(x.clamp(1, width - 2)).expect("clamped maze probe column is non-negative");
    let merged = cols[column - 1] | cols[column] | cols[column + 1];
    #[allow(clippy::needless_bitwise_bool)]
    let mut ok = (x > 0) & (x < width - 1);
    for _ in 0..2_u32 {
        y += dy;
        let row = y.clamp(1, height - 2);
        let occupied = (merged >> row) & 1;
        #[allow(clippy::needless_bitwise_bool)]
        {
            ok &= (y > 0) & (y < height - 1) & (occupied == 0);
        }
    }
    ok
}

/// A horizontal step's three cells share one column, and `y` is the same for
/// both steps, so the 3-bit mask reads the same row of two columns.
fn valid_move_horizontal(
    cols: &[u64],
    width: i32,
    height: i32,
    mut x: i32,
    y: i32,
    dx: i32,
) -> bool {
    let row = y.clamp(1, height - 2);
    #[allow(clippy::needless_bitwise_bool)]
    let mut ok = (y > 0) & (y < height - 1);
    for _ in 0..2_u32 {
        x += dx;
        let column = usize::try_from(x.clamp(1, width - 2))
            .expect("clamped maze probe column is non-negative");
        let occupied = (cols[column] >> (row - 1)) & 0b111;
        #[allow(clippy::needless_bitwise_bool)]
        {
            ok &= (x > 0) & (x < width - 1) & (occupied == 0);
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
