//! Fuse scalar numeric leaves without changing operation order.
//!
//! Each operation still uses the interpreter's arithmetic and error helpers.
//! No value is hoisted across an expression or statement boundary.

use super::{
    eval_fast_binary, eval_fast_number_function, eval_fast_unary, BasicResult, BinaryOp,
    CachedNumericSlot, FastNumberExpr, FastNumberFunction, Interpreter, UnaryOp,
};

#[derive(Debug, Clone)]
enum Leaf {
    Number(f64),
    Variable {
        name: String,
        slot: CachedNumericSlot,
    },
}

impl Leaf {
    fn accepts(expression: &FastNumberExpr) -> bool {
        matches!(
            expression,
            FastNumberExpr::Number(_) | FastNumberExpr::Var { .. }
        )
    }

    fn from_expression(expression: FastNumberExpr) -> Result<Self, FastNumberExpr> {
        match expression {
            FastNumberExpr::Number(value) => Ok(Self::Number(value)),
            FastNumberExpr::Var { name, slot } => Ok(Self::Variable { name, slot }),
            expression => Err(expression),
        }
    }

    #[inline(always)]
    fn eval(&self, interpreter: &mut Interpreter) -> BasicResult<f64> {
        match self {
            Self::Number(value) => Ok(*value),
            Self::Variable { name, slot } => interpreter.get_fast_number_variable(name, slot),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct BinaryLeaves {
    op: BinaryOp,
    left: Leaf,
    right: Leaf,
}

impl BinaryLeaves {
    #[inline(always)]
    pub(super) fn eval(&self, interpreter: &mut Interpreter) -> BasicResult<f64> {
        let (left, right) = if interpreter.function_call_stack.is_empty() {
            let left = match &self.left {
                Leaf::Number(value) => *value,
                Leaf::Variable { name, slot } => {
                    interpreter.numeric_variables.get_cached_or_zero(name, slot)
                }
            };
            let right = match &self.right {
                Leaf::Number(value) => *value,
                Leaf::Variable { name, slot } => {
                    interpreter.numeric_variables.get_cached_or_zero(name, slot)
                }
            };
            (left, right)
        } else {
            let left = self.left.eval(interpreter)?;
            let right = self.right.eval(interpreter)?;
            (left, right)
        };
        eval_fast_binary(self.op, left, right)
    }

    // The existing Windows index specialization is retained. Fusing an I+J
    // node must not turn its previous specialized index into a general one.
    #[cfg(windows)]
    pub(super) fn into_index(self: Box<Self>) -> super::FastIndexExpr {
        let operation = match self.op {
            BinaryOp::Add => Some(super::FastIndexOperation::Add),
            BinaryOp::Sub => Some(super::FastIndexOperation::Sub),
            _ => None,
        };
        if operation.is_some()
            && matches!(&self.left, Leaf::Variable { .. })
            && matches!(&self.right, Leaf::Variable { .. })
        {
            let Self { op: _, left, right } = *self;
            let Leaf::Variable {
                name: left_name,
                slot: left_slot,
            } = left
            else {
                unreachable!()
            };
            let Leaf::Variable {
                name: right_name,
                slot: right_slot,
            } = right
            else {
                unreachable!()
            };
            super::FastIndexExpr::Variables {
                left_name,
                left_slot,
                right_name,
                right_slot,
                op: operation.unwrap(),
            }
        } else {
            super::FastIndexExpr::General(FastNumberExpr::BinaryLeaves(self))
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct BinaryConstVariable {
    op: BinaryOp,
    name: String,
    slot: CachedNumericSlot,
    constant: f64,
    constant_left: bool,
}

impl BinaryConstVariable {
    #[inline(always)]
    pub(super) fn eval(&self, interpreter: &mut Interpreter) -> BasicResult<f64> {
        let value = interpreter.get_fast_number_variable(&self.name, &self.slot)?;
        if self.constant_left {
            eval_fast_binary(self.op, self.constant, value)
        } else {
            eval_fast_binary(self.op, value, self.constant)
        }
    }

    #[cfg(windows)]
    pub(super) fn into_index(self: Box<Self>) -> super::FastIndexExpr {
        let operation = match self.op {
            BinaryOp::Add => Some(super::FastIndexOperation::Add),
            BinaryOp::Sub => Some(super::FastIndexOperation::Sub),
            _ => None,
        };
        if let Some(op) = operation {
            let Self {
                op: _,
                name,
                slot,
                constant,
                constant_left,
            } = *self;
            super::FastIndexExpr::Offset {
                name,
                slot,
                op,
                constant,
                constant_left,
            }
        } else {
            super::FastIndexExpr::General(FastNumberExpr::BinaryConstVariable(self))
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct FunctionLeaf {
    function: FastNumberFunction,
    arg: Leaf,
}

impl FunctionLeaf {
    #[cfg(test)]
    pub(super) fn function(&self) -> FastNumberFunction {
        self.function
    }

    #[inline(always)]
    pub(super) fn eval(&self, interpreter: &mut Interpreter) -> BasicResult<f64> {
        let arg = self.arg.eval(interpreter)?;
        eval_fast_number_function(self.function, arg, interpreter.angle_degrees)
    }
}

#[derive(Debug, Clone)]
pub(super) struct UnaryLeaf {
    op: UnaryOp,
    arg: Leaf,
}

impl UnaryLeaf {
    #[inline(always)]
    pub(super) fn eval(&self, interpreter: &mut Interpreter) -> BasicResult<f64> {
        let value = self.arg.eval(interpreter)?;
        Ok(eval_fast_unary(self.op, value))
    }
}

pub(super) fn binary(op: BinaryOp, left: FastNumberExpr, right: FastNumberExpr) -> FastNumberExpr {
    match (left, right) {
        (FastNumberExpr::Number(constant), FastNumberExpr::Var { name, slot }) => {
            FastNumberExpr::BinaryConstVariable(Box::new(BinaryConstVariable {
                op,
                name,
                slot,
                constant,
                constant_left: true,
            }))
        }
        (FastNumberExpr::Var { name, slot }, FastNumberExpr::Number(constant)) => {
            FastNumberExpr::BinaryConstVariable(Box::new(BinaryConstVariable {
                op,
                name,
                slot,
                constant,
                constant_left: false,
            }))
        }
        (left, right) if Leaf::accepts(&left) && Leaf::accepts(&right) => {
            FastNumberExpr::BinaryLeaves(Box::new(BinaryLeaves {
                op,
                left: Leaf::from_expression(left).unwrap(),
                right: Leaf::from_expression(right).unwrap(),
            }))
        }
        (FastNumberExpr::Number(constant), expr) => FastNumberExpr::BinaryConst {
            op,
            expr: Box::new(expr),
            constant,
            constant_left: true,
        },
        (expr, FastNumberExpr::Number(constant)) => FastNumberExpr::BinaryConst {
            op,
            expr: Box::new(expr),
            constant,
            constant_left: false,
        },
        (left, right) => FastNumberExpr::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        },
    }
}

pub(super) fn function(function: FastNumberFunction, arg: FastNumberExpr) -> FastNumberExpr {
    match Leaf::from_expression(arg) {
        Ok(arg) => FastNumberExpr::FunctionLeaf(Box::new(FunctionLeaf { function, arg })),
        Err(arg) => FastNumberExpr::Function1 {
            function,
            arg: Box::new(arg),
        },
    }
}

pub(super) fn unary(op: UnaryOp, arg: FastNumberExpr) -> FastNumberExpr {
    match Leaf::from_expression(arg) {
        Ok(arg) => FastNumberExpr::UnaryLeaf(Box::new(UnaryLeaf { op, arg })),
        Err(expr) => FastNumberExpr::Unary {
            op,
            expr: Box::new(expr),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;
    use crate::expr::{compile_expression, eval_compiled_number};

    fn assert_matches_generic(source: &str, degrees: bool) {
        let expression = compile_expression(source).unwrap();
        let fast = super::super::compile_fast_number_expr(&expression, true).unwrap();
        let tree = super::super::compile_fast_number_expr_unfused(&expression, true).unwrap();
        let prepare = || {
            let mut interpreter = Interpreter::new();
            interpreter.angle_degrees = degrees;
            interpreter.numeric_variables.insert("A".into(), -0.0);
            interpreter.numeric_variables.insert("B".into(), 2.5);
            interpreter.numeric_variables.insert("C".into(), -3.0);
            interpreter
        };
        let mut candidate = prepare();
        let mut reference = prepare();
        let mut reference_tree = prepare();
        let actual = fast.eval(&mut candidate);
        let tree_result = tree.eval(&mut reference_tree);
        match (&actual, &tree_result) {
            (Ok(actual), Ok(expected)) => {
                assert_eq!(actual.to_bits(), expected.to_bits(), "{source}: tree")
            }
            (Err(actual), Err(expected)) => {
                assert_eq!(actual.code, expected.code, "{source}: tree")
            }
            (actual, expected) => panic!("{source}: tree {actual:?} != {expected:?}"),
        }
        match (actual, eval_compiled_number(&mut reference, &expression)) {
            (Ok(actual), Ok(expected)) => {
                assert_eq!(actual.to_bits(), expected.to_bits(), "{source}")
            }
            (Err(actual), Err(expected)) => assert_eq!(actual.code, expected.code, "{source}"),
            (actual, expected) => panic!("{source}: {actual:?} != {expected:?}"),
        }
        let next = candidate.rng.next_f64().to_bits();
        assert_eq!(
            next,
            reference.rng.next_f64().to_bits(),
            "{source}: generic RNG"
        );
        assert_eq!(
            next,
            reference_tree.rng.next_f64().to_bits(),
            "{source}: tree RNG"
        );
    }

    #[test]
    fn fused_leaves_preserve_layout_and_operation_structure() {
        type LargestTreePayload = (
            String,
            super::super::CachedArraySlot,
            Box<super::super::FastIndexExpr>,
            Box<super::super::FastIndexExpr>,
        );
        assert_eq!(
            std::mem::size_of::<FastNumberExpr>(),
            std::mem::size_of::<LargestTreePayload>()
        );
        for source in [
            "A+B", "1+2", "B+3", "3-B", "SIN(B)", "ABS(-1)", "-A", "NOT B",
        ] {
            let expression = compile_expression(source).unwrap();
            let fast = super::super::compile_fast_number_expr(&expression, true).unwrap();
            assert!(matches!(
                fast,
                FastNumberExpr::BinaryLeaves(_)
                    | FastNumberExpr::BinaryConstVariable(_)
                    | FastNumberExpr::FunctionLeaf(_)
                    | FastNumberExpr::UnaryLeaf(_)
                    | FastNumberExpr::Function1 { .. }
            ));
        }
    }

    #[test]
    fn fused_leaves_match_values_signed_zero_errors_and_rng_order() {
        for degrees in [false, true] {
            for source in [
                "A+B",
                "A-A",
                "A*B",
                "A/2",
                "2/A",
                "A\\0",
                "B MOD 0",
                "B^C",
                "A=B",
                "A AND B",
                "-(A+B)",
                "-A",
                "+A",
                "NOT C",
                "SIN(B)",
                "COS(B)",
                "ATN(B)",
                "SQR(C)",
                "LOG(A)",
                "EXP(C)",
                "ROUND(B)",
                "INT(C)",
                "B/(C+3)",
                "RND+(B/C)",
                "(B/C)+RND",
                "RND+(B MOD 0)",
                "(B MOD 0)+RND",
                "MIN(A,B,C)+MAX(A,B,C)",
            ] {
                assert_matches_generic(source, degrees);
            }
        }
        assert_eq!(
            eval_fast_binary(BinaryOp::Div, 1.0, 0.0).unwrap_err().code,
            ErrorCode::DivisionByZero
        );
    }

    #[test]
    fn fused_leaves_preserve_fn_array_type_checks_and_rng_error_order() {
        use super::super::{ActiveFunctionFrame, ArrayValue};
        use std::rc::Rc;
        for source in [
            "VALUE+RND",
            "RND+VALUE",
            "VALUE+2",
            "2+VALUE",
            "VALUE+LATER",
            "LATER+VALUE",
            "+VALUE",
            "ABS(VALUE)",
        ] {
            let expression = compile_expression(source).unwrap();
            let fast = super::super::compile_fast_number_expr(&expression, true).unwrap();
            let tree = super::super::compile_fast_number_expr_unfused(&expression, true).unwrap();
            if source.contains("LATER") {
                assert!(matches!(&fast, FastNumberExpr::BinaryLeaves(_)));
            }
            let prepare = || {
                let mut interpreter = Interpreter::new();
                let name = Rc::<str>::from("FNWORK");
                interpreter.function_call_stack.push(name.clone());
                interpreter.active_functions.push(ActiveFunctionFrame {
                    name,
                    return_value: None,
                });
                interpreter
                    .arrays
                    .insert("SOURCE".to_string(), ArrayValue::new("SOURCE", vec![0]));
                interpreter.set_array_alias_binding("VALUE", "SOURCE".to_string());
                interpreter
            };
            let mut candidate = prepare();
            let mut reference = prepare();
            let mut reference_tree = prepare();
            assert_eq!(
                fast.eval(&mut candidate).unwrap_err().code,
                ErrorCode::TypeMismatch,
                "{source}"
            );
            assert_eq!(
                tree.eval(&mut reference_tree).unwrap_err().code,
                ErrorCode::TypeMismatch,
                "{source}"
            );
            assert_eq!(
                eval_compiled_number(&mut reference, &expression)
                    .unwrap_err()
                    .code,
                ErrorCode::TypeMismatch,
                "{source}"
            );
            if source == "VALUE+LATER" {
                assert!(!candidate.numeric_variables.names.contains_key("LATER"));
            } else if source == "LATER+VALUE" {
                assert!(candidate.numeric_variables.names.contains_key("LATER"));
                assert_eq!(candidate.numeric_variables.get("LATER"), None);
            }
            let next = candidate.rng.next_f64().to_bits();
            assert_eq!(
                next,
                reference.rng.next_f64().to_bits(),
                "{source}: generic RNG"
            );
            assert_eq!(
                next,
                reference_tree.rng.next_f64().to_bits(),
                "{source}: tree RNG"
            );

            for value in [7.5, -0.0, -2.0] {
                candidate.numeric_variables.insert("VALUE".into(), value);
                reference.numeric_variables.insert("VALUE".into(), value);
                let actual = fast.eval(&mut candidate).unwrap();
                let expected = eval_compiled_number(&mut reference, &expression).unwrap();
                assert_eq!(
                    actual.to_bits(),
                    expected.to_bits(),
                    "{source}: scalar shadows array"
                );
            }
            candidate.numeric_variables.remove("VALUE");
            assert_eq!(
                fast.eval(&mut candidate).unwrap_err().code,
                ErrorCode::TypeMismatch,
                "{source}: removal"
            );
        }
    }

    #[test]
    fn fused_leaves_rebind_slots_between_interpreters_and_after_clear() {
        let expression = compile_expression("B+C*B").unwrap();
        let fast = super::super::compile_fast_number_expr(&expression, true).unwrap();
        let mut first = Interpreter::new();
        first.numeric_variables.insert("B".into(), 2.0);
        first.numeric_variables.insert("C".into(), 3.0);
        assert_eq!(fast.eval(&mut first).unwrap(), 8.0);
        let mut second = Interpreter::new();
        second.numeric_variables.insert("C".into(), 5.0);
        second.numeric_variables.insert("B".into(), 4.0);
        assert_eq!(fast.eval(&mut second).unwrap(), 24.0);
        first.numeric_variables.clear();
        first.numeric_variables.insert("C".into(), 7.0);
        first.numeric_variables.insert("B".into(), 6.0);
        assert_eq!(fast.eval(&mut first).unwrap(), 48.0);
        assert_eq!(fast.eval(&mut second).unwrap(), 24.0);
    }
}
