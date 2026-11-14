//! Cache-blocked FP64 matrix multiplication.
//!
//! Pack the right operand into contiguous column micro-panels, and keep
//! separate multiply and add instructions. Parallelism only partitions output
//! rows. Each output starts at +0 and accumulates in ascending inner index.

#[cfg(all(test, target_arch = "x86_64"))]
pub(super) use x86::try_multiply_queued;
#[cfg(target_arch = "x86_64")]
pub(super) use x86::{try_multiply, try_multiply_small};

#[cfg(not(target_arch = "x86_64"))]
pub(super) fn try_multiply(
    _a: &[f64],
    _b: &[f64],
    _out: &mut [f64],
    _m: usize,
    _k: usize,
    _n: usize,
    _threads: usize,
) -> bool {
    false
}

#[cfg(not(target_arch = "x86_64"))]
pub(super) fn try_multiply_small(
    _a: &[f64],
    _b: &[f64],
    _out: &mut [f64],
    _m: usize,
    _k: usize,
    _n: usize,
) -> bool {
    false
}

#[cfg(target_arch = "x86_64")]
mod x86 {
    const COLUMN_PANEL: usize = 128;
    const LARGE_COLUMN_PANEL: usize = 120;
    const DEPTH_PANEL: usize = 128;
    const MICRO_COLUMNS: usize = 8;
    const PACK_COLUMNS: usize = 12;
    const MICRO_ROWS: usize = 4;

    /// Computes a small product without packing either input or starting workers.
    pub(crate) fn try_multiply_small(
        a: &[f64],
        b: &[f64],
        out: &mut [f64],
        m: usize,
        k: usize,
        n: usize,
    ) -> bool {
        if m < 5
            || k < 5
            || n < 5
            || m.checked_mul(k) != Some(a.len())
            || k.checked_mul(n) != Some(b.len())
            || m.checked_mul(n) != Some(out.len())
            || !std::arch::is_x86_feature_detected!("avx")
        {
            return false;
        }
        // SAFETY: The dimension and slice contracts match the blocked path, and
        // the CPU and OS support AVX.
        unsafe { avx::compute_small(a, b, out, m, k, n) };
        true
    }

    /// Returns false before writing any output if the blocked kernel is unavailable.
    pub(crate) fn try_multiply(
        a: &[f64],
        b: &[f64],
        out: &mut [f64],
        m: usize,
        k: usize,
        n: usize,
        threads: usize,
    ) -> bool {
        if m == 0
            || k == 0
            || n < 4
            || m.checked_mul(k) != Some(a.len())
            || k.checked_mul(n) != Some(b.len())
            || m.checked_mul(n) != Some(out.len())
            || !std::arch::is_x86_feature_detected!("avx")
        {
            return false;
        }
        let workers = threads.max(1).min(m.div_ceil(MICRO_ROWS));
        if workers >= 8 && m >= 2048 && k >= 2048 && n >= 2048 {
            // SAFETY: AVX and the input/output lengths were checked above.
            return unsafe { multiply_queued(a, b, out, k, n, workers) };
        }
        let Some(packed) = PackedRight::try_new::<COLUMN_PANEL, MICRO_COLUMNS>(b, k, n) else {
            return false;
        };
        let chunk_rows = m.div_ceil(workers * MICRO_ROWS) * MICRO_ROWS;
        if chunk_rows >= m {
            compute(a, out, m, k, n, &packed);
            return true;
        }

        let completed = std::thread::scope(|scope| {
            let mut chunks = out.chunks_mut(chunk_rows * n).enumerate();
            let (_, first) = chunks.next().expect("nonempty output");
            for (index, chunk) in chunks {
                let rows = chunk.len() / n;
                let begin = index * chunk_rows * k;
                let a_rows = &a[begin..begin + rows * k];
                let packed = &packed;
                if std::thread::Builder::new()
                    .spawn_scoped(scope, move || compute(a_rows, chunk, rows, k, n, packed))
                    .is_err()
                {
                    // The scope joins the workers already started. Recompute all rows
                    // from +0 on this thread after they have finished.
                    return false;
                }
            }
            compute(&a[..chunk_rows * k], first, chunk_rows, k, n, &packed);
            true
        });
        if !completed {
            compute(a, out, m, k, n, &packed);
        }
        true
    }

