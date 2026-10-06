//! HP-style subarray copies. Selections copy only their own elements and leave
//! the destination unchanged until all bounds and shape checks have succeeded.
use super::*;

#[derive(Debug, Clone)]
pub(super) struct CopyReference {
    pub(super) name: String,
    pub(super) selectors: Option<Vec<CopySelectorText>>,
}

#[derive(Debug, Clone)]
pub(super) enum CopySelectorText {
    All,
    Single(String),
    Range(String, String),
    Invalid,
}

impl CopySelectorText {
    fn parse(part: String) -> Self {
        if part.is_empty() {
            return Self::All;
        }
        let parts = split_top_level(&part, &[':']);
        match parts.as_slice() {
            [one] => Self::Single(one.clone()),
            [first, last] => Self::Range(first.clone(), last.clone()),
            _ => Self::Invalid,
        }
    }
}

impl CopyReference {
    pub(super) fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if is_basic_identifier(text) {
            return Some(Self {
                name: text.to_ascii_uppercase(),
                selectors: None,
            });
        }
        let open = text.find('(')?;
        let name = text[..open].trim();
        if !is_basic_identifier(name) || !text.ends_with(')') {
            return None;
        }
        let mut depth = 0;
        let mut quoted = false;
        for (pos, ch) in text[open..].char_indices() {
            if ch == '"' {
                quoted = !quoted;
            }
            if quoted {
                continue;
            }
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth < 0 || (depth == 0 && open + pos != text.len() - 1) {
                        return None;
                    }
                }
                _ => {}
            }
        }
        if depth != 0 || quoted {
            return None;
        }
        Some(Self {
            name: name.to_ascii_uppercase(),
            selectors: Some(
                split_top_level(&text[open + 1..text.len() - 1], &[','])
                    .into_iter()
                    .map(CopySelectorText::parse)
                    .collect(),
            ),
        })
    }

    pub(super) fn explicit_slice(&self) -> bool {
        self.selectors.as_ref().is_some_and(|parts| {
            parts
                .iter()
                .any(|part| !matches!(part, CopySelectorText::Single(_)))
        })
    }
}

enum CopySelector {
    All,
    Single(i64),
    Range(i64, i64),
}

#[derive(Clone, Copy)]
struct CopyAxis {
    start: usize,
    len: usize,
    reverse: bool,
}

impl CopyAxis {
    fn at(self, offset: usize) -> usize {
        if self.reverse {
            self.start - offset
        } else {
            self.start + offset
        }
    }

    fn resolve(selector: &CopySelector, bound: usize, base: usize) -> BasicResult<Self> {
        let bad = || BasicError::new(ErrorCode::IndexOutOfRange);
        let (start, end, reverse) = match *selector {
            CopySelector::All => {
                return Ok(Self {
                    start: base,
                    len: (bound + 1).saturating_sub(base),
                    reverse: false,
                });
            }
            CopySelector::Single(n) => (n, n, false),
            CopySelector::Range(a, b) if a == b + 1 => {
                if a < 0 || a > bound as i64 + 1 || b < -1 || b > bound as i64 {
                    return Err(bad());
                }
                return Ok(Self {
                    start: a as usize,
                    len: 0,
                    reverse: false,
                });
            }
            // HP reverse intervals exclude both written endpoints. This also
            // admits the n+1 and -1 sentinels for a complete reversed axis.
            CopySelector::Range(a, b) if a > b => (a - 1, b + 1, true),
            CopySelector::Range(a, b) => (a, b, false),
        };
        if start < 0 || end < 0 || start > bound as i64 || end > bound as i64 {
            return Err(bad());
        }
        Ok(Self {
            start: start as usize,
            len: (start - end).unsigned_abs() as usize + 1,
            reverse,
        })
    }
}

struct CopyBlock {
    rows: CopyAxis,
    cols: CopyAxis,
    stride: usize,
    rank: usize,
}

impl CopyBlock {
    fn resolve(
        dims: &[usize],
        selectors: Option<&[CopySelector]>,
        base: usize,
    ) -> BasicResult<Self> {
        if !(1..=2).contains(&dims.len()) || selectors.is_some_and(|s| s.len() != dims.len()) {
            return Err(BasicError::new(ErrorCode::InvalidDimensions));
        }
        let axis = |index: usize| {
            CopyAxis::resolve(
                selectors.map(|s| &s[index]).unwrap_or(&CopySelector::All),
                dims[index],
                base,
            )
        };
        Ok(Self {
            rows: axis(0)?,
            cols: if dims.len() == 2 {
                axis(1)?
            } else {
                CopyAxis {
                    start: 0,
                    len: 1,
                    reverse: false,
                }
            },
            stride: if dims.len() == 2 { dims[1] + 1 } else { 1 },
            rank: dims.len(),
        })
    }

