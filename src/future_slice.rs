//! This module re-implements a unstable slice functions, these should be removed once they
//! are stabilized. These is copy-pasted with slight modifications from std::slice for
//! functions that do not need language magic.

use std::ops;

#[track_caller]
#[must_use]
pub(crate) fn range<R>(range: R, bounds: ops::RangeTo<usize>) -> ops::Range<usize>
where
    R: ops::RangeBounds<usize>,
{
    let len = bounds.end;

    let start = match range.start_bound() {
        ops::Bound::Included(&start) => start,
        ops::Bound::Excluded(start) => start
            .checked_add(1)
            .unwrap_or_else(|| slice_start_index_overflow_fail()),
        ops::Bound::Unbounded => 0,
    };

    let end = match range.end_bound() {
        ops::Bound::Included(end) => end
            .checked_add(1)
            .unwrap_or_else(|| slice_end_index_overflow_fail()),
        ops::Bound::Excluded(&end) => end,
        ops::Bound::Unbounded => len,
    };

    if start > end {
        slice_index_order_fail(start, end);
    }
    if end > len {
        slice_end_index_len_fail(end, len);
    }

    ops::Range { start, end }
}

#[track_caller]
const fn slice_start_index_overflow_fail() -> ! {
    panic!("attempted to index slice from after maximum usize");
}

#[track_caller]
const fn slice_end_index_overflow_fail() -> ! {
    panic!("attempted to index slice up to maximum usize");
}

#[track_caller]
fn slice_index_order_fail(index: usize, end: usize) -> ! {
    panic!("slice index start is larger than end, slice index starts at {index} but ends at {end}")
}

#[track_caller]
fn slice_end_index_len_fail(index: usize, len: usize) -> ! {
    panic!(
        "slice end index is out of range for slice, range end index {index} out of range for slice of length {len}"
    )
}
