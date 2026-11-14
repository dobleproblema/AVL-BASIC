use super::SimpleRng;
use std::arch::x86_64::*;

#[target_feature(enable = "avx2")]
pub(super) unsafe fn fill(rng: &mut SimpleRng, values: &mut [f64], dims: &[usize], base_one: bool) {
    if base_one {
        rng.fill_base_one(values, dims, |rng, row| fill_row(rng, row));
    } else {
        fill_row(rng, values);
    }
}

#[target_feature(enable = "avx2")]
fn fill_row(rng: &mut SimpleRng, mut dest: &mut [f64]) {
    let even = _mm256_setr_epi32(0, 2, 4, 6, 0, 0, 0, 0);
    let odd = _mm256_setr_epi32(1, 3, 5, 7, 0, 0, 0, 0);
    let scale_first = _mm256_set1_pd(1.0 / ((1u64 << 27) as f64));
    let scale_second = _mm256_set1_pd(1.0 / ((1u64 << 53) as f64));
    let temper7 = _mm256_set1_epi32(0x9d2c_5680u32 as i32);
    let temper15 = _mm256_set1_epi32(0xefc6_0000u32 as i32);

    while !dest.is_empty() {
        if rng.index == rng.state.len() {
            twist(&mut rng.state);
            rng.index = 0;
        }
        // Every public draw uses two words, so the index is always even here.
        let count = dest.len().min((rng.state.len() - rng.index) / 2);
        let end = rng.index + count * 2;
        let words = &rng.state[rng.index..end];
        let (batch, remaining) = dest.split_at_mut(count);
        let mut pairs = words.chunks_exact(8);
        let mut output = batch.chunks_exact_mut(4);
        for (words, output) in pairs.by_ref().zip(output.by_ref()) {
            // SAFETY: The chunks contain eight u32s and four f64s respectively.
            unsafe {
                let mut x = _mm256_loadu_si256(words.as_ptr().cast());
                x = _mm256_xor_si256(x, _mm256_srli_epi32::<11>(x));
                x = _mm256_xor_si256(x, _mm256_and_si256(_mm256_slli_epi32::<7>(x), temper7));
                x = _mm256_xor_si256(x, _mm256_and_si256(_mm256_slli_epi32::<15>(x), temper15));
                x = _mm256_xor_si256(x, _mm256_srli_epi32::<18>(x));
                let a = _mm256_permutevar8x32_epi32(_mm256_srli_epi32::<5>(x), even);
                let b = _mm256_permutevar8x32_epi32(_mm256_srli_epi32::<6>(x), odd);
                let a = _mm256_cvtepi32_pd(_mm256_castsi256_si128(a));
                let b = _mm256_cvtepi32_pd(_mm256_castsi256_si128(b));
                // Both scaled terms and their sum are exact in binary64:
                // the combined numerator contains at most 53 bits.
                let numbers = _mm256_add_pd(
                    _mm256_mul_pd(a, scale_first),
                    _mm256_mul_pd(b, scale_second),
                );
                _mm256_storeu_pd(output.as_mut_ptr(), numbers);
            }
        }
        for (pair, value) in pairs
            .remainder()
            .chunks_exact(2)
            .zip(output.into_remainder())
        {
            *value = SimpleRng::number_from_words(
                SimpleRng::temper(pair[0]),
                SimpleRng::temper(pair[1]),
            );
        }
        rng.index = end;
        dest = remaining;
    }
}

#[target_feature(enable = "avx2")]
fn twist(state: &mut [u32; 624]) {
    let high_bit = _mm256_set1_epi32(i32::MIN);
    let low_bits = _mm256_set1_epi32(i32::MAX);
    let one = _mm256_set1_epi32(1);
    let magic = _mm256_set1_epi32(0x9908_b0dfu32 as i32);
    // Before the wrap, the source is ahead of the destination. After it wraps,
    // the scalar recurrence reads words already updated in this same twist.
    for (start, end, source_start) in [(0, 227, 397), (227, 623, 0)] {
        let mut index = start;
        let mut source = source_start;
        let ptr = state.as_mut_ptr();
        while index + 8 <= end {
            // SAFETY: Each window is within the 624 words. The source is at
            // least 227 words away, so vector stores preserve recurrence order.
            unsafe {
                let a = _mm256_loadu_si256(ptr.add(index).cast());
                let b = _mm256_loadu_si256(ptr.add(index + 1).cast());
                let c = _mm256_loadu_si256(ptr.add(source).cast());
                let x =
                    _mm256_or_si256(_mm256_and_si256(a, high_bit), _mm256_and_si256(b, low_bits));
                let odd = _mm256_cmpeq_epi32(_mm256_and_si256(b, one), one);
                let shifted = _mm256_srli_epi32::<1>(x);
                let output =
                    _mm256_xor_si256(c, _mm256_xor_si256(shifted, _mm256_and_si256(odd, magic)));
                _mm256_storeu_si256(ptr.add(index).cast(), output);
            }
            index += 8;
            source += 8;
        }
        while index < end {
            state[index] = twist_word(state[index], state[index + 1], state[source]);
            index += 1;
            source += 1;
        }
    }
    state[623] = twist_word(state[623], state[0], state[396]);
}

#[inline]
fn twist_word(a: u32, b: u32, source: u32) -> u32 {
    let x = (a & 0x8000_0000) | (b & 0x7fff_ffff);
    source ^ (x >> 1) ^ (0u32.wrapping_sub(b & 1) & 0x9908_b0df)
}