    #[cfg(test)]
    pub(crate) fn try_multiply_queued(
        a: &[f64],
        b: &[f64],
        out: &mut [f64],
        m: usize,
        k: usize,
        n: usize,
        workers: usize,
    ) -> bool {
        if !std::arch::is_x86_feature_detected!("avx")
            || m.checked_mul(k) != Some(a.len())
            || k.checked_mul(n) != Some(b.len())
            || m.checked_mul(n) != Some(out.len())
            || n == 0
            || k == 0
        {
            return false;
        }
        // SAFETY: AVX and all dimensions were just validated.
        unsafe { multiply_queued(a, b, out, k, n, workers) }
    }

    // Requires AVX and complete input and output slices for the supplied strides.
    unsafe fn multiply_queued(
        a: &[f64],
        b: &[f64],
        out: &mut [f64],
        k: usize,
        n: usize,
        workers: usize,
    ) -> bool {
        let Some(packed) = PackedRight::try_new::<LARGE_COLUMN_PANEL, PACK_COLUMNS>(b, k, n) else {
            return false;
        };

        // Workers claim disjoint output rows. Faster cores can claim more
        // strips, and the queue never partitions an individual dot product.
        const ROWS_PER_JOB: usize = 56;
        let jobs = std::sync::Mutex::new(out.chunks_mut(ROWS_PER_JOB * n).enumerate());
        std::thread::scope(|scope| {
            let run_jobs = || loop {
                // Release the lock before computing an entire strip.
                let next = jobs.lock().unwrap().next();
                let Some((index, chunk)) = next else {
                    return;
                };
                let rows = chunk.len() / n;
                let begin = index * ROWS_PER_JOB * k;
                // SAFETY: AVX and all dimensions were validated by the caller.
                unsafe {
                    avx::compute_large(&a[begin..begin + rows * k], chunk, rows, k, n, &packed)
                };
            };
            for _ in 1..workers {
                if std::thread::Builder::new()
                    .spawn_scoped(scope, run_jobs)
                    .is_err()
                {
                    // Workers already started and this thread finish the queue.
                    break;
                }
            }
            run_jobs();
        });
        true
    }

    #[inline]
    fn compute(a: &[f64], out: &mut [f64], rows: usize, k: usize, n: usize, packed: &PackedRight) {
        // SAFETY: try_multiply verifies that the CPU and OS support AVX.
        unsafe { avx::compute(a, out, rows, k, n, packed) };
    }

    struct Panel {
        column: usize,
        columns: usize,
        depth: usize,
        depth_len: usize,
        packed_start: usize,
    }

    struct PackedRight {
        values: Vec<f64>,
        panels: Vec<Panel>,
    }

    impl PackedRight {
        fn try_new<const PANEL_WIDTH: usize, const TILE_WIDTH: usize>(
            b: &[f64],
            k: usize,
            n: usize,
        ) -> Option<Self> {
            let panel_count = n
                .div_ceil(PANEL_WIDTH)
                .checked_mul(k.div_ceil(DEPTH_PANEL))?;
            let mut values = Vec::new();
            let mut panels = Vec::new();
            values.try_reserve_exact(b.len()).ok()?;
            panels.try_reserve_exact(panel_count).ok()?;
            for column in (0..n).step_by(PANEL_WIDTH) {
                let columns = PANEL_WIDTH.min(n - column);
                for depth in (0..k).step_by(DEPTH_PANEL) {
                    let depth_len = DEPTH_PANEL.min(k - depth);
                    panels.push(Panel {
                        column,
                        columns,
                        depth,
                        depth_len,
                        packed_start: values.len(),
                    });
                    for tile_column in (0..columns).step_by(TILE_WIDTH) {
                        let tile_width = TILE_WIDTH.min(columns - tile_column);
                        let begin = depth * n + column + tile_column;
                        for row in 0..depth_len {
                            let start = begin + row * n;
                            values.extend_from_slice(&b[start..start + tile_width]);
                        }
                    }
                }
            }
            Some(Self { values, panels })
        }
    }

