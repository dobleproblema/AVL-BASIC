use super::{ArrayData, ArrayValue, BasicError, BasicResult, ErrorCode};
use std::cell::Cell;
use std::mem::MaybeUninit;
use std::sync::{Mutex, OnceLock};

const TILE: usize = 32;

pub(super) fn try_transpose(input: &ArrayValue, base: i32) -> Option<BasicResult<ArrayValue>> {
    let ArrayData::Number(values) = &input.data else {
        return None;
    };
    let &[row_bound, col_bound] = input.dims.as_slice() else {
        return None;
    };
    let border = usize::from(base == 1);
    let rows = row_bound + 1 - border;
    let cols = col_bound + 1 - border;
    if rows < TILE || cols < TILE {
        return None;
    }
    Some(transpose(values, rows, cols, border))
}

fn transpose(input: &[f64], rows: usize, cols: usize, border: usize) -> BasicResult<ArrayValue> {
    let dims = vec![cols + border - 1, rows + border - 1];
    let len = ArrayValue::checked_data_len(&dims)?;
    let layout = Layout { rows, cols, border };
    let mut output = Vec::<f64>::new();
    output
        .try_reserve_exact(len)
        .map_err(|_| BasicError::new(ErrorCode::Overflow))?;
    let (top, body) =
        output.spare_capacity_mut()[..len].split_at_mut(border * layout.output_stride());
    for value in top {
        value.write(0.0);
    }

    #[cfg(target_arch = "x86_64")]
    let avx = std::arch::is_x86_feature_detected!("avx");
    #[cfg(not(target_arch = "x86_64"))]
    let avx = false;
    let workers = worker_count(rows * cols, cols);
    if workers == 1 {
        write_rows(input, body, layout, 0, avx);
    } else {
        let block = cols.div_ceil(TILE).div_ceil(workers) * TILE;
        // Retain each exclusive output range until a worker takes it. If
        // thread creation fails, the calling thread can still write the range.
        let chunks: Vec<_> = body
            .chunks_mut(block * layout.output_stride())
            .map(|chunk| Mutex::new(Some(chunk)))
            .collect();
        let write = |index: usize, chunk: &Mutex<Option<&mut [MaybeUninit<f64>]>>| {
            let output = chunk.lock().unwrap().take().expect("one writer per range");
            write_rows(input, output, layout, index * block, avx);
        };
        std::thread::scope(|scope| {
            for (index, chunk) in chunks.iter().enumerate().skip(1) {
                let write = &write;
                if std::thread::Builder::new()
                    .spawn_scoped(scope, move || write(index, chunk))
                    .is_err()
                {
                    write(index, chunk);
                }
            }
            write(0, &chunks[0]);
        });
    }
    // SAFETY: The top border and all body rows, including the left border,
    // have been written. All workers have joined.
    unsafe { output.set_len(len) };
    Ok(ArrayValue {
        dims,
        data: ArrayData::Number(output),
        last_debug_write: None,
        debug_writes: None,
        debug_binding_id: Cell::new(0),
    })
}

fn worker_count(cells: usize, cols: usize) -> usize {
    let target = match cells {
        0..1_048_576 => return 1,
        1_048_576..4_194_304 => 4,
        _ => 8,
    };
    static AVAILABLE: OnceLock<usize> = OnceLock::new();
    target
        .min(cols.div_ceil(TILE))
        .min(*AVAILABLE.get_or_init(|| std::thread::available_parallelism().map_or(1, usize::from)))
}

#[derive(Clone, Copy)]
struct Layout {
    rows: usize,
    cols: usize,
    border: usize,
}

impl Layout {
    fn output_stride(self) -> usize {
        self.rows + self.border
    }

    fn source_index(self, row: usize, col: usize) -> usize {
        (row + self.border) * (self.cols + self.border) + col + self.border
    }
}

