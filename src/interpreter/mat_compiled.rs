//! Cache MAT syntax while retaining its dynamic array/scalar interpretation.
//!
//! MAT arithmetic shares the scalar parser's tokens, precedence and grouping.
//! Array names remain dynamically resolved; scalar calls use the ordinary
//! evaluator, while TRN/INV and arithmetic operate on whole matrices.
use super::mat_subarrays::CopyReference;
use super::*;

#[derive(Debug, Clone)]
pub(super) struct CompiledMatAssignment {
    operation: CompiledMatOperation,
}

#[derive(Debug, Clone)]
enum CompiledMatOperation {
    Arithmetic {
        target: String,
        expression: CompiledMatExpr,
    },
    Copy {
        left: CopyReference,
        right: CopyReference,
        source: String,
        rhs: String,
    },
}

#[derive(Debug, Clone)]
pub(super) enum CompiledMatExpr {
    Number(f64),
    Name {
        name: String,
        scalar: Expr,
    },
    Negate(Box<Self>),
    Positive(Box<Self>),
    Binary {
        left: Box<Self>,
        right: Box<Self>,
        op: char,
    },
    MatrixFunction {
        name: String,
        args: Vec<Self>,
    },
    ScalarUnary {
        op: UnaryOp,
        expr: Box<Self>,
    },
    ScalarBinary {
        op: BinaryOp,
        left: Box<Self>,
        right: Box<Self>,
    },
    Scalar(Expr),
}

impl CompiledMatAssignment {
    pub(super) fn compile(source: &str) -> Option<Self> {
        let equal = find_assignment_equal(source)?;
        let lhs = source[..equal].trim();
        let rhs = source[equal + 1..].trim();
        if let (Some(left), Some(right)) = (CopyReference::parse(lhs), CopyReference::parse(rhs)) {
            if !left.name.ends_with('$')
                && !right.name.ends_with('$')
                && !left.name.starts_with("FN")
                && !right.name.starts_with("FN")
                && (left.selectors.is_some() || right.explicit_slice())
            {
                return Some(Self {
                    operation: CompiledMatOperation::Copy {
                        left,
                        right,
                        source: source.to_string(),
                        rhs: rhs.to_string(),
                    },
                });
            }
        }
        let target = lhs.to_ascii_uppercase();
        if !is_basic_identifier(&target) {
            return None;
        }
        if matches!(rhs.to_ascii_uppercase().as_str(), "CON" | "ZER" | "IDN") {
            return None;
        }
        Some(Self {
            operation: CompiledMatOperation::Arithmetic {
                target,
                expression: CompiledMatExpr::compile(rhs)?,
            },
        })
    }
}

impl CompiledMatExpr {
    pub(super) fn compile(source: &str) -> Option<Self> {
        Self::parse(source).ok()
    }

    pub(super) fn parse(source: &str) -> BasicResult<Self> {
        Ok(Self::from_expr(compile_expression(source)?))
    }

    fn from_expr(expr: Expr) -> Self {
        match expr {
            Expr::Number(number) => Self::Number(number),
            Expr::Var(name) if !name.starts_with("FN") => Self::Name {
                scalar: Expr::Var(name.clone()),
                name,
            },
            Expr::Unary {
                op: UnaryOp::Minus,
                expr,
            } => Self::Negate(Box::new(Self::from_expr(*expr))),
            Expr::Unary {
                op: UnaryOp::Plus,
                expr,
            } => Self::Positive(Box::new(Self::from_expr(*expr))),
            Expr::Binary { op, left, right }
                if matches!(
                    op,
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Pow
                ) =>
            {
                let op = match op {
                    BinaryOp::Add => '+',
                    BinaryOp::Sub => '-',
                    BinaryOp::Mul => '*',
                    BinaryOp::Div => '/',
                    BinaryOp::Pow => '^',
                    _ => unreachable!(),
                };
                Self::Binary {
                    left: Box::new(Self::from_expr(*left)),
                    right: Box::new(Self::from_expr(*right)),
                    op,
                }
            }
            Expr::ArrayOrCall { name, args, .. } if matches!(name.as_str(), "TRN" | "INV") => {
                Self::MatrixFunction {
                    name,
                    args: args.into_iter().map(Self::from_expr).collect(),
                }
            }
            Expr::Unary { op, expr } => Self::ScalarUnary {
                op,
                expr: Box::new(Self::from_expr(*expr)),
            },
            Expr::Binary { op, left, right } => Self::ScalarBinary {
                op,
                left: Box::new(Self::from_expr(*left)),
                right: Box::new(Self::from_expr(*right)),
            },
            scalar => Self::Scalar(scalar),
        }
    }

