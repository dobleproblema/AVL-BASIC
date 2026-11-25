use super::*;

impl Interpreter {
    #[inline(never)]
    pub(super) fn solve_mat_arrays(
        &self,
        coefficients: ArrayValue,
        rhs: ArrayValue,
    ) -> BasicResult<ArrayValue> {
        let vector_result = rhs.dims.len() == 1;
        let a = self.owned_array_to_numeric_matrix(coefficients)?;
        let mut b = self.owned_array_to_numeric_matrix(rhs)?;
        eliminate(a, &mut b).map_err(|error| self.with_current_line(error))?;

        if vector_result {
            // The compact RHS buffer already has room for the original border.
            let base = self.mat_base as usize;
            if base == 1 {
                b.data.insert(0, 0.0);
            }
            return Ok(ArrayValue {
                dims: vec![(b.rows + base).saturating_sub(1)],
                data: ArrayData::Number(b.data),
                last_debug_write: None,
                debug_before: Default::default(),
                debug_binding_id: Cell::new(0),
            });
        }
        ArrayValue::from_numeric_matrix("", self.mat_base, b.rows, b.cols, b.data)
            .map_err(|error| self.with_current_line(error))
    }
}

fn eliminate(a: NumericMatrix, b: &mut NumericMatrix) -> BasicResult<()> {
    let threads =
        mat_elimination::enabled(a.rows).then(|| mat_elimination::solve_threads(&a.data, a.rows));
    eliminate_with_threads(a, b, threads)
}

fn eliminate_with_threads(
    mut a: NumericMatrix,
    b: &mut NumericMatrix,
    threads: Option<usize>,
) -> BasicResult<()> {
    let order = a.rows;
    let rhs_cols = b.cols;
    if a.cols != order || b.rows != order {
        return Err(BasicError::new(ErrorCode::InvalidDimensions));
    }
    if order == 0 {
        return Ok(());
    }
    for (row, coefficients) in a.data.chunks_exact_mut(order).enumerate() {
        let rhs = &mut b.data[row * rhs_cols..(row + 1) * rhs_cols];
        normalize_equation(coefficients, rhs)?;
    }

    if let Some(threads) = threads {
        mat_elimination::run(threads, |workers| {
            eliminate_forward(&mut a, b, Some(workers))
        })?;
    } else {
        eliminate_forward(&mut a, b, None)?;
    }

    if a.data.iter().any(|value| !value.is_finite()) {
        return Err(BasicError::new(ErrorCode::InvalidValue));
    }
    // Substitute backwards in place, with one solution per column of B.
    for row in (0..order).rev() {
        let (top, solved) = b.data.split_at_mut((row + 1) * rhs_cols);
        let rhs = &mut top[row * rhs_cols..];
        let a_row = &a.data[row * order..(row + 1) * order];
        for (i, &coefficient) in a_row[row + 1..].iter().enumerate() {
            let x = &solved[i * rhs_cols..(i + 1) * rhs_cols];
            for (value, &solution) in rhs.iter_mut().zip(x) {
                *value -= coefficient * solution;
            }
        }
        for value in rhs {
            *value /= a_row[row];
            if !value.is_finite() {
                return Err(BasicError::new(ErrorCode::InvalidValue));
            }
        }
    }
    Ok(())
}

fn eliminate_forward(
    a: &mut NumericMatrix,
    b: &mut NumericMatrix,
    mut workers: Option<&mut mat_elimination::Workers<'_>>,
) -> BasicResult<()> {
    let order = a.rows;
    let rhs_cols = b.cols;
    for col in 0..order {
        let mut pivot = col;
        for row in col + 1..order {
            if a.data[row * order + col].abs() > a.data[pivot * order + col].abs() {
                pivot = row;
            }
        }
        // The preceding coefficient columns have already been eliminated.
        if pivot != col {
            for j in col..order {
                a.data.swap(col * order + j, pivot * order + j);
            }
            for j in 0..rhs_cols {
                b.data.swap(col * rhs_cols + j, pivot * rhs_cols + j);
            }
        }
        let divisor = a.data[col * order + col];
        if divisor == 0.0 || !divisor.is_finite() {
            return Err(BasicError::new(ErrorCode::InvalidValue));
        }
        if let Some(workers) = workers.as_deref_mut() {
            workers.update_solve(&mut a.data, &mut b.data, order, rhs_cols, col);
            continue;
        }
        let (a_top, a_bottom) = a.data.split_at_mut((col + 1) * order);
        let pivot_row = &a_top[col * order + col..];
        let (b_top, b_bottom) = b.data.split_at_mut((col + 1) * rhs_cols);
        let pivot_rhs = &b_top[col * rhs_cols..];
        for (i, row) in a_bottom.chunks_exact_mut(order).enumerate() {
            let factor = row[col] / divisor;
            if factor == 0.0 {
                continue;
            }
            for (value, &pivot_value) in row[col + 1..].iter_mut().zip(&pivot_row[1..]) {
                *value -= factor * pivot_value;
            }
            let rhs = &mut b_bottom[i * rhs_cols..(i + 1) * rhs_cols];
            for (value, &pivot_value) in rhs.iter_mut().zip(pivot_rhs) {
                *value -= factor * pivot_value;
            }
        }
    }

    Ok(())
}

fn normalize_equation(coefficients: &mut [f64], rhs: &mut [f64]) -> BasicResult<()> {
    let mut largest_coefficient = 0.0_f64;
    for value in coefficients.iter() {
        if !value.is_finite() {
            return Err(BasicError::new(ErrorCode::InvalidValue));
        }
        largest_coefficient = largest_coefficient.max(value.abs());
    }
    let mut largest = largest_coefficient;
    for value in rhs.iter() {
        if !value.is_finite() {
            return Err(BasicError::new(ErrorCode::InvalidValue));
        }
        largest = largest.max(value.abs());
    }

    // Use binary scaling on extreme rows only. Scaling each equation can
    // avoid both overflowing an update and underflowing a pivot multiplier.
    let magnitude = if largest > 3.273390607896142e150 {
        largest
    } else if largest_coefficient > 0.0 && largest_coefficient < 3.054936363499605e-151 {
        largest_coefficient
    } else {
        return Ok(());
    };
    let exponent = (-magnitude.log2().floor() as i32).clamp(-1022, 1023);
    let scale = 2.0_f64.powi(exponent);
    if coefficients
        .iter()
        .chain(rhs.iter())
        .all(|&value| (value * scale) / scale == value)
    {
        for value in coefficients.iter_mut().chain(rhs.iter_mut()) {
            *value *= scale;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "mat_solve_tests.rs"]
mod tests;
