use super::{ArrayData, ArrayValue, BasicResult};

// Keep scratch storage bounded even when dynamic LOCAL dimensions vary widely.
const MAX_RETAINED_BYTES: usize = 8 * 1024 * 1024;
const MAX_RETAINED_ARRAYS: usize = 32;

#[derive(Debug)]
struct PooledArray {
    array: ArrayValue,
    retained_bytes: usize,
}

#[derive(Debug, Default)]
pub(super) struct LocalArrayPool {
    entries: Vec<PooledArray>,
    retained_bytes: usize,
}

impl LocalArrayPool {
    pub(super) fn take_or_new(&mut self, name: &str, dims: &[usize]) -> BasicResult<ArrayValue> {
        // Dimensions are checked on every entry, including a potential hit.
        ArrayValue::checked_data_len(dims)?;
        let is_string = name.ends_with('$');
        let matching = self.entries.iter().rposition(|entry| {
            entry.array.dims.as_slice() == dims && entry.array.is_string() == is_string
        });
        let Some(index) = matching else {
            return ArrayValue::try_new(name, dims.to_vec());
        };

        let entry = self.entries.swap_remove(index);
        self.retained_bytes -= entry.retained_bytes;
        let mut array = entry.array;
        match &mut array.data {
            ArrayData::Number(values) => values.fill(0.0),
            ArrayData::Str(values) => values.iter_mut().for_each(String::clear),
        }
        array.clear_debug_write();
        Ok(array)
    }

    // Only detached, owned LOCAL storage may be recycled. Caller arrays and
    // storage still reachable through a live alias must remain outside the pool.
    pub(super) fn recycle(&mut self, array: ArrayValue) {
        self.recycle_with_limits(array, MAX_RETAINED_BYTES, MAX_RETAINED_ARRAYS);
    }

    fn recycle_with_limits(&mut self, mut array: ArrayValue, max_bytes: usize, max_arrays: usize) {
        array.clear_debug_write();
        let retained_bytes = array_retained_bytes(&array);
        if retained_bytes > max_bytes || max_arrays == 0 {
            return;
        }

        // Prefer recently used shapes. The entry cap also bounds shape searches
        // and the small amount of metadata retained by the entries vector.
        while !self.entries.is_empty()
            && (self.entries.len() >= max_arrays
                || self.retained_bytes > max_bytes - retained_bytes)
        {
            let evicted = self.entries.remove(0);
            self.retained_bytes -= evicted.retained_bytes;
        }
        self.entries.push(PooledArray {
            array,
            retained_bytes,
        });
        self.retained_bytes += retained_bytes;
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.retained_bytes = 0;
    }
}

