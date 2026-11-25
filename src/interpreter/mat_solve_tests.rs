use super::{eliminate_with_threads, ErrorCode, NumericMatrix};
use crate::interpreter::mat_elimination;

fn pivoting_system(order: usize, width: usize) -> (NumericMatrix, NumericMatrix, Vec<f64>) {
    // Rotate the first row of a diagonally dominant matrix to the end. The
    // resulting system stays well conditioned while requiring row swaps.
    let coefficients: Vec<f64> = (0..order * order)
        .map(|index| {
            let (row, col) = (index / order, index % order);
            if col == (row + 1) % order {
                4.0
            } else {
                (((row * 13 + col * 11) % 37) as f64 - 18.0) / 16_384.0
            }
        })
        .collect();
    let mut solution = Vec::with_capacity(order * width);
    for row in 0..order {
        for col in 0..width {
            solution.push(if width > 1 && col == width - 1 {
                0.0
            } else {
                (((row * 17 + col * 3) % 29) as f64 - 14.0) / 32.0
            });
        }
    }
    let mut rhs = Vec::with_capacity(order * width);
    for coefficients in coefficients.chunks_exact(order) {
        for col in 0..width {
            let mut value = 0.0;
            for (index, coefficient) in coefficients.iter().enumerate() {
                value += coefficient * solution[index * width + col];
            }
            rhs.push(if value == 0.0 { -0.0 } else { value });
        }
    }
    (
        NumericMatrix {
            rows: order,
            cols: order,
            data: coefficients,
        },
        NumericMatrix {
            rows: order,
            cols: width,
            data: rhs,
        },
        solution,
    )
}

fn assert_scalar_and_avx_agree(
    coefficients: NumericMatrix,
    rhs: NumericMatrix,
    expected: &[f64],
    thread_counts: &[usize],
) {
    let mut scalar = rhs.clone();
    eliminate_with_threads(coefficients.clone(), &mut scalar, None).unwrap();
    assert_eq!(scalar.data.len(), expected.len());
    for (index, (&actual, &expected)) in scalar.data.iter().zip(expected).enumerate() {
        assert!(
            (actual - expected).abs() <= 2e-13,
            "scalar result at cell {index}: expected {expected}, got {actual}"
        );
    }
    for &threads in thread_counts {
        let mut avx = rhs.clone();
        eliminate_with_threads(coefficients.clone(), &mut avx, Some(threads)).unwrap();
        assert_eq!(avx.rows, rhs.rows);
        assert_eq!(avx.cols, rhs.cols);
        assert_eq!(avx.data.len(), scalar.data.len());
        for (index, (&actual, &expected)) in avx.data.iter().zip(&scalar.data).enumerate() {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "order {}, RHS width {}, {threads} worker(s), cell {index}",
                rhs.rows,
                rhs.cols
            );
        }
    }
}

#[test]
fn avx_solve_matches_scalar_for_rhs_widths_and_tails() {
    let order = 65;
    if !mat_elimination::enabled(order) {
        return;
    }
    for width in [0, 1, 3, 4, 5, 8, 9, 17] {
        let (mut coefficients, mut rhs, expected) = pivoting_system(order, width);
        // Exact dyadic rescaling forces both normalization directions.
        for (row, factor) in [(2, 2.0_f64.powi(800)), (3, 2.0_f64.powi(-800))] {
            for value in &mut coefficients.data[row * order..(row + 1) * order] {
                *value *= factor;
            }
            for value in &mut rhs.data[row * width..(row + 1) * width] {
                *value *= factor;
            }
        }
        assert_scalar_and_avx_agree(coefficients, rhs, &expected, &[1]);
    }
}

#[test]
fn parallel_solve_matches_scalar_and_one_worker_with_large_pivot_swaps() {
    let order = 529;
    if !mat_elimination::enabled(order) {
        return;
    }
    let (coefficients, rhs, expected) = pivoting_system(order, 9);
    assert_scalar_and_avx_agree(coefficients, rhs, &expected, &[1, 4]);
}

#[test]
fn solve_checks_for_singularity_with_zero_rhs_columns() {
    let order = 65;
    let (mut coefficients, mut rhs, _) = pivoting_system(order, 0);
    coefficients.data.copy_within(0..order, (order - 1) * order);
    let scalar = eliminate_with_threads(coefficients.clone(), &mut rhs, None).unwrap_err();
    assert_eq!(scalar.code, ErrorCode::InvalidValue);
    if mat_elimination::enabled(order) {
        let avx = eliminate_with_threads(coefficients, &mut rhs, Some(1)).unwrap_err();
        assert_eq!(avx.code, scalar.code);
    }
}