    #[inline]
    pub(super) fn eval(&self, interpreter: &mut Interpreter) -> BasicResult<MatExprValue> {
        match self {
            Self::Number(number) => Ok(MatExprValue::Scalar(Value::number(*number))),
            Self::Name { name, scalar } => {
                // A name can become an array after DIM, CLEAR, a function call,
                // or an alias change. Never bind its kind during compilation.
                if let Some(array) = interpreter.array_ref(name) {
                    return Ok(MatExprValue::Matrix(array.clone()));
                }
                match eval_compiled(interpreter, scalar)
                    .map_err(|error| interpreter.with_current_line(error))?
                {
                    Value::ArrayRef(name) => {
                        let array = interpreter
                            .array_ref(&name)
                            .cloned()
                            .ok_or_else(|| interpreter.err(ErrorCode::Undefined))?;
                        Ok(MatExprValue::Matrix(array))
                    }
                    value => Ok(MatExprValue::Scalar(value)),
                }
            }
            Self::Negate(inner) => {
                let value = inner.eval(interpreter)?;
                interpreter.mat_unary_minus(value)
            }
            Self::Positive(inner) => {
                let value = inner.eval(interpreter)?;
                match &value {
                    MatExprValue::Matrix(array) if array.is_string() => {
                        Err(interpreter.err(ErrorCode::ForbiddenExpression))
                    }
                    // Scalar unary plus preserves the value, including text,
                    // just as it does inside ordinary function arguments.
                    _ => Ok(value),
                }
            }
            Self::Binary { left, right, op } => {
                let left = left.eval(interpreter)?;
                // Preserve scalar conversion errors before RHS effects without
                // interpreting a matrix operand as a scalar.
                if let MatExprValue::Scalar(value) = &left {
                    if *op != '+' {
                        value
                            .as_number()
                            .map_err(|error| interpreter.with_current_line(error))?;
                    }
                }
                let right = right.eval(interpreter)?;
                interpreter.mat_binary(left, right, *op)
            }
            extended => extended.eval_extended(interpreter),
        }
    }