    // Evaluate the columns from column_offset to width. The vectorized kernel
    // also uses this routine for its final one to three columns.
    #[inline]
    fn scalar_tile(
        left: &[f64],
        right: &[f64],
        out: &mut [f64],
        width: usize,
        column_offset: usize,
        first_depth: bool,
    ) {
        for (column, target) in out.iter_mut().enumerate() {
            let mut acc = if first_depth { 0.0 } else { *target };
            for (coefficient, right_row) in left.iter().zip(right.chunks_exact(width)) {
                acc += *coefficient * right_row[column + column_offset];
            }
            *target = acc;
        }
    }

    mod avx {
        use super::{scalar_tile, PackedRight, MICRO_COLUMNS, MICRO_ROWS, PACK_COLUMNS};
        use std::arch::x86_64::{
            __m256d, _mm256_add_pd, _mm256_loadu_pd, _mm256_mul_pd, _mm256_set1_pd,
            _mm256_setzero_pd, _mm256_storeu_pd,
        };

        #[target_feature(enable = "avx")]
        pub(super) fn compute_small(
            a: &[f64],
            b: &[f64],
            out: &mut [f64],
            m: usize,
            k: usize,
            n: usize,
        ) {
            let full_rows = m / MICRO_ROWS * MICRO_ROWS;
            for row in (0..full_rows).step_by(MICRO_ROWS) {
                small_rows::<4>(&a[row * k..], b, &mut out[row * n..], k, n);
            }
            if full_rows != m {
                let left = &a[full_rows * k..];
                let output = &mut out[full_rows * n..];
                match m - full_rows {
                    1 => small_rows::<1>(left, b, output, k, n),
                    2 => small_rows::<2>(left, b, output, k, n),
                    3 => small_rows::<3>(left, b, output, k, n),
                    _ => unreachable!("the last tile has at most three rows"),
                }
            }
        }

        #[inline]
        #[target_feature(enable = "avx")]
        fn small_rows<const ROWS: usize>(
            a: &[f64],
            b: &[f64],
            out: &mut [f64],
            k: usize,
            n: usize,
        ) {
            let full_cols = n / MICRO_COLUMNS * MICRO_COLUMNS;
            // SAFETY: All tiles cover complete four-element AVX vectors, and
            // the caller supplies ROWS complete rows of the input and output.
            unsafe {
                for column in (0..full_cols).step_by(MICRO_COLUMNS) {
                    small_tile::<ROWS, 2>(
                        a.as_ptr(),
                        b.as_ptr().add(column),
                        out.as_mut_ptr().add(column),
                        k,
                        n,
                    );
                }
                if n - full_cols >= 4 {
                    small_tile::<ROWS, 1>(
                        a.as_ptr(),
                        b.as_ptr().add(full_cols),
                        out.as_mut_ptr().add(full_cols),
                        k,
                        n,
                    );
                }
            }
            let tail_start = n / 4 * 4;
            if tail_start == n {
                return;
            }
            for (a_row, out_row) in a.chunks_exact(k).zip(out.chunks_exact_mut(n)).take(ROWS) {
                for (column, value) in out_row.iter_mut().enumerate().skip(tail_start) {
                    let mut sum = 0.0;
                    for (&coefficient, right_row) in a_row.iter().zip(b.chunks_exact(n)) {
                        sum += coefficient * right_row[column];
                    }
                    *value = sum;
                }
            }
        }

        #[inline]
        #[target_feature(enable = "avx")]
        unsafe fn small_tile<const ROWS: usize, const VECTORS: usize>(
            left: *const f64,
            right: *const f64,
            dst: *mut f64,
            k: usize,
            n: usize,
        ) {
            let mut sums = [[_mm256_setzero_pd(); VECTORS]; ROWS];
            // SAFETY: The caller provides ROWS complete rows of A/C and
            // VECTORS complete four-element columns of B/C, with A's row
            // stride k and B/C's row stride n.
            unsafe {
                for depth in 0..k {
                    let right_row = right.add(depth * n);
                    let values = std::array::from_fn(|col| _mm256_loadu_pd(right_row.add(col * 4)));
                    update(&mut sums, left, k, depth, values);
                }
                for (row, sums) in sums.iter().enumerate() {
                    for (col, sum) in sums.iter().enumerate() {
                        _mm256_storeu_pd(dst.add(row * n + col * 4), *sum);
                    }
                }
            }
        }

