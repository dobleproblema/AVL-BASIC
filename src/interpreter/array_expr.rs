use super::{
    compile_fast_number_expr, ArrayData, ArrayOrCallKind, BasicError, BasicResult, BinaryOp,
    CachedArraySlot, CachedNumericSlot, ErrorCode, Expr, FastNumberExpr, Interpreter, UnaryOp,
};
use crate::expr::checked_number;

const MAX_VARIABLES: usize = 8;
const MAX_ARRAYS: usize = 8;
const MAX_STACK: usize = 16;
const MAX_OPERATIONS: usize = 128;
const MIN_READS: usize = 4;

#[derive(Debug, Clone)]
struct ScalarInput {
    name: String,
    slot: CachedNumericSlot,
}

#[derive(Debug, Clone)]
struct ArrayInput {
    name: String,
    slot: CachedArraySlot,
}

#[derive(Debug, Clone, Copy)]
enum IndexOperation {
    Add,
    Sub,
}

impl IndexOperation {
    #[inline]
    fn eval(self, left: f64, right: f64) -> BasicResult<f64> {
        match self {
            Self::Add => checked_number(left + right),
            Self::Sub => checked_number(left - right),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ArrayIndex {
    Constant(f64),
    Scalar(u8),
    Offset {
        scalar: u8,
        constant: f64,
        operation: IndexOperation,
        constant_left: bool,
    },
    Pair {
        left: u8,
        right: u8,
        operation: IndexOperation,
    },
}

impl ArrayIndex {
    #[inline]
    fn eval(self, scalars: &[f64; MAX_VARIABLES]) -> BasicResult<i32> {
        let value = match self {
            Self::Constant(value) => value,
            Self::Scalar(index) => scalars[index as usize],
            Self::Offset {
                scalar,
                constant,
                operation,
                constant_left,
            } => {
                let value = scalars[scalar as usize];
                if constant_left {
                    operation.eval(constant, value)?
                } else {
                    operation.eval(value, constant)?
                }
            }
            Self::Pair {
                left,
                right,
                operation,
            } => operation.eval(scalars[left as usize], scalars[right as usize])?,
        };
        let index = value as i32;
        if value == index as f64 {
            return Ok(index);
        }
        if value.fract() != 0.0 {
            return Err(BasicError::new(ErrorCode::InvalidIndex));
        }
        Ok(index)
    }
}

#[derive(Debug, Clone, Copy)]
enum Operation {
    Constant(f64),
    Scalar(u8),
    Read { array: u8, index: ArrayIndex },
    Add,
    Sub,
    Mul,
    Negate,
}

/// Pure scalar arithmetic containing affine 1D reads. The plan is an optional
/// optimization of an existing RHS; it never owns assignment or array creation.
#[derive(Debug, Clone)]
pub(super) struct CompiledArrayExpr {
    variables: Box<[ScalarInput]>,
    arrays: Box<[ArrayInput]>,
    operations: Box<[Operation]>,
}

impl CompiledArrayExpr {
    pub(super) fn compile(expr: &Expr) -> Option<Box<Self>> {
        let mut builder = Builder::default();
        builder.compile_node(expr)?;
        if builder.stack != 1 || builder.reads < MIN_READS {
            return None;
        }
        Some(Box::new(Self {
            variables: builder.variables.into_boxed_slice(),
            arrays: builder.arrays.into_boxed_slice(),
            operations: builder.operations.into_boxed_slice(),
        }))
    }

    /// None means the guard declined, before reading scalar values. Once the
    /// plan starts, every success/error is final and must not trigger fallback.
    pub(super) fn eval(&self, interpreter: &mut Interpreter) -> Option<BasicResult<f64>> {
        // In a DEF FN, an initially absent scalar can become an array reference
        // after an implicit array read. Keep all function scope on its old path.
        if !interpreter.function_call_stack.is_empty() {
            return None;
        }
        let mut array_slots = [0usize; MAX_ARRAYS];
        for (index, input) in self.arrays.iter().enumerate() {
            let slot = interpreter.resolve_cached_array_slot(&input.name, &input.slot);
            let array = interpreter.arrays.get_resolved(slot)?;
            if array.dims.len() != 1 || array.is_string() {
                return None;
            }
            array_slots[index] = slot;
        }

        // No function calls, implicit DIM, or mutation can occur during this
        // RHS. Outside DEF FN, ordinary scalar reads cannot raise an error and
        // ignore array aliases exactly as get_fast_number_variable does.
        let mut scalars = [0.0; MAX_VARIABLES];
        for (index, input) in self.variables.iter().enumerate() {
            scalars[index] = interpreter
                .numeric_variables
                .get_cached(&input.name, &input.slot)
                .unwrap_or(0.0);
        }
        let mut arrays: [Option<&[f64]>; MAX_ARRAYS] = [None; MAX_ARRAYS];
        for index in 0..self.arrays.len() {
            let array = interpreter
                .arrays
                .get_resolved(array_slots[index])
                .expect("scalar-slot resolution cannot change guarded arrays");
            let ArrayData::Number(values) = &array.data else {
                unreachable!("the guard already verified numeric arrays");
            };
            arrays[index] = Some(values);
        }
        Some(self.eval_resolved(&scalars, &arrays))
    }

    fn eval_resolved(
        &self,
        scalars: &[f64; MAX_VARIABLES],
        arrays: &[Option<&[f64]>; MAX_ARRAYS],
    ) -> BasicResult<f64> {
        let mut stack = [0.0; MAX_STACK];
        let mut length = 0usize;
        for operation in &self.operations {
            match *operation {
                Operation::Constant(value) => {
                    stack[length] = value;
                    length += 1;
                }
                Operation::Scalar(index) => {
                    stack[length] = scalars[index as usize];
                    length += 1;
                }
                Operation::Read { array, index } => {
                    let index = index.eval(scalars)?;
                    let values = arrays[array as usize].expect("resolved numeric array");
                    if index < 0 || index as usize >= values.len() {
                        return Err(BasicError::new(ErrorCode::IndexOutOfRange));
                    }
                    stack[length] = values[index as usize];
                    length += 1;
                }
                Operation::Negate => stack[length - 1] = -stack[length - 1],
                Operation::Add | Operation::Sub | Operation::Mul => {
                    let right = stack[length - 1];
                    let left = stack[length - 2];
                    let result = match operation {
                        Operation::Add => checked_number(left + right),
                        Operation::Sub => checked_number(left - right),
                        Operation::Mul => checked_number(left * right),
                        _ => unreachable!(),
                    }?;
                    length -= 1;
                    stack[length - 1] = result;
                }
            }
        }
        debug_assert_eq!(length, 1);
        Ok(stack[0])
    }
}

#[derive(Default)]
struct Builder {
    variables: Vec<ScalarInput>,
    arrays: Vec<ArrayInput>,
    operations: Vec<Operation>,
    stack: usize,
    reads: usize,
}

impl Builder {
    fn push(&mut self, operation: Operation, stack_change: i8) -> Option<()> {
        if self.operations.len() == MAX_OPERATIONS {
            return None;
        }
        self.stack = self.stack.checked_add_signed(stack_change as isize)?;
        if self.stack > MAX_STACK {
            return None;
        }
        self.operations.push(operation);
        Some(())
    }

    fn variable(&mut self, name: &str) -> Option<u8> {
        if let Some(index) = self.variables.iter().position(|input| input.name == name) {
            return Some(index as u8);
        }
        if self.variables.len() == MAX_VARIABLES {
            return None;
        }
        let index = self.variables.len() as u8;
        self.variables.push(ScalarInput {
            name: name.to_string(),
            slot: CachedNumericSlot::default(),
        });
        Some(index)
    }

    fn array(&mut self, name: &str) -> Option<u8> {
        if let Some(index) = self.arrays.iter().position(|input| input.name == name) {
            return Some(index as u8);
        }
        if self.arrays.len() == MAX_ARRAYS {
            return None;
        }
        let index = self.arrays.len() as u8;
        self.arrays.push(ArrayInput {
            name: name.to_string(),
            slot: CachedArraySlot::default(),
        });
        Some(index)
    }

    fn scalar_leaf(&mut self, expr: &Expr) -> Option<ArrayIndex> {
        match compile_fast_number_expr(expr, false)? {
            FastNumberExpr::Number(value) => Some(ArrayIndex::Constant(value)),
            FastNumberExpr::Var { name, .. } => Some(ArrayIndex::Scalar(self.variable(&name)?)),
            _ => None,
        }
    }

    fn compile_index(&mut self, expr: &Expr) -> Option<ArrayIndex> {
        if let Some(leaf) = self.scalar_leaf(expr) {
            return Some(leaf);
        }
        let Expr::Binary { op, left, right } = expr else {
            return None;
        };
        let operation = match op {
            BinaryOp::Add => IndexOperation::Add,
            BinaryOp::Sub => IndexOperation::Sub,
            _ => return None,
        };
        match (self.scalar_leaf(left)?, self.scalar_leaf(right)?) {
            (ArrayIndex::Scalar(scalar), ArrayIndex::Constant(constant)) => {
                Some(ArrayIndex::Offset {
                    scalar,
                    constant,
                    operation,
                    constant_left: false,
                })
            }
            (ArrayIndex::Constant(constant), ArrayIndex::Scalar(scalar)) => {
                Some(ArrayIndex::Offset {
                    scalar,
                    constant,
                    operation,
                    constant_left: true,
                })
            }
            (ArrayIndex::Scalar(left), ArrayIndex::Scalar(right)) => Some(ArrayIndex::Pair {
                left,
                right,
                operation,
            }),
            // Keep even constant-index arithmetic unfused: constant folding
            // could move an overflow/NaN error from execution to compilation.
            _ => None,
        }
    }

    fn compile_node(&mut self, expr: &Expr) -> Option<()> {
        match expr {
            Expr::Number(value) => self.push(Operation::Constant(*value), 1),
            Expr::Var(_) => match self.scalar_leaf(expr)? {
                ArrayIndex::Constant(value) => self.push(Operation::Constant(value), 1),
                ArrayIndex::Scalar(index) => self.push(Operation::Scalar(index), 1),
                _ => unreachable!(),
            },
            Expr::ArrayOrCall { name, args, kind }
                if *kind == ArrayOrCallKind::Array && !name.ends_with('$') =>
            {
                let [index] = args.as_slice() else {
                    return None;
                };
                let index = self.compile_index(index)?;
                let array = self.array(name)?;
                self.reads += 1;
                self.push(Operation::Read { array, index }, 1)
            }
            Expr::Unary { op, expr } => {
                self.compile_node(expr)?;
                match op {
                    UnaryOp::Plus => Some(()),
                    UnaryOp::Minus => self.push(Operation::Negate, 0),
                    UnaryOp::Not => None,
                }
            }
            Expr::Binary { op, left, right } => {
                let operation = match op {
                    BinaryOp::Add => Operation::Add,
                    BinaryOp::Sub => Operation::Sub,
                    BinaryOp::Mul => Operation::Mul,
                    _ => return None,
                };
                self.compile_node(left)?;
                self.compile_node(right)?;
                self.push(operation, -1)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{compile_expression, eval_compiled_number};
    use crate::interpreter::{ArrayValue, SimpleRng};
    use std::rc::Rc;

    const PRESSURE: &str = "0.375*(DIV(I)+P(I-1)+P(I+1)+P(I-S)+P(I+S))-0.5*P(I)";

    fn setup() -> Interpreter {
        let mut interpreter = Interpreter::new();
        interpreter.graphics_window_enabled = false;
        interpreter.rng = SimpleRng::new(1234);
        for (name, multiplier) in [("P", 0.125), ("DIV", 0.3125)] {
            let mut array = ArrayValue::new(name, vec![12]);
            for index in 0..=12 {
                array
                    .set_number_direct_1d(index, index as f64 * multiplier)
                    .unwrap();
            }
            interpreter.arrays.insert(name.to_string(), array);
        }
        interpreter.numeric_variables.insert("I".to_string(), 6.0);
        interpreter.numeric_variables.insert("S".to_string(), 3.0);
        interpreter
    }

    fn assert_same(actual: BasicResult<f64>, expected: BasicResult<f64>, source: &str) {
        match (actual, expected) {
            (Ok(actual), Ok(expected)) => {
                assert_eq!(actual.to_bits(), expected.to_bits(), "{source}")
            }
            (Err(actual), Err(expected)) => {
                assert_eq!(actual.code, expected.code, "{source}");
                assert_eq!(actual.line, expected.line, "{source}");
                assert_eq!(actual.detail, expected.detail, "{source}");
            }
            (actual, expected) => panic!("{source}: {actual:?} != {expected:?}"),
        }
    }

    #[test]
    fn array_expr_pressure_and_arithmetic_match_both_existing_evaluators() {
        for source in [
            PRESSURE,
            "P(I)+(DIV(I)-P(I-S))*0.375+P(I+1)",
            "(P(I)+DIV(I))+P(I+1)+P(I-1)",
            "P(I)+(DIV(I)+P(I+1))+P(I-1)",
            "-P(I)+(+DIV(I))*S+P(I+1)+DIV(I+1)",
            "P(I)-P(1+I)+P(S-I)+DIV(I)",
        ] {
            let expr = compile_expression(source).unwrap();
            let plan = CompiledArrayExpr::compile(&expr).unwrap();
            let fast = compile_fast_number_expr(&expr, true).unwrap();
            let mut candidate = setup();
            let mut old_fast = setup();
            let mut generic = setup();
            let actual = plan.eval(&mut candidate).unwrap();
            assert_same(actual.clone(), fast.eval(&mut old_fast), source);
            assert_same(actual, eval_compiled_number(&mut generic, &expr), source);
        }
    }

    #[test]
    fn array_expr_keeps_association_and_ieee_errors_in_each_operation() {
        for source in ["P(I)+DIV(I)+P(I+1)+P(I-1)", "P(I)+(DIV(I)+P(I+1))+P(I-1)"] {
            let expr = compile_expression(source).unwrap();
            let plan = CompiledArrayExpr::compile(&expr).unwrap();
            let fast = compile_fast_number_expr(&expr, true).unwrap();
            for values in [
                [1.0e16, -1.0e16, 1.0],
                [-0.0, -0.0, -0.0],
                [f64::MAX, f64::MAX, -f64::MAX],
                [f64::INFINITY, f64::NEG_INFINITY, 1.0],
                [f64::NAN, 1.0, 2.0],
            ] {
                let mut candidate = setup();
                let mut old_fast = setup();
                let mut generic = setup();
                for interpreter in [&mut candidate, &mut old_fast, &mut generic] {
                    interpreter
                        .array_mut("P")
                        .unwrap()
                        .set_number_direct_1d(5, 0.0)
                        .unwrap();
                    interpreter
                        .array_mut("P")
                        .unwrap()
                        .set_number_direct_1d(6, values[0])
                        .unwrap();
                    interpreter
                        .array_mut("DIV")
                        .unwrap()
                        .set_number_direct_1d(6, values[1])
                        .unwrap();
                    interpreter
                        .array_mut("P")
                        .unwrap()
                        .set_number_direct_1d(7, values[2])
                        .unwrap();
                }
                let actual = plan.eval(&mut candidate).unwrap();
                assert_same(actual.clone(), fast.eval(&mut old_fast), source);
                assert_same(actual, eval_compiled_number(&mut generic, &expr), source);
            }
        }
    }

    #[test]
    fn array_expr_indices_and_first_errors_match_existing_evaluation() {
        for source in [
            PRESSURE,
            "P(I-S)+DIV(I)+P(I)+DIV(I+1)",
            "P(1-I)+DIV(I+S)+P(I)+DIV(I+1)",
        ] {
            let expr = compile_expression(source).unwrap();
            let plan = CompiledArrayExpr::compile(&expr).unwrap();
            let fast = compile_fast_number_expr(&expr, true).unwrap();
            for (index, stride) in [
                (-0.0, 0.0),
                (1.0, 1.0),
                (6.0, 3.0),
                (0.5, 0.5),
                (-1.0, 2.0),
                (13.0, 1.0),
                (6.0, 0.5),
                (1.0e100, 1.0),
                (f64::MAX, f64::MAX),
                (f64::INFINITY, 1.0),
                (f64::NAN, 1.0),
                (6.0, f64::INFINITY),
            ] {
                let mut candidate = setup();
                let mut old_fast = setup();
                let mut generic = setup();
                for interpreter in [&mut candidate, &mut old_fast, &mut generic] {
                    interpreter.numeric_variables.insert("I".to_string(), index);
                    interpreter
                        .numeric_variables
                        .insert("S".to_string(), stride);
                }
                let actual = plan.eval(&mut candidate).unwrap();
                assert_same(actual.clone(), fast.eval(&mut old_fast), source);
                assert_same(actual, eval_compiled_number(&mut generic, &expr), source);
            }
        }
        let expr = compile_expression("P(I)+DIV(I-S)+P(I+1)+DIV(I)").unwrap();
        let plan = CompiledArrayExpr::compile(&expr).unwrap();
        let mut interpreter = setup();
        interpreter.numeric_variables.insert("I".to_string(), 13.0);
        interpreter
            .numeric_variables
            .insert("S".to_string(), f64::INFINITY);
        assert_eq!(
            plan.eval(&mut interpreter).unwrap().unwrap_err().code,
            ErrorCode::IndexOutOfRange
        );
    }

    #[test]
    fn array_expr_guards_decline_missing_wrong_shape_and_string_arrays_without_evaluation() {
        let expr = compile_expression(PRESSURE).unwrap();
        let plan = CompiledArrayExpr::compile(&expr).unwrap();
        for replacement in [
            None,
            Some(ArrayValue::new("P", vec![12, 1])),
            Some(ArrayValue::new("P$", vec![12])),
        ] {
            let mut candidate = setup();
            candidate.arrays.remove("P");
            if let Some(array) = replacement {
                candidate.arrays.insert("P".to_string(), array);
            }
            candidate.numeric_variables.clear();
            assert!(plan.eval(&mut candidate).is_none());
            assert!(
                candidate.numeric_variables.names.is_empty(),
                "guard must not resolve/read scalars"
            );
            assert_eq!(candidate.rng.next_f64(), SimpleRng::new(1234).next_f64());
        }
    }

    #[test]
    fn array_expr_declined_autodim_and_fn_scope_keep_the_old_path() {
        let expr = compile_expression("P(I)+DIV(I)+P(I)+DIV(I)").unwrap();
        let plan = CompiledArrayExpr::compile(&expr).unwrap();
        let fast = compile_fast_number_expr(&expr, true).unwrap();
        let mut candidate = Interpreter::new();
        let mut reference = Interpreter::new();
        assert!(plan.eval(&mut candidate).is_none());
        assert!(candidate.array_ref("P").is_none());
        assert!(candidate.array_ref("DIV").is_none());
        assert_same(
            fast.eval(&mut candidate),
            eval_compiled_number(&mut reference, &expr),
            "autodim fallback",
        );
        assert_eq!(candidate.array_ref("P").unwrap().dims, vec![10]);

        let mut scoped = setup();
        scoped.function_call_stack.push(Rc::from("FNREAD"));
        scoped.numeric_variables.clear();
        assert!(plan.eval(&mut scoped).is_none());
        assert!(scoped.numeric_variables.names.is_empty());
    }

    #[test]
    fn array_expr_cache_rebinds_after_alias_redim_and_clear() {
        let expr = compile_expression("P(I)+DIV(I)+P(I)+DIV(I)").unwrap();
        let plan = CompiledArrayExpr::compile(&expr).unwrap();
        let mut first = setup();
        let mut second = setup();
        assert_eq!(plan.eval(&mut first).unwrap().unwrap(), 5.25);
        second.set_array_alias_binding("P", "DIV".to_string());
        assert_eq!(plan.eval(&mut second).unwrap().unwrap(), 7.5);
        assert_eq!(plan.eval(&mut first).unwrap().unwrap(), 5.25);
        first.process_immediate("REDIM P(2):I=1").unwrap();
        let expected = eval_compiled_number(&mut first, &expr).unwrap();
        assert_eq!(plan.eval(&mut first).unwrap().unwrap(), expected);
        first.process_immediate("CLEAR").unwrap();
        assert!(plan.eval(&mut first).is_none());
        first
            .process_immediate("DIM P(2),DIV(2):I=1:P(1)=3:DIV(1)=7")
            .unwrap();
        assert_eq!(plan.eval(&mut first).unwrap().unwrap(), 20.0);
    }

    #[test]
    fn array_expr_rejects_effects_and_unsupported_operators_without_running_them() {
        for source in [
            "P(I)+RND",
            "P(RND)",
            "P(INT(RND))",
            "P(I)+FNSTEP()",
            "P(I)+IIF(1,2,1/0)",
            "P(I)+TIME",
            "P(I)/2",
            "P(I)^2",
            "P(I,J)",
            "P(I*2)",
            "P$(I)",
            "P(I)+ABS(S)",
            "1+2",
            "P(I)+1",
            "P(I)+DIV(I)+P(I+1)",
        ] {
            let expr = compile_expression(source).unwrap();
            assert!(CompiledArrayExpr::compile(&expr).is_none(), "{source}");
        }
    }
}
