use crate::error::{BasicError, BasicResult, ErrorCode};
use crate::lexer::{Lexer, Token};
use crate::value::{logical_round, round_half_away, Value};

#[derive(Debug, Clone)]
pub enum Expr {
    Number(f64),
    Str(String),
    Var(String),
    ArrayOrCall {
        name: String,
        args: Vec<Expr>,
    },
    StringIndex {
        target: Box<Expr>,
        index: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Plus,
    Minus,
    Not,
}

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,
    Mod,
    Pow,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Xor,
    Or,
}

pub trait EvalContext {
    fn get_variable(&mut self, name: &str) -> BasicResult<Value>;
    fn get_number_variable(&mut self, name: &str) -> BasicResult<f64> {
        self.get_variable(name)?.as_number()
    }
    fn get_array_value(&mut self, name: &str, indexes: &[i32]) -> BasicResult<Value>;
    fn get_array_number(&mut self, name: &str, indexes: &[i32]) -> BasicResult<f64> {
        self.get_array_value(name, indexes)?.as_number()
    }
    fn array_reference(&mut self, name: &str) -> Option<Value>;
    fn call_runtime_function(&mut self, name: &str, args: Vec<Value>) -> BasicResult<Value>;
    fn with_string_variable<R, F: FnOnce(&str) -> R>(&mut self, _name: &str, _f: F) -> Option<R> {
        None
    }
    fn string_variable_slice(
        &mut self,
        name: &str,
        start: usize,
        count: Option<usize>,
    ) -> Option<BasicResult<String>> {
        self.with_string_variable(name, |text| Ok(string_slice(text, start, count)))
    }
}

pub fn compile_expression(source: &str) -> BasicResult<Expr> {
    let tokens = Lexer::new(source).tokenize()?;
    let mut parser = Parser { tokens, pos: 0 };
    let expr = parser.parse_or()?;
    if !matches!(parser.peek(), Token::Eof) {
        return Err(BasicError::new(ErrorCode::Syntax));
    }
    Ok(expr)
}

pub fn eval_expression(ctx: &mut impl EvalContext, source: &str) -> BasicResult<Value> {
    let expr = compile_expression(source)?;
    eval_compiled(ctx, &expr)
}

pub fn eval_compiled(ctx: &mut impl EvalContext, expr: &Expr) -> BasicResult<Value> {
    expr.eval(ctx)
}

pub fn eval_compiled_number(ctx: &mut impl EvalContext, expr: &Expr) -> BasicResult<f64> {
    expr.eval_number(ctx)
}

pub fn split_arguments(source: &str) -> Vec<String> {
    crate::lexer::split_top_level(source, &[','])
}

impl Expr {
    fn eval_number(&self, ctx: &mut impl EvalContext) -> BasicResult<f64> {
        match self {
            Expr::Number(n) => Ok(*n),
            Expr::Str(_) => Err(BasicError::new(ErrorCode::TypeMismatch)),
            Expr::Var(name) => {
                if is_zero_arg_function(name) || name.starts_with("FN") {
                    ctx.call_runtime_function(name, Vec::new())?.as_number()
                } else {
                    ctx.get_number_variable(name)
                }
            }
            Expr::ArrayOrCall { name, args } => {
                if is_array_name_function(name) {
                    let values = eval_array_name_args(ctx, name, args)?;
                    return ctx
                        .call_runtime_function(name, values)
                        .and_then(|v| v.as_number());
                }
                if let Some(result) = eval_direct_numeric_function(ctx, name, args)? {
                    return Ok(result);
                }
                if is_builtin_function(name) || name.starts_with("FN") {
                    let values = eval_call_args(ctx, name, args)?;
                    ctx.call_runtime_function(name, values)?.as_number()
                } else {
                    eval_array_number(ctx, name, args)
                }
            }
            Expr::StringIndex { .. } => self.eval(ctx)?.as_number(),
            Expr::Unary { op, expr } => {
                let value = expr.eval_number(ctx)?;
                match op {
                    UnaryOp::Plus => Ok(value),
                    UnaryOp::Minus => Ok(-value),
                    UnaryOp::Not => Ok((!logical_round(value)) as f64),
                }
            }
            Expr::Binary { op, left, right } => match op {
                BinaryOp::Add => checked_number(left.eval_number(ctx)? + right.eval_number(ctx)?),
                BinaryOp::Sub => checked_number(left.eval_number(ctx)? - right.eval_number(ctx)?),
                BinaryOp::Mul => checked_number(left.eval_number(ctx)? * right.eval_number(ctx)?),
                BinaryOp::Div => {
                    let l = left.eval_number(ctx)?;
                    let r = right.eval_number(ctx)?;
                    if r == 0.0 {
                        return Err(BasicError::new(ErrorCode::DivisionByZero));
                    }
                    checked_number(l / r)
                }
                BinaryOp::IntDiv => {
                    let l = left.eval_number(ctx)?;
                    let r = right.eval_number(ctx)?;
                    if r == 0.0 {
                        return Err(BasicError::new(ErrorCode::DivisionByZero));
                    }
                    checked_number((l / r).floor())
                }
                BinaryOp::Mod => {
                    let l = left.eval_number(ctx)?;
                    let r = right.eval_number(ctx)?;
                    if r == 0.0 {
                        return Err(BasicError::new(ErrorCode::DivisionByZero));
                    }
                    checked_number(l % r)
                }
                BinaryOp::Pow => checked_power(left.eval_number(ctx)?, right.eval_number(ctx)?),
                BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Lt
                | BinaryOp::Gt
                | BinaryOp::Le
                | BinaryOp::Ge => {
                    if left.is_statically_numeric() && right.is_statically_numeric() {
                        let l = left.eval_number(ctx)?;
                        let r = right.eval_number(ctx)?;
                        let result = match op {
                            BinaryOp::Eq => l == r,
                            BinaryOp::Ne => l != r,
                            BinaryOp::Lt => l < r,
                            BinaryOp::Gt => l > r,
                            BinaryOp::Le => l <= r,
                            BinaryOp::Ge => l >= r,
                            _ => unreachable!(),
                        };
                        return Value::basic_bool(result).as_number();
                    }
                    let op_text = match op {
                        BinaryOp::Eq => "=",
                        BinaryOp::Ne => "<>",
                        BinaryOp::Lt => "<",
                        BinaryOp::Gt => ">",
                        BinaryOp::Le => "<=",
                        BinaryOp::Ge => ">=",
                        _ => unreachable!(),
                    };
                    if let Some(result) = compare_direct_string(ctx, left, right, op_text) {
                        return result.map(|value| if value { -1.0 } else { 0.0 });
                    }
                    Ok(Value::basic_bool(compare_values(
                        &left.eval(ctx)?,
                        &right.eval(ctx)?,
                        op_text,
                    )?)
                    .as_number()?)
                }
                BinaryOp::And => {
                    let l = logical_round(left.eval_number(ctx)?);
                    let r = logical_round(right.eval_number(ctx)?);
                    Ok((l & r) as f64)
                }
                BinaryOp::Xor => {
                    let l = logical_round(left.eval_number(ctx)?);
                    let r = logical_round(right.eval_number(ctx)?);
                    Ok((l ^ r) as f64)
                }
                BinaryOp::Or => {
                    let l = logical_round(left.eval_number(ctx)?);
                    let r = logical_round(right.eval_number(ctx)?);
                    Ok((l | r) as f64)
                }
            },
        }
    }

