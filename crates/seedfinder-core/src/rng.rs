//! Bit-for-bit replicas of `java.util.Random` and Shattered's RNG wrapper.

const MULTIPLIER: u64 = 0x0005_DEEC_E66D;
const ADDEND: u64 = 0xB;
const MASK: u64 = (1_u64 << 48) - 1;
const MX3_MULTIPLIER: u64 = 0xBEA2_25F9_EB34_556D;

// Jump-ahead compositions of the LCG: applying `state * Ak + Ck` advances the
// generator by k steps in one multiply-add. Truncation to 48 bits commutes
// with wrapping 64-bit multiply/add, so composing unmasked stays exact.
const MULTIPLIER_2: u64 = MULTIPLIER.wrapping_mul(MULTIPLIER);
const ADDEND_2: u64 = MULTIPLIER.wrapping_mul(ADDEND).wrapping_add(ADDEND);
const MULTIPLIER_3: u64 = MULTIPLIER_2.wrapping_mul(MULTIPLIER);
const ADDEND_3: u64 = MULTIPLIER.wrapping_mul(ADDEND_2).wrapping_add(ADDEND);
const MULTIPLIER_4: u64 = MULTIPLIER_3.wrapping_mul(MULTIPLIER);
const ADDEND_4: u64 = MULTIPLIER.wrapping_mul(ADDEND_3).wrapping_add(ADDEND);
const MULTIPLIER_5: u64 = MULTIPLIER_4.wrapping_mul(MULTIPLIER);
const ADDEND_5: u64 = MULTIPLIER.wrapping_mul(ADDEND_4).wrapping_add(ADDEND);
const MULTIPLIER_6: u64 = MULTIPLIER_5.wrapping_mul(MULTIPLIER);
const ADDEND_6: u64 = MULTIPLIER.wrapping_mul(ADDEND_5).wrapping_add(ADDEND);
const MULTIPLIER_7: u64 = MULTIPLIER_6.wrapping_mul(MULTIPLIER);
const ADDEND_7: u64 = MULTIPLIER.wrapping_mul(ADDEND_6).wrapping_add(ADDEND);
const MULTIPLIER_8: u64 = MULTIPLIER_7.wrapping_mul(MULTIPLIER);
const ADDEND_8: u64 = MULTIPLIER.wrapping_mul(ADDEND_7).wrapping_add(ADDEND);

// Lemire's exact division-free `x % d` for 32-bit operands: with
// `magic = floor(2^64 / d) + 1`, the remainder is the high 64 bits of
// `(magic * x mod 2^64) * d`. Exact for every u32 `x` and `d >= 2`.
const fn bound_magic(divisor: u64) -> u64 {
    (u64::MAX / divisor).wrapping_add(1)
}

const SMALL_BOUND_MAGIC: [u64; 65] = {
    let mut table = [0_u64; 65];
    let mut divisor = 2_usize;
    while divisor <= 64 {
        table[divisor] = bound_magic(divisor as u64);
        divisor += 1;
    }
    table
};

#[inline]
#[allow(clippy::cast_possible_truncation)]
fn fast_rem_u32(value: u32, divisor: u32, magic: u64) -> u32 {
    let low = magic.wrapping_mul(u64::from(value));
    ((u128::from(low) * u128::from(divisor)) >> 64) as u32
}

/// The 48-bit linear-congruential generator implemented by `java.util.Random`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JavaRandom {
    state: u64,
}

impl JavaRandom {
    /// Mirrors `new java.util.Random(seed)` exactly.
    #[must_use]
    pub const fn new(seed: i64) -> Self {
        Self {
            state: (u64::from_ne_bytes(seed.to_ne_bytes()) ^ MULTIPLIER) & MASK,
        }
    }

    fn next_bits(&mut self, bits: u32) -> u32 {
        debug_assert!((1..=32).contains(&bits));
        self.state = self.state.wrapping_mul(MULTIPLIER).wrapping_add(ADDEND) & MASK;
        match u32::try_from(self.state >> (48 - bits)) {
            Ok(value) => value,
            Err(_) => unreachable!(),
        }
    }