fn write_rows(
    input: &[f64],
    output: &mut [MaybeUninit<f64>],
    layout: Layout,
    start: usize,
    avx: bool,
) {
    let stride = layout.output_stride();
    let columns = output.len() / stride;
    if layout.border != 0 {
        for row in output.chunks_exact_mut(stride) {
            row[0].write(0.0);
        }
    }
    for col in (0..columns).step_by(TILE) {
        let col_end = (col + TILE).min(columns);
        for row in (0..layout.rows).step_by(TILE) {
            let row_end = (row + TILE).min(layout.rows);
            #[cfg(target_arch = "x86_64")]
            if avx {
                // SAFETY: The CPU supports AVX and the tile is clipped to
                // the input shape and to this worker's output rows.
                unsafe { avx_tile(input, output, layout, start, row..row_end, col..col_end) };
                continue;
            }
            #[cfg(not(target_arch = "x86_64"))]
            let _ = avx;
            scalar_tile(input, output, layout, start, row..row_end, col..col_end);
        }
    }
}

fn scalar_tile(
    input: &[f64],
    output: &mut [MaybeUninit<f64>],
    layout: Layout,
    start: usize,
    rows: std::ops::Range<usize>,
    columns: std::ops::Range<usize>,
) {
    for row in rows {
        for col in columns.clone() {
            output[col * layout.output_stride() + row + layout.border]
                .write(input[layout.source_index(row, start + col)]);
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
fn avx_tile(
    input: &[f64],
    output: &mut [MaybeUninit<f64>],
    layout: Layout,
    start: usize,
    rows: std::ops::Range<usize>,
    columns: std::ops::Range<usize>,
) {
    use std::arch::x86_64::{
        _mm256_loadu_pd, _mm256_permute2f128_pd, _mm256_storeu_pd, _mm256_unpackhi_pd,
        _mm256_unpacklo_pd,
    };
    let rows4 = rows.end - rows.len() % 4;
    let cols4 = columns.end - columns.len() % 4;
    let source_stride = layout.cols + layout.border;
    let output_stride = layout.output_stride();
    for row in (rows.start..rows4).step_by(4) {
        for col in (columns.start..cols4).step_by(4) {
            // SAFETY: only complete 4 × 4 blocks enter this loop. Both
            // strides include the inactive border if BASE 1 is in use.
            unsafe {
                let source = input.as_ptr().add(layout.source_index(row, start + col));
                let a = _mm256_loadu_pd(source);
                let b = _mm256_loadu_pd(source.add(source_stride));
                let c = _mm256_loadu_pd(source.add(source_stride * 2));
                let d = _mm256_loadu_pd(source.add(source_stride * 3));
                let ab_even = _mm256_unpacklo_pd(a, b);
                let ab_odd = _mm256_unpackhi_pd(a, b);
                let cd_even = _mm256_unpacklo_pd(c, d);
                let cd_odd = _mm256_unpackhi_pd(c, d);
                let values = [
                    _mm256_permute2f128_pd::<0x20>(ab_even, cd_even),
                    _mm256_permute2f128_pd::<0x20>(ab_odd, cd_odd),
                    _mm256_permute2f128_pd::<0x31>(ab_even, cd_even),
                    _mm256_permute2f128_pd::<0x31>(ab_odd, cd_odd),
                ];
                let target = output
                    .as_mut_ptr()
                    .cast::<f64>()
                    .add(col * output_stride + row + layout.border);
                for (index, value) in values.into_iter().enumerate() {
                    _mm256_storeu_pd(target.add(index * output_stride), value);
                }
            }
        }
    }
    scalar_tile(
        input,
        output,
        layout,
        start,
        rows.start..rows4,
        cols4..columns.end,
    );
    scalar_tile(input, output, layout, start, rows4..rows.end, columns);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strided_transpose_preserves_bits_and_clears_the_inactive_border() {
        for (rows, cols, border) in [(37, 65, 0), (65, 37, 1), (1025, 1027, 1)] {
            let values: Vec<f64> = (0..(rows + border) * (cols + border))
                .map(|index| match index % 127 {
                    0 => f64::from_bits(0x7ff0_0000_0000_1234),
                    1 => -0.0,
                    _ => f64::from_bits(index as u64),
                })
                .collect();
            let transposed = transpose(&values, rows, cols, border).unwrap();
            assert_eq!(transposed.dims, [cols + border - 1, rows + border - 1]);
            let ArrayData::Number(output) = transposed.data else {
                unreachable!();
            };
            for r in 0..cols + border {
                for c in 0..rows + border {
                    let expected = if r < border || c < border {
                        0.0
                    } else {
                        values[c * (cols + border) + r]
                    };
                    assert_eq!(
                        output[r * (rows + border) + c].to_bits(),
                        expected.to_bits()
                    );
                }
            }
        }
    }
}