        #[target_feature(enable = "avx")]
        pub(super) fn compute(
            a: &[f64],
            out: &mut [f64],
            rows: usize,
            k: usize,
            n: usize,
            packed: &PackedRight,
        ) {
            for panel in &packed.panels {
                let first_depth = panel.depth == 0;
                for tile_column in (0..panel.columns).step_by(MICRO_COLUMNS) {
                    let width = MICRO_COLUMNS.min(panel.columns - tile_column);
                    let start = panel.packed_start + tile_column * panel.depth_len;
                    let right = &packed.values[start..start + width * panel.depth_len];
                    let output_column = panel.column + tile_column;
                    for row in (0..rows).step_by(MICRO_ROWS) {
                        let tile_rows = MICRO_ROWS.min(rows - row);
                        let left_start = row * k + panel.depth;
                        let out_start = row * n + output_column;
                        if width >= 4 {
                            // SAFETY: The selected const dimensions fit the row
                            // and column tails. The slices and strides account for
                            // every element of this tile and the packed panel.
                            unsafe {
                                let left = a.as_ptr().add(left_start);
                                let dst = out.as_mut_ptr().add(out_start);
                                if width == 8 {
                                    row_tile::<2>(
                                        left,
                                        right,
                                        dst,
                                        k,
                                        n,
                                        width,
                                        tile_rows,
                                        first_depth,
                                    );
                                } else {
                                    row_tile::<1>(
                                        left,
                                        right,
                                        dst,
                                        k,
                                        n,
                                        width,
                                        tile_rows,
                                        first_depth,
                                    );
                                }
                            }
                        }
                        let tail = width % 4;
                        if tail != 0 {
                            let offset = width - tail;
                            for local_row in 0..tile_rows {
                                let left_start = left_start + local_row * k;
                                let out_start = out_start + local_row * n + offset;
                                scalar_tile(
                                    &a[left_start..left_start + panel.depth_len],
                                    right,
                                    &mut out[out_start..out_start + tail],
                                    width,
                                    offset,
                                    first_depth,
                                );
                            }
                        }
                    }
                }
            }
        }

        #[target_feature(enable = "avx")]
        pub(super) fn compute_large(
            a: &[f64],
            out: &mut [f64],
            rows: usize,
            k: usize,
            n: usize,
            packed: &PackedRight,
        ) {
            for panel in &packed.panels {
                let first_depth = panel.depth == 0;
                for tile_column in (0..panel.columns).step_by(PACK_COLUMNS) {
                    let width = PACK_COLUMNS.min(panel.columns - tile_column);
                    let start = panel.packed_start + tile_column * panel.depth_len;
                    let right = &packed.values[start..start + width * panel.depth_len];
                    let output_column = panel.column + tile_column;
                    for row in (0..rows).step_by(MICRO_ROWS) {
                        let tile_rows = MICRO_ROWS.min(rows - row);
                        let left_start = row * k + panel.depth;
                        let out_start = row * n + output_column;
                        if width >= 4 {
                            // SAFETY: The selected const dimensions fit the row
                            // and column tails. The slices and strides account for
                            // every element of this tile and the packed panel.
                            unsafe {
                                let left = a.as_ptr().add(left_start);
                                let dst = out.as_mut_ptr().add(out_start);
                                if tile_rows == 4 && width == 12 {
                                    full_tile(
                                        left,
                                        right.as_ptr(),
                                        dst,
                                        k,
                                        n,
                                        panel.depth_len,
                                        first_depth,
                                    );
                                } else {
                                    match width / 4 {
                                        1 => row_tile::<1>(
                                            left,
                                            right,
                                            dst,
                                            k,
                                            n,
                                            width,
                                            tile_rows,
                                            first_depth,
                                        ),
                                        2 => row_tile::<2>(
                                            left,
                                            right,
                                            dst,
                                            k,
                                            n,
                                            width,
                                            tile_rows,
                                            first_depth,
                                        ),
                                        3 => row_tile::<3>(
                                            left,
                                            right,
                                            dst,
                                            k,
                                            n,
                                            width,
                                            tile_rows,
                                            first_depth,
                                        ),
                                        _ => unreachable!(),
                                    }
                                }
                            }
                        }
                        let tail = width % 4;
                        if tail != 0 {
                            let offset = width - tail;
                            for local_row in 0..tile_rows {
                                let left_start = left_start + local_row * k;
                                let out_start = out_start + local_row * n + offset;
                                scalar_tile(
                                    &a[left_start..left_start + panel.depth_len],
                                    right,
                                    &mut out[out_start..out_start + tail],
                                    width,
                                    offset,
                                    first_depth,
                                );
                            }
                        }
                    }
                }
            }
        }