    /// Mirrors Java's unbounded `nextInt()`.
    pub fn next_i32(&mut self) -> i32 {
        i32::from_ne_bytes(self.next_bits(32).to_ne_bytes())
    }

    /// Mirrors Java's `nextInt(bound)` for a strictly positive bound.
    ///
    /// # Panics
    ///
    /// Panics if `bound <= 0`, as the JDK method does.
    pub fn next_i32_bound(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");

        if (bound & -bound) == bound {
            return i32::try_from(((i64::from(bound)) * i64::from(self.next_bits(31))) >> 31)
                .unwrap_or_default();
        }

        loop {
            let bits = i32::try_from(self.next_bits(31)).unwrap_or_default();
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
            let value = if bound <= 64 {
                // Division-free remainder for the small bounds that dominate
                // level generation; bit-identical to `bits % bound`.
                fast_rem_u32(bits as u32, bound as u32, SMALL_BOUND_MAGIC[bound as usize]) as i32
            } else {
                bits % bound
            };
            // Java performs this expression in wrapping signed 32-bit math.
            if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                return value;
            }
        }
    }

    /// `nextInt(bound)` with a caller-precomputed [`FastBound`], for loops
    /// that draw many values against the same non-power-of-two bound.
    pub(crate) fn next_i32_fast_bound(&mut self, fast: &FastBound) -> i32 {
        let bound = fast.bound;
        if (bound & -bound) == bound {
            return i32::try_from(((i64::from(bound)) * i64::from(self.next_bits(31))) >> 31)
                .unwrap_or_default();
        }
        loop {
            let bits = i32::try_from(self.next_bits(31)).unwrap_or_default();
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
            let value = fast_rem_u32(bits as u32, bound as u32, fast.magic) as i32;
            if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                return value;
            }
        }
    }

    /// Draws one canonical `nextFloat()` per cell and stores `draw < fill`,
    /// returning how many cells came up filled. The eight-step jump-ahead
    /// constants break the LCG's serial dependency chain so eight draws
    /// compute in parallel; the emitted stream is bit-identical to calling
    /// [`Self::next_f32`] once per cell.
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn fill_f32_below(&mut self, cells: &mut [bool], fill: f32) -> i32 {
        let mut filled = 0_i32;
        let mut state = self.state;
        let mut chunks = cells.chunks_exact_mut(8);
        // `draw / 2^24 < fill` over exact 24-bit integers is equivalent to
        // the pure-integer test `draw < ceil(fill * 2^24)`: multiplying an
        // f32 by a power of two is exact, so no rounding can flip the
        // comparison. Negative or oversized thresholds clamp to "none" /
        // "all" exactly as the float comparison would.
        let threshold = (fill * 16_777_216.0_f32).ceil();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let threshold = if threshold <= 0.0_f32 {
            0_u64
        } else if threshold >= 16_777_216.0_f32 {
            1 << 24
        } else {
            threshold as u64
        };
        for chunk in &mut chunks {
            let states = [
                state.wrapping_mul(MULTIPLIER).wrapping_add(ADDEND),
                state.wrapping_mul(MULTIPLIER_2).wrapping_add(ADDEND_2),
                state.wrapping_mul(MULTIPLIER_3).wrapping_add(ADDEND_3),
                state.wrapping_mul(MULTIPLIER_4).wrapping_add(ADDEND_4),
                state.wrapping_mul(MULTIPLIER_5).wrapping_add(ADDEND_5),
                state.wrapping_mul(MULTIPLIER_6).wrapping_add(ADDEND_6),
                state.wrapping_mul(MULTIPLIER_7).wrapping_add(ADDEND_7),
                state.wrapping_mul(MULTIPLIER_8).wrapping_add(ADDEND_8),
            ];
            for (cell, lane_state) in chunk.iter_mut().zip(states) {
                *cell = ((lane_state & MASK) >> 24) < threshold;
                filled += i32::from(*cell);
            }
            state = states[7] & MASK;
        }
        self.state = state;
        for cell in chunks.into_remainder() {
            *cell = self.next_f32() < fill;
            filled += i32::from(*cell);
        }
        filled
    }

    /// Mirrors Java's `nextLong()` including sign-extension of its low word.
    pub fn next_i64(&mut self) -> i64 {
        let high = i64::from(self.next_i32()) << 32;
        let low = i64::from(self.next_i32());
        high.wrapping_add(low)
    }

    /// Mirrors Java's `nextFloat()` with a single 24-bit draw.
    #[allow(clippy::cast_precision_loss)] // Java deliberately rounds this 24-bit value to float.
    pub fn next_f32(&mut self) -> f32 {
        self.next_bits(24) as f32 / 16_777_216.0_f32
    }

    /// Mirrors Java's `nextBoolean()`.
    pub fn next_bool(&mut self) -> bool {
        self.next_bits(1) != 0
    }
}

