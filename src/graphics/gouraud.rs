pub(super) struct Rows {
    pub left: usize,
    pub right: usize,
    pub bottom: usize,
    pub top: usize,
    pub weight_dx: [f64; 3],
    pub weight_dy: [f64; 3],
    pub weights: [f64; 3],
    pub attribute_dx: [f64; 4],
    pub attribute_dy: [f64; 4],
    pub attributes: [f64; 4],
}

pub(super) struct Span {
    y: usize,
    start: usize,
    end: usize,
    values: [f64; 4],
}

impl Iterator for Rows {
    type Item = Span;

    #[inline]
    fn next(&mut self) -> Option<Span> {
        while self.bottom <= self.top {
            let y = self.bottom;
            let mut span_left = self.left as f64;
            let mut span_right = self.right as f64;
            for edge in 0..3 {
                let slope = self.weight_dx[edge];
                if slope.abs() < 1e-12 {
                    if self.weights[edge] < -1e-9 {
                        span_left = 1.0;
                        span_right = 0.0;
                        break;
                    }
                } else {
                    let boundary = self.left as f64 + (-1e-9 - self.weights[edge]) / slope;
                    if slope > 0.0 {
                        span_left = span_left.max(boundary);
                    } else {
                        span_right = span_right.min(boundary);
                    }
                }
            }
            let span_left = span_left.ceil();
            let span_right = span_right.floor();
            let span = if span_left <= span_right
                && span_left.is_finite()
                && span_right.is_finite()
                && self.weights.iter().all(|value| value.is_finite())
            {
                let start = span_left as usize;
                let end = span_right as usize;
                let skipped = (start - self.left) as f64;
                let values = std::array::from_fn(|component| {
                    self.attributes[component] + skipped * self.attribute_dx[component]
                });
                Some(Span {
                    y,
                    start,
                    end,
                    values,
                })
            } else {
                None
            };
            // Keep the original sequence of additions, including on clipped rows.
            self.bottom += 1;
            for edge in 0..3 {
                self.weights[edge] += self.weight_dy[edge];
            }
            for component in 0..4 {
                self.attributes[component] += self.attribute_dy[component];
            }
            if span.is_some() {
                return span;
            }
        }
        None
    }
}

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, OnceLock};

const BAND_ROWS: usize = 8;
const PARALLEL_PIXELS: usize = 32_768;
const VECTOR_PIXELS: usize = 512;
const MAX_WORKERS: usize = 8;

#[inline]
pub(super) fn should_vectorize(width: usize, height: usize) -> bool {
    width >= 8 && width * height >= VECTOR_PIXELS && std::arch::is_x86_feature_detected!("avx")
}

#[inline(never)]
pub(super) fn draw(
    color: &mut [u32],
    depth: &mut [f64],
    width: usize,
    height: usize,
    rows: Rows,
) -> bool {
    let bounding_rows = rows.top - rows.bottom + 1;
    let bounding_pixels = (rows.right - rows.left + 1) * bounding_rows;
    let dx = rows.attribute_dx;
    let target = Target {
        color: color.as_mut_ptr(),
        depth: depth.as_mut_ptr(),
        width,
        height,
    };
    // No allocation or worker dispatch for the usual small primitives.
    if bounding_pixels < PARALLEL_PIXELS || bounding_rows <= BAND_ROWS {
        // SAFETY: rows are clipped to the buffers, which stay exclusively borrowed.
        return unsafe { draw_spans(target, rows, dx) };
    }

    static AVAILABLE: OnceLock<usize> = OnceLock::new();
    let available = *AVAILABLE.get_or_init(|| {
        std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(MAX_WORKERS)
    });
    if available == 1 {
        // SAFETY: same clipped, exclusive buffers as above.
        return unsafe { draw_spans(target, rows, dx) };
    }

    let mut spans = Vec::with_capacity(bounding_rows);
    let mut pixels = 0;
    for span in rows {
        pixels += span.end - span.start + 1;
        spans.push(span);
    }
    let workers = if pixels >= PARALLEL_PIXELS {
        pixels
            .div_ceil(16_384)
            .min(available)
            .min(spans.len().div_ceil(BAND_ROWS))
    } else {
        1
    };
    if workers <= 1 {
        // SAFETY: all spans come from the same clipped iterator.
        return unsafe { draw_spans(target, spans, dx) };
    }

    // SAFETY: the dispatch waits for every worker before either buffer can be
    // used again. Each worker owns exclusive groups of y coordinates.
    unsafe { Pool::get(available).draw(target, spans, dx, workers) }
}

