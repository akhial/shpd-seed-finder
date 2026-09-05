//! Conversion between byte-sized boolean cells and bit-parallel terrain rows.
//!
//! SSE2 handles sixteen cells at a time on x86-64 without runtime dispatch or
//! additional CPU requirements. Other architectures keep the portable path.

#![allow(unsafe_code)]

/// Packs at most 64 cells, with the first cell in the least significant bit.
pub(crate) fn pack(cells: &[bool]) -> u64 {
    assert!(cells.len() <= 64);
    let mut bits = 0_u64;
    #[cfg(not(target_arch = "x86_64"))]
    let offset = 0;
    #[cfg(target_arch = "x86_64")]
    let offset = {
        use std::arch::x86_64::{_mm_loadu_si128, _mm_movemask_epi8, _mm_slli_epi16};
        let mut offset = 0;
        for chunk in cells.chunks_exact(16) {
            // SAFETY: SSE2 is baseline on x86-64. The chunk contains sixteen
            // initialized bool bytes (0 or 1); the unaligned load stays inside
            // it. Shifting each word by seven puts both bools in its sign bits.
            let mask = unsafe {
                _mm_movemask_epi8(_mm_slli_epi16::<7>(_mm_loadu_si128(chunk.as_ptr().cast())))
            };
            bits |= u64::from(mask.unsigned_abs()) << offset;
            offset += 16;
        }
        offset
    };
    for (index, &cell) in cells[offset..].iter().enumerate() {
        bits |= u64::from(cell) << (offset + index);
    }
    bits
}

/// Unpacks the low bits into at most 64 cells.
pub(crate) fn unpack(bits: u64, cells: &mut [bool]) {
    assert!(cells.len() <= 64);
    #[cfg(not(target_arch = "x86_64"))]
    let offset = 0;
    #[cfg(target_arch = "x86_64")]
    let offset = {
        use std::arch::x86_64::{
            _mm_and_si128, _mm_cmpeq_epi8, _mm_set_epi64x, _mm_set1_epi8, _mm_storeu_si128,
            _mm_unpacklo_epi64,
        };
        let mut offset = 0;
        for chunk in cells.chunks_exact_mut(16) {
            let bytes = (bits >> offset).to_le_bytes();
            // SAFETY: SSE2 is baseline on x86-64, and the unaligned store is
            // limited to the sixteen writable bytes in this chunk. Masking
            // the comparison with 1 writes only valid bool representations.
            unsafe {
                let repeated = _mm_unpacklo_epi64(
                    _mm_set1_epi8(i8::from_ne_bytes([bytes[0]])),
                    _mm_set1_epi8(i8::from_ne_bytes([bytes[1]])),
                );
                let masks = _mm_set_epi64x(
                    i64::from_ne_bytes([1, 2, 4, 8, 16, 32, 64, 128]),
                    i64::from_ne_bytes([1, 2, 4, 8, 16, 32, 64, 128]),
                );
                let values = _mm_and_si128(
                    _mm_cmpeq_epi8(_mm_and_si128(repeated, masks), masks),
                    _mm_set1_epi8(1),
                );
                _mm_storeu_si128(chunk.as_mut_ptr().cast(), values);
            }
            offset += 16;
        }
        offset
    };
    for (index, cell) in cells[offset..].iter_mut().enumerate() {
        *cell = bits & (1 << (offset + index)) != 0;
    }
}

#[cfg(test)]
mod tests {
    use super::{pack, unpack};

    #[test]
    fn conversions_match_scalar_for_all_lengths_and_unaligned_slices() {
        let mut patterns = vec![0, u64::MAX, 0xAAAA_AAAA_AAAA_AAAA, 0x5555_5555_5555_5555];
        patterns.extend((0..64).map(|bit| 1_u64 << bit));
        let mut random = crate::rng::JavaRandom::new(123);
        patterns.extend((0..128).map(|_| u64::from_ne_bytes(random.next_i64().to_ne_bytes())));
        for bits in patterns {
            for length in 0..=64 {
                for offset in 0..16 {
                    let mut cells = [true; 96];
                    unpack(bits, &mut cells[offset..offset + length]);
                    for (index, &cell) in cells[offset..offset + length].iter().enumerate() {
                        assert_eq!(cell, bits & (1 << index) != 0);
                    }
                    assert!(cells[..offset].iter().all(|&cell| cell));
                    assert!(cells[offset + length..].iter().all(|&cell| cell));
                    let mask = u64::MAX
                        .checked_shr(64 - u32::try_from(length).unwrap())
                        .unwrap_or(0);
                    assert_eq!(pack(&cells[offset..offset + length]), bits & mask);
                }
            }
        }
    }
}