/// Precomputed division-free reciprocal for repeated `nextInt(bound)` draws
/// against one bound. Power-of-two bounds never consult `magic`.
pub(crate) struct FastBound {
    bound: i32,
    magic: u64,
}

impl FastBound {
    /// # Panics
    ///
    /// Panics if `bound <= 0`, matching `nextInt(bound)`.
    pub(crate) fn new(bound: i32) -> Self {
        assert!(bound > 0, "bound must be positive");
        #[allow(clippy::cast_sign_loss)]
        let magic = if bound >= 2 {
            bound_magic(u64::from(bound.unsigned_abs()))
        } else {
            0
        };
        Self { bound, magic }
    }
}

/// Shattered Pixel Dungeon's MX3 pre-scramble for deliberately seeded RNGs.
#[must_use]
pub const fn scramble_seed(seed: i64) -> i64 {
    let mut value = u64::from_ne_bytes(seed.to_ne_bytes());
    value ^= value >> 32;
    value = value.wrapping_mul(MX3_MULTIPLIER);
    value ^= value >> 29;
    value = value.wrapping_mul(MX3_MULTIPLIER);
    value ^= value >> 32;
    value = value.wrapping_mul(MX3_MULTIPLIER);
    value ^= value >> 29;
    i64::from_ne_bytes(value.to_ne_bytes())
}

/// A deterministic equivalent of `com.watabou.utils.Random`'s generator stack.
///
/// The real game keeps an unseeded base generator. Seed-finding code must never
/// depend on it, so this type starts with one explicit deterministic base seed.
#[derive(Clone, Debug)]
pub struct RandomStack {
    generators: Vec<JavaRandom>,
}

impl RandomStack {
    /// Creates a stack with a deterministic base generator.
    #[must_use]
    pub fn with_base_seed(base_seed: i64) -> Self {
        Self {
            generators: vec![JavaRandom::new(base_seed)],
        }
    }

    /// Pushes the same generator as Shattered's `Random.pushGenerator(seed)`.
    pub fn push(&mut self, seed: i64) {
        self.generators.push(JavaRandom::new(scramble_seed(seed)));
    }

    /// Removes the current seeded generator.
    ///
    /// # Panics
    ///
    /// Panics when asked to remove the last/base generator. The game reports a
    /// runtime exception for the same programming error.
    pub fn pop(&mut self) {
        assert!(self.generators.len() > 1, "cannot pop the base RNG");
        self.generators.pop();
    }

    fn current(&mut self) -> &mut JavaRandom {
        self.generators
            .last_mut()
            .expect("RNG stack always has a base")
    }

    /// Direct access to the active generator so tight draw loops can skip the
    /// per-draw stack lookup. The mutable borrow freezes the stack while it
    /// lives, so push/pop discipline cannot be violated mid-loop.
    pub(crate) fn current_generator(&mut self) -> &mut JavaRandom {
        self.current()
    }

    /// Mirrors Shattered's `Random.Int()`.
    pub fn int(&mut self) -> i32 {
        self.current().next_i32()
    }

    /// Mirrors Shattered's forgiving `Random.Int(max)` behavior.
    pub fn int_bound(&mut self, max: i32) -> i32 {
        if max <= 0 {
            0
        } else {
            self.current().next_i32_bound(max)
        }
    }