#[derive(Clone, Copy)]
struct Target {
    color: *mut u32,
    depth: *mut f64,
    width: usize,
    height: usize,
}

// Each job has an exclusive borrow of both buffers until completion. Within
// a job, disjoint scanlines do not alias in either buffer layout.
unsafe impl Send for Target {}
unsafe impl Sync for Target {}

struct Job {
    target: Target,
    spans: Vec<Span>,
    dx: [f64; 4],
    next_band: AtomicUsize,
    wrote: AtomicBool,
    active: Mutex<usize>,
    finished: Condvar,
}

impl Job {
    fn run(&self) {
        let mut wrote = false;
        loop {
            let start = self.next_band.fetch_add(BAND_ROWS, Ordering::Relaxed);
            if start >= self.spans.len() {
                break;
            }
            let band = &self.spans[start..self.spans.len().min(start + BAND_ROWS)];
            // SAFETY: each fetch claims a separate set of clipped scanlines.
            for span in band {
                wrote |= unsafe { draw_span(self.target, span, &self.dx) };
            }
        }
        if wrote {
            self.wrote.store(true, Ordering::Relaxed);
        }
    }

    fn worker_done(&self) {
        let mut active = self.active.lock().unwrap();
        *active -= 1;
        if *active == 0 {
            self.finished.notify_one();
        }
    }

    fn wait(&self) {
        let active = self.active.lock().unwrap();
        let _finished = self
            .finished
            .wait_while(active, |active| *active != 0)
            .unwrap();
    }
}

struct WorkerDone<'a>(&'a Job);
impl Drop for WorkerDone<'_> {
    fn drop(&mut self) {
        self.0.worker_done();
    }
}

struct WaitForWorkers<'a>(&'a Job);
impl Drop for WaitForWorkers<'_> {
    fn drop(&mut self) {
        self.0.wait();
    }
}

struct Pool {
    workers: Vec<mpsc::Sender<Arc<Job>>>,
}

impl Pool {
    fn get(available: usize) -> &'static Self {
        static POOL: OnceLock<Pool> = OnceLock::new();
        POOL.get_or_init(|| {
            let workers = (1..available)
                .filter_map(|index| {
                    let (tx, rx) = mpsc::channel::<Arc<Job>>();
                    std::thread::Builder::new()
                        .name(format!("avl-gouraud-{index}"))
                        .spawn(move || {
                            while let Ok(job) = rx.recv() {
                                let _done = WorkerDone(&job);
                                job.run();
                            }
                        })
                        .ok()?;
                    Some(tx)
                })
                .collect();
            Pool { workers }
        })
    }

    // SAFETY: target points to both live, exclusive buffers and each span
    // covers a disjoint y coordinate within their extents.
    unsafe fn draw(&self, target: Target, spans: Vec<Span>, dx: [f64; 4], limit: usize) -> bool {
        let workers = &self.workers[..self.workers.len().min(limit - 1)];
        let job = Arc::new(Job {
            target,
            spans,
            dx,
            next_band: AtomicUsize::new(0),
            wrote: AtomicBool::new(false),
            active: Mutex::new(workers.len()),
            finished: Condvar::new(),
        });
        let wait = WaitForWorkers(&job);
        for worker in workers {
            if worker.send(Arc::clone(&job)).is_err() {
                job.worker_done();
            }
        }
        job.run();
        drop(wait);
        job.wrote.load(Ordering::Relaxed)
    }
}