    fn is_statically_numeric(&self) -> bool {
        match self {
            Expr::Number(_) => true,
            Expr::Str(_) | Expr::StringIndex { .. } => false,
            Expr::Var(name) => !name.ends_with('$'),
            Expr::ArrayOrCall { name, args } => {
                !name.ends_with('$') && args.iter().all(Expr::is_statically_numeric)
            }
            Expr::Unary { expr, .. } => expr.is_statically_numeric(),
            Expr::Binary { op, left, right } => match op {
                BinaryOp::Add => left.is_statically_numeric() && right.is_statically_numeric(),
                BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::IntDiv
                | BinaryOp::Mod
                | BinaryOp::Pow
                | BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Lt
                | BinaryOp::Gt
                | BinaryOp::Le
                | BinaryOp::Ge
                | BinaryOp::And
                | BinaryOp::Xor
                | BinaryOp::Or => left.is_statically_numeric() && right.is_statically_numeric(),
            },
        }
    }

    fn is_statically_string(&self) -> bool {
        match self {
            Expr::Str(_) | Expr::StringIndex { .. } => true,
            Expr::Number(_) => false,
            Expr::Var(name) => name.ends_with('$'),
            Expr::ArrayOrCall { name, .. } => name.ends_with('$'),
            Expr::Unary { .. } => false,
            Expr::Binary { op, left, right } => {
                matches!(op, BinaryOp::Add)
                    && left.is_statically_string()
                    && right.is_statically_string()
            }
        }
    }

