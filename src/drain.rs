#![cfg(feature = "std")]

use core::{
    any::type_name,
    fmt,
    mem::{self},
    ptr::{self, NonNull},
};

use std::{iter::FusedIterator, mem::ManuallyDrop, slice};

use crate::HeaderVec;

/// A draining iterator for `HeaderVec<H, T>`.
///
/// This `struct` is created by [`HeaderVec::drain`].
/// See its documentation for more.
///
/// # Feature compatibility
///
/// The `drain()` API and [`Drain`] iterator are only available when the `std` feature is
/// enabled.
///
/// # Example
///
/// ```
/// # use header_vec::HeaderVec;
/// let mut hv: HeaderVec<(), _> = HeaderVec::from([0, 1, 2]);
/// let iter: header_vec::Drain<'_, _, _> = hv.drain(..);
/// ```
pub struct Drain<'a, H, T> {
    /// Index of tail to preserve
    pub(super) tail_start: usize,
    /// End index of tail to preserve
    pub(super) tail_len: usize,
    /// Current remaining range to remove
    pub(super) iter: slice::Iter<'a, T>,
    pub(super) vec: NonNull<HeaderVec<H, T>>,
}

impl<H: fmt::Debug, T: fmt::Debug> fmt::Debug for Drain<'_, H, T> {
    #[mutants::skip]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(&format!(
            "Drain<{}, {}>",
            type_name::<H>(),
            type_name::<T>()
        ))
        .field("header", unsafe { self.vec.as_ref() })
        .field("iter", &self.iter.as_slice())
        .finish()
    }
}

impl<H, T> Drain<'_, H, T> {
    /// Returns the remaining items of this iterator as a slice.
    ///
    /// # Examples
    ///
    /// ```
    /// # use header_vec::HeaderVec;
    /// let mut hv: HeaderVec<(), _> = HeaderVec::from(['a', 'b', 'c']);
    /// let mut drain = hv.drain(..);
    /// assert_eq!(drain.as_slice(), &['a', 'b', 'c']);
    /// let _ = drain.next().unwrap();
    /// assert_eq!(drain.as_slice(), &['b', 'c']);
    /// ```
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        self.iter.as_slice()
    }

    /// Keep unyielded elements in the source `HeaderVec`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use header_vec::HeaderVec;
    /// let mut hv: HeaderVec<(), _> = HeaderVec::from(['a', 'b', 'c']);
    /// let mut drain = hv.drain(..);
    ///
    /// assert_eq!(drain.next().unwrap(), 'a');
    ///
    /// // This call keeps 'b' and 'c' in the vec.
    /// drain.keep_rest();
    ///
    /// // If we wouldn't call `keep_rest()`,
    /// // `hv` would be empty.
    /// assert_eq!(hv.as_slice(), ['b', 'c']);
    /// ```
    pub fn keep_rest(self) {
        let mut this = ManuallyDrop::new(self);

        unsafe {
            let source_vec = this.vec.as_mut();

            let start = source_vec.len();
            let tail = this.tail_start;

            let unyielded_len = this.iter.len();
            let unyielded_ptr = this.iter.as_slice().as_ptr();

            let start_ptr = source_vec.as_mut_ptr().add(start);

            // memmove back unyielded elements
            if unyielded_ptr != start_ptr {
                let src = unyielded_ptr;
                let dst = start_ptr;

                ptr::copy(src, dst, unyielded_len);
            }

            // memmove back untouched tail
            if tail != (start + unyielded_len) {
                let src = source_vec.as_ptr().add(tail);
                let dst = start_ptr.add(unyielded_len);
                ptr::copy(src, dst, this.tail_len);
            }

            source_vec.set_len(start + unyielded_len + this.tail_len);
        }
    }
}

impl<H, T> AsRef<[T]> for Drain<'_, H, T> {
    fn as_ref(&self) -> &[T] {
        self.as_slice()
    }
}

unsafe impl<H: Sync, T: Sync> Sync for Drain<'_, H, T> {}
unsafe impl<H: Send, T: Send> Send for Drain<'_, H, T> {}

impl<H, T> Iterator for Drain<'_, H, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<T> {
        self.iter
            .next()
            .map(|elt| unsafe { ptr::read(elt as *const _) })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<H, T> DoubleEndedIterator for Drain<'_, H, T> {
    #[inline]
    fn next_back(&mut self) -> Option<T> {
        self.iter
            .next_back()
            .map(|elt| unsafe { ptr::read(elt as *const _) })
    }
}

impl<H, T> Drop for Drain<'_, H, T> {
    fn drop(&mut self) {
        /// Moves back the un-`Drain`ed elements to restore the original `Vec`.
        struct DropGuard<'r, 'a, H, T>(&'r mut Drain<'a, H, T>);

        impl<H, T> Drop for DropGuard<'_, '_, H, T> {
            fn drop(&mut self) {
                if self.0.tail_len > 0 {
                    unsafe {
                        let source_vec = self.0.vec.as_mut();
                        // memmove back untouched tail, update to new length
                        let start = source_vec.len();
                        let tail = self.0.tail_start;
                        if tail != start {
                            let src = source_vec.as_ptr().add(tail);
                            let dst = source_vec.as_mut_ptr().add(start);
                            ptr::copy(src, dst, self.0.tail_len);
                        }
                        source_vec.set_len(start + self.0.tail_len);
                    }
                }
            }
        }

        let iter = mem::take(&mut self.iter);
        let drop_len = iter.len();

        let mut vec = self.vec;

        // ensure elements are moved back into their appropriate places, even when drop_in_place panics
        let _guard = DropGuard(self);

        if drop_len == 0 {
            return;
        }

        // as_slice() must only be called when iter.len() is > 0 because
        // it also gets touched by vec::Splice which may turn it into a dangling pointer
        // which would make it and the vec pointer point to different allocations which would
        // lead to invalid pointer arithmetic below.
        let drop_ptr = iter.as_slice().as_ptr();

        unsafe {
            // drop_ptr comes from a slice::Iter which only gives us a &[T] but for drop_in_place
            // a pointer with mutable provenance is necessary. Therefore we must reconstruct
            // it from the original vec but also avoid creating a &mut to the front since that could
            // invalidate raw pointers to it which some unsafe code might rely on.
            let vec_ptr = vec.as_mut().as_mut_ptr();

            // PLANNED: let drop_offset = drop_ptr.sub_ptr(vec_ptr); is in nightly
            let drop_offset = usize::try_from(drop_ptr.offset_from(vec_ptr)).unwrap_unchecked();
            let to_drop = ptr::slice_from_raw_parts_mut(vec_ptr.add(drop_offset), drop_len);
            ptr::drop_in_place(to_drop);
        }
    }
}

impl<H, T> FusedIterator for Drain<'_, H, T> {}

// PLANNED: unstable features
// impl<H, T> ExactSizeIterator for Drain<'_, H, T> {
//     fn is_empty(&self) -> bool {
//         self.iter.is_empty()
//     }
// }
//
// #[unstable(feature = "trusted_len", issue = "37572")]
// unsafe impl<H, T> TrustedLen for Drain<'_, H, T> {}
//