    // Keep ordinary fills, copies and arithmetic independent of the stack and
    // code size needed by structural functions and less common scalar nodes.
    #[inline(never)]
    fn eval_extended(&self, interpreter: &mut Interpreter) -> BasicResult<MatExprValue> {
        match self {
            Self::MatrixFunction { name, args } => {
                if args.len() != 1 {
                    return Err(interpreter.err(ErrorCode::ArgumentMismatch));
                }
                let value = args[0].eval(interpreter)?;
                // Unlike TRN, Python's INV preserves the inactive row/column
                // of its input. Save them from this already evaluated snapshot
                // before converting its active block to a compact matrix.
                let inactive_border = if name == "INV" && interpreter.mat_base == 1 {
                    match &value {
                        MatExprValue::Matrix(ArrayValue {
                            dims,
                            data: ArrayData::Number(values),
                            ..
                        }) if dims.len() == 2 => {
                            let stride = dims[1] + 1;
                            let mut border = Vec::with_capacity(stride + dims[0]);
                            border.extend_from_slice(&values[..stride]);
                            for row in 1..=dims[0] {
                                border.push(values[row * stride]);
                            }
                            Some((stride, border))
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                let mut matrix = interpreter.mat_value_to_numeric_matrix(value)?;
                if name == "INV" {
                    let mut inverse = interpreter.invert_numeric_matrix(matrix)?;
                    if let Some((stride, border)) = inactive_border {
                        let ArrayData::Number(values) = &mut inverse.data else {
                            unreachable!();
                        };
                        values[..stride].copy_from_slice(&border[..stride]);
                        for (offset, value) in border[stride..].iter().enumerate() {
                            values[(offset + 1) * stride] = *value;
                        }
                    }
                    return Ok(MatExprValue::Matrix(inverse));
                }
                let out = if matrix.rows == matrix.cols {
                    // The argument is already an owned snapshot with a compact
                    // active block. Square transposes only exchange elements;
                    // keep their exact bits and reuse this numeric buffer.
                    let size = matrix.rows;
                    for r in 0..size {
                        for c in r + 1..size {
                            matrix.data.swap(r * size + c, c * size + r);
                        }
                    }
                    matrix.data
                } else {
                    let mut out = checked_zeroed_numeric_buffer(matrix.rows, matrix.cols)
                        .map_err(|error| interpreter.with_current_line(error))?;
                    for r in 0..matrix.rows {
                        for c in 0..matrix.cols {
                            out[c * matrix.rows + r] = matrix.data[r * matrix.cols + c];
                        }
                    }
                    out
                };
                Ok(MatExprValue::Matrix(
                    ArrayValue::from_numeric_matrix(
                        "",
                        interpreter.mat_base,
                        matrix.cols,
                        matrix.rows,
                        out,
                    )
                    .map_err(|error| interpreter.with_current_line(error))?,
                ))
            }
            Self::ScalarUnary { op, expr } => {
                let value = expr.eval(interpreter)?;
                let MatExprValue::Scalar(value) = value else {
                    return Err(interpreter.err(ErrorCode::ForbiddenExpression));
                };
                let expression = Expr::Unary {
                    op: *op,
                    expr: Box::new(value_as_expr(value)),
                };
                eval_compiled(interpreter, &expression)
                    .map(MatExprValue::Scalar)
                    .map_err(|error| interpreter.with_current_line(error))
            }
            Self::ScalarBinary { op, left, right } => {
                let MatExprValue::Scalar(left) = left.eval(interpreter)? else {
                    return Err(interpreter.err(ErrorCode::ForbiddenExpression));
                };
                // Numeric scalar operators validate the left operand before
                // evaluating the right one. Preserve that error/effect order;
                // comparisons instead accept both numbers and text.
                if matches!(
                    op,
                    BinaryOp::IntDiv | BinaryOp::Mod | BinaryOp::And | BinaryOp::Xor | BinaryOp::Or
                ) {
                    left.as_number()
                        .map_err(|error| interpreter.with_current_line(error))?;
                }
                let MatExprValue::Scalar(right) = right.eval(interpreter)? else {
                    return Err(interpreter.err(ErrorCode::ForbiddenExpression));
                };
                let expression = Expr::Binary {
                    op: *op,
                    left: Box::new(value_as_expr(left)),
                    right: Box::new(value_as_expr(right)),
                };
                eval_compiled(interpreter, &expression)
                    .map(MatExprValue::Scalar)
                    .map_err(|error| interpreter.with_current_line(error))
            }
            Self::Scalar(scalar) => {
                match eval_compiled(interpreter, scalar)
                    .map_err(|error| interpreter.with_current_line(error))?
                {
                    Value::ArrayRef(name) => interpreter
                        .array_ref(&name)
                        .cloned()
                        .map(MatExprValue::Matrix)
                        .ok_or_else(|| interpreter.err(ErrorCode::Undefined)),
                    value => Ok(MatExprValue::Scalar(value)),
                }
            }
            _ => unreachable!("ordinary MAT nodes are evaluated by the inline path"),
        }
    }
}

fn value_as_expr(value: Value) -> Expr {
    match value {
        Value::Number(number) => Expr::Number(number),
        Value::Str(text) => Expr::Str(text),
        Value::ArrayRef(_) => unreachable!("MAT resolves array references before scalar operators"),
    }
}

// Slice selectors belong exclusively to the copy grammar. Keep their MAT
// expression diagnostic without teaching the scalar parser slice syntax.
pub(super) fn has_array_slice(source: &str, interpreter: &Interpreter) -> bool {
    use crate::lexer::{Lexer, Token};
    let Ok(tokens) = Lexer::new(source).tokenize() else {
        return false;
    };
    for (position, token) in tokens.iter().enumerate() {
        let Token::Ident(name) = token else {
            continue;
        };
        if !interpreter.array_exists(name)
            || !matches!(tokens.get(position + 1), Some(Token::LParen))
        {
            continue;
        }
        let mut depth = 0;
        let mut previous: Option<&Token> = None;
        for token in &tokens[position + 1..] {
            match token {
                Token::LParen => depth += 1,
                Token::RParen => {
                    if depth == 1 && matches!(previous, Some(&Token::Comma)) {
                        return true;
                    }
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                Token::Colon if depth == 1 => return true,
                Token::Comma
                    if depth == 1 && matches!(previous, Some(&Token::LParen | &Token::Comma)) =>
                {
                    return true
                }
                _ => {}
            }
            previous = Some(token);
        }
    }
    false
}

impl Interpreter {
    #[inline(never)]
    pub(super) fn execute_compiled_mat_assignment(
        &mut self,
        compiled: &CompiledMatAssignment,
    ) -> BasicResult<()> {
        let CompiledMatOperation::Arithmetic { target, expression } = &compiled.operation else {
            let CompiledMatOperation::Copy {
                left,
                right,
                source,
                rhs,
            } = &compiled.operation
            else {
                unreachable!();
            };
            // Bare destinations only select the copy grammar while their RHS
            // name is an array. Preserve the legacy error/autodimension path
            // when that binding is absent, even after this plan was cached.
            if left.selectors.is_none() && !self.array_exists(&right.name) {
                return self.execute_mat_assignment(source);
            }
            return self.execute_mat_subarray_copy_refs(left, right, rhs);
        };
        let target_key = self.array_lookup_key(target);
        // This implicit declaration precedes RHS evaluation in the ordinary
        // MAT path, including when the expression later fails.
        if !self.arrays.contains_key(target_key.as_ref()) {
            self.arrays.insert(
                target_key.to_string(),
                ArrayValue::new(target_key.as_ref(), vec![10]),
            );
        }
        let value = expression
            .eval(self)
            .map_err(|error| self.with_current_line(error))?;
        self.finish_mat_assignment(target, target_key, value)
    }

    #[inline]
    pub(super) fn finish_mat_assignment(
        &mut self,
        target: &str,
        target_key: Cow<'_, str>,
        value: MatExprValue,
    ) -> BasicResult<()> {
        match value {
            MatExprValue::Scalar(value) => {
                self.mat_fill_array(target_key.as_ref(), value)?;
                self.return_array_for_active_function(target);
                Ok(())
            }
            MatExprValue::Matrix(mut matrix) => {
                if matrix.dims.is_empty()
                    || (matrix.dims.len() > 2
                        && !self
                            .active_function_name()
                            .is_some_and(|name| name.eq_ignore_ascii_case(target)))
                {
                    return Err(self.err(ErrorCode::InvalidDimensions));
                }
                if matrix.is_string()
                    != self
                        .arrays
                        .get(target_key.as_ref())
                        .map(|array| array.is_string())
                        .unwrap_or_else(|| target.ends_with('$'))
                {
                    return Err(self.err(ErrorCode::TypeMismatch));
                }
                matrix.clear_debug_write();
                self.arrays.insert(target_key.into_owned(), matrix);
                self.return_array_for_active_function(target);
                Ok(())
            }
        }
    }
}
