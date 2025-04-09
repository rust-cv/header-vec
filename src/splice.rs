#![cfg(feature = "std")]

use core::{any::type_name, fmt, ptr, slice};

use crate::{Drain, WeakFixupFn};

/// A splicing iterator for a `HeaderVec`.
///
/// This struct is created by [`Vec::splice()`].
/// See its documentation for more.
///
/// # Example
///
/// ```
/// # use header_vec::HeaderVec;
/// let mut hv: HeaderVec<(), _> = HeaderVec::from([0, 1, 2]);
/// let new = [7, 8];
/// let iter = hv.splice(1.., new);
/// ```
pub struct Splice<'a, H, I: Iterator + 'a> {
    pub(super) drain: Drain<'a, H, I::Item>,
    pub(super) replace_with: I,
    pub(super) weak_fixup: Option<WeakFixupFn<'a>>,
}

impl<H, I: Iterator> Iterator for Splice<'_, H, I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.drain.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.drain.size_hint()
    }
}

impl<H, I: Iterator> Splice<'_, H, I> {
    /// Not a standard function, might be useful nevertheless, we use it in tests.
    pub fn drained_slice(&self) -> &[I::Item] {
        self.drain.as_slice()
    }
}

impl<H, I: Iterator> DoubleEndedIterator for Splice<'_, H, I> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.drain.next_back()
    }
}

impl<H, I: Iterator> ExactSizeIterator for Splice<'_, H, I> {}

impl<H, I> fmt::Debug for Splice<'_, H, I>
where
    I: Iterator + fmt::Debug,
    I::Item: fmt::Debug,
{
    #[mutants::skip]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(&format!(
            "Splice<{}, {}>",
            type_name::<H>(),
            type_name::<I>()
        ))
        .field("drain", &self.drain.as_slice())
        .field("replace_with", &self.replace_with)
        .field("weak_fixup", &self.weak_fixup.is_some())
        .finish()
    }
}

impl<H, I: Iterator> Drop for Splice<'_, H, I> {
    #[track_caller]
    #[mutants::skip]
    fn drop(&mut self) {
        self.drain.by_ref().for_each(drop);
        // At this point draining is done and the only remaining tasks are splicing
        // and moving things into the final place.
        // Which means we can replace the slice::Iter with pointers that won't point to deallocated
        // memory, so that Drain::drop is still allowed to call iter.len(), otherwise it would break
        // the ptr.sub_ptr contract.

        unsafe {
            let vec = self.drain.vec.as_mut();

            if self.drain.tail_len == 0 {
                vec.extend(self.replace_with.by_ref());
                return;
            }

            // First fill the range left by drain().
            if !self.drain.fill(&mut self.replace_with) {
                return;
            }

            // There may be more elements. Use the lower bound as an estimate.
            // FIXME: Is the upper bound a better guess? Or something else?
            let (lower_bound, _upper_bound) = self.replace_with.size_hint();
            if lower_bound > 0 {
                self.drain.move_tail(lower_bound, &mut self.weak_fixup);
                if !self.drain.fill(&mut self.replace_with) {
                    return;
                }
            }

            // Collect any remaining elements.
            // This is a zero-length vector which does not allocate if `lower_bound` was exact.
            let mut collected = self
                .replace_with
                .by_ref()
                .collect::<Vec<I::Item>>()
                .into_iter();
            // Now we have an exact count.
            if collected.len() > 0 {
                self.drain.move_tail(collected.len(), &mut self.weak_fixup);
                let filled = self.drain.fill(&mut collected);
                debug_assert!(filled);
                debug_assert_eq!(collected.len(), 0);
            }
        }
    }
}

/// Private helper methods for `Splice::drop`
impl<H, T> Drain<'_, H, T> {
    /// The range from `self.vec.len` to `self.tail_start` contains elements
    /// that have been moved out.
    /// Fill that range as much as possible with new elements from the `replace_with` iterator.
    /// Returns `true` if we filled the entire range. (`replace_with.next()` didn’t return `None`.)
    unsafe fn fill<I: Iterator<Item = T>>(&mut self, replace_with: &mut I) -> bool {
        let vec = unsafe { self.vec.as_mut() };
        let range_start = vec.len_exact();
        let range_end = self.tail_start;
        let range_slice = unsafe {
            slice::from_raw_parts_mut(vec.as_mut_ptr().add(range_start), range_end - range_start)
        };

        for place in range_slice {
            if let Some(new_item) = replace_with.next() {
                unsafe { ptr::write(place, new_item) };
                let len = vec.len_exact();
                vec.set_len(len + 1);
            } else {
                return false;
            }
        }
        true
    }

    /// Makes room for inserting more elements before the tail.
    #[track_caller]
    unsafe fn move_tail(&mut self, additional: usize, weak_fixup: &mut Option<WeakFixupFn<'_>>) {
        let vec = unsafe { self.vec.as_mut() };
        let len = self.tail_start + self.tail_len;
        vec.reserve_intern(len + additional, false, weak_fixup);

        let new_tail_start = self.tail_start + additional;
        unsafe {
            let src = vec.as_ptr().add(self.tail_start);
            let dst = vec.as_mut_ptr().add(new_tail_start);
            ptr::copy(src, dst, self.tail_len);
        }
        self.tail_start = new_tail_start;
    }
}
