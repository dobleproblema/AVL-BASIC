use crate::error::{BasicError, BasicResult, ErrorCode};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Str(String),
    ArrayRef(String),
}

impl Default for Value {
    fn default() -> Self {
        Value::Number(0.0)
    }
}

impl Value {
    pub fn number(n: f64) -> Self {
        Value::Number(n)
    }

    pub fn string(s: impl Into<String>) -> Self {
        Value::Str(s.into())
    }

    pub fn as_number(&self) -> BasicResult<f64> {
        match self {
            Value::Number(n) => Ok(*n),
            Value::Str(_) | Value::ArrayRef(_) => Err(BasicError::new(ErrorCode::TypeMismatch)),
        }
    }

    pub fn into_string(self) -> BasicResult<String> {
        match self {
            Value::Str(s) => Ok(s),
            Value::Number(_) | Value::ArrayRef(_) => Err(BasicError::new(ErrorCode::TypeMismatch)),
        }
    }

    pub fn is_true(&self) -> BasicResult<bool> {
        Ok(self.as_number()? != 0.0)
    }

    pub fn basic_bool(v: bool) -> Self {
        if v {
            Value::Number(-1.0)
        } else {
            Value::Number(0.0)
        }
    }

    pub fn default_for_name(name: &str) -> Self {
        if name.trim_end().ends_with('$') {
            Value::Str(String::new())
        } else {
            Value::Number(0.0)
        }
    }
}

pub fn round_half_away(v: f64, digits: i32) -> f64 {
    if !v.is_finite() {
        return v;
    }
    if digits == 0 {
        if v >= 0.0 {
            (v + 0.5).floor()
        } else {
            (v - 0.5).ceil()
        }
    } else {
        let scale = 10_f64.powi(digits.abs());
        if digits > 0 {
            round_half_away(v * scale, 0) / scale
        } else {
            round_half_away(v / scale, 0) * scale
        }
    }
}

pub fn logical_round(v: f64) -> i64 {
    round_half_away(v, 0) as i64
}

pub fn format_basic_number(n: f64) -> String {
    if n == f64::INFINITY {
        return " inf".to_string();
    }
    if n == f64::NEG_INFINITY {
        return "-inf".to_string();
    }
    if n == 0.0 {
        return " 0".to_string();
    }
    let sign = if n < 0.0 { "-" } else { " " };
    let abs = n.abs();
    let body = if abs.fract() == 0.0 && abs < 1e14 {
        format!("{abs:.0}")
    } else {
        let mut s = if (1e-4..1e14).contains(&abs) {
            let decimals = significant_decimal_places(abs, 14);
            format!("{abs:.decimals$}")
        } else {
            format_scientific(abs, 14)
        };
        while !s.contains('E') && s.contains('.') && s.ends_with('0') {
            s.pop();
        }
        if !s.contains('E') && s.ends_with('.') {
            s.pop();
        }
        s
    };
    format!("{sign}{body}")
}

fn significant_decimal_places(abs: f64, significant: i32) -> usize {
    if abs >= 1.0 {
        let integer_digits = abs.log10().floor() as i32 + 1;
        (significant - integer_digits).max(0) as usize
    } else {
        let leading_zeros = (-abs.log10().floor() as i32 - 1).max(0);
        (significant + leading_zeros) as usize
    }
}

fn format_scientific(abs: f64, significant: usize) -> String {
    let decimals = significant.saturating_sub(1);
    let raw = format!("{abs:.decimals$E}");
    let Some((mantissa, exponent)) = raw.split_once('E') else {
        return raw;
    };
    let mut mantissa = mantissa.to_string();
    while mantissa.contains('.') && mantissa.ends_with('0') {
        mantissa.pop();
    }
    if mantissa.ends_with('.') {
        mantissa.pop();
    }
    let exp = exponent.parse::<i32>().unwrap_or(0);
    let exp_sign = if exp < 0 { '-' } else { '+' };
    format!("{mantissa}E{exp_sign}{:02}", exp.abs())
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Number(n) => f.write_str(&format_basic_number(*n)),
            Value::Str(s) => f.write_str(s),
            Value::ArrayRef(name) => f.write_str(name),
        }
    }
}