        #[inline]
        #[target_feature(enable = "avx")]
        unsafe fn full_tile(
            left: *const f64,
            right: *const f64,
            dst: *mut f64,
            k: usize,
            n: usize,
            len: usize,
            first: bool,
        ) {
            unsafe {
                let a0 = left;
                let a1 = left.add(k);
                let a2 = left.add(2 * k);
                let a3 = left.add(3 * k);
                let mut s00 = if first {
                    _mm256_setzero_pd()
                } else {
                    _mm256_loadu_pd(dst)
                };
                let mut s01 = if first {
                    _mm256_setzero_pd()
                } else {
                    _mm256_loadu_pd(dst.add(4))
                };
                let mut s02 = if first {
                    _mm256_setzero_pd()
                } else {
                    _mm256_loadu_pd(dst.add(8))
                };
                let mut s10 = if first {
                    _mm256_setzero_pd()
                } else {
                    _mm256_loadu_pd(dst.add(n))
                };
                let mut s11 = if first {
                    _mm256_setzero_pd()
                } else {
                    _mm256_loadu_pd(dst.add(n + 4))
                };
                let mut s12 = if first {
                    _mm256_setzero_pd()
                } else {
                    _mm256_loadu_pd(dst.add(n + 8))
                };
                let mut s20 = if first {
                    _mm256_setzero_pd()
                } else {
                    _mm256_loadu_pd(dst.add(2 * n))
                };
                let mut s21 = if first {
                    _mm256_setzero_pd()
                } else {
                    _mm256_loadu_pd(dst.add(2 * n + 4))
                };
                let mut s22 = if first {
                    _mm256_setzero_pd()
                } else {
                    _mm256_loadu_pd(dst.add(2 * n + 8))
                };
                let mut s30 = if first {
                    _mm256_setzero_pd()
                } else {
                    _mm256_loadu_pd(dst.add(3 * n))
                };
                let mut s31 = if first {
                    _mm256_setzero_pd()
                } else {
                    _mm256_loadu_pd(dst.add(3 * n + 4))
                };
                let mut s32 = if first {
                    _mm256_setzero_pd()
                } else {
                    _mm256_loadu_pd(dst.add(3 * n + 8))
                };
                let mut depth: usize = 0;
                macro_rules! step {
                    ($d:expr) => {{
                        let b0 = _mm256_loadu_pd(right.add($d * 12 + 0));
                        let b1 = _mm256_loadu_pd(right.add($d * 12 + 4));
                        let b2 = _mm256_loadu_pd(right.add($d * 12 + 8));
                        let coeff = _mm256_set1_pd(*a0.add($d));
                        s00 = _mm256_add_pd(s00, _mm256_mul_pd(coeff, b0));
                        s01 = _mm256_add_pd(s01, _mm256_mul_pd(coeff, b1));
                        s02 = _mm256_add_pd(s02, _mm256_mul_pd(coeff, b2));
                        let coeff = _mm256_set1_pd(*a1.add($d));
                        s10 = _mm256_add_pd(s10, _mm256_mul_pd(coeff, b0));
                        s11 = _mm256_add_pd(s11, _mm256_mul_pd(coeff, b1));
                        s12 = _mm256_add_pd(s12, _mm256_mul_pd(coeff, b2));
                        let coeff = _mm256_set1_pd(*a2.add($d));
                        s20 = _mm256_add_pd(s20, _mm256_mul_pd(coeff, b0));
                        s21 = _mm256_add_pd(s21, _mm256_mul_pd(coeff, b1));
                        s22 = _mm256_add_pd(s22, _mm256_mul_pd(coeff, b2));
                        let coeff = _mm256_set1_pd(*a3.add($d));
                        s30 = _mm256_add_pd(s30, _mm256_mul_pd(coeff, b0));
                        s31 = _mm256_add_pd(s31, _mm256_mul_pd(coeff, b1));
                        s32 = _mm256_add_pd(s32, _mm256_mul_pd(coeff, b2));
                    }};
                }
                while depth + 4 <= len {
                    step!(depth + 0);
                    step!(depth + 1);
                    step!(depth + 2);
                    step!(depth + 3);
                    depth += 4;
                }
                while depth < len {
                    step!(depth);
                    depth += 1;
                }
                _mm256_storeu_pd(dst, s00);
                _mm256_storeu_pd(dst.add(4), s01);
                _mm256_storeu_pd(dst.add(8), s02);
                _mm256_storeu_pd(dst.add(n), s10);
                _mm256_storeu_pd(dst.add(n + 4), s11);
                _mm256_storeu_pd(dst.add(n + 8), s12);
                _mm256_storeu_pd(dst.add(2 * n), s20);
                _mm256_storeu_pd(dst.add(2 * n + 4), s21);
                _mm256_storeu_pd(dst.add(2 * n + 8), s22);
                _mm256_storeu_pd(dst.add(3 * n), s30);
                _mm256_storeu_pd(dst.add(3 * n + 4), s31);
                _mm256_storeu_pd(dst.add(3 * n + 8), s32);
            }
        }

