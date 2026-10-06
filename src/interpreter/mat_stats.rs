use super::{ArrayData, ArrayValue, BasicResult, ErrorCode, Interpreter, Value};

// Borrow the active MAT region without copying data or constructing BASIC
// indices. A vector is a column matrix; BASE 1 matrices retain their physical
// border, so their active rows need not be adjacent in memory.
struct NumericArrayView<'a> {
    data: &'a [f64],
    offset: usize,
    stride: usize,
    rows: usize,
    cols: usize,
    lower: i32,
}

impl NumericArrayView<'_> {
    fn is_empty(&self) -> bool {
        self.rows == 0 || self.cols == 0
    }

    fn row(&self, row: usize) -> &[f64] {
        let start = self.offset + row * self.stride;
        &self.data[start..start + self.cols]
    }

    fn fold<T>(&self, initial: T, mut operation: impl FnMut(T, f64) -> T) -> T {
        if self.is_empty() {
            return initial;
        }
        if self.stride == self.cols {
            return self.data[self.offset..self.offset + self.rows * self.cols]
                .iter()
                .copied()
                .fold(initial, operation);
        }
        let mut result = initial;
        for row in 0..self.rows {
            result = self.row(row).iter().copied().fold(result, &mut operation);
        }
        result
    }

    fn extremum(&self, absolute: bool, minimum: bool) -> (f64, Option<(i32, i32)>) {
        if self.is_empty() {
            return (0.0, None);
        }
        let first = self.data[self.offset];
        let first = if absolute { first.abs() } else { first };
        let (value, position, _) = self.fold((first, 0, 0), |(best, position, index), value| {
            let value = if absolute { value.abs() } else { value };
            // Strict comparisons retain the first tie. They also preserve the
            // existing behavior: a first NaN wins, later NaNs are ignored.
            if if minimum { value < best } else { value > best } {
                (value, index, index + 1)
            } else {
                (best, position, index + 1)
            }
        });
        (
            value,
            Some((
                self.lower + (position / self.cols) as i32,
                self.lower + (position % self.cols) as i32,
            )),
        )
    }

    fn row_norm(&self) -> (f64, i32) {
        if self.is_empty() {
            return (0.0, 0);
        }
        let mut best = 0.0;
        let mut position = 0;
        for row in 0..self.rows {
            let sum = self
                .row(row)
                .iter()
                .fold(0.0, |sum, value| sum + value.abs());
            if row == 0 || sum > best {
                best = sum;
                position = row;
            }
        }
        (best, self.lower + position as i32)
    }

    fn column_norm(&self) -> (f64, i32) {
        if self.is_empty() {
            return (0.0, 0);
        }
        if self.cols == 1 {
            return (self.fold(0.0, |sum, value| sum + value.abs()), self.lower);
        }
        let mut sums = vec![0.0; self.cols];
        for row in 0..self.rows {
            for (sum, value) in sums.iter_mut().zip(self.row(row)) {
                *sum += value.abs();
            }
        }
        let mut best = sums[0];
        let mut position = 0;
        for (column, &sum) in sums.iter().enumerate().skip(1) {
            if sum > best {
                best = sum;
                position = column;
            }
        }
        (best, self.lower + position as i32)
    }

    fn frobenius_norm(&self) -> f64 {
        // Keep the original reduction and rounding order for ordinary values.
        // A 64-bit Vec holds fewer than 2^60 f64 elements. Even their combined
        // subnormal square/accumulation rounding is below 2^-1014, far below
        // the 2^-952 ulp at the bottom of this conservative ordinary band.
        // Its upper end also stays far from overflow. Inspecting the aggregate
        // avoids adding checks to every element of the common reduction.
        const ORDINARY_MIN: f64 = f64::from_bits(((1023 - 900) as u64) << 52);
        const ORDINARY_MAX: f64 = f64::from_bits(((1023 + 900) as u64) << 52);
        let squares = self.fold(0.0, |sum, value| sum + value * value);
        if (ORDINARY_MIN..=ORDINARY_MAX).contains(&squares) || squares.is_nan() {
            return squares.sqrt();
        }

        // A scaled sum of squares keeps finite norms representable even when
        // the unscaled squares overflow or underflow. Non-finite inputs retain
        // their previous statistical result (NaN above, infinity below).
        let (scale, squares) = self.fold((0.0, 1.0), |(scale, squares), value| {
            let value = value.abs();
            if value == 0.0 || scale == f64::INFINITY {
                (scale, squares)
            } else if value == f64::INFINITY {
                (f64::INFINITY, 1.0)
            } else if value > scale {
                let ratio = scale / value;
                (value, 1.0 + squares * ratio * ratio)
            } else {
                let ratio = value / scale;
                (scale, squares + ratio * ratio)
            }
        });
        scale * squares.sqrt()
    }
}