    fn count(&self) -> usize {
        self.rows.len * self.cols.len
    }
    fn vector_shaped(&self) -> bool {
        self.rank == 1 || self.rows.len == 1 || self.cols.len == 1
    }

    fn matches(&self, other: &Self) -> bool {
        if self.rank == 1 || other.rank == 1 {
            self.vector_shaped() && other.vector_shaped() && self.count() == other.count()
        } else {
            self.rows.len == other.rows.len && self.cols.len == other.cols.len
        }
    }

    fn gather<T: Clone>(&self, data: &[T]) -> Vec<T> {
        let mut out = Vec::with_capacity(self.count());
        if self.count() == 0 {
            return out;
        }
        if self.rank == 1 && !self.rows.reverse {
            out.extend_from_slice(&data[self.rows.start..self.rows.start + self.rows.len]);
        } else {
            for r in 0..self.rows.len {
                let offset = self.rows.at(r) * self.stride;
                if !self.cols.reverse {
                    out.extend_from_slice(
                        &data[offset + self.cols.start..offset + self.cols.start + self.cols.len],
                    );
                } else {
                    for c in 0..self.cols.len {
                        out.push(data[offset + self.cols.at(c)].clone());
                    }
                }
            }
        }
        out
    }

    fn scatter<T: Clone>(&self, data: &mut [T], values: &[T]) {
        if values.is_empty() {
            return;
        }
        if self.rank == 1 && !self.rows.reverse {
            data[self.rows.start..self.rows.start + self.rows.len].clone_from_slice(values);
        } else {
            for r in 0..self.rows.len {
                let offset = self.rows.at(r) * self.stride;
                let input = &values[r * self.cols.len..(r + 1) * self.cols.len];
                if !self.cols.reverse {
                    data[offset + self.cols.start..offset + self.cols.start + self.cols.len]
                        .clone_from_slice(input);
                } else {
                    for (c, value) in input.iter().enumerate() {
                        data[offset + self.cols.at(c)] = value.clone();
                    }
                }
            }
        }
    }
}

impl Interpreter {
    fn copy_selectors(
        &mut self,
        reference: &CopyReference,
    ) -> BasicResult<Option<Vec<CopySelector>>> {
        let Some(parts) = &reference.selectors else {
            return Ok(None);
        };
        let mut out = Vec::with_capacity(parts.len());
        for part in parts {
            let mut number = |text: &str| -> BasicResult<i64> {
                if text.is_empty() {
                    return Err(self.err(ErrorCode::Syntax));
                }
                let value = self.eval_number(text)?;
                if !value.is_finite() {
                    return Err(self.err(ErrorCode::InvalidIndex));
                }
                if value < i32::MIN as f64 || value > i32::MAX as f64 {
                    return Err(self.err(ErrorCode::IndexOutOfRange));
                }
                Ok(value.trunc() as i64)
            };
            out.push(match part {
                CopySelectorText::All => CopySelector::All,
                CopySelectorText::Single(one) => CopySelector::Single(number(one)?),
                CopySelectorText::Range(first, last) => {
                    CopySelector::Range(number(first)?, number(last)?)
                }
                CopySelectorText::Invalid => return Err(self.err(ErrorCode::Syntax)),
            });
        }
        Ok(Some(out))
    }