fn array_retained_bytes(array: &ArrayValue) -> usize {
    let dims_bytes = array
        .dims
        .capacity()
        .saturating_mul(std::mem::size_of::<usize>());
    let data_bytes = match &array.data {
        ArrayData::Number(values) => values.capacity().saturating_mul(std::mem::size_of::<f64>()),
        ArrayData::Str(values) => values.iter().fold(
            values
                .capacity()
                .saturating_mul(std::mem::size_of::<String>()),
            |bytes, value| bytes.saturating_add(value.capacity()),
        ),
    };
    dims_bytes.saturating_add(data_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    #[test]
    fn numeric_buffer_is_reused_and_reset_with_debug_metadata_cleared() {
        let mut pool = LocalArrayPool::default();
        let mut array = pool.take_or_new("TEMP", &[2, 1]).unwrap();
        let pointer = match &mut array.data {
            ArrayData::Number(values) => {
                values.fill(f64::NAN);
                values.as_ptr()
            }
            _ => unreachable!(),
        };
        array.remember_debug_write("TEMP", &[2, 1]);
        pool.recycle(array);

        // Storage is independent of the LOCAL's source name.
        let reused = pool.take_or_new("OTHER", &[2, 1]).unwrap();
        match &reused.data {
            ArrayData::Number(values) => {
                assert_eq!(values.as_ptr(), pointer);
                assert!(values.iter().all(|value| value.to_bits() == 0));
            }
            _ => unreachable!(),
        }
        assert!(reused.last_debug_write.is_none());
        assert_eq!(pool.retained_bytes, 0);
    }

    #[test]
    fn string_buffers_are_empty_on_reuse_and_character_capacity_is_accounted() {
        let mut pool = LocalArrayPool::default();
        let mut array = pool.take_or_new("TEMP$", &[1]).unwrap();
        let pointer = match &mut array.data {
            ArrayData::Str(values) => {
                values[0] = "previous invocation".repeat(10);
                values[1] = "other value".to_string();
                values.as_ptr()
            }
            _ => unreachable!(),
        };
        let expected_bytes = array_retained_bytes(&array);
        pool.recycle(array);
        assert_eq!(pool.retained_bytes, expected_bytes);
        let reused = pool.take_or_new("OTHER$", &[1]).unwrap();
        match &reused.data {
            ArrayData::Str(values) => {
                assert_eq!(values.as_ptr(), pointer);
                assert!(values.iter().all(String::is_empty));
                assert!(values[0].capacity() >= "previous invocation".len() * 10);
            }
            _ => unreachable!(),
        }
        assert_eq!(pool.retained_bytes, 0);
    }

    #[test]
    fn only_exact_shape_and_type_are_reused() {
        let mut pool = LocalArrayPool::default();
        let numeric = pool.take_or_new("A", &[2, 1]).unwrap();
        pool.recycle(numeric);
        let different_shape = pool.take_or_new("A", &[1, 2]).unwrap();
        assert_eq!(different_shape.dims, vec![1, 2]);
        assert_eq!(pool.entries.len(), 1);
        let string = pool.take_or_new("A$", &[2, 1]).unwrap();
        assert!(string.is_string());
        assert_eq!(pool.entries.len(), 1);
        let numeric = pool.take_or_new("A", &[2, 1]).unwrap();
        assert!(!numeric.is_string());
        assert!(pool.entries.is_empty());
    }

    #[test]
    fn limits_evict_old_storage_and_reject_oversized_string_capacity() {
        let mut pool = LocalArrayPool::default();
        let first = ArrayValue::try_new("A", vec![1]).unwrap();
        let one_array_bytes = array_retained_bytes(&first);
        pool.recycle_with_limits(first, one_array_bytes * 2, 2);
        pool.recycle_with_limits(
            ArrayValue::try_new("B", vec![1]).unwrap(),
            one_array_bytes * 2,
            2,
        );
        pool.recycle_with_limits(
            ArrayValue::try_new("C", vec![1]).unwrap(),
            one_array_bytes,
            2,
        );
        assert_eq!(pool.entries.len(), 1);
        assert_eq!(pool.retained_bytes, one_array_bytes);

        let mut oversized = ArrayValue::try_new("A$", vec![0]).unwrap();
        if let ArrayData::Str(values) = &mut oversized.data {
            values[0] = "x".repeat(4096);
        }
        pool.recycle_with_limits(oversized, one_array_bytes, 2);
        assert_eq!(pool.entries.len(), 1);
        assert_eq!(pool.retained_bytes, one_array_bytes);
        pool.clear();
        assert!(pool.entries.is_empty());
        assert_eq!(pool.retained_bytes, 0);
    }

    #[test]
    fn entry_limit_and_invalid_dimensions_remain_enforced() {
        let mut pool = LocalArrayPool::default();
        for bound in 0..MAX_RETAINED_ARRAYS + 3 {
            pool.recycle(ArrayValue::try_new("A", vec![bound]).unwrap());
        }
        assert_eq!(pool.entries.len(), MAX_RETAINED_ARRAYS);
        assert!(pool.retained_bytes <= MAX_RETAINED_BYTES);
        let retained_bytes = pool.retained_bytes;
        assert_eq!(
            pool.take_or_new("A", &[i32::MAX as usize + 1])
                .unwrap_err()
                .code,
            ErrorCode::Overflow
        );
        assert_eq!(pool.retained_bytes, retained_bytes);
    }
}
