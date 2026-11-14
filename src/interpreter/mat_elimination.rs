//! Strict AVX row updates for Gaussian and Gauss–Jordan elimination.
//!
//! The caller selects and normalizes pivots. Workers share a read-only pivot
//! and update disjoint rows, then finish before the caller selects the next one.

use std::sync::{Barrier, Condvar, Mutex, OnceLock};

const MIN_AVX_ORDER: usize = 32;
const MIN_PARALLEL_DET_ORDER: usize = 768;
const MIN_PARALLEL_INV_ORDER: usize = 384;
const MIN_PARALLEL_TRAILING_ORDER: usize = 363;

pub(super) fn enabled(order: usize) -> bool {
    if order < MIN_AVX_ORDER {
        return false;
    }
    #[cfg(target_arch = "x86_64")]
    return std::arch::is_x86_feature_detected!("avx");
    #[cfg(not(target_arch = "x86_64"))]
    false
}

pub(super) fn determinant_threads(order: usize) -> usize {
    if order < MIN_PARALLEL_DET_ORDER {
        1
    } else {
        available_threads(4)
    }
}

pub(super) fn inverse_threads(matrix: &[f64], order: usize) -> usize {
    if order < MIN_PARALLEL_INV_ORDER {
        return 1;
    }
    let threads = available_threads(if order < 768 { 4 } else { 8 });
    if threads == 1 {
        return 1;
    }

    // INV skips zero elimination coefficients. For sparse inputs, avoid
    // starting workers based on the worst-case dense work per pivot.
    let stride = order / 16;
    let nonzero = matrix
        .chunks_exact(order)
        .step_by(stride)
        .flat_map(|row| row.iter().step_by(stride))
        .filter(|&&value| value != 0.0)
        .take(64)
        .count();
    if nonzero < 64 {
        1
    } else {
        threads
    }
}

fn available_threads(limit: usize) -> usize {
    static AVAILABLE: OnceLock<usize> = OnceLock::new();
    limit
        .min(*AVAILABLE.get_or_init(|| std::thread::available_parallelism().map_or(1, usize::from)))
}

#[derive(Clone, Copy)]
struct Pivot {
    matrix: *mut f64,
    inverse: Option<*mut f64>,
    order: usize,
    column: usize,
}

// Only the owner can publish a pivot. It borrows the backing matrices until
// all worker accesses are complete, and each worker gets exclusive rows.
unsafe impl Send for Pivot {}

