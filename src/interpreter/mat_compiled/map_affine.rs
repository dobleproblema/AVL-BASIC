use super::{
    ArrayData, BasicResult, CompiledMatExpr, FastNumberFunction, Interpreter, MatExprValue,
};

#[derive(Debug, Clone, Copy)]
enum Input {
    Direct,
    Scale(f64),
    Affine(f64, f64),
}

#[derive(Debug, Clone)]
enum Operand {
    Literal(f64),
    Name(String),
}

impl Operand {
    fn compile(expression: &CompiledMatExpr) -> Option<Self> {
        match expression {
            CompiledMatExpr::Name { name, .. } => Some(Self::Name(name.clone())),
            _ => Self::literal(expression).map(Self::Literal),
        }
    }

    fn literal(expression: &CompiledMatExpr) -> Option<f64> {
        match expression {
            CompiledMatExpr::Number(value) => Some(*value),
            CompiledMatExpr::Positive(inner) => Self::literal(inner),
            CompiledMatExpr::Negate(inner) => Self::literal(inner).map(|value| -value),
            _ => None,
        }
    }

    fn resolve<'a>(&'a self, interpreter: &mut Interpreter) -> Option<ResolvedOperand<'a>> {
        match self {
            Self::Literal(value) => Some(ResolvedOperand::Scalar(*value)),
            Self::Name(name) if interpreter.array_ref(name).is_some() => {
                Some(ResolvedOperand::Array(name))
            }
            Self::Name(name) if crate::expr::is_pi_constant_name(name) => {
                Some(ResolvedOperand::Scalar(std::f64::consts::PI))
            }
            Self::Name(name) if !crate::expr::is_zero_arg_function(name) => {
                super::EvalContext::get_number_variable(interpreter, name)
                    .ok()
                    .map(ResolvedOperand::Scalar)
            }
            Self::Name(_) => None,
        }
    }
}

enum ResolvedOperand<'a> {
    Scalar(f64),
    Array(&'a str),
}

#[derive(Debug, Clone)]
enum MapInput {
    Direct(String),
    Scale(Operand, Operand),
    Affine(Operand, Operand, Operand),
}

impl MapInput {
    fn compile(expression: &CompiledMatExpr) -> Option<Self> {
        use CompiledMatExpr::{Binary, Name};
        match expression {
            Name { name, .. } => Some(Self::Direct(name.clone())),
            Binary { op: '*', .. } => {
                let (left, right) = Self::scale(expression)?;
                Some(Self::Scale(left, right))
            }
            Binary {
                left,
                right,
                op: '+',
            } => {
                let (product, offset) = if matches!(left.as_ref(), Binary { op: '*', .. }) {
                    (left, right)
                } else {
                    (right, left)
                };
                let (left, right) = Self::scale(product)?;
                Some(Self::Affine(left, right, Operand::compile(offset)?))
            }
            _ => None,
        }
    }

    fn scale(expression: &CompiledMatExpr) -> Option<(Operand, Operand)> {
        let CompiledMatExpr::Binary {
            left,
            right,
            op: '*',
        } = expression
        else {
            return None;
        };
        let left = Operand::compile(left)?;
        let right = Operand::compile(right)?;
        if matches!((&left, &right), (Operand::Literal(_), Operand::Literal(_))) {
            return None;
        }
        Some((left, right))
    }

