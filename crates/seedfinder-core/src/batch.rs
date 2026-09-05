//! Data-parallel seed primitives.
//!
//! Independent candidate seeds provide a SIMD boundary before branch-heavy
//! spatial generation. ARM64 builds use NEON lanes for MX3 and Java LCG
//! arithmetic; `bit_rows` handles x86-64 SIMD conversion of terrain cells.

#![allow(unsafe_code)]

#[cfg(not(target_arch = "aarch64"))]
use crate::rng::seed_for_depth;

/// Computes four independent depth roots. On ARM64 this uses two 128-bit NEON
/// vectors; other targets use the scalar parity implementation.
#[must_use]
pub fn seed_for_depth_batch4(seeds: [i64; 4], depth: u32, branch: u32) -> [i64; 4] {
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: AArch64 guarantees Advanced SIMD/NEON support. The helper
        // only loads/stores fixed local arrays and performs lane arithmetic.
        unsafe { aarch64::seed_for_depth_batch4_neon(seeds, depth, branch) }
    }

    #[cfg(not(target_arch = "aarch64"))]
    seeds.map(|seed| seed_for_depth(seed, depth, branch))
}

#[cfg(target_arch = "aarch64")]
mod aarch64 {
    use std::arch::aarch64::{
        uint32x2_t, uint64x2_t, vaddq_u64, vandq_u64, vdupq_n_u64, veorq_u64, vld1q_u64, vmovn_u64,
        vmull_u32, vshlq_n_u64, vshrq_n_u64, vst1q_u64,
    };

    const MULTIPLIER: u64 = 0x0005_DEEC_E66D;
    const ADDEND: u64 = 0xB;
    const MASK: u64 = (1_u64 << 48) - 1;
    const MX3_MULTIPLIER: u64 = 0xBEA2_25F9_EB34_556D;

    pub(super) unsafe fn seed_for_depth_batch4_neon(
        seeds: [i64; 4],
        depth: u32,
        branch: u32,
    ) -> [i64; 4] {
        let bits = seeds.map(|seed| u64::from_ne_bytes(seed.to_ne_bytes()));
        // SAFETY: Both pointers address at least two contiguous u64 values.
        let first = unsafe { vld1q_u64(bits.as_ptr()) };
        let second = unsafe { vld1q_u64(bits.as_ptr().add(2)) };
        let first_result = unsafe { depth_pair(first, depth, branch) };
        let second_result = unsafe { depth_pair(second, depth, branch) };
        [
            first_result[0],
            first_result[1],
            second_result[0],
            second_result[1],
        ]
    }

    unsafe fn depth_pair(seeds: uint64x2_t, depth: u32, branch: u32) -> [i64; 2] {
        let mut scrambled = seeds;
        scrambled = unsafe { veorq_u64(scrambled, vshrq_n_u64::<32>(scrambled)) };
        scrambled = unsafe { mul_low_u64(scrambled, vdupq_n_u64(MX3_MULTIPLIER)) };
        scrambled = unsafe { veorq_u64(scrambled, vshrq_n_u64::<29>(scrambled)) };
        scrambled = unsafe { mul_low_u64(scrambled, vdupq_n_u64(MX3_MULTIPLIER)) };
        scrambled = unsafe { veorq_u64(scrambled, vshrq_n_u64::<32>(scrambled)) };
        scrambled = unsafe { mul_low_u64(scrambled, vdupq_n_u64(MX3_MULTIPLIER)) };
        scrambled = unsafe { veorq_u64(scrambled, vshrq_n_u64::<29>(scrambled)) };

        let mask = unsafe { vdupq_n_u64(MASK) };
        let multiplier = unsafe { vdupq_n_u64(MULTIPLIER) };
        let addend = unsafe { vdupq_n_u64(ADDEND) };
        let mut state = unsafe { vandq_u64(veorq_u64(scrambled, multiplier), mask) };
        let pairs = depth
            .saturating_add(30_u32.saturating_mul(branch))
            .saturating_add(1);
        let mut high_state = state;
        let mut low_state = state;
        for _ in 0..pairs {
            state = unsafe { lcg_step(state, multiplier, addend, mask) };
            high_state = state;
            state = unsafe { lcg_step(state, multiplier, addend, mask) };
            low_state = state;
        }

        let mut high = [0_u64; 2];
        let mut low = [0_u64; 2];
        // SAFETY: Both output arrays contain two writable u64 values.
        unsafe {
            vst1q_u64(high.as_mut_ptr(), high_state);
            vst1q_u64(low.as_mut_ptr(), low_state);
        }
        [
            combine_next_long(high[0], low[0]),
            combine_next_long(high[1], low[1]),
        ]
    }

    unsafe fn lcg_step(
        state: uint64x2_t,
        multiplier: uint64x2_t,
        addend: uint64x2_t,
        mask: uint64x2_t,
    ) -> uint64x2_t {
        unsafe { vandq_u64(vaddq_u64(mul_low_u64(state, multiplier), addend), mask) }
    }

    // NEON has widening 32×32 multiplication but no baseline 64×64 vector
    // multiply. Decomposing each lane yields the exact low 64 bits required by
    // Java's wrapping long arithmetic.
    unsafe fn mul_low_u64(left: uint64x2_t, right: uint64x2_t) -> uint64x2_t {
        let left_low: uint32x2_t = unsafe { vmovn_u64(left) };
        let left_high: uint32x2_t = unsafe { vmovn_u64(vshrq_n_u64::<32>(left)) };
        let right_low: uint32x2_t = unsafe { vmovn_u64(right) };
        let right_high: uint32x2_t = unsafe { vmovn_u64(vshrq_n_u64::<32>(right)) };
        let low = unsafe { vmull_u32(left_low, right_low) };
        let cross = unsafe {
            vaddq_u64(
                vmull_u32(left_low, right_high),
                vmull_u32(left_high, right_low),
            )
        };
        unsafe { vaddq_u64(low, vshlq_n_u64::<32>(cross)) }
    }

    fn combine_next_long(high_state: u64, low_state: u64) -> i64 {
        let high_bits = u32::try_from(high_state >> 16).unwrap_or_default();
        let low_bits = u32::try_from(low_state >> 16).unwrap_or_default();
        let high = i64::from(i32::from_ne_bytes(high_bits.to_ne_bytes())) << 32;
        let low = i64::from(i32::from_ne_bytes(low_bits.to_ne_bytes()));
        high.wrapping_add(low)
    }
}

#[cfg(test)]
mod tests {
    use super::seed_for_depth_batch4;
    use crate::rng::seed_for_depth;

    #[test]
    fn batch_matches_scalar_for_boundaries_and_signed_bit_patterns() {
        let batches = [
            [0, 1, 25, 26],
            [-1, i64::MIN, i64::MAX, 8_687_205_886],
            [123, 456_789, -987_654_321, 5_429_503_678_975],
        ];
        for seeds in batches {
            for (depth, branch) in [(1, 0), (5, 0), (24, 0), (11, 1), (16, 7)] {
                assert_eq!(
                    seed_for_depth_batch4(seeds, depth, branch),
                    seeds.map(|seed| seed_for_depth(seed, depth, branch))
                );
            }
        }
    }
}