impl Pivot {
    fn update(self, worker: usize, threads: usize) {
        let Self {
            matrix,
            inverse,
            order,
            column,
        } = self;
        if let Some(inverse) = inverse {
            let begin = order * worker / threads;
            let end = order * (worker + 1) / threads;
            // SAFETY: Workers never write either pivot row and their writable
            // ranges do not overlap. Both matrices hold order * order elements.
            unsafe {
                let a_pivot = std::slice::from_raw_parts(matrix.add(column * order), order);
                let inverse_pivot = std::slice::from_raw_parts(inverse.add(column * order), order);
                for row in begin..end {
                    if row == column {
                        continue;
                    }
                    let a_row = matrix.add(row * order);
                    let factor = *a_row.add(column);
                    if factor != 0.0 {
                        subtract_scaled(
                            std::slice::from_raw_parts_mut(a_row, order),
                            a_pivot,
                            factor,
                        );
                        subtract_scaled(
                            std::slice::from_raw_parts_mut(inverse.add(row * order), order),
                            inverse_pivot,
                            factor,
                        );
                    }
                }
            }
        } else {
            let trailing = order - column - 1;
            let begin = column + 1 + trailing * worker / threads;
            let end = column + 1 + trailing * (worker + 1) / threads;
            // SAFETY: Only trailing rows change. The pivot remains read-only
            // and each mutable slice covers one worker's row only.
            unsafe {
                let a_pivot = matrix.add(column * order + column);
                let divisor = *a_pivot;
                let pivot_tail = std::slice::from_raw_parts(a_pivot.add(1), trailing);
                for row in begin..end {
                    let a_row = matrix.add(row * order + column);
                    let factor = *a_row / divisor;
                    subtract_scaled(
                        std::slice::from_raw_parts_mut(a_row.add(1), trailing),
                        pivot_tail,
                        factor,
                    );
                    *a_row = 0.0;
                }
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
unsafe fn subtract_scaled(row: &mut [f64], pivot: &[f64], factor: f64) {
    use std::arch::x86_64::{
        _mm256_loadu_pd, _mm256_mul_pd, _mm256_set1_pd, _mm256_storeu_pd, _mm256_sub_pd,
    };
    let factor_v = _mm256_set1_pd(factor);
    let vector_end = row.len() / 4 * 4;
    for column in (0..vector_end).step_by(4) {
        // SAFETY: The row and pivot are disjoint and have the same length.
        unsafe {
            let old = _mm256_loadu_pd(row.as_ptr().add(column));
            let value = _mm256_loadu_pd(pivot.as_ptr().add(column));
            // Separate multiply and subtract preserve the two scalar roundings.
            let updated = _mm256_sub_pd(old, _mm256_mul_pd(factor_v, value));
            _mm256_storeu_pd(row.as_mut_ptr().add(column), updated);
        }
    }
    for (value, &pivot) in row[vector_end..].iter_mut().zip(&pivot[vector_end..]) {
        *value -= factor * pivot;
    }
}

#[cfg(not(target_arch = "x86_64"))]
unsafe fn subtract_scaled(row: &mut [f64], pivot: &[f64], factor: f64) {
    for (value, &pivot) in row.iter_mut().zip(pivot) {
        *value -= factor * pivot;
    }
}

struct SharedWorkers {
    threads: usize,
    barrier: Barrier,
    pivot: Mutex<Option<Pivot>>,
}

impl SharedWorkers {
    fn new(threads: usize) -> Self {
        Self {
            threads,
            barrier: Barrier::new(threads),
            pivot: Mutex::new(None),
        }
    }

    fn work(&self, index: usize) {
        loop {
            self.barrier.wait();
            let pivot = *self.pivot.lock().unwrap();
            let Some(pivot) = pivot else { return };
            pivot.update(index, self.threads);
            self.barrier.wait();
        }
    }
}

pub(super) struct Workers<'a>(&'a SharedWorkers);

impl Workers<'_> {
    pub(super) fn update(
        &mut self,
        matrix: &mut [f64],
        inverse: Option<&mut [f64]>,
        order: usize,
        column: usize,
    ) {
        assert!(column < order);
        assert_eq!(order.checked_mul(order), Some(matrix.len()));
        if let Some(inverse) = &inverse {
            assert_eq!(matrix.len(), inverse.len());
        }
        let shared = self.0;
        let parallel = shared.threads > 1
            && (inverse.is_some() || order - column - 1 >= MIN_PARALLEL_TRAILING_ORDER);
        let pivot = Pivot {
            matrix: matrix.as_mut_ptr(),
            inverse: inverse.map(<[f64]>::as_mut_ptr),
            order,
            column,
        };
        if parallel {
            *shared.pivot.lock().unwrap() = Some(pivot);
            shared.barrier.wait();
            pivot.update(0, shared.threads);
            shared.barrier.wait();
        } else {
            pivot.update(0, 1);
        }
    }
}

pub(super) fn run<T>(threads: usize, operation: impl FnOnce(&mut Workers<'_>) -> T) -> T {
    // The AVX calls run on this thread and on the workers it creates.
    assert!(enabled(MIN_AVX_ORDER));
    if threads == 1 {
        return operation(&mut Workers(&SharedWorkers::new(1)));
    }
    let workers = SharedWorkers::new(threads);
    let (start, ready) = (Mutex::new(None), Condvar::new());
    std::thread::scope(|scope| {
        let mut all_spawned = true;
        for index in 1..threads {
            let (workers, start, ready) = (&workers, &start, &ready);
            if std::thread::Builder::new()
                .spawn_scoped(scope, move || {
                    let start = start.lock().unwrap();
                    let start = ready.wait_while(start, |start| start.is_none()).unwrap();
                    let active = *start == Some(true);
                    drop(start);
                    if active {
                        workers.work(index);
                    }
                })
                .is_err()
            {
                all_spawned = false;
                break;
            }
        }
        // Workers only enter the fixed-size barrier if every launch succeeded.
        *start.lock().unwrap() = Some(all_spawned);
        ready.notify_all();
        if !all_spawned {
            return operation(&mut Workers(&SharedWorkers::new(1)));
        }

        struct Finish<'a>(&'a SharedWorkers);
        impl Drop for Finish<'_> {
            fn drop(&mut self) {
                *self.0.pivot.lock().unwrap() = None;
                self.0.barrier.wait();
            }
        }
        let _finish = Finish(&workers);
        operation(&mut Workers(&workers))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::{ArrayData, Interpreter, NumericMatrix};

    fn dense_matrix(order: usize) -> Vec<f64> {
        (0..order * order)
            .map(|i| {
                if i / order == (i % order + 1) % order {
                    1.00390625
                } else {
                    ((i * 13 % 31) as f64 - 15.0) / 8192.0
                }
            })
            .collect()
    }

    fn bits(values: &[f64]) -> impl Iterator<Item = u64> + '_ {
        values.iter().map(|value| value.to_bits())
    }

    #[test]
    fn elimination_keeps_order_and_rounding() {
        let order = 65;
        if !enabled(order) {
            return;
        }
        let mut input = dense_matrix(order);
        input[0] = -0.0;
        input[1] = f64::from_bits(1);
        let mut reference = input.clone();
        let mut candidate = input.clone();
        let expected = Interpreter::eliminate_determinant(&mut reference, order, None);
        let actual = run(1, |workers| {
            Interpreter::eliminate_determinant(&mut candidate, order, Some(workers))
        });
        assert_eq!(expected.to_bits(), actual.to_bits());
        assert!(bits(&reference).eq(bits(&candidate)));

        for base in [0, 1] {
            let mut interpreter = Interpreter::new();
            interpreter.mat_base = base;
            let matrix = || NumericMatrix {
                rows: order,
                cols: order,
                data: input.clone(),
            };
            let expected = interpreter
                .invert_numeric_matrix_with_avx(matrix(), false)
                .unwrap();
            let actual = interpreter.invert_numeric_matrix(matrix()).unwrap();
            assert_eq!(expected.dims, actual.dims);
            let (ArrayData::Number(expected), ArrayData::Number(actual)) =
                (expected.data, actual.data)
            else {
                unreachable!();
            };
            assert!(bits(&expected).eq(bits(&actual)));
        }
    }

    #[test]
    fn parallel_pivot_updates_match_one_worker() {
        let order = 385;
        if !enabled(order) {
            return;
        }
        let input = dense_matrix(order);
        for inverse in [false, true] {
            let mut reference = input.clone();
            let mut candidate = input.clone();
            let mut reference_inverse = input.clone();
            let mut candidate_inverse = input.clone();
            for threads in [1, 4] {
                let (matrix, inverse_matrix) = if threads == 1 {
                    (&mut reference, &mut reference_inverse)
                } else {
                    (&mut candidate, &mut candidate_inverse)
                };
                run(threads, |workers| {
                    for column in [3, 9, 14] {
                        workers.update(
                            matrix,
                            inverse.then_some(&mut *inverse_matrix),
                            order,
                            column,
                        );
                    }
                });
            }
            assert!(bits(&reference).eq(bits(&candidate)));
            assert!(bits(&reference_inverse).eq(bits(&candidate_inverse)));
        }
    }
}
