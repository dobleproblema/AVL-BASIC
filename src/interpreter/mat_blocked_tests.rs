use super::*;

#[test]
fn small_mat_product_preserves_separate_multiply_and_add_at_odd_sizes() {
    let (rows, inner, columns) = (5, 7, 13);
    let mut left = vec![0.0; rows * inner];
    let mut right = vec![0.0; inner * columns];
    let delta = 2.0_f64.powi(-27);
    left[0] = -1.0;
    left[1] = 1.0 + delta;
    right[0] = 1.0;
    right[columns] = 1.0 - delta;
    left[rows * inner - 1] = f64::from_bits(1);
    right[inner * columns - 1] = -1.0;

    let mut interpreter = Interpreter::new();
    for (name, height, width, values) in [("A", rows, inner, left), ("B", inner, columns, right)] {
        interpreter.arrays.insert(
            name.to_owned(),
            ArrayValue::from_numeric_matrix(name, 0, height, width, values).unwrap(),
        );
    }
    interpreter.process_immediate("MAT C=A*B").unwrap();
    let output = interpreter.array_ref("C").unwrap();
    let ArrayData::Number(values) = &output.data else {
        unreachable!();
    };
    assert_eq!(values[0].to_bits(), 0);
    assert_eq!(
        values[rows * columns - 1].to_bits(),
        (-f64::from_bits(1)).to_bits()
    );
}