    fn resolve(&self, interpreter: &mut Interpreter) -> Option<(&str, Input)> {
        use ResolvedOperand::{Array, Scalar};
        match self {
            Self::Direct(name) => Some((name, Input::Direct)),
            Self::Scale(left, right) | Self::Affine(left, right, _) => {
                // A name can be an array in one invocation and a scalar in the
                // next. Only one array and ordinary scalar values can be fused.
                let (name, factor) = match (left.resolve(interpreter)?, right.resolve(interpreter)?)
                {
                    (Array(name), Scalar(factor)) | (Scalar(factor), Array(name)) => (name, factor),
                    _ => return None,
                };
                if let Self::Affine(_, _, offset) = self {
                    let Scalar(offset) = offset.resolve(interpreter)? else {
                        return None;
                    };
                    Some((name, Input::Affine(factor, offset)))
                } else {
                    Some((name, Input::Scale(factor)))
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct NamedMap {
    input: MapInput,
    function: FastNumberFunction,
    fallback: CompiledMatExpr,
}

impl NamedMap {
    pub(super) fn compile(expression: &CompiledMatExpr) -> Option<Self> {
        let CompiledMatExpr::Map { function, array } = expression else {
            return None;
        };
        let input = MapInput::compile(array)?;
        // Direct fallible maps already transform one temporary in one pass.
        // The fused route only saves work when there is arithmetic to combine.
        if matches!(input, MapInput::Direct(_)) && !Self::can_reuse_destination(*function) {
            return None;
        }
        Some(Self {
            input,
            function: *function,
            fallback: expression.clone(),
        })
    }

    fn can_reuse_destination(function: FastNumberFunction) -> bool {
        use FastNumberFunction::*;
        // Only functions with no domain or overflow errors may write into the
        // real destination before the entire operation is known to succeed.
        matches!(
            function,
            Abs | Int | Sin | Cos | Fix | Sgn | Frac | Tan | Atn | Rtd | Dtr | Round
        )
    }
}

#[derive(Clone, Copy)]
struct MapKernel {
    input: Input,
    function: FastNumberFunction,
}

impl MapKernel {
    fn transform(
        &self,
        values: &mut [f64],
        dims: &[usize],
        base: i32,
        degrees: bool,
    ) -> BasicResult<()> {
        if matches!(self.function, FastNumberFunction::Int) {
            let active_len = if base == 1 {
                dims[dims.len() - 1]
            } else {
                values.len()
            };
            let kernel = active_kernel(active_len, self.input);
            let (factor, offset) = match self.input {
                Input::Direct => (1.0, 0.0),
                Input::Scale(factor) => (factor, 0.0),
                Input::Affine(factor, offset) => (factor, offset),
            };
            return self.transform_regions(values, dims, base, |cells| {
                // SAFETY: active_kernel selects AVX only when supported.
                unsafe { kernel(cells, factor, offset) };
                Ok(())
            });
        }
        match self.input {
            Input::Direct => self.transform_function(values, dims, base, degrees, |cell| cell),
            Input::Scale(factor) => {
                self.transform_function(values, dims, base, degrees, |cell| cell * factor)
            }
            Input::Affine(factor, offset) => {
                self.transform_function(values, dims, base, degrees, |cell| {
                    (cell * factor) + offset
                })
            }
        }
    }

    fn transform_function(
        &self,
        values: &mut [f64],
        dims: &[usize],
        base: i32,
        degrees: bool,
        input: impl Fn(f64) -> f64,
    ) -> BasicResult<()> {
        self.transform_regions(values, dims, base, |cells| {
            for cell in cells {
                *cell = super::eval_fast_number_function(self.function, input(*cell), degrees)?;
            }
            Ok(())
        })
    }

    fn transform_regions(
        &self,
        values: &mut [f64],
        dims: &[usize],
        base: i32,
        apply: impl Fn(&mut [f64]) -> BasicResult<()>,
    ) -> BasicResult<()> {
        if base == 0 {
            return apply(values);
        }
        let (stride, first_column) = Interpreter::mat_base_one_row_layout(dims);
        let (border, rows) = values.split_at_mut(stride);
        self.transform_border(border);
        for row in rows.chunks_exact_mut(stride) {
            let (border, active) = row.split_at_mut(first_column);
            self.transform_border(border);
            apply(active)?;
        }
        Ok(())
    }

    fn transform_border(&self, cells: &mut [f64]) {
        match self.input {
            Input::Direct => {}
            Input::Scale(factor) => {
                for cell in cells {
                    *cell *= factor;
                }
            }
            Input::Affine(factor, offset) => {
                for cell in cells {
                    *cell = (*cell * factor) + offset;
                }
            }
        }
    }
}

type Kernel = unsafe fn(&mut [f64], f64, f64);

fn active_kernel(_len: usize, input: Input) -> Kernel {
    #[cfg(target_arch = "x86_64")]
    if _len >= 4 && super::super::mat_divide::avx_available() {
        return match input {
            Input::Direct => floor_avx,
            Input::Scale(_) => transform_avx::<false>,
            Input::Affine(_, _) => transform_avx::<true>,
        };
    }
    match input {
        Input::Direct => floor_scalar,
        Input::Scale(_) => transform_scalar::<false>,
        Input::Affine(_, _) => transform_scalar::<true>,
    }
}

fn floor_scalar(cells: &mut [f64], _: f64, _: f64) {
    for cell in cells {
        *cell = cell.floor();
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
fn floor_avx(cells: &mut [f64], _: f64, _: f64) {
    use std::arch::x86_64::{_mm256_floor_pd, _mm256_loadu_pd, _mm256_storeu_pd};
    let mut chunks = cells.chunks_exact_mut(4);
    for chunk in &mut chunks {
        // SAFETY: each chunk has four elements and the accesses are unaligned.
        unsafe {
            let input = _mm256_loadu_pd(chunk.as_ptr());
            _mm256_storeu_pd(chunk.as_mut_ptr(), _mm256_floor_pd(input));
        }
    }
    floor_scalar(chunks.into_remainder(), 0.0, 0.0);
}

fn transform_scalar<const ADD_OFFSET: bool>(cells: &mut [f64], factor: f64, offset: f64) {
    for cell in cells {
        let product = *cell * factor;
        *cell = if ADD_OFFSET {
            product + offset
        } else {
            product
        }
        .floor();
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
fn transform_avx<const ADD_OFFSET: bool>(cells: &mut [f64], factor: f64, offset: f64) {
    use std::arch::x86_64::{
        _mm256_add_pd, _mm256_floor_pd, _mm256_loadu_pd, _mm256_mul_pd, _mm256_set1_pd,
        _mm256_storeu_pd,
    };
    let scale = _mm256_set1_pd(factor);
    let shift = _mm256_set1_pd(offset);
    let mut chunks = cells.chunks_exact_mut(4);
    for chunk in &mut chunks {
        // SAFETY: each chunk has four elements and the accesses are unaligned.
        unsafe {
            let input = _mm256_loadu_pd(chunk.as_ptr());
            let product = _mm256_mul_pd(input, scale);
            // Separate multiply and add preserve both f64 roundings.
            let sum = if ADD_OFFSET {
                _mm256_add_pd(product, shift)
            } else {
                product
            };
            _mm256_storeu_pd(chunk.as_mut_ptr(), _mm256_floor_pd(sum));
        }
    }
    transform_scalar::<ADD_OFFSET>(chunks.into_remainder(), factor, offset);
}

impl Interpreter {
    #[inline(never)]
    pub(super) fn execute_named_map(&mut self, target: &str, plan: &NamedMap) -> BasicResult<()> {
        let target_key = self.array_lookup_key(target);
        if !self.arrays.contains_key(target_key.as_ref()) {
            self.arrays.insert(
                target_key.to_string(),
                super::ArrayValue::new(target_key.as_ref(), vec![10]),
            );
        }
        if let Some((source, input)) = plan.input.resolve(self) {
            let kernel = MapKernel {
                input,
                function: plan.function,
            };
            let source_key = self.array_lookup_key(source);
            let stamp = self.debug_array_write_stamp();
            let reusable = NamedMap::can_reuse_destination(plan.function)
                .then(|| {
                    try_reuse_map_buffer(&mut self.arrays, source_key.as_ref(), target_key.as_ref())
                })
                .flatten();
            if let Some(array) = reusable {
                let ArrayData::Number(values) = &mut array.data else {
                    unreachable!("numeric target verified");
                };
                kernel
                    .transform(values, &array.dims, self.mat_base, self.angle_degrees)
                    .expect("reusing the destination requires an infallible function");
                array.clear_debug_write();
                if let Some(stamp) = stamp {
                    array.remember_debug_array_change(stamp, super::DebugArrayChange::All);
                }
                self.return_array_for_active_function(target);
                return Ok(());
            }
            if let Some(mut array) = self
                .array_ref(source)
                .filter(|source| matches!(source.dims.len(), 1 | 2) && !source.is_string())
                .cloned()
            {
                let ArrayData::Number(values) = &mut array.data else {
                    unreachable!("numeric source verified");
                };
                kernel
                    .transform(values, &array.dims, self.mat_base, self.angle_degrees)
                    .map_err(|error| self.with_current_line(error))?;
                return self.finish_mat_assignment(target, target_key, MatExprValue::Matrix(array));
            }
        }
        let value = plan
            .fallback
            .eval(self)
            .map_err(|error| self.with_current_line(error))?;
        self.finish_mat_assignment(target, target_key, value)
    }
}

// For infallible maps over equal shapes, keep the destination allocation
// and its debug binding.
fn try_reuse_map_buffer<'a>(
    arrays: &'a mut super::ArrayVariables,
    source: &str,
    target: &str,
) -> Option<&'a mut super::ArrayValue> {
    let source = *arrays.names.get(source)?;
    let target = *arrays.names.get(target)?;
    if target == source {
        let array = arrays.slots[target].as_mut()?;
        return (matches!(array.dims.len(), 1 | 2) && !array.is_string()).then_some(array);
    }
    let (source, target) = if source < target {
        let (left, right) = arrays.slots.split_at_mut(target);
        (left[source].as_ref()?, right[0].as_mut()?)
    } else {
        let (left, right) = arrays.slots.split_at_mut(source);
        (right[0].as_ref()?, left[target].as_mut()?)
    };
    if !matches!(source.dims.len(), 1 | 2) || source.dims != target.dims {
        return None;
    }
    let (ArrayData::Number(source), ArrayData::Number(values)) = (&source.data, &mut target.data)
    else {
        return None;
    };
    values.copy_from_slice(source);
    Some(target)
}