    pub(super) fn try_mat_subarray_copy(&mut self, lhs: &str, rhs: &str) -> BasicResult<bool> {
        // Leave ordinary MAT expressions on their existing evaluation path.
        // A range such as A(3:3) explicitly requests a one-element subarray.
        let left_selected = lhs.contains('(');
        if !left_selected && !rhs.contains('(') {
            return Ok(false);
        }
        let right = CopyReference::parse(rhs);
        if !left_selected
            && !right.as_ref().is_some_and(|r| {
                !r.name.starts_with("FN") && self.array_exists(&r.name) && r.explicit_slice()
            })
        {
            return Ok(false);
        }
        let left = CopyReference::parse(lhs).ok_or_else(|| self.err(ErrorCode::InvalidArgument))?;
        if left.name.starts_with("FN") && self.active_function_name() != Some(left.name.as_str()) {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        let right = right.ok_or_else(|| self.err(ErrorCode::InvalidArgument))?;
        self.execute_mat_subarray_copy_refs(&left, &right, rhs)?;
        Ok(true)
    }

    pub(super) fn execute_mat_subarray_copy_refs(
        &mut self,
        left: &CopyReference,
        right: &CopyReference,
        rhs: &str,
    ) -> BasicResult<()> {
        let returned = if right.name.starts_with("FN") {
            match self.eval_mat_expr(rhs)? {
                MatExprValue::Matrix(array) => Some(array),
                _ => return Err(self.err(ErrorCode::InvalidArgument)),
            }
        } else {
            None
        };
        let right_selectors = if returned.is_none() {
            self.copy_selectors(right)?
        } else {
            None
        };
        let base = self.mat_base.max(0) as usize;
        let source = returned
            .as_ref()
            .or_else(|| self.array_ref(&right.name))
            .ok_or_else(|| self.err(ErrorCode::Undefined))?;
        let source_block = CopyBlock::resolve(&source.dims, right_selectors.as_deref(), base)?;
        if source.is_string() != left.name.ends_with('$') {
            return Err(self.err(ErrorCode::TypeMismatch));
        }
        // Snapshot the RHS before evaluating target index expressions, which
        // may call functions that modify the source or resize the destination.
        let values = match &source.data {
            ArrayData::Number(data) => ArrayData::Number(source_block.gather(data)),
            ArrayData::Str(data) => ArrayData::Str(source_block.gather(data)),
        };
        let left_selectors = self.copy_selectors(left)?;
        let destination = self.array_ref(&left.name);
        if destination.is_some_and(|a| a.is_string() != left.name.ends_with('$')) {
            return Err(self.err(ErrorCode::TypeMismatch));
        }

        let mut new_dims = None;
        let target_block = if left.selectors.is_some() {
            let destination = destination.ok_or_else(|| self.err(ErrorCode::Undefined))?;
            CopyBlock::resolve(&destination.dims, left_selectors.as_deref(), base)?
        } else {
            let rank = destination.map_or(source_block.rank, |array| array.dims.len());
            let lengths = match rank {
                1 if source_block.vector_shaped() => vec![source_block.count()],
                2 => vec![source_block.rows.len, source_block.cols.len],
                _ => return Err(self.err(ErrorCode::InvalidDimensions)),
            };
            if base == 0 && lengths.contains(&0) {
                return Err(self.err(ErrorCode::InvalidDimensions));
            }
            let dims: Vec<_> = lengths.iter().map(|n| n + base - 1).collect();
            let block = CopyBlock::resolve(&dims, None, base)?;
            new_dims = Some(dims);
            block
        };
        if !target_block.matches(&source_block) {
            return Err(self.err(ErrorCode::InvalidDimensions));
        }

        // Mutation starts only after all validation; values contain only K
        // selected elements, also when two names alias the same array.
        let key = self.array_lookup_key(&left.name).into_owned();
        if let Some(dims) = new_dims {
            let mut result = if base == 0 {
                ArrayValue::checked_data_len(&dims)
                    .map_err(|error| self.with_current_line(error))?;
                ArrayValue {
                    dims,
                    data: values,
                    last_debug_write: None,
                }
            } else {
                let mut array = ArrayValue::try_new(&key, dims)
                    .map_err(|error| self.with_current_line(error))?;
                match (&mut array.data, &values) {
                    (ArrayData::Number(data), ArrayData::Number(values)) => {
                        target_block.scatter(data, values)
                    }
                    (ArrayData::Str(data), ArrayData::Str(values)) => {
                        target_block.scatter(data, values)
                    }
                    _ => unreachable!(),
                }
                array
            };
            result.clear_debug_write();
            self.arrays.insert(key, result);
        } else {
            let destination = self.arrays.get_mut(&key).unwrap();
            match (&mut destination.data, &values) {
                (ArrayData::Number(data), ArrayData::Number(values)) => {
                    target_block.scatter(data, values)
                }
                (ArrayData::Str(data), ArrayData::Str(values)) => {
                    target_block.scatter(data, values)
                }
                _ => unreachable!(),
            }
            destination.clear_debug_write();
        }
        self.return_array_for_active_function(&left.name);
        Ok(())
    }
}