impl Interpreter {
    fn numeric_array_view<'a>(&self, array: &'a ArrayValue) -> BasicResult<NumericArrayView<'a>> {
        let ArrayData::Number(data) = &array.data else {
            return Err(self.err(ErrorCode::TypeMismatch));
        };
        let lower = self.mat_base.max(0) as usize;
        let count = |bound: usize| (bound + 1).saturating_sub(lower);
        let (rows, cols, stride, offset) = match array.dims.as_slice() {
            [n] => (count(*n), 1, 1, lower),
            [r, c] => (count(*r), count(*c), c + 1, lower * (c + 1) + lower),
            _ => return Err(self.err(ErrorCode::InvalidDimensions)),
        };
        Ok(NumericArrayView {
            data,
            offset,
            stride,
            rows,
            cols,
            lower: lower as i32,
        })
    }

    pub(super) fn call_mat_stat_function(
        &mut self,
        name: &str,
        args: Vec<Value>,
    ) -> BasicResult<Value> {
        let upper = name.to_ascii_uppercase();
        if upper == "DOT" {
            if args.len() != 2 {
                return Err(self.err(ErrorCode::ArgumentMismatch));
            }
            let left_name = args[0].clone().into_string()?.to_ascii_uppercase();
            let right_name = args[1].clone().into_string()?.to_ascii_uppercase();
            let left = self
                .array_ref(&left_name)
                .ok_or_else(|| self.err(ErrorCode::Undefined))?;
            let right = self
                .array_ref(&right_name)
                .ok_or_else(|| self.err(ErrorCode::Undefined))?;
            let left = self.numeric_array_view(left)?;
            if left.cols != 1 {
                return Err(self.err(ErrorCode::InvalidDimensions));
            }
            let right = self.numeric_array_view(right)?;
            if right.cols != 1 || left.rows != right.rows {
                return Err(self.err(ErrorCode::InvalidDimensions));
            }
            // Keep the original iterator sum, including its empty/signed-zero
            // identity and the left-to-right order of products.
            let total = (0..left.rows)
                .map(|row| {
                    left.data[left.offset + row * left.stride]
                        * right.data[right.offset + row * right.stride]
                })
                .sum::<f64>();
            return Ok(Value::number(total));
        }
        if args.len() != 1 {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let array_name = args[0].clone().into_string()?.to_ascii_uppercase();
        let array = self
            .array_ref(&array_name)
            .ok_or_else(|| self.err(ErrorCode::Undefined))?;
        let view = self.numeric_array_view(array)?;
        let value = match upper.as_str() {
            "SUM" => view.fold(0.0, |sum, value| sum + value),
            "ABSUM" => view.fold(0.0, |sum, value| sum + value.abs()),
            "FNORM" => view.frobenius_norm(),
            "AMAX" | "AMIN" | "MAXAB" => {
                let (value, position) = view.extremum(upper == "MAXAB", upper == "AMIN");
                self.set_mat_stat_context(&upper, position);
                value
            }
            "RNORM" => {
                let (value, row) = view.row_norm();
                self.numeric_variables
                    .insert("RNORMROW".to_string(), row as f64);
                value
            }
            "CNORM" => {
                let (value, column) = view.column_norm();
                self.numeric_variables
                    .insert("CNORMCOL".to_string(), column as f64);
                value
            }
            _ => return Err(self.err(ErrorCode::Undefined)),
        };
        Ok(Value::number(value))
    }

    fn set_mat_stat_context(&mut self, prefix: &str, position: Option<(i32, i32)>) {
        let (row, col) = position.unwrap_or((0, 0));
        self.numeric_variables
            .insert(format!("{prefix}ROW"), row as f64);
        self.numeric_variables
            .insert(format!("{prefix}COL"), col as f64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(data: &[f64], rows: usize, cols: usize) -> NumericArrayView<'_> {
        NumericArrayView {
            data,
            offset: 0,
            stride: cols,
            rows,
            cols,
            lower: 0,
        }
    }

    #[test]
    fn extrema_keep_first_nan_signed_zero_and_later_nan_behavior() {
        for absolute in [false, true] {
            for minimum in [false, true] {
                let values = [f64::NAN, 2.0, -3.0];
                let (value, position) = view(&values, 3, 1).extremum(absolute, minimum);
                assert!(value.is_nan());
                assert_eq!(position, Some((0, 0)));
            }
        }
        let values = [1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY];
        let matrix = view(&values, 2, 2);
        assert_eq!(matrix.extremum(false, false), (f64::INFINITY, Some((1, 0))));
        assert_eq!(
            matrix.extremum(false, true),
            (f64::NEG_INFINITY, Some((1, 1)))
        );
        assert_eq!(matrix.extremum(true, false), (f64::INFINITY, Some((1, 0))));
        let values = [-0.0, 0.0];
        let matrix = view(&values, 2, 1);
        assert_eq!(
            matrix.extremum(false, false).0.to_bits(),
            (-0.0_f64).to_bits()
        );
        assert_eq!(
            matrix.extremum(false, true).0.to_bits(),
            (-0.0_f64).to_bits()
        );
        assert_eq!(matrix.extremum(true, false).0.to_bits(), 0.0_f64.to_bits());
    }

    #[test]
    fn norms_keep_nan_comparison_and_zero_accumulation_behavior() {
        let first_nan = [f64::NAN, 2.0, 3.0, 4.0];
        let matrix = view(&first_nan, 2, 2);
        assert!(matrix.row_norm().0.is_nan());
        assert_eq!(matrix.row_norm().1, 0);
        assert!(matrix.column_norm().0.is_nan());
        assert_eq!(matrix.column_norm().1, 0);
        let later_nan = [1.0, 2.0, 3.0, f64::NAN];
        let matrix = view(&later_nan, 2, 2);
        assert_eq!(matrix.row_norm(), (3.0, 0));
        assert_eq!(matrix.column_norm(), (4.0, 0));
        let zeroes = [-0.0, -0.0];
        let matrix = view(&zeroes, 2, 1);
        assert_eq!(
            matrix.fold(0.0, |sum, value| sum + value).to_bits(),
            0.0_f64.to_bits()
        );
        assert_eq!(matrix.row_norm().0.to_bits(), 0.0_f64.to_bits());
        assert_eq!(matrix.column_norm().0.to_bits(), 0.0_f64.to_bits());
    }

    #[test]
    fn frobenius_keeps_common_rounding_and_nonfinite_statistical_results() {
        let values = [1e8, 1.0, -1.0, -0.0];
        let expected = values
            .iter()
            .fold(0.0_f64, |sum, value| sum + value * value)
            .sqrt();
        assert_eq!(
            view(&values, 2, 2).frobenius_norm().to_bits(),
            expected.to_bits()
        );
        for values in [[f64::NAN, f64::INFINITY], [f64::INFINITY, f64::NAN]] {
            assert!(view(&values, 2, 1).frobenius_norm().is_nan());
        }
        assert_eq!(
            view(&[f64::INFINITY, 1.0], 2, 1).frobenius_norm(),
            f64::INFINITY
        );
        let smallest = f64::from_bits(1);
        assert_eq!(view(&[smallest], 1, 1).frobenius_norm(), smallest);
        assert_eq!(
            view(&[-0.0], 1, 1).frobenius_norm().to_bits(),
            0.0_f64.to_bits()
        );
    }

    #[test]
    fn frobenius_aggregate_band_edges_and_zero_regions() {
        // Squaring these powers gives the inclusive 2^-900 / 2^900 limits.
        // Adjacent values exercise the ordinary and scaled sides of each edge.
        for exponent in [-450, 450] {
            let center = f64::from_bits(((1023 + exponent) as u64) << 52);
            for bits in [center.to_bits() - 1, center.to_bits(), center.to_bits() + 1] {
                let value = f64::from_bits(bits);
                assert_eq!(view(&[value], 1, 1).frobenius_norm().to_bits(), bits);
            }
        }
        let zeroes = [-0.0, 0.0, -0.0, 0.0];
        assert_eq!(
            view(&zeroes, 2, 2).frobenius_norm().to_bits(),
            0.0_f64.to_bits()
        );
        assert_eq!(
            view(&[], 0, 3).frobenius_norm().to_bits(),
            0.0_f64.to_bits()
        );
        assert_eq!(
            view(&[], 3, 0).frobenius_norm().to_bits(),
            0.0_f64.to_bits()
        );
    }

    #[test]
    fn inverse_extreme_diagonals_keep_representable_results() {
        for base in [0, 1] {
            let mut interpreter = Interpreter::new();
            interpreter
                .process_immediate(&format!("MAT BASE {base}"))
                .unwrap();
            for values in [
                [1e200, 1e200, 1e-200, 1e-200],
                [1e-200, 1e-200, 1e200, 1e200],
                [1e-160, 1e-160, 1e160, 1e160],
            ] {
                let mut data = vec![0.0; 16];
                for (index, value) in values.into_iter().enumerate() {
                    data[index * 4 + index] = value;
                }
                let inverse = interpreter
                    .invert_numeric_matrix(super::super::NumericMatrix {
                        rows: 4,
                        cols: 4,
                        data,
                    })
                    .unwrap();
                for (index, value) in values.into_iter().enumerate() {
                    let actual = inverse
                        .get(&[base + index as i32, base + index as i32])
                        .unwrap()
                        .as_number()
                        .unwrap();
                    assert!(actual.is_finite());
                    assert!((actual / (1.0 / value) - 1.0).abs() < 1e-14);
                }
            }
            let inverse = interpreter
                .invert_numeric_matrix(super::super::NumericMatrix {
                    rows: 1,
                    cols: 1,
                    data: vec![1e200],
                })
                .unwrap();
            assert_eq!(
                inverse.get(&[base, base]).unwrap().as_number().unwrap(),
                1.0 / 1e200
            );
        }
    }

    #[test]
    fn scaled_product_restores_subnormal_range_and_signed_underflow() {
        let smallest = f64::from_bits(1);
        let mut product = (1.0, 0);
        Interpreter::multiply_scaled_numeric_product(&mut product, smallest);
        assert_eq!(Interpreter::scaled_numeric_product_value(product), smallest);
        assert_eq!(Interpreter::scaled_numeric_product_value((1.0, -1075)), 0.0);
        assert_eq!(
            Interpreter::scaled_numeric_product_value((1.5, -1075)),
            smallest
        );
        assert_eq!(
            Interpreter::scaled_numeric_product_value((-1.0, -1076)).to_bits(),
            (-0.0_f64).to_bits()
        );
        assert_eq!(
            Interpreter::scaled_numeric_product_value((-1.0, 1024)),
            f64::NEG_INFINITY
        );
    }

    #[test]
    fn inverse_aggregate_magnitude_check_rejects_all_nonfinite_bit_patterns() {
        let interpreter = Interpreter::new();
        for bits in [
            f64::INFINITY.to_bits(),
            f64::NEG_INFINITY.to_bits(),
            0x7ff8000000000042, // quiet NaN with a payload
            0xfff8000000000042,
            0x7ff0000000000001, // signaling NaN with a payload
            0xfff0000000000001,
        ] {
            for position in 0..4 {
                let mut data = vec![-2.0, -0.0, 0.0, -3.0];
                data[position] = f64::from_bits(bits);
                let error = interpreter
                    .invert_numeric_matrix(super::super::NumericMatrix {
                        rows: 2,
                        cols: 2,
                        data,
                    })
                    .unwrap_err();
                assert_eq!(
                    error.code,
                    ErrorCode::InvalidValue,
                    "{bits:x}, position {position}"
                );
            }
        }
        for value in [-2.0, -1e200] {
            let inverse = interpreter
                .invert_numeric_matrix(super::super::NumericMatrix {
                    rows: 1,
                    cols: 1,
                    data: vec![value],
                })
                .unwrap();
            assert_eq!(
                inverse.get(&[0, 0]).unwrap().as_number().unwrap(),
                1.0 / value
            );
        }
    }
}