#[test]
fn dense_mat_product_preserves_ordered_f64_arithmetic_and_base() {
    // An odd rectangular product with tails in all three dimensions. The
    // inner dimension crosses two boundaries of the packed depth panels.
    let (rows, inner, columns) = (193, 259, 137);
    let mut left: Vec<f64> = (0..rows * inner)
        .map(|i| ((11 * i + 37) % 97) as f64 / 31.0 - 1.5)
        .collect();
    let mut right: Vec<f64> = (0..inner * columns)
        .map(|i| ((17 * i + 41) % 103) as f64 / 43.0 - 1.25)
        .collect();
    left[..8 * inner].fill(0.0);
    for row in right.chunks_exact_mut(columns) {
        row[..8].fill(0.0);
    }

    // FMA would keep a residual where separate multiply and add give zero.
    let delta = 2.0_f64.powi(-27);
    left[0] = -1.0;
    left[1] = 1.0 + delta;
    right[0] = 1.0;
    right[columns] = 1.0 - delta;

    // Accumulating depth panels independently would lose the final 1.
    left[inner + 127..inner + 130].copy_from_slice(&[1.0e16, -1.0e16, 1.0]);
    for depth in 127..130 {
        right[depth * columns + 1] = 1.0;
    }

    left[2 * inner + 2] = f64::INFINITY;
    left[3 * inner + 2] = f64::NEG_INFINITY;
    right[2 * columns + 3] = 1.0;
    let smallest_subnormal = f64::from_bits(1);
    left[4 * inner + 4] = smallest_subnormal;
    left[5 * inner + 4] = -smallest_subnormal;
    right[4 * columns + 4] = 1.0;
    left[6 * inner] = -0.0;
    right[5] = 1.0;
    left[7 * inner] = f64::MAX;
    right[6] = 2.0;
    right[3 * columns + 7] = f64::NAN;

    let mut expected = vec![0.0; rows * columns];
    for (r, row) in expected.chunks_exact_mut(columns).enumerate() {
        for (c, value) in row.iter_mut().enumerate() {
            for depth in 0..inner {
                *value += left[r * inner + depth] * right[depth * columns + c];
            }
        }
    }
    assert_eq!(expected[0].to_bits(), 0.0_f64.to_bits());
    assert_eq!(expected[columns + 1], 1.0);
    assert!(expected[2 * columns + 2].is_nan());
    assert_eq!(expected[2 * columns + 3], f64::INFINITY);
    assert_eq!(expected[3 * columns + 3], f64::NEG_INFINITY);
    assert_eq!(expected[4 * columns + 4], smallest_subnormal);
    assert_eq!(expected[5 * columns + 4], -smallest_subnormal);
    assert_eq!(expected[6 * columns + 5].to_bits(), 0.0_f64.to_bits());
    assert_eq!(expected[7 * columns + 6], f64::INFINITY);
    assert!(expected[7].is_nan());

    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        interpreter
            .process_immediate(&format!("MAT BASE {base}"))
            .unwrap();
        for (name, height, width, values) in [
            ("A", rows, inner, left.clone()),
            ("B", inner, columns, right.clone()),
        ] {
            let mut array =
                ArrayValue::from_numeric_matrix(name, base, height, width, values).unwrap();
            if base == 1 {
                let ArrayData::Number(data) = &mut array.data else {
                    unreachable!();
                };
                let stride = width + 1;
                data[..stride].fill(f64::NAN);
                for row in data.chunks_exact_mut(stride) {
                    row[0] = f64::NAN;
                }
            }
            interpreter.arrays.insert(name.to_string(), array);
        }

        interpreter.process_immediate("MAT A=A*B").unwrap();
        let result = interpreter.array_ref("A").unwrap();
        let lower = base as usize;
        assert_eq!(result.dims, [rows - 1 + lower, columns - 1 + lower]);
        let ArrayData::Number(actual) = &result.data else {
            unreachable!();
        };
        let stride = columns + lower;
        if base == 1 {
            assert!(actual[..stride].iter().all(|v| v.to_bits() == 0));
            assert!(actual.iter().step_by(stride).all(|v| v.to_bits() == 0));
        }
        for (r, row) in expected.chunks_exact(columns).enumerate() {
            let start = (r + lower) * stride + lower;
            for (c, (&expected, &actual)) in row.iter().zip(&actual[start..][..columns]).enumerate()
            {
                if expected.is_nan() {
                    assert!(actual.is_nan(), "MAT BASE {base}, cell {r},{c}: {actual:?}");
                } else {
                    assert_eq!(
                        actual.to_bits(),
                        expected.to_bits(),
                        "MAT BASE {base}, cell {r},{c}"
                    );
                }
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn queued_mat_product_preserves_order_across_partial_strips_and_panels() {
    if !std::arch::is_x86_feature_detected!("avx") {
        return;
    }

    // Two complete 56-row jobs and a five-row tail, with tails at the
    // packed column- and depth-panel boundaries as well.
    let (rows, inner, columns) = (117, 131, 125);
    let mut left: Vec<f64> = (0..rows * inner)
        .map(|i| ((23 * i + 17) % 109) as f64 / 37.0 - 1.25)
        .collect();
    let mut right: Vec<f64> = (0..inner * columns)
        .map(|i| ((29 * i + 11) % 127) as f64 / 71.0 - 1.0)
        .collect();
    for row in [0, 56, rows - 1] {
        left[row * inner..(row + 1) * inner].fill(0.0);
    }
    for row in right.chunks_exact_mut(columns) {
        row[0] = 0.0;
        row[1] = 0.0;
        row[columns - 1] = 0.0;
    }

    // Separate roundings make this first cell exactly +0. With FMA the
    // second term would leave a negative residual.
    let delta = 2.0_f64.powi(-27);
    left[0] = -1.0;
    left[1] = 1.0 + delta;
    right[0] = 1.0;
    right[columns] = 1.0 - delta;

    // The second job has a cancellation spanning two depth panels.
    let cancellation = 56 * inner + 127;
    left[cancellation..cancellation + 3].copy_from_slice(&[1.0e16, -1.0e16, 1.0]);
    for depth in 127..130 {
        right[depth * columns + 1] = 1.0;
    }

    let smallest_subnormal = f64::from_bits(1);
    left[rows * inner - 1] = smallest_subnormal;
    right[inner * columns - 1] = -1.0;

    let mut actual = vec![f64::NAN; rows * columns];
    assert!(mat_blocked::try_multiply_queued(
        &left,
        &right,
        &mut actual,
        rows,
        inner,
        columns,
        8,
    ));
    assert_eq!(actual[0].to_bits(), 0.0_f64.to_bits());
    assert_eq!(actual[56 * columns + 1], 1.0);
    assert_eq!(
        actual[rows * columns - 1].to_bits(),
        (-smallest_subnormal).to_bits(),
    );
    for (row, values) in actual.chunks_exact(columns).enumerate() {
        for (col, actual) in values.iter().enumerate() {
            let mut expected = 0.0;
            for depth in 0..inner {
                expected += left[row * inner + depth] * right[depth * columns + col];
            }
            assert_eq!(actual.to_bits(), expected.to_bits(), "cell {row},{col}");
        }
    }
}