    fn eval(&self, ctx: &mut impl EvalContext) -> BasicResult<Value> {
        match self {
            Expr::Number(n) => Ok(Value::number(*n)),
            Expr::Str(s) => Ok(Value::string(s.clone())),
            Expr::Var(name) => {
                if is_zero_arg_function(name) || name.starts_with("FN") {
                    ctx.call_runtime_function(name, Vec::new())
                } else {
                    ctx.get_variable(name)
                }
            }
            Expr::ArrayOrCall { name, args } => {
                if is_array_name_function(name) {
                    let values = eval_array_name_args(ctx, name, args)?;
                    return ctx.call_runtime_function(name, values);
                }
                match name.to_ascii_uppercase().as_str() {
                    "LEN" => return eval_len_function(ctx, args),
                    "ASC" => return eval_asc_function(ctx, args),
                    "INSTR" => return eval_instr_function(ctx, args),
                    "LEFT$" => return eval_left_function(ctx, args),
                    "RIGHT$" => return eval_right_function(ctx, args),
                    "MID$" => return eval_mid_function(ctx, args),
                    _ => {}
                }
                let values = eval_call_args(ctx, name, args)?;
                if is_builtin_function(name) || name.starts_with("FN") {
                    ctx.call_runtime_function(name, values)
                } else {
                    eval_array_value(ctx, name, args)
                }
            }
            Expr::StringIndex { target, index } => {
                let n = index.eval_number(ctx)?;
                if n.fract() != 0.0 || n < 1.0 {
                    return Err(BasicError::new(ErrorCode::InvalidIndex));
                }
                let idx = n as usize;
                if let Some(name) = direct_string_variable_name(target.as_ref()) {
                    if let Some(result) = ctx.with_string_variable(name, |text| {
                        string_char_at(text, idx).ok_or_else(|| {
                            if idx == 0 {
                                BasicError::new(ErrorCode::InvalidIndex)
                            } else {
                                BasicError::new(ErrorCode::IndexOutOfRange)
                            }
                        })
                    }) {
                        return result.map(Value::string);
                    }
                }
                let value = target.eval(ctx)?;
                let Value::Str(text) = value else {
                    return Err(BasicError::new(ErrorCode::TypeMismatch));
                };
                string_char_at(&text, idx)
                    .map(Value::string)
                    .ok_or_else(|| BasicError::new(ErrorCode::IndexOutOfRange))
            }
            Expr::Unary { op, expr } => {
                let value = expr.eval(ctx)?;
                match op {
                    UnaryOp::Plus => Ok(value),
                    UnaryOp::Minus => Ok(Value::number(-value.as_number()?)),
                    UnaryOp::Not => Ok(Value::number((!logical_round(value.as_number()?)) as f64)),
                }
            }
            Expr::Binary { op, left, right } => match op {
                BinaryOp::Add => {
                    if left.is_statically_string() && right.is_statically_string() {
                        return eval_string_concat(ctx, self).map(Value::string);
                    }
                    add_values(left.eval(ctx)?, right.eval(ctx)?)
                }
                BinaryOp::Sub => Ok(Value::number(checked_number(
                    left.eval(ctx)?.as_number()? - right.eval(ctx)?.as_number()?,
                )?)),
                BinaryOp::Mul => Ok(Value::number(checked_number(
                    left.eval(ctx)?.as_number()? * right.eval(ctx)?.as_number()?,
                )?)),
                BinaryOp::Div => {
                    let l = left.eval(ctx)?.as_number()?;
                    let r = right.eval(ctx)?.as_number()?;
                    if r == 0.0 {
                        return Err(BasicError::new(ErrorCode::DivisionByZero));
                    }
                    Ok(Value::number(checked_number(l / r)?))
                }
                BinaryOp::IntDiv => {
                    let l = left.eval(ctx)?.as_number()?;
                    let r = right.eval(ctx)?.as_number()?;
                    if r == 0.0 {
                        return Err(BasicError::new(ErrorCode::DivisionByZero));
                    }
                    Ok(Value::number(checked_number((l / r).floor())?))
                }
                BinaryOp::Mod => {
                    let l = left.eval(ctx)?.as_number()?;
                    let r = right.eval(ctx)?.as_number()?;
                    if r == 0.0 {
                        return Err(BasicError::new(ErrorCode::DivisionByZero));
                    }
                    Ok(Value::number(checked_number(l % r)?))
                }
                BinaryOp::Pow => Ok(Value::number(checked_power(
                    left.eval(ctx)?.as_number()?,
                    right.eval(ctx)?.as_number()?,
                )?)),
                BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Lt
                | BinaryOp::Gt
                | BinaryOp::Le
                | BinaryOp::Ge => {
                    let op_text = match op {
                        BinaryOp::Eq => "=",
                        BinaryOp::Ne => "<>",
                        BinaryOp::Lt => "<",
                        BinaryOp::Gt => ">",
                        BinaryOp::Le => "<=",
                        BinaryOp::Ge => ">=",
                        _ => unreachable!(),
                    };
                    if let Some(result) = compare_direct_string(ctx, left, right, op_text) {
                        return result.map(Value::basic_bool);
                    }
                    Ok(Value::basic_bool(compare_values(
                        &left.eval(ctx)?,
                        &right.eval(ctx)?,
                        op_text,
                    )?))
                }
                BinaryOp::And => {
                    let l = logical_round(left.eval(ctx)?.as_number()?);
                    let r = logical_round(right.eval(ctx)?.as_number()?);
                    Ok(Value::number((l & r) as f64))
                }
                BinaryOp::Xor => {
                    let l = logical_round(left.eval(ctx)?.as_number()?);
                    let r = logical_round(right.eval(ctx)?.as_number()?);
                    Ok(Value::number((l ^ r) as f64))
                }
                BinaryOp::Or => {
                    let l = logical_round(left.eval(ctx)?.as_number()?);
                    let r = logical_round(right.eval(ctx)?.as_number()?);
                    Ok(Value::number((l | r) as f64))
                }
            },
        }
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn bump(&mut self) -> Token {
        let token = self.peek().clone();
        if !matches!(token, Token::Eof) {
            self.pos += 1;
        }
        token
    }

    fn consume_op(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Token::Op(s) if s.eq_ignore_ascii_case(op)) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn consume_ident(&mut self, ident: &str) -> bool {
        if matches!(self.peek(), Token::Ident(s) if s.eq_ignore_ascii_case(ident)) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn consume_lparen(&mut self) -> bool {
        if matches!(self.peek(), Token::LParen) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect_rparen(&mut self) -> BasicResult<()> {
        if matches!(self.bump(), Token::RParen) {
            Ok(())
        } else {
            Err(BasicError::new(ErrorCode::Syntax))
        }
    }

    fn consume_lbracket(&mut self) -> bool {
        if matches!(self.peek(), Token::LBracket) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect_rbracket(&mut self) -> BasicResult<()> {
        if matches!(self.bump(), Token::RBracket) {
            Ok(())
        } else {
            Err(BasicError::new(ErrorCode::Syntax))
        }
    }

    fn parse_or(&mut self) -> BasicResult<Expr> {
        let mut left = self.parse_xor()?;
        while self.consume_ident("OR") {
            left = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(self.parse_xor()?),
            };
        }
        Ok(left)
    }

    fn parse_xor(&mut self) -> BasicResult<Expr> {
        let mut left = self.parse_and()?;
        while self.consume_ident("XOR") {
            left = Expr::Binary {
                op: BinaryOp::Xor,
                left: Box::new(left),
                right: Box::new(self.parse_and()?),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> BasicResult<Expr> {
        let mut left = self.parse_comparison()?;
        while self.consume_ident("AND") {
            left = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(self.parse_comparison()?),
            };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> BasicResult<Expr> {
        let mut left = self.parse_add()?;
        loop {
            let op = match self.peek() {
                Token::Op(op) if matches!(op.as_str(), "=" | "<>" | "<" | ">" | "<=" | ">=") => {
                    match op.as_str() {
                        "=" => BinaryOp::Eq,
                        "<>" => BinaryOp::Ne,
                        "<" => BinaryOp::Lt,
                        ">" => BinaryOp::Gt,
                        "<=" => BinaryOp::Le,
                        ">=" => BinaryOp::Ge,
                        _ => unreachable!(),
                    }
                }
                _ => break,
            };
            self.bump();
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(self.parse_add()?),
            };
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> BasicResult<Expr> {
        let mut left = self.parse_mul()?;
        loop {
            if self.consume_op("+") {
                left = Expr::Binary {
                    op: BinaryOp::Add,
                    left: Box::new(left),
                    right: Box::new(self.parse_mul()?),
                };
            } else if self.consume_op("-") {
                left = Expr::Binary {
                    op: BinaryOp::Sub,
                    left: Box::new(left),
                    right: Box::new(self.parse_mul()?),
                };
            } else {
                return Ok(left);
            }
        }
    }

    fn parse_mul(&mut self) -> BasicResult<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            if self.consume_op("*") {
                left = Expr::Binary {
                    op: BinaryOp::Mul,
                    left: Box::new(left),
                    right: Box::new(self.parse_unary()?),
                };
            } else if self.consume_op("/") {
                left = Expr::Binary {
                    op: BinaryOp::Div,
                    left: Box::new(left),
                    right: Box::new(self.parse_unary()?),
                };
            } else if self.consume_op("\\") {
                left = Expr::Binary {
                    op: BinaryOp::IntDiv,
                    left: Box::new(left),
                    right: Box::new(self.parse_unary()?),
                };
            } else if self.consume_ident("MOD") {
                left = Expr::Binary {
                    op: BinaryOp::Mod,
                    left: Box::new(left),
                    right: Box::new(self.parse_unary()?),
                };
            } else {
                return Ok(left);
            }
        }
    }

    fn parse_pow(&mut self) -> BasicResult<Expr> {
        let mut left = self.parse_primary()?;
        while self.consume_op("^") {
            left = Expr::Binary {
                op: BinaryOp::Pow,
                left: Box::new(left),
                right: Box::new(self.parse_power_operand()?),
            };
        }
        Ok(left)
    }

    fn parse_power_operand(&mut self) -> BasicResult<Expr> {
        if self.consume_op("+") {
            return Ok(Expr::Unary {
                op: UnaryOp::Plus,
                expr: Box::new(self.parse_power_operand()?),
            });
        }
        if self.consume_op("-") {
            return Ok(Expr::Unary {
                op: UnaryOp::Minus,
                expr: Box::new(self.parse_power_operand()?),
            });
        }
        if self.consume_ident("NOT") {
            return Ok(Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(self.parse_power_operand()?),
            });
        }
        self.parse_primary()
    }

    fn parse_unary(&mut self) -> BasicResult<Expr> {
        if self.consume_op("+") {
            return Ok(Expr::Unary {
                op: UnaryOp::Plus,
                expr: Box::new(self.parse_unary()?),
            });
        }
        if self.consume_op("-") {
            return Ok(Expr::Unary {
                op: UnaryOp::Minus,
                expr: Box::new(self.parse_unary()?),
            });
        }
        if self.consume_ident("NOT") {
            return Ok(Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(self.parse_unary()?),
            });
        }
        self.parse_pow()
    }

    fn parse_primary(&mut self) -> BasicResult<Expr> {
        match self.bump() {
            Token::Number(n) => Ok(Expr::Number(n)),
            Token::Str(s) => Ok(Expr::Str(s)),
            Token::Ident(name) => self.parse_ident_value(name),
            Token::LParen => {
                let value = self.parse_or()?;
                self.expect_rparen()?;
                Ok(value)
            }
            _ => Err(BasicError::new(ErrorCode::Syntax)),
        }
    }

    fn parse_ident_value(&mut self, name: String) -> BasicResult<Expr> {
        let mut expr = if self.consume_lparen() {
            let mut args = Vec::new();
            if !matches!(self.peek(), Token::RParen) {
                loop {
                    args.push(self.parse_or()?);
                    if !matches!(self.peek(), Token::Comma) {
                        break;
                    }
                    self.bump();
                }
            }
            self.expect_rparen()?;
            Expr::ArrayOrCall { name, args }
        } else {
            Expr::Var(name)
        };
        while self.consume_lbracket() {
            let index = self.parse_or()?;
            self.expect_rbracket()?;
            expr = Expr::StringIndex {
                target: Box::new(expr),
                index: Box::new(index),
            };
        }
        Ok(expr)
    }
}

fn add_values(left: Value, right: Value) -> BasicResult<Value> {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => Ok(Value::number(checked_number(a + b)?)),
        (Value::Str(mut a), Value::Str(b)) => {
            a.push_str(&b);
            Ok(Value::string(a))
        }
        _ => Err(BasicError::new(ErrorCode::TypeMismatch)),
    }
}

fn eval_string_concat(ctx: &mut impl EvalContext, expr: &Expr) -> BasicResult<String> {
    let mut out = String::new();
    if let Some(capacity) = string_concat_capacity(ctx, expr) {
        out.reserve(capacity);
    }
    eval_string_concat_into(ctx, expr, &mut out)?;
    Ok(out)
}

fn eval_string_concat_into(
    ctx: &mut impl EvalContext,
    expr: &Expr,
    out: &mut String,
) -> BasicResult<()> {
    match expr {
        Expr::Binary {
            op: BinaryOp::Add,
            left,
            right,
        } if left.is_statically_string() && right.is_statically_string() => {
            eval_string_concat_into(ctx, left, out)?;
            eval_string_concat_into(ctx, right, out)
        }
        Expr::Str(text) => {
            out.push_str(text);
            Ok(())
        }
        Expr::Var(_) => {
            if let Some(()) = direct_string_variable_name(expr)
                .and_then(|name| ctx.with_string_variable(name, |text| out.push_str(text)))
            {
                return Ok(());
            }
            out.push_str(&expr.eval(ctx)?.into_string()?);
            Ok(())
        }
        _ => {
            out.push_str(&expr.eval(ctx)?.into_string()?);
            Ok(())
        }
    }
}

fn string_concat_capacity(ctx: &mut impl EvalContext, expr: &Expr) -> Option<usize> {
    match expr {
        Expr::Binary {
            op: BinaryOp::Add,
            left,
            right,
        } if left.is_statically_string() && right.is_statically_string() => {
            Some(string_concat_capacity(ctx, left)? + string_concat_capacity(ctx, right)?)
        }
        Expr::Str(text) => Some(text.len()),
        Expr::Var(_) => {
            let name = direct_string_variable_name(expr)?;
            ctx.with_string_variable(name, |text| text.len())
        }
        _ => None,
    }
}

fn checked_number(value: f64) -> BasicResult<f64> {
    if value.is_nan() {
        return Err(BasicError::new(ErrorCode::InvalidValue));
    }
    if !value.is_finite() {
        return Err(BasicError::new(ErrorCode::Overflow));
    }
    Ok(value)
}

fn checked_power(base: f64, exponent: f64) -> BasicResult<f64> {
    checked_number(base.powf(exponent))
}

fn compare_values(left: &Value, right: &Value, op: &str) -> BasicResult<bool> {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => Ok(match op {
            "=" => a == b,
            "<>" => a != b,
            "<" => a < b,
            ">" => a > b,
            "<=" => a <= b,
            ">=" => a >= b,
            _ => return Err(BasicError::new(ErrorCode::Syntax)),
        }),
        (Value::Str(a), Value::Str(b)) => Ok(match op {
            "=" => a == b,
            "<>" => a != b,
            "<" => a < b,
            ">" => a > b,
            "<=" => a <= b,
            ">=" => a >= b,
            _ => return Err(BasicError::new(ErrorCode::Syntax)),
        }),
        _ => Err(BasicError::new(ErrorCode::TypeMismatch)),
    }
}

fn compare_direct_string(
    ctx: &mut impl EvalContext,
    left: &Expr,
    right: &Expr,
    op: &str,
) -> Option<BasicResult<bool>> {
    if let (Some(name), Expr::Str(rhs)) = (direct_string_variable_name(left), right) {
        return ctx.with_string_variable(name, |lhs| Ok(compare_strings(lhs, rhs, op)));
    }
    if let (Expr::Str(lhs), Some(name)) = (left, direct_string_variable_name(right)) {
        return ctx.with_string_variable(name, |rhs| Ok(compare_strings(lhs, rhs, op)));
    }
    None
}

fn compare_strings(left: &str, right: &str, op: &str) -> bool {
    match op {
        "=" => left == right,
        "<>" => left != right,
        "<" => left < right,
        ">" => left > right,
        "<=" => left <= right,
        ">=" => left >= right,
        _ => false,
    }
}

fn parse_basic_val(text: &str) -> f64 {
    let text = text.trim_start();
    let mut sign = 1.0;
    let mut rest = text;
    if let Some(next) = rest.strip_prefix('-') {
        sign = -1.0;
        rest = next;
    } else if let Some(next) = rest.strip_prefix('+') {
        rest = next;
    }
    let lower = rest.to_ascii_lowercase();
    if let Some(digits) = lower.strip_prefix("&h") {
        let raw: String = digits
            .chars()
            .take_while(|ch| ch.is_ascii_hexdigit())
            .collect();
        return sign * i64::from_str_radix(&raw, 16).unwrap_or(0) as f64;
    }
    if let Some(digits) = lower.strip_prefix("&x") {
        let raw: String = digits
            .chars()
            .take_while(|ch| matches!(ch, '0' | '1'))
            .collect();
        return sign * i64::from_str_radix(&raw, 2).unwrap_or(0) as f64;
    }
    let mut end = 0usize;
    let mut saw_digit = false;
    let chars: Vec<(usize, char)> = rest.char_indices().collect();
    let mut i = 0usize;
    while i < chars.len() && chars[i].1.is_ascii_digit() {
        saw_digit = true;
        end = chars[i].0 + 1;
        i += 1;
    }
    if i < chars.len() && chars[i].1 == '.' {
        end = chars[i].0 + 1;
        i += 1;
        while i < chars.len() && chars[i].1.is_ascii_digit() {
            saw_digit = true;
            end = chars[i].0 + 1;
            i += 1;
        }
    }
    if saw_digit && i < chars.len() && matches!(chars[i].1, 'e' | 'E') {
        let exp_start = i;
        i += 1;
        if i < chars.len() && matches!(chars[i].1, '+' | '-') {
            i += 1;
        }
        let before_digits = i;
        while i < chars.len() && chars[i].1.is_ascii_digit() {
            end = chars[i].0 + 1;
            i += 1;
        }
        if i == before_digits {
            end = chars[exp_start - 1].0 + chars[exp_start - 1].1.len_utf8();
        }
    }
    if !saw_digit {
        return 0.0;
    }
    sign * rest[..end].parse::<f64>().unwrap_or(0.0)
}

fn format_radix_string(value: i64, width: Option<usize>, radix: u32) -> String {
    let bits = if radix == 2 { 8 } else { 16 };
    let unsigned = if value < 0 {
        ((1i64 << bits) + value) as u64
    } else {
        value as u64
    };
    let mut text = if radix == 2 {
        format!("{unsigned:b}")
    } else {
        format!("{unsigned:X}")
    };
    if value < 0 && radix == 2 && text.len() > 8 {
        text = text[text.len() - 8..].to_string();
    }
    if let Some(width) = width {
        if text.len() < width {
            text = format!("{}{}", "0".repeat(width - text.len()), text);
        } else if text.len() > width {
            text = text[text.len() - width..].to_string();
        }
    }
    text
}

fn format_dec_string(value: f64, fmt: &str) -> String {
    if let Some(dot) = fmt.find('.') {
        let frac_digits = fmt[dot + 1..]
            .chars()
            .filter(|ch| matches!(ch, '0' | '#'))
            .count();
        return format!("{:.*}", frac_digits, value);
    }
    let value = value as i64;
    let width = fmt.chars().filter(|ch| matches!(ch, '0' | '#')).count();
    let suffix: String = fmt.chars().filter(|ch| !matches!(ch, '0' | '#')).collect();
    let negative = value < 0;
    let sign = if negative { "-" } else { "" };
    let digits = value.abs().to_string();
    let width = if negative {
        width.saturating_sub(1)
    } else {
        width
    };
    let padded = if digits.len() < width {
        format!("{}{}", "0".repeat(width - digits.len()), digits)
    } else {
        digits
    };
    format!("{sign}{padded}{suffix}")
}

fn eval_array_name_args(
    ctx: &mut impl EvalContext,
    function: &str,
    args: &[Expr],
) -> BasicResult<Vec<Value>> {
    if function.eq_ignore_ascii_case("DOT") {
        if args.len() != 2 {
            return Err(BasicError::new(ErrorCode::ArgumentMismatch));
        }
        let left = match &args[0] {
            Expr::Var(name) => name.clone(),
            _ => return Err(BasicError::new(ErrorCode::Syntax)),
        };
        let right = match &args[1] {
            Expr::Var(name) => name.clone(),
            _ => return Err(BasicError::new(ErrorCode::Syntax)),
        };
        return Ok(vec![Value::string(left), Value::string(right)]);
    }
    let max_args = if matches!(
        function.to_ascii_uppercase().as_str(),
        "LBOUND" | "UBOUND" | "LBND" | "UBND"
    ) {
        2
    } else {
        1
    };
    if args.is_empty() || args.len() > max_args {
        return Err(BasicError::new(ErrorCode::ArgumentMismatch));
    }
    let array_name = match &args[0] {
        Expr::Var(name) => name.clone(),
        _ => return Err(BasicError::new(ErrorCode::Syntax)),
    };
    let mut values = vec![Value::string(array_name)];
    if let Some(dimension) = args.get(1) {
        values.push(dimension.eval(ctx)?);
    }
    Ok(values)
}

fn eval_array_value(ctx: &mut impl EvalContext, name: &str, args: &[Expr]) -> BasicResult<Value> {
    match args.len() {
        1 => {
            let index = eval_index(ctx, &args[0])?;
            ctx.get_array_value(name, &[index])
        }
        2 => {
            let indexes = [eval_index(ctx, &args[0])?, eval_index(ctx, &args[1])?];
            ctx.get_array_value(name, &indexes)
        }
        _ => {
            let indexes = args
                .iter()
                .map(|arg| eval_index(ctx, arg))
                .collect::<BasicResult<Vec<_>>>()?;
            ctx.get_array_value(name, &indexes)
        }
    }
}

fn eval_array_number(ctx: &mut impl EvalContext, name: &str, args: &[Expr]) -> BasicResult<f64> {
    match args.len() {
        1 => {
            let index = eval_index(ctx, &args[0])?;
            ctx.get_array_number(name, &[index])
        }
        2 => {
            let indexes = [eval_index(ctx, &args[0])?, eval_index(ctx, &args[1])?];
            ctx.get_array_number(name, &indexes)
        }
        _ => {
            let indexes = args
                .iter()
                .map(|arg| eval_index(ctx, arg))
                .collect::<BasicResult<Vec<_>>>()?;
            ctx.get_array_number(name, &indexes)
        }
    }
}

fn eval_direct_numeric_function(
    ctx: &mut impl EvalContext,
    name: &str,
    args: &[Expr],
) -> BasicResult<Option<f64>> {
    let upper_name;
    let name = if name.bytes().any(|byte| byte.is_ascii_lowercase()) {
        upper_name = name.to_ascii_uppercase();
        upper_name.as_str()
    } else {
        name
    };
    let result = match name {
        "ABS" if args.len() == 1 => args[0].eval_number(ctx)?.abs(),
        "INT" if args.len() == 1 => args[0].eval_number(ctx)?.floor(),
        "FIX" if args.len() == 1 => {
            let x = args[0].eval_number(ctx)?;
            if x >= 0.0 {
                x.floor()
            } else {
                x.ceil()
            }
        }
        "SGN" if args.len() == 1 => {
            let x = args[0].eval_number(ctx)?;
            if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                0.0
            }
        }
        "FRAC" if args.len() == 1 => args[0].eval_number(ctx)?.fract(),
        "SQR" if args.len() == 1 => {
            let x = args[0].eval_number(ctx)?;
            if x < 0.0 {
                return Err(BasicError::new(ErrorCode::InvalidArgument));
            }
            checked_number(x.sqrt())?
        }
        "LOG" if args.len() == 1 => {
            let x = args[0].eval_number(ctx)?;
            if x <= 0.0 {
                return Err(BasicError::new(ErrorCode::InvalidArgument));
            }
            x.ln()
        }
        "LOG10" if args.len() == 1 => {
            let x = args[0].eval_number(ctx)?;
            if x <= 0.0 {
                return Err(BasicError::new(ErrorCode::InvalidArgument));
            }
            x.log10()
        }
        "EXP" if args.len() == 1 => checked_number(args[0].eval_number(ctx)?.exp())?,
        "ROUND" if args.len() == 1 => round_half_away(args[0].eval_number(ctx)?, 0),
        "ROUND" if args.len() == 2 => {
            round_half_away(args[0].eval_number(ctx)?, args[1].eval_number(ctx)? as i32)
        }
        "MIN" if !args.is_empty() => {
            let mut min = args[0].eval_number(ctx)?;
            for arg in &args[1..] {
                min = min.min(arg.eval_number(ctx)?);
            }
            min
        }
        "MAX" if !args.is_empty() => {
            let mut max = args[0].eval_number(ctx)?;
            for arg in &args[1..] {
                max = max.max(arg.eval_number(ctx)?);
            }
            max
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}

fn eval_index(ctx: &mut impl EvalContext, expr: &Expr) -> BasicResult<i32> {
    let n = expr.eval_number(ctx)?;
    if n.fract() != 0.0 {
        return Err(BasicError::new(ErrorCode::InvalidIndex));
    }
    Ok(n as i32)
}

fn eval_call_args(
    ctx: &mut impl EvalContext,
    function: &str,
    args: &[Expr],
) -> BasicResult<Vec<Value>> {
    args.iter()
        .map(|arg| {
            if function.starts_with("FN") {
                if let Expr::Var(name) = arg {
                    if let Some(value) = ctx.array_reference(name) {
                        return Ok(value);
                    }
                }
            }
            arg.eval(ctx).map_err(|err| {
                if function.eq_ignore_ascii_case("ABS") && err.code == ErrorCode::InvalidValue {
                    BasicError::new(ErrorCode::TypeMismatch)
                } else {
                    err
                }
            })
        })
        .collect()
}

fn eval_mid_function(ctx: &mut impl EvalContext, args: &[Expr]) -> BasicResult<Value> {
    if !(2..=3).contains(&args.len()) {
        return Err(BasicError::new(ErrorCode::ArgumentMismatch));
    }
    let start = args[1].eval_number(ctx)? as isize - 1;
    let start = start.max(0) as usize;
    let count = args
        .get(2)
        .map(|arg| arg.eval_number(ctx).map(|value| value as usize))
        .transpose()?;

    if let Some(name) = direct_string_variable_name(&args[0]) {
        if let Some(result) = ctx.string_variable_slice(name, start, count) {
            return result.map(Value::string);
        }
    }

    let text = args[0].eval(ctx)?.into_string()?;
    Ok(Value::string(string_slice(&text, start, count)))
}

fn eval_len_function(ctx: &mut impl EvalContext, args: &[Expr]) -> BasicResult<Value> {
    if args.len() != 1 {
        return Err(BasicError::new(ErrorCode::ArgumentMismatch));
    }
    if let Some(name) = direct_string_variable_name(&args[0]) {
        if let Some(len) = ctx.with_string_variable(name, string_len) {
            return Ok(Value::number(len as f64));
        }
    }
    let text = args[0].eval(ctx)?.into_string()?;
    Ok(Value::number(string_len(&text) as f64))
}

fn eval_asc_function(ctx: &mut impl EvalContext, args: &[Expr]) -> BasicResult<Value> {
    if args.len() != 1 {
        return Err(BasicError::new(ErrorCode::ArgumentMismatch));
    }
    if let Some(name) = direct_string_variable_name(&args[0]) {
        if let Some(code) = ctx.with_string_variable(name, first_char_code) {
            return Ok(Value::number(code? as f64));
        }
    }
    let text = args[0].eval(ctx)?.into_string()?;
    Ok(Value::number(first_char_code(&text)? as f64))
}

fn eval_instr_function(ctx: &mut impl EvalContext, args: &[Expr]) -> BasicResult<Value> {
    if args.len() != 2 && args.len() != 3 {
        return Err(BasicError::new(ErrorCode::ArgumentMismatch));
    }
    let (start, text_arg, needle_arg) = if args.len() == 2 {
        (1usize, &args[0], &args[1])
    } else {
        let start = args[0].eval_number(ctx)? as isize;
        if start < 1 {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        (start as usize, &args[1], &args[2])
    };
    let needle = needle_arg.eval(ctx)?.into_string()?;
    if let Some(name) = direct_string_variable_name(text_arg) {
        if let Some(pos) =
            ctx.with_string_variable(name, |text| instr_position(text, &needle, start))
        {
            return Ok(Value::number(pos as f64));
        }
    }
    let text = text_arg.eval(ctx)?.into_string()?;
    Ok(Value::number(instr_position(&text, &needle, start) as f64))
}

fn eval_left_function(ctx: &mut impl EvalContext, args: &[Expr]) -> BasicResult<Value> {
    if args.len() != 2 {
        return Err(BasicError::new(ErrorCode::ArgumentMismatch));
    }
    let count = args[1].eval_number(ctx)?.max(0.0) as usize;
    if let Some(name) = direct_string_variable_name(&args[0]) {
        if let Some(result) = ctx.string_variable_slice(name, 0, Some(count)) {
            return result.map(Value::string);
        }
    }
    let text = args[0].eval(ctx)?.into_string()?;
    Ok(Value::string(string_slice(&text, 0, Some(count))))
}

fn eval_right_function(ctx: &mut impl EvalContext, args: &[Expr]) -> BasicResult<Value> {
    if args.len() != 2 {
        return Err(BasicError::new(ErrorCode::ArgumentMismatch));
    }
    let count = args[1].eval_number(ctx)?.max(0.0) as usize;
    if let Some(name) = direct_string_variable_name(&args[0]) {
        if let Some(text) = ctx.with_string_variable(name, |text| right_string(text, count)) {
            return Ok(Value::string(text));
        }
    }
    let text = args[0].eval(ctx)?.into_string()?;
    Ok(Value::string(right_string(&text, count)))
}

fn direct_string_variable_name(expr: &Expr) -> Option<&str> {
    let Expr::Var(name) = expr else {
        return None;
    };
    let upper = name.to_ascii_uppercase();
    if upper.ends_with('$') && !upper.starts_with("FN") && !is_zero_arg_function(&upper) {
        Some(name)
    } else {
        None
    }
}

fn string_len(text: &str) -> usize {
    if text.is_ascii() {
        text.len()
    } else {
        text.chars().count()
    }
}

fn first_char_code(text: &str) -> BasicResult<u32> {
    text.chars()
        .next()
        .map(|ch| ch as u32)
        .ok_or_else(|| BasicError::new(ErrorCode::InvalidValue))
}

fn string_slice(text: &str, start: usize, count: Option<usize>) -> String {
    if text.is_ascii() {
        if start >= text.len() {
            return String::new();
        }
        let end = count
            .map_or(text.len(), |count| start.saturating_add(count))
            .min(text.len());
        return text[start..end].to_string();
    }

    let Some(byte_start) = byte_index_for_char_offset(text, start) else {
        return String::new();
    };
    let byte_end = count
        .and_then(|count| byte_index_for_char_offset(text, start.saturating_add(count)))
        .unwrap_or(text.len());
    text[byte_start..byte_end].to_string()
}

fn right_string(text: &str, count: usize) -> String {
    if text.is_ascii() {
        let start = text.len().saturating_sub(count);
        text[start..].to_string()
    } else {
        let len = string_len(text);
        string_slice(text, len.saturating_sub(count), None)
    }
}

fn byte_index_for_char_offset(text: &str, offset: usize) -> Option<usize> {
    if offset == 0 {
        return Some(0);
    }
    text.char_indices()
        .nth(offset)
        .map(|(idx, _)| idx)
        .or_else(|| (offset == string_len(text)).then_some(text.len()))
}

fn byte_index_for_char_start(text: &str, start_1_based: usize) -> usize {
    if start_1_based == 0 {
        return 0;
    }
    if text.is_ascii() {
        return (start_1_based - 1).min(text.len());
    }
    byte_index_for_char_offset(text, start_1_based - 1).unwrap_or(text.len())
}

fn string_char_at(text: &str, index_1_based: usize) -> Option<String> {
    if index_1_based == 0 {
        return None;
    }
    if text.is_ascii() {
        return text
            .as_bytes()
            .get(index_1_based - 1)
            .map(|byte| (*byte as char).to_string());
    }
    text.chars().nth(index_1_based - 1).map(|ch| ch.to_string())
}

fn instr_position(text: &str, needle: &str, start_1_based: usize) -> usize {
    let byte_start = byte_index_for_char_start(text, start_1_based);
    let Some(found) = text[byte_start..].find(needle) else {
        return 0;
    };
    let byte_pos = byte_start + found;
    if text.is_ascii() {
        byte_pos + 1
    } else {
        text[..byte_pos].chars().count() + 1
    }
}

pub fn call_pure_function(name: &str, args: Vec<Value>) -> BasicResult<Option<Value>> {
    let name = name.to_ascii_uppercase();
    let n = |v: &Value| v.as_number();
    let s = |v: Value| v.into_string();
    let result = match name.as_str() {
        "ABS" if args.len() == 1 => Value::number(n(&args[0])?.abs()),
        "INT" if args.len() == 1 => Value::number(n(&args[0])?.floor()),
        "FIX" if args.len() == 1 => {
            let x = n(&args[0])?;
            Value::number(if x >= 0.0 { x.floor() } else { x.ceil() })
        }
        "SGN" if args.len() == 1 => {
            let x = n(&args[0])?;
            Value::number(if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                0.0
            })
        }
        "FRAC" if args.len() == 1 => {
            let x = n(&args[0])?;
            Value::number(x.fract())
        }
        "SQR" if args.len() == 1 => {
            let x = n(&args[0])?;
            if x < 0.0 {
                return Err(BasicError::new(ErrorCode::InvalidArgument));
            }
            Value::number(checked_number(x.sqrt())?)
        }
        "LOG" if args.len() == 1 => {
            let x = n(&args[0])?;
            if x <= 0.0 {
                return Err(BasicError::new(ErrorCode::InvalidArgument));
            }
            Value::number(x.ln())
        }
        "LOG10" if args.len() == 1 => {
            let x = n(&args[0])?;
            if x <= 0.0 {
                return Err(BasicError::new(ErrorCode::InvalidArgument));
            }
            Value::number(x.log10())
        }
        "EXP" if args.len() == 1 => Value::number(checked_number(n(&args[0])?.exp())?),
        "SIN" if args.len() == 1 => Value::number(n(&args[0])?.sin()),
        "COS" if args.len() == 1 => Value::number(n(&args[0])?.cos()),
        "TAN" if args.len() == 1 => Value::number(n(&args[0])?.tan()),
        "ASN" if args.len() == 1 => {
            let x = n(&args[0])?;
            if !(-1.0..=1.0).contains(&x) {
                return Err(BasicError::new(ErrorCode::InvalidArgument));
            }
            Value::number(x.asin())
        }
        "ACS" if args.len() == 1 => {
            let x = n(&args[0])?;
            if !(-1.0..=1.0).contains(&x) {
                return Err(BasicError::new(ErrorCode::InvalidArgument));
            }
            Value::number(x.acos())
        }
        "ATN" if args.len() == 1 => Value::number(n(&args[0])?.atan()),
        "COT" if args.len() == 1 => {
            let t = n(&args[0])?.tan();
            if t == 0.0 {
                return Err(BasicError::new(ErrorCode::InvalidArgument));
            }
            Value::number(1.0 / t)
        }
        "PI" if args.is_empty() => Value::number(std::f64::consts::PI),
        "ROUND" if args.len() == 1 => Value::number(round_half_away(n(&args[0])?, 0)),
        "ROUND" if args.len() == 2 => {
            Value::number(round_half_away(n(&args[0])?, n(&args[1])? as i32))
        }
        "MIN" if !args.is_empty() => {
            let mut min = n(&args[0])?;
            for arg in &args[1..] {
                min = min.min(n(arg)?);
            }
            Value::number(min)
        }
        "MAX" if !args.is_empty() => {
            let mut max = n(&args[0])?;
            for arg in &args[1..] {
                max = max.max(n(arg)?);
            }
            Value::number(max)
        }
        "LEN" if args.len() == 1 => {
            Value::number(string_len(&s(args.into_iter().next().unwrap())?) as f64)
        }
        "ASC" if args.len() == 1 => {
            let text = s(args.into_iter().next().unwrap())?;
            Value::number(first_char_code(&text)? as f64)
        }
        "VAL" if args.len() == 1 => {
            let text = s(args.into_iter().next().unwrap())?;
            Value::number(parse_basic_val(&text))
        }
        "INSTR" if args.len() == 2 || args.len() == 3 => {
            let mut it = args.into_iter();
            let (start, text, needle) = if it.len() == 2 {
                (1usize, s(it.next().unwrap())?, s(it.next().unwrap())?)
            } else {
                let start = n(&it.next().unwrap())? as isize;
                if start < 1 {
                    return Err(BasicError::new(ErrorCode::InvalidArgument));
                }
                (
                    start as usize,
                    s(it.next().unwrap())?,
                    s(it.next().unwrap())?,
                )
            };
            Value::number(instr_position(&text, &needle, start) as f64)
        }
        "LEFT$" if args.len() == 2 => {
            let mut it = args.into_iter();
            let text = s(it.next().unwrap())?;
            let count = n(&it.next().unwrap())?.max(0.0) as usize;
            Value::string(string_slice(&text, 0, Some(count)))
        }
        "RIGHT$" if args.len() == 2 => {
            let mut it = args.into_iter();
            let text = s(it.next().unwrap())?;
            let count = n(&it.next().unwrap())?.max(0.0) as usize;
            Value::string(right_string(&text, count))
        }
        "MID$" if args.len() == 2 || args.len() == 3 => {
            let mut it = args.into_iter();
            let text = s(it.next().unwrap())?;
            let start = n(&it.next().unwrap())? as isize - 1;
            let count = it
                .next()
                .map(|v| v.as_number())
                .transpose()?
                .map(|v| v as usize);
            let start = start.max(0) as usize;
            Value::string(string_slice(&text, start, count))
        }
        "STR$" if args.len() == 1 => {
            let mut text = format!("{}", args[0]);
            if text.starts_with(' ') {
                text.remove(0);
            }
            Value::string(text)
        }
        "BIN$" if args.len() == 1 || args.len() == 2 => {
            let value = n(&args[0])? as i64;
            let width = args.get(1).map(n).transpose()?.map(|v| v as usize);
            Value::string(format_radix_string(value, width, 2))
        }
        "HEX$" if args.len() == 1 || args.len() == 2 => {
            let value = n(&args[0])? as i64;
            let width = args.get(1).map(n).transpose()?.map(|v| v as usize);
            Value::string(format_radix_string(value, width, 16))
        }
        "DEC$" if args.len() == 2 => {
            let value = n(&args[0])?;
            let fmt = args[1].clone().into_string()?;
            Value::string(format_dec_string(value, &fmt))
        }
        "CHR$" if args.len() == 1 => {
            let code = n(&args[0])? as u32;
            Value::string(char::from_u32(code).unwrap_or('\u{fffd}').to_string())
        }
        "UPPER$" if args.len() == 1 => {
            Value::string(s(args.into_iter().next().unwrap())?.to_ascii_uppercase())
        }
        "LOWER$" if args.len() == 1 => {
            Value::string(s(args.into_iter().next().unwrap())?.to_ascii_lowercase())
        }
        "SPACE$" if args.len() == 1 => Value::string(" ".repeat(n(&args[0])?.max(0.0) as usize)),
        "STRING$" if args.len() == 2 => {
            let count = n(&args[0])?.max(0.0) as usize;
            let text = match &args[1] {
                Value::Str(s) => s
                    .chars()
                    .find(|ch| !ch.is_whitespace())
                    .or_else(|| s.chars().next())
                    .unwrap_or(' ')
                    .to_string(),
                Value::Number(n) => char::from_u32(*n as u32).unwrap_or(' ').to_string(),
                Value::ArrayRef(_) => return Err(BasicError::new(ErrorCode::TypeMismatch)),
            };
            Value::string(text.repeat(count))
        }
        "TRIM$" if args.len() == 1 => {
            Value::string(s(args.into_iter().next().unwrap())?.trim().to_string())
        }
        _ if is_pure_function_name(&name) => {
            return Err(BasicError::new(ErrorCode::ArgumentMismatch))
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}

fn is_pure_function_name(name: &str) -> bool {
    matches!(
        name,
        "ABS"
            | "INT"
            | "FIX"
            | "SGN"
            | "FRAC"
            | "SQR"
            | "LOG"
            | "LOG10"
            | "EXP"
            | "SIN"
            | "COS"
            | "TAN"
            | "ASN"
            | "ACS"
            | "ATN"
            | "COT"
            | "ROUND"
            | "MIN"
            | "MAX"
            | "LEN"
            | "ASC"
            | "VAL"
            | "INSTR"
            | "LEFT$"
            | "RIGHT$"
            | "MID$"
            | "STR$"
            | "BIN$"
            | "HEX$"
            | "DEC$"
            | "CHR$"
            | "UPPER$"
            | "LOWER$"
            | "SPACE$"
            | "STRING$"
            | "TRIM$"
            | "PI"
    )
}

fn is_builtin_function(name: &str) -> bool {
    matches!(
        name,
        "ABS"
            | "INT"
            | "FIX"
            | "SGN"
            | "FRAC"
            | "SQR"
            | "LOG"
            | "LOG10"
            | "EXP"
            | "SIN"
            | "COS"
            | "TAN"
            | "ASN"
            | "ACS"
            | "ATN"
            | "COT"
            | "ROUND"
            | "MIN"
            | "MAX"
            | "LEN"
            | "ASC"
            | "VAL"
            | "INSTR"
            | "LEFT$"
            | "RIGHT$"
            | "MID$"
            | "STR$"
            | "BIN$"
            | "HEX$"
            | "DEC$"
            | "CHR$"
            | "UPPER$"
            | "LOWER$"
            | "SPACE$"
            | "STRING$"
            | "TRIM$"
            | "VERSION$"
            | "RND"
            | "TIME"
            | "REMAIN"
            | "ERR"
            | "ERL"
            | "PI"
            | "RGB"
            | "RGB$"
            | "SCREEN$"
            | "SPRITE$"
            | "TEST"
            | "WIDTH"
            | "HEIGHT"
            | "XPOS"
            | "YPOS"
            | "HPOS"
            | "VPOS"
            | "MOUSEX"
            | "MOUSEY"
            | "MOUSELEFT"
            | "MOUSERIGHT"
            | "MOUSEEVENT$"
            | "HIT"
            | "HITCOLOR"
            | "HITSPRITE"
            | "HITID"
            | "LBOUND"
            | "UBOUND"
            | "LBND"
            | "UBND"
            | "ABSUM"
            | "AMAX"
            | "AMAXCOL"
            | "AMAXROW"
            | "AMIN"
            | "AMINCOL"
            | "AMINROW"
            | "CNORM"
            | "CNORMCOL"
            | "DOT"
            | "FNORM"
            | "MAXAB"
            | "MAXABCOL"
            | "MAXABROW"
            | "RNORM"
            | "RNORMROW"
            | "SUM"
            | "INKEY$"
            | "KEYDOWN"
            | "DET"
            | "TRN"
            | "INV"
            | "SPC"
            | "TAB"
    )
}

fn is_array_name_function(name: &str) -> bool {
    matches!(
        name,
        "LBOUND"
            | "UBOUND"
            | "LBND"
            | "UBND"
            | "DET"
            | "ABSUM"
            | "AMAX"
            | "AMIN"
            | "CNORM"
            | "DOT"
            | "FNORM"
            | "MAXAB"
            | "RNORM"
            | "SUM"
    )
}

fn is_zero_arg_function(name: &str) -> bool {
    matches!(
        name,
        "RND"
            | "TIME"
            | "ERR"
            | "ERL"
            | "PI"
            | "VERSION$"
            | "SCREEN$"
            | "WIDTH"
            | "HEIGHT"
            | "XPOS"
            | "YPOS"
            | "HPOS"
            | "VPOS"
            | "MOUSEX"
            | "MOUSEY"
            | "MOUSELEFT"
            | "MOUSERIGHT"
            | "MOUSEEVENT$"
            | "HIT"
            | "HITCOLOR"
            | "HITSPRITE"
            | "HITID"
            | "INKEY$"
            | "AMAXCOL"
            | "AMAXROW"
            | "AMINCOL"
            | "AMINROW"
            | "CNORMCOL"
            | "MAXABCOL"
            | "MAXABROW"
            | "RNORMROW"
    )
}
