#[cfg(target_arch = "x86_64")]
pub(super) fn avx_available() -> bool {
    std::arch::is_x86_feature_detected!("avx")
}

#[cfg(not(target_arch = "x86_64"))]
pub(super) fn avx_available() -> bool {
    false
}

pub(super) fn divide(numerators: &mut [f64], divisors: &[f64]) -> bool {
    #[cfg(target_arch = "x86_64")]
    if avx_available() {
        // SAFETY: AVX is supported by the CPU and enabled by the OS.
        return unsafe { divide_avx(numerators, divisors) };
    }
    divide_scalar(numerators, divisors)
}

fn divide_scalar(numerators: &mut [f64], divisors: &[f64]) -> bool {
    for (numerator, &divisor) in numerators.iter_mut().zip(divisors) {
        if divisor == 0.0 {
            return false;
        }
        *numerator /= divisor;
    }
    true
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
fn divide_avx(numerators: &mut [f64], divisors: &[f64]) -> bool {
    use std::arch::x86_64::{
        _mm256_cmp_pd, _mm256_div_pd, _mm256_loadu_pd, _mm256_movemask_pd, _mm256_setzero_pd,
        _mm256_storeu_pd, _CMP_EQ_OQ,
    };

    let mut left = numerators.chunks_exact_mut(4);
    let mut right = divisors.chunks_exact(4);
    let zero = _mm256_setzero_pd();
    for (numerator, divisor) in left.by_ref().zip(right.by_ref()) {
        // SAFETY: each chunk contains four elements and allows unaligned access.
        let divisor = unsafe { _mm256_loadu_pd(divisor.as_ptr()) };
        if _mm256_movemask_pd(_mm256_cmp_pd::<_CMP_EQ_OQ>(divisor, zero)) != 0 {
            return false;
        }
        unsafe {
            let values = _mm256_loadu_pd(numerator.as_ptr());
            _mm256_storeu_pd(numerator.as_mut_ptr(), _mm256_div_pd(values, divisor));
        }
    }
    divide_scalar(left.into_remainder(), right.remainder())
}