#[target_feature(enable = "avx")]
unsafe fn draw_spans(target: Target, spans: impl IntoIterator<Item = Span>, dx: [f64; 4]) -> bool {
    let mut wrote = false;
    for span in spans {
        wrote |= unsafe { draw_span(target, &span, &dx) };
    }
    wrote
}

// Keep the Linux x86-64 Gouraud kernels together in their own executable section.
#[cfg_attr(all(target_os = "linux", target_arch = "x86_64"), inline(never))]
#[cfg_attr(
    all(target_os = "linux", target_arch = "x86_64"),
    unsafe(link_section = ".avlgouraud")
)]
#[target_feature(enable = "avx")]
unsafe fn draw_span(target: Target, span: &Span, dx: &[f64; 4]) -> bool {
    use std::arch::x86_64::*;
    let Span { y, start, end, .. } = *span;

    #[target_feature(enable = "avx")]
    fn finite(values: __m256d, absolute: __m256d, max_float: __m256d) -> __m256d {
        _mm256_cmp_pd::<_CMP_LE_OQ>(_mm256_andnot_pd(absolute, values), max_float)
    }

    #[target_feature(enable = "avx")]
    fn color_channel(values: __m256d, half: __m256d, zero: __m256d, limit: __m256d) -> __m128i {
        // Clamp-before-truncate equals the legacy floor-before-clamp for finite values.
        let rounded = _mm256_add_pd(values, half);
        _mm256_cvttpd_epi32(_mm256_min_pd(_mm256_max_pd(rounded, zero), limit))
    }

    let canvas_row = (target.height - 1 - y) * target.width;
    let mut values = unsafe { _mm256_loadu_pd(span.values.as_ptr()) };
    let delta = unsafe { _mm256_loadu_pd(dx.as_ptr()) };
    let half = _mm256_set1_pd(0.5);
    let one = _mm256_set1_pd(1.0);
    let zero = _mm256_setzero_pd();
    let limit = _mm256_set1_pd(255.0);
    let absolute = _mm256_set1_pd(-0.0);
    let max_float = _mm256_set1_pd(f64::MAX);
    let tolerance_ratio = _mm256_set1_pd(1e-9);
    let tolerance_floor = _mm256_set1_pd(1e-12);
    let mut wrote = false;
    let mut x = start;
    let full_end = start + ((end + 1 - start) / 4) * 4;
    while x < full_end {
        // Generate neighboring pixels with the legacy additions, including the
        // original rounding between consecutive pixels. Transpose afterwards.
        let first = values;
        let second = _mm256_add_pd(first, delta);
        let third = _mm256_add_pd(second, delta);
        let fourth = _mm256_add_pd(third, delta);
        let qg_lo = _mm256_unpacklo_pd(first, second);
        let qg_hi = _mm256_unpacklo_pd(third, fourth);
        let q = _mm256_permute2f128_pd::<0x20>(qg_lo, qg_hi);
        values = _mm256_add_pd(fourth, delta);

        // SAFETY: x..x+4 is inside the current clipped row.
        let old = unsafe {
            let depth = target.depth.add(x * target.height + y);
            _mm256_set_pd(
                *depth.add(3 * target.height),
                *depth.add(2 * target.height),
                *depth.add(target.height),
                *depth,
            )
        };
        let q_abs = _mm256_andnot_pd(absolute, q);
        // When old is NaN, VMAXPD returns its second argument, matching
        // f64::max for a finite q. Invalid q is excluded by active_q.
        let relative = _mm256_max_pd(_mm256_andnot_pd(absolute, old), q_abs);
        let tolerance = _mm256_mul_pd(tolerance_ratio, _mm256_max_pd(relative, tolerance_floor));
        let depth_pass = _mm256_cmp_pd::<_CMP_NLE_UQ>(q, _mm256_add_pd(old, tolerance));
        let active_q = _mm256_and_pd(
            _mm256_cmp_pd::<_CMP_GT_OQ>(q, zero),
            _mm256_cmp_pd::<_CMP_LE_OQ>(q, max_float),
        );
        let depth_mask = _mm256_and_pd(depth_pass, active_q);
        if _mm256_movemask_pd(depth_mask) != 0 {
            let green = _mm256_permute2f128_pd::<0x31>(qg_lo, qg_hi);
            let rb_lo = _mm256_unpackhi_pd(first, second);
            let rb_hi = _mm256_unpackhi_pd(third, fourth);
            let red = _mm256_permute2f128_pd::<0x20>(rb_lo, rb_hi);
            let blue = _mm256_permute2f128_pd::<0x31>(rb_lo, rb_hi);
            let inverse = _mm256_div_pd(one, q);
            let r = _mm256_mul_pd(red, inverse);
            let g = _mm256_mul_pd(green, inverse);
            let b = _mm256_mul_pd(blue, inverse);
            // Nonfinite source channels cannot give finite products by a
            // finite or infinite nonnegative reciprocal, nor by zero.
            let color_finite = _mm256_and_pd(
                finite(r, absolute, max_float),
                _mm256_and_pd(
                    finite(g, absolute, max_float),
                    finite(b, absolute, max_float),
                ),
            );
            let mask = _mm256_movemask_pd(_mm256_and_pd(depth_mask, color_finite)) as u32;
            if mask != 0 {
                let red = _mm_slli_epi32::<16>(color_channel(r, half, zero, limit));
                let green = _mm_slli_epi32::<8>(color_channel(g, half, zero, limit));
                let blue = color_channel(b, half, zero, limit);
                let rgb = _mm_or_si128(red, _mm_or_si128(green, blue));
                let mut depths = [0.0; 4];
                unsafe { _mm256_storeu_pd(depths.as_mut_ptr(), q) };
                if mask == 15 {
                    unsafe {
                        _mm_storeu_si128(target.color.add(canvas_row + x).cast(), rgb);
                        for (lane, value) in depths.into_iter().enumerate() {
                            *target.depth.add((x + lane) * target.height + y) = value;
                        }
                    }
                } else {
                    let mut colors = [0u32; 4];
                    unsafe { _mm_storeu_si128(colors.as_mut_ptr().cast(), rgb) };
                    let mut pending = mask;
                    while pending != 0 {
                        let lane = pending.trailing_zeros() as usize;
                        pending &= pending - 1;
                        unsafe {
                            *target.depth.add((x + lane) * target.height + y) = depths[lane];
                            *target.color.add(canvas_row + x + lane) = colors[lane];
                        }
                    }
                }
                wrote = true;
            }
        }
        x += 4;
    }

    while x <= end {
        // Adjacent pixels retain the scalar addition order. The four lanes
        // hold q and its three weighted color channels for a single pixel.
        // SAFETY: this pixel lies inside the clipped span in both buffers.
        let depth_ptr = unsafe { target.depth.add(x * target.height + y) };
        let old_q = unsafe { *depth_ptr };
        let q = _mm_cvtsd_f64(_mm256_castpd256_pd128(values));
        let tolerance = 1e-9 * q.abs().max(old_q.abs()).max(1e-12);
        if q > 0.0 && q.is_finite() && !(q <= old_q + tolerance) {
            let rgbq = _mm256_mul_pd(values, _mm256_set1_pd(1.0 / q));
            // Only the three RGB lanes contribute to the stored color.
            const RGB_LANES: i32 = 0b1110;
            if _mm256_movemask_pd(finite(rgbq, absolute, max_float)) & RGB_LANES == RGB_LANES {
                let channels = color_channel(rgbq, half, zero, limit);
                let words = _mm_packs_epi32(channels, channels);
                let bytes = _mm_packus_epi16(words, words);
                // Packing gives [q, R, G, B] in the low bytes on x86.
                let rgb = (_mm_cvtsi128_si32(bytes) as u32).swap_bytes() & 0x00ffffff;
                unsafe {
                    *depth_ptr = q;
                    *target.color.add(canvas_row + x) = rgb;
                }
                wrote = true;
            }
        }
        values = _mm256_add_pd(values, delta);
        x += 1;
    }
    wrote
}