    /// Mirrors Shattered's `Random.Int(min, max)` behavior.
    pub fn int_between(&mut self, min: i32, max: i32) -> i32 {
        min.wrapping_add(self.int_bound(max.wrapping_sub(min)))
    }

    /// Mirrors Shattered's inclusive `Random.IntRange(min, max)` behavior.
    pub fn int_range(&mut self, min: i32, max: i32) -> i32 {
        min.wrapping_add(self.int_bound(max.wrapping_sub(min).wrapping_add(1)))
    }

    /// Mirrors Shattered's `Random.Long()`.
    pub fn long(&mut self) -> i64 {
        self.current().next_i64()
    }

    /// Mirrors Shattered's intentionally modulo-biased `Random.Long(max)`.
    pub fn long_bound(&mut self, max: i64) -> i64 {
        let mut result = self.long();
        if result < 0 {
            result = result.wrapping_add(i64::MAX);
        }
        result % max
    }

    /// Mirrors Shattered's `Random.Float()`.
    pub fn float(&mut self) -> f32 {
        self.current().next_f32()
    }

    /// Mirrors Shattered's `Random.Float(max)`.
    pub fn float_bound(&mut self, max: f32) -> f32 {
        self.float() * max
    }

    /// Mirrors Shattered's `Random.Float(min, max)`.
    pub fn float_between(&mut self, min: f32, max: f32) -> f32 {
        min + self.float_bound(max - min)
    }

    /// Mirrors Shattered's triangular `Random.NormalFloat(min, max)`.
    #[allow(clippy::manual_midpoint)] // Preserve Java's exact operation order.
    pub fn normal_float(&mut self, min: f32, max: f32) -> f32 {
        let range = max - min;
        let first = self.float_bound(range);
        let second = self.float_bound(range);
        min + (first + second) / 2.0
    }

    /// Mirrors Shattered's triangular inclusive integer distribution.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // Java casts both ways.
    pub fn normal_int_range(&mut self, min: i32, max: i32) -> i32 {
        let width = max.wrapping_sub(min).wrapping_add(1) as f32;
        let result = (self.float() + self.float()) * width / 2.0;
        min.wrapping_add(result as i32)
    }

    /// Mirrors Shattered's inverse-triangular inclusive integer distribution.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // Java casts both ways.
    pub fn inverse_normal_int_range(&mut self, min: i32, max: i32) -> i32 {
        let first = self.float();
        let second = self.float();
        let selected = if (first - 0.5).abs() >= (second - 0.5).abs() {
            first
        } else {
            second
        };
        let width = max.wrapping_sub(min).wrapping_add(1) as f32;
        min.wrapping_add((selected * width) as i32)
    }

    /// Selects an index using Shattered's ordered `Random.chances(float[])`.
    pub fn chances(&mut self, weights: &[f32]) -> Option<usize> {
        let mut sum = 0.0_f32;
        for weight in weights {
            sum += weight.max(0.0);
        }
        if sum <= 0.0 {
            return None;
        }

        let value = self.float() * sum;
        sum = 0.0;
        for (index, weight) in weights.iter().enumerate() {
            sum += weight.max(0.0);
            if value < sum {
                return Some(index);
            }
        }
        None
    }

    /// Mirrors Shattered's forward Fisher-Yates array shuffle overloads.
    pub fn shuffle_array<T>(&mut self, values: &mut [T]) {
        for index in 0..values.len().saturating_sub(1) {
            let remaining = values.len() - index;
            let Ok(remaining) = i32::try_from(remaining) else {
                return;
            };
            let offset = self.int_bound(remaining);
            let other = index + usize::try_from(offset).unwrap_or_default();
            values.swap(index, other);
        }
    }

    /// Mirrors `Collections.shuffle(list, currentGenerator)`, which uses the
    /// reverse Fisher-Yates form and therefore produces a different ordering.
    pub fn shuffle_list<T>(&mut self, values: &mut [T]) {
        for length in (2..=values.len()).rev() {
            let Ok(bound) = i32::try_from(length) else {
                return;
            };
            let other = usize::try_from(self.int_bound(bound)).unwrap_or_default();
            values.swap(length - 1, other);
        }
    }
}