        #[inline]
        #[target_feature(enable = "avx")]
        unsafe fn row_tile<const VECTORS: usize>(
            left: *const f64,
            right: &[f64],
            dst: *mut f64,
            k: usize,
            n: usize,
            width: usize,
            rows: usize,
            first_depth: bool,
        ) {
            // SAFETY: The caller selects complete AVX vectors and valid row
            // tails. Every arm stays inside those same rows and columns.
            unsafe {
                match rows {
                    4 => tile::<4, VECTORS>(left, right, dst, k, n, width, first_depth),
                    3 => tile::<3, VECTORS>(left, right, dst, k, n, width, first_depth),
                    2 => tile::<2, VECTORS>(left, right, dst, k, n, width, first_depth),
                    1 => tile::<1, VECTORS>(left, right, dst, k, n, width, first_depth),
                    _ => unreachable!("a tile has one to four rows"),
                }
            }
        }

        #[inline]
        #[target_feature(enable = "avx")]
        unsafe fn tile<const ROWS: usize, const VECTORS: usize>(
            left: *const f64,
            right: &[f64],
            dst: *mut f64,
            k: usize,
            n: usize,
            width: usize,
            first_depth: bool,
        ) {
            let mut sums = [[_mm256_setzero_pd(); VECTORS]; ROWS];
            // SAFETY: The caller guarantees ROWS rows with strides k/n and
            // VECTORS complete four-element columns in each source/dest row.
            unsafe {
                if !first_depth {
                    for (row, sums) in sums.iter_mut().enumerate() {
                        for (col, sum) in sums.iter_mut().enumerate() {
                            *sum = _mm256_loadu_pd(dst.add(row * n + col * 4));
                        }
                    }
                }
                for (depth, right_row) in right.chunks_exact(width).enumerate() {
                    let mut b = [_mm256_setzero_pd(); VECTORS];
                    for (col, value) in b.iter_mut().enumerate() {
                        *value = _mm256_loadu_pd(right_row.as_ptr().add(col * 4));
                    }
                    update::<ROWS, VECTORS>(&mut sums, left, k, depth, b);
                }
                for (row, sums) in sums.iter().enumerate() {
                    for (col, sum) in sums.iter().enumerate() {
                        _mm256_storeu_pd(dst.add(row * n + col * 4), *sum);
                    }
                }
            }
        }

        #[inline]
        #[target_feature(enable = "avx")]
        unsafe fn update<const ROWS: usize, const VECTORS: usize>(
            sums: &mut [[__m256d; VECTORS]; ROWS],
            left: *const f64,
            k: usize,
            depth: usize,
            b: [__m256d; VECTORS],
        ) {
            // SAFETY: The caller's tile contains this depth of each left row.
            unsafe {
                for (row, sums) in sums.iter_mut().enumerate() {
                    let coefficient = _mm256_set1_pd(*left.add(row * k + depth));
                    for (sum, value) in sums.iter_mut().zip(b) {
                        *sum = _mm256_add_pd(*sum, _mm256_mul_pd(coefficient, value));
                    }
                }
            }
        }
    }
}