/// Returns the deterministic per-depth RNG seed used by the game.
#[must_use]
pub fn seed_for_depth(dungeon_seed: i64, depth: u32, branch: u32) -> i64 {
    let look_ahead = depth + 30 * branch;
    let mut random = JavaRandom::new(scramble_seed(dungeon_seed));
    for _ in 0..look_ahead {
        random.next_i64();
    }
    random.next_i64()
}

#[cfg(test)]
mod tests {
    use super::{JavaRandom, RandomStack, scramble_seed, seed_for_depth};

    #[test]
    fn java_random_matches_openjdk_reference_vector_for_seed_zero() {
        let mut random = JavaRandom::new(0);
        assert_eq!(random.next_i32(), -1_155_484_576);
        assert_eq!(random.next_i32(), -723_955_400);
        assert_eq!(random.next_i64(), 4_437_113_781_045_784_766);
        assert_eq!(random.next_f32().to_bits(), 1_059_270_089);
    }

    #[test]
    fn bounded_java_random_matches_openjdk_reference_vector() {
        let mut random = JavaRandom::new(0x1234_5678_9ABC_DEF0_i64);
        let actual: Vec<i32> = [1, 2, 3, 7, 16, 31, 100, 1_073_741_825]
            .into_iter()
            .map(|bound| random.next_i32_bound(bound))
            .collect();
        assert_eq!(actual, [0, 0, 1, 2, 12, 25, 88, 595_637_773]);
    }

    #[test]
    fn shattered_scramble_uses_wrapping_unsigned_mx3_math() {
        assert_eq!(scramble_seed(0), 0);
        assert_eq!(scramble_seed(1), 511_322_238_924_462_111);
        assert_eq!(scramble_seed(-1), -7_581_867_460_419_221_002);
    }

    #[test]
    fn generator_stack_restores_parent_state() {
        let mut stack = RandomStack::with_base_seed(42);
        let first = stack.long();
        stack.push(99);
        let nested = stack.long();
        stack.pop();
        let second = stack.long();

        let mut reference = JavaRandom::new(42);
        assert_eq!(first, reference.next_i64());
        assert_eq!(second, reference.next_i64());

        let mut nested_reference = JavaRandom::new(scramble_seed(99));
        assert_eq!(nested, nested_reference.next_i64());
    }

    #[test]
    fn depth_seeds_are_stable() {
        assert_eq!(seed_for_depth(0, 1, 0), 4_437_113_781_045_784_766);
        assert_eq!(seed_for_depth(1, 1, 0), 5_214_745_486_231_521_494);
        assert_eq!(seed_for_depth(1, 5, 0), -8_947_671_207_897_652_788);
        assert_eq!(seed_for_depth(1, 1, 1), -4_130_634_971_943_404_754);
    }

    #[test]
    fn chances_uses_java_float_precision_and_order() {
        let mut stack = RandomStack::with_base_seed(0);
        stack.push(1234);
        let picks: Vec<usize> = (0..8)
            .map(|_| stack.chances(&[1.0, 2.0, 3.0]).unwrap())
            .collect();
        assert_eq!(picks, [2, 1, 2, 1, 2, 1, 2, 2]);
    }

    #[test]
    fn list_and_array_shuffle_match_their_distinct_java_algorithms() {
        let expected_list = [2, 4, 0, 9, 8, 7, 3, 1, 5, 6];
        let expected_array = [6, 0, 3, 1, 5, 2, 9, 7, 4, 8];

        let mut list_stack = RandomStack::with_base_seed(0);
        list_stack.push(42);
        let mut list = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        list_stack.shuffle_list(&mut list);
        assert_eq!(list, expected_list);

        let mut array_stack = RandomStack::with_base_seed(0);
        array_stack.push(42);
        let mut array = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        array_stack.shuffle_array(&mut array);
        assert_eq!(array, expected_array);
    }
}
