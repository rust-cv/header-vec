#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use core::{
    convert::{AsRef, From},
    fmt::Debug,
    mem::{self, ManuallyDrop, MaybeUninit},
    ops::{Deref, DerefMut, Index, IndexMut},
    ptr,
    ptr::NonNull,
    slice,
    slice::SliceIndex,
};

#[cfg(feature = "std")]
use std::{
    // core::range::RangeBounds is unstable, we have to rely on std
    ops::{Range, RangeBounds},
};

mod weak;
pub use weak::HeaderVecWeak;

mod drain;
#[cfg(feature = "std")]
pub use drain::Drain;

mod splice;
#[cfg(feature = "std")]
pub use splice::Splice;

// To implement std/Vec compatibility we would need a few nightly features.
// For the time being we just reimplement them here until they become stabilized.
#[cfg(feature = "std")]
mod future_slice;

#[cfg(feature = "atomic_append")]
use core::sync::atomic::{AtomicUsize, Ordering};

/// A closure that becomes called when a `HeaderVec` becomes reallocated.
/// This is closure is responsible for updating weak nodes.
pub type WeakFixupFn<'a> = &'a mut dyn FnMut(*const ());

struct HeaderVecHeader<H> {
    head: H,
    capacity: usize,
    #[cfg(feature = "atomic_append")]
    len: AtomicUsize,
    #[cfg(not(feature = "atomic_append"))]
    len: usize,
}

// This struct will be properly aligned and sized to store headers followed by T's.
#[repr(C)]
struct AlignedHeader<H, T> {
    align: [T; 0],
    header: HeaderVecHeader<H>,
}

/// A vector with a header of your choosing behind a thin pointer
///
/// # Example
///
/// ```
/// use core::mem::size_of_val;
/// use header_vec::HeaderVec;
///
/// #[derive(Debug)]
/// struct OurHeaderType {
///     a: usize,
/// }
///
/// let h = OurHeaderType{ a: 2 };
/// let mut hv = HeaderVec::<OurHeaderType, char>::new(h);
/// hv.push('x');
/// hv.push('z');
/// ```
///
/// [`HeaderVec`] itself consists solely of a non-null pointer, it's only 8 bytes big.
/// All of the data, like our header `OurHeaderType { a: 2 }`, the length of the vector: `2`,
/// and the contents of the vector `['x', 'z']` resides on the other side of the pointer.
pub struct HeaderVec<H, T> {
    ptr: NonNull<AlignedHeader<H, T>>,
}

impl<H, T> HeaderVec<H, T> {
    pub fn new(head: H) -> Self {
        Self::with_capacity(1, head)
    }

    pub fn with_capacity(capacity: usize, head: H) -> Self {
        const { assert!(mem::size_of::<T>() > 0, "HeaderVec does not support ZST's") };
        // Allocate the initial memory, which is uninitialized.
        let layout = Self::layout(capacity);
        let ptr = unsafe { alloc::alloc::alloc(layout) } as *mut AlignedHeader<H, T>;

        let Some(ptr) = NonNull::new(ptr) else {
            // Handle out-of-memory.
            alloc::alloc::handle_alloc_error(layout);
        };

        // Create self.
        let mut this = Self { ptr };

        // Set the header.
        let header = this.header_mut();
        // This makes sure to avoid the fact that the memory is initially uninitialized
        // and we don't want to trigger a call to drop() on uninitialized memory.
        unsafe { core::ptr::write(&mut header.head, head) };
        // These primitive types don't have drop implementations.
        header.capacity = capacity;
        header.len = 0usize.into();

        this
    }

    /// Creates a new `HeaderVec` with the given header from owned elements.
    /// This functions consumes elements from a `IntoIterator<Item = T>` and creates
    /// a `HeaderVec` from these. See [`HeaderVec::from_header_slice()`] which creates a `HeaderVec`
    /// by cloning elements from a slice.
    ///
    /// # Example
    ///
    /// ```
    /// # use header_vec::HeaderVec;
    /// let hv = HeaderVec::from_header_elements(42, [1, 2, 3]);
    /// assert_eq!(hv.as_slice(), [1, 2, 3]);
    /// ```
    pub fn from_header_elements(header: H, elements: impl IntoIterator<Item = T>) -> Self {
        let iter = elements.into_iter();
        let mut hv = HeaderVec::with_capacity(iter.size_hint().0, header);
        hv.extend(iter);
        hv
    }

    /// Get the length of the vector from a mutable reference.  When one has a `&mut
    /// HeaderVec`, this is the method is always exact and can be slightly faster than the non
    /// mutable `len()`.
    #[mutants::skip]
    #[inline(always)]
    pub fn len_exact(&mut self) -> usize {
        #[cfg(feature = "atomic_append")]
        {
            *self.header_mut().len.get_mut()
        }
        #[cfg(not(feature = "atomic_append"))]
        {
            self.header_mut().len
        }
    }

    /// This gives the length of the `HeaderVec`. This is the non synchronized variant may
    /// produce racy results in case another thread atomically appended to
    /// `&self`. Nevertheless it is always safe to use.
    #[mutants::skip]
    #[inline(always)]
    pub fn len(&self) -> usize {
        #[cfg(feature = "atomic_append")]
        {
            self.len_atomic_relaxed()
        }
        #[cfg(not(feature = "atomic_append"))]
        {
            self.header().len
        }
    }

    /// This gives the length of the `HeaderVec`. With `atomic_append` enabled this gives a
    /// exact result *after* another thread atomically appended to this `HeaderVec`. It still
    /// requires synchronization because the length may become invalidated when another thread
    /// atomically appends data to this `HeaderVec` while we still work with the result of
    /// this method.
    #[inline(always)]
    pub fn len_strict(&self) -> usize {
        #[cfg(feature = "atomic_append")]
        {
            self.len_atomic_acquire()
        }
        #[cfg(not(feature = "atomic_append"))]
        {
            self.header().len
        }
    }

    /// Check whenever a `HeaderVec` is empty. This uses a `&mut self` reference and is
    /// always exact and may be slightly faster than the non mutable variant.
    #[inline(always)]
    pub fn is_empty_exact(&mut self) -> bool {
        self.len_exact() == 0
    }

    /// Check whenever a `HeaderVec` is empty. This uses a `&self` reference and may be racy
    /// when another thread atomically appended to this `HeaderVec`.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check whenever a `HeaderVec` is empty. see [`HeaderVec::len_strict()`] about the exactness guarantees.
    #[inline(always)]
    pub fn is_empty_strict(&self) -> bool {
        self.len_strict() == 0
    }

    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.header().capacity
    }

    /// This is the amount of elements that can be added to the `HeaderVec` without reallocation.
    #[inline(always)]
    pub fn spare_capacity(&self) -> usize {
        self.header().capacity - self.len_strict()
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[T] {
        unsafe { core::slice::from_raw_parts(self.as_ptr(), self.len_strict()) }
    }

    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { core::slice::from_raw_parts_mut(self.as_mut_ptr(), self.len_exact()) }
    }

    /// This is useful to check if two nodes are the same. Use it with [`HeaderVec::is`].
    #[inline(always)]
    pub fn ptr(&self) -> *const () {
        self.ptr.as_ptr() as *const ()
    }

    /// This is used to check if this is the `HeaderVec` that corresponds to the given pointer.
    /// This is useful for updating weak references after [`HeaderVec::push`] returns the pointer.
    #[inline(always)]
    pub fn is(&self, ptr: *const ()) -> bool {
        self.ptr() == ptr
    }

    /// Create a (dangerous) weak reference to the `HeaderVec`. This is useful to be able
    /// to create, for instance, graph data structures. Edges can utilize `HeaderVecWeak`
    /// so that they can traverse the graph immutably without needing to go to memory
    /// twice to look up first the pointer to the underlying dynamic edge store (like a `Vec`).
    /// The caveat is that the user is responsible for updating all `HeaderVecWeak` if the
    /// `HeaderVec` needs to reallocate when [`HeaderVec::push`] is called. [`HeaderVec::push`]
    /// returns true when it reallocates, and this indicates that the `HeaderVecWeak` need to be updated.
    /// Therefore, this works best for implemented undirected graphs where it is easy to find
    /// neighbor nodes. Directed graphs with an alternative method to traverse directed edges backwards
    /// should also work with this technique.
    ///
    /// # Safety
    ///
    /// A `HeaderVecWeak` can only be used while its corresponding `HeaderVec` is still alive.
    /// `HeaderVecWeak` also MUST be updated manually by the user when [`HeaderVec::push`] returns `true`,
    /// since the pointer has now changed. As there is no reference counting mechanism, or
    /// method by which all the weak references could be updated, it is up to the user to do this.
    /// That is why this is unsafe. Make sure you update your `HeaderVecWeak` appropriately.
    #[inline(always)]
    pub unsafe fn weak(&self) -> HeaderVecWeak<H, T> {
        HeaderVecWeak {
            header_vec: ManuallyDrop::new(Self { ptr: self.ptr }),
        }
    }

    /// If a `HeaderVec` is updated through a weak reference and reallocates, you must use this method
    /// to update the internal pointer to the `HeaderVec` (along with any other weak references).
    ///
    /// # Safety
    ///
    /// See the safety section in [`HeaderVec::weak`] for an explanation of why this is necessary.
    #[inline(always)]
    pub unsafe fn update(&mut self, weak: HeaderVecWeak<H, T>) {
        self.ptr = weak.ptr;
    }

    /// Reserves capacity for at least `additional` more elements to be inserted in the given `HeaderVec`.
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.reserve_intern(additional, false, &mut None);
    }

    /// Reserves capacity for at least `additional` more elements to be inserted in the given `HeaderVec`.
    /// This method must be used when `HeaderVecWeak` are used. It takes a closure that is responsible for
    /// updating the weak references as additional parameter.
    #[inline]
    pub fn reserve_with_weakfix(&mut self, additional: usize, weak_fixup: WeakFixupFn) {
        self.reserve_intern(additional, false, &mut Some(weak_fixup));
    }

    /// Reserves capacity for exactly `additional` more elements to be inserted in the given `HeaderVec`.
    #[mutants::skip]
    #[inline]
    pub fn reserve_exact(&mut self, additional: usize) {
        self.reserve_intern(additional, true, &mut None);
    }

    /// Reserves capacity for exactly `additional` more elements to be inserted in the given `HeaderVec`.
    /// This method must be used when `HeaderVecWeak` are used. It takes a closure that is responsible for
    /// updating the weak references as additional parameter.
    #[mutants::skip]
    #[inline]
    pub fn reserve_exact_with_weakfix(&mut self, additional: usize, weak_fixup: WeakFixupFn) {
        self.reserve_intern(additional, true, &mut Some(weak_fixup));
    }

    /// Reserves capacity for at least `additional` more elements to be inserted in the given `HeaderVec`.
    #[inline(always)]
    pub(crate) fn reserve_intern(
        &mut self,
        additional: usize,
        exact: bool,
        weak_fixup: &mut Option<WeakFixupFn>,
    ) {
        if self.spare_capacity() < additional {
            let len = self.len_exact();
            // using saturating_add here ensures that we get a allocation error instead wrapping over and
            // allocating a total wrong size
            unsafe { self.resize_cold(len.saturating_add(additional), exact, weak_fixup) };
        }
    }

    /// Shrinks the capacity of the `HeaderVec` to the `min_capacity` or `self.len()`, whichever is larger.
    #[inline]
    pub fn shrink_to(&mut self, min_capacity: usize) {
        let requested_capacity = self.len_exact().max(min_capacity);
        unsafe { self.resize_cold(requested_capacity, true, &mut None) };
    }

    /// Shrinks the capacity of the `HeaderVec` to the `min_capacity` or `self.len()`, whichever is larger.
    /// This method must be used when `HeaderVecWeak` are used. It takes a closure that is responsible for
    /// updating the weak references as additional parameter.
    #[inline]
    pub fn shrink_to_with_weakfix(&mut self, min_capacity: usize, weak_fixup: WeakFixupFn) {
        let requested_capacity = self.len_exact().max(min_capacity);
        unsafe { self.resize_cold(requested_capacity, true, &mut Some(weak_fixup)) };
    }

    /// Resizes the vector hold exactly `self.len()` elements.
    #[mutants::skip]
    #[inline(always)]
    pub fn shrink_to_fit(&mut self) {
        self.shrink_to(0);
    }

    /// Resizes the vector hold exactly `self.len()` elements.
    /// This method must be used when `HeaderVecWeak` are used. It takes a closure that is responsible for
    /// updating the weak references as additional parameter.
    #[mutants::skip]
    #[inline(always)]
    pub fn shrink_to_fit_with_weakfix(&mut self, weak_fixup: WeakFixupFn) {
        self.shrink_to_with_weakfix(0, weak_fixup);
    }

    /// Resize the vector to least `requested_capacity` elements.
    /// Does exact resizing if `exact` is true.
    ///
    /// Returns `Some(*const ())` if the memory was moved to a new location.
    ///
    /// # Safety
    ///
    /// `requested_capacity` must be greater or equal than `self.len()`
    #[cold]
    unsafe fn resize_cold(
        &mut self,
        requested_capacity: usize,
        exact: bool,
        weak_fixup: &mut Option<WeakFixupFn>,
    ) {
        // For efficiency we do only a debug_assert here, this is a internal unsafe function
        // it's contract should be already enforced by the caller which is under our control
        debug_assert!(
            self.len_exact() <= requested_capacity,
            "requested capacity is less than current length"
        );
        let old_capacity = self.capacity();

        // Shortcut when nothing is to be done.
        if requested_capacity == old_capacity {
            return;
        }

        let new_capacity = if requested_capacity > old_capacity {
            if exact {
                // exact growing
                requested_capacity
            } else if requested_capacity <= old_capacity * 2 {
                // doubling the capacity is sufficient
                old_capacity * 2
            } else if old_capacity > 0 {
                // requested more than twice as much space, reserve the next multiple of
                // old_capacity that is greater than the requested capacity. This gives headroom
                // for new inserts while not doubling the memory requirement with bulk requests
                (requested_capacity / old_capacity + 1).saturating_mul(old_capacity)
            } else {
                // special case when we start at capacity 0
                requested_capacity
            }
        } else if exact {
            // exact shrinking
            requested_capacity
        } else {
            unimplemented!()
            // or: (has no public API yet)
            // // shrink to the next power of two or self.capacity, whichever is smaller
            // requested_capacity.next_power_of_two().min(self.capacity())
        };
        // Reallocate the pointer.
        let ptr = unsafe {
            alloc::alloc::realloc(
                self.ptr() as *mut u8,
                Self::layout(old_capacity),
                Self::elems_to_mem_bytes(new_capacity),
            ) as *mut AlignedHeader<H, T>
        };

        let Some(ptr) = NonNull::new(ptr) else {
            // Handle out-of-memory.
            alloc::alloc::handle_alloc_error(Self::layout(new_capacity));
        };

        // Check if the new pointer is different than the old one.
        let previous_pointer = if ptr != self.ptr {
            // Store old pointer for weak_fixup.
            Some(self.ptr())
        } else {
            None
        };
        // Assign the new pointer.
        self.ptr = ptr;
        // And set the new capacity.
        self.header_mut().capacity = new_capacity;

        // Finally run the weak_fixup closure when provided
        previous_pointer.map(|ptr| weak_fixup.as_mut().map(|weak_fixup| weak_fixup(ptr)));
    }

    /// Adds an item to the end of the list.
    pub fn push(&mut self, item: T) {
        self.push_intern(item, &mut None);
    }

    /// Adds an item to the end of the list.
    /// This method must be used when `HeaderVecWeak` are used. It takes a closure that is responsible for
    /// updating the weak references as additional parameter.
    pub fn push_with_weakfix(&mut self, item: T, weak_fixup: WeakFixupFn) {
        self.push_intern(item, &mut Some(weak_fixup));
    }

    #[inline(always)]
    fn push_intern(&mut self, item: T, weak_fixup: &mut Option<WeakFixupFn>) {
        let old_len = self.len_exact();
        let new_len = old_len + 1;
        self.reserve_intern(1, false, weak_fixup);
        unsafe {
            core::ptr::write(self.as_mut_ptr().add(old_len), item);
        }
        self.header_mut().len = new_len.into();
    }

    /// Retains only the elements specified by the predicate.
    ///
    /// In other words, remove all elements `e` such that `f(&e)` returns `false`.
    /// This method operates in place, visiting each element exactly once in the original order,
    /// and preserves the order of the retained elements.
    pub fn retain(&mut self, mut f: impl FnMut(&T) -> bool) {
        // This keeps track of the length (and next position) of the contiguous retained elements
        // at the beginning of the vector.
        let mut head = 0;
        let original_len = self.len_exact();
        // Get the offset of the beginning of the slice.
        let start_ptr = self.as_mut_ptr();
        // Go through each index.
        for index in 0..original_len {
            unsafe {
                // Call the retain function on the derefed pointer to each index.
                if f(&*start_ptr.add(index)) {
                    // If the head and index are at different indices, the memory needs to be copied to be retained.
                    if head != index {
                        ptr::copy_nonoverlapping(start_ptr.add(index), start_ptr.add(head), 1);
                    }
                    // In either case, the head needs to move forwards since we now have a new item at
                    // the end of the contiguous retained items.
                    head += 1;
                } else {
                    // In this case, we just need to drop the item at the address.
                    ptr::drop_in_place(start_ptr.add(index));
                }
            }
        }
        // The head now represents the new length of the vector.
        self.header_mut().len = head.into();
    }

    /// Returns the remaining spare capacity of the vector as a slice of
    /// `MaybeUninit<T>`.
    ///
    /// The returned slice can be used to fill the vector with data (e.g. by
    /// reading from a file) before marking the data as initialized using the
    /// [`HeaderVec::set_len()`] method.
    ///
    pub fn spare_capacity_mut(&mut self) -> &mut [MaybeUninit<T>] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.end_ptr_mut() as *mut MaybeUninit<T>,
                self.spare_capacity(),
            )
        }
    }

    /// Forces the length of the headervec to `new_len`.
    ///
    /// This is a low-level operation that maintains none of the normal
    /// invariants of the type. Normally changing the length of a vector
    /// is done using one of the safe operations instead. Noteworthy is that
    /// this method does not drop any of the elements that are removed when
    /// shrinking the vector.
    ///
    /// # Safety
    ///
    /// - `new_len` must be less than or equal to [`HeaderVec::capacity()`].
    /// - The elements at `old_len..new_len` must be initialized.
    pub unsafe fn set_len(&mut self, new_len: usize) {
        debug_assert!(
            new_len <= self.capacity(),
            "new_len [{new_len}] is greater than capacity [{}]",
            self.capacity()
        );
        self.header_mut().len = new_len.into();
    }

    /// Shortens a `HeaderVec`, keeping the first `len` elements and dropping
    /// the rest.
    ///
    /// If `len` is greater or equal to the vector's current length, this has
    /// no effect.
    ///
    /// The [`drain`] method can emulate `truncate`, but causes the excess
    /// elements to be returned instead of dropped.
    ///
    /// Note that this method has no effect on the allocated capacity
    /// of the vector.
    ///
    /// # Examples
    ///
    /// Truncating a five element `HeaderVec` to two elements:
    ///
    /// ```
    /// use header_vec::HeaderVec;
    /// let mut hv: HeaderVec<(), _> = HeaderVec::from([1, 2, 3, 4, 5]);
    /// hv.truncate(2);
    /// assert_eq!(hv.as_slice(), [1, 2]);
    /// ```
    ///
    /// No truncation occurs when `len` is greater than the vector's current
    /// length:
    ///
    /// ```
    /// use header_vec::HeaderVec;
    /// let mut hv: HeaderVec<(), _> = HeaderVec::from([1, 2, 3]);
    /// hv.truncate(8);
    /// assert_eq!(hv.as_slice(), [1, 2, 3]);
    /// ```
    ///
    /// Truncating when `len == 0` is equivalent to calling the [`clear`]
    /// method.
    ///
    /// ```
    /// use header_vec::HeaderVec;
    /// let mut hv: HeaderVec<(), _> = HeaderVec::from([1, 2, 3]);
    /// hv.truncate(0);
    /// assert_eq!(hv.as_slice(), []);
    /// ```
    ///
    /// [`clear`]: HeaderVec::clear
    /// [`drain`]: HeaderVec::drain
    #[mutants::skip]
    pub fn truncate(&mut self, len: usize) {
        unsafe {
            let old_len = self.len_exact();
            if len > old_len {
                return;
            }
            let remaining_len = old_len - len;
            let s = ptr::slice_from_raw_parts_mut(self.as_mut_ptr().add(len), remaining_len);
            self.header_mut().len = len.into();
            ptr::drop_in_place(s);
        }
    }

    /// Clears a `HeaderVec`, removing all values.
    ///
    /// Note that this method has no effect on the allocated capacity
    /// of the vector.
    ///
    /// # Examples
    ///
    /// ```
    /// use header_vec::HeaderVec;
    /// let mut hv: HeaderVec<(), _> = HeaderVec::from([1, 2, 3]);
    ///
    /// hv.clear();
    ///
    /// assert!(hv.is_empty());
    /// ```
    #[inline]
    pub fn clear(&mut self) {
        let elems: *mut [T] = self.as_mut_slice();

        // SAFETY:
        // - `elems` comes directly from `as_mut_slice` and is therefore valid.
        // - Setting the length before calling `drop_in_place` means that,
        //   if an element's `Drop` impl panics, the vector's `Drop` impl will
        //   do nothing (leaking the rest of the elements) instead of dropping
        //   some twice.
        unsafe {
            self.set_len(0);
            ptr::drop_in_place(elems);
        }
    }

    /// Consumes a `HeaderVec`, returning references to the header and data.
    ///
    /// Note that the header type H must outlive the chosen lifetime 'a and the data type T
    /// must outlive the chosen lifetime 'b.  When the types have only static references, or
    /// none at all, then these may be chosen to be 'static.
    ///
    /// This method does not reallocate or shrink the `HeaderVec`, so the leaked allocation
    /// may include unused capacity that is not part of the returned slice.
    ///
    /// This function is mainly useful for data that lives for the remainder of the program’s
    /// life. Dropping the returned references will cause a memory leak.
    ///
    /// # Example
    ///
    //  This example can't be run in miri because it leaks memory.
    /// ```
    /// # #[cfg(miri)] fn main() {}
    /// # #[cfg(not(miri))]
    /// # fn main() {
    /// use header_vec::HeaderVec;
    ///
    /// let mut hv = HeaderVec::from_header_elements(42, [1, 2, 3]);
    /// let (header, data) = hv.leak();
    /// assert_eq!(header, &42);
    /// assert_eq!(data, &[1, 2, 3]);
    /// # }
    /// ```
    pub fn leak<'a,'b>(mut self) -> (&'a H, &'b mut [T]) {
        let len = self.len_exact();
        let ptr = self.as_mut_ptr();
        let header = &mut self.header_mut().head as *mut H;
        let slice = unsafe { slice::from_raw_parts_mut(ptr, len) };
        mem::forget(self);
        (unsafe {header.as_mut().unwrap_unchecked()}, slice)
    }

    /// Gives the offset in units of T (as if the pointer started at an array of T) that the slice actually starts at.
    #[mutants::skip]
    #[inline(always)]
    const fn offset() -> usize {
        // The first location, in units of size_of::<T>(), that is after the header
        // It's the end of the header, rounded up to the nearest size_of::<T>()
        (mem::size_of::<AlignedHeader<H, T>>() - 1) / mem::size_of::<T>() + 1
    }

    /// Compute the number of elements (in units of T) to allocate for a given capacity.
    #[inline(always)]
    fn elems_to_mem_elems(capacity: usize) -> usize {
        Self::offset() + capacity
    }

    /// Compute the number of elements (in units of T) to allocate for a given capacity.
    #[inline(always)]
    fn elems_to_mem_bytes(capacity: usize) -> usize {
        Self::elems_to_mem_elems(capacity) * mem::size_of::<T>()
    }

    /// Compute the number of elements (in units of T) to allocate for a given capacity.
    #[inline(always)]
    fn layout(capacity: usize) -> alloc::alloc::Layout {
        alloc::alloc::Layout::from_size_align(
            Self::elems_to_mem_bytes(capacity),
            mem::align_of::<AlignedHeader<H,T>>()
        )
        .expect("unable to produce memory layout with Hrc key type (is it a zero sized type? they are not permitted)")
    }

    /// Gets the pointer to the start of the slice.
    #[inline(always)]
    pub fn as_ptr(&self) -> *const T {
        unsafe { (self.ptr() as *const T).add(Self::offset()) }
    }

    /// Gets the pointer to the start of the slice.
    #[inline(always)]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        unsafe { (self.ptr() as *mut T).add(Self::offset()) }
    }

    /// Gets the pointer to the end of the slice. This returns a mutable pointer to
    /// uninitialized memory behind the last element.
    #[inline(always)]
    fn end_ptr_mut(&mut self) -> *mut T {
        unsafe { self.as_mut_ptr().add(self.len_exact()) }
    }

    #[inline(always)]
    fn header(&self) -> &HeaderVecHeader<H> {
        // The beginning of the memory is always the header.
        unsafe { &*(self.ptr() as *const HeaderVecHeader<H>) }
    }

    #[inline(always)]
    fn header_mut(&mut self) -> &mut HeaderVecHeader<H> {
        // The beginning of the memory is always the header.
        unsafe { &mut *(self.ptr() as *mut HeaderVecHeader<H>) }
    }
}

impl<H, T: Clone> HeaderVec<H, T> {
    /// Creates a new `HeaderVec` with the given header from some data.
    /// The data cloned from a `AsRef<[T]>`, see [`HeaderVec::from_header_elements()`] for
    /// constructing a `HeaderVec` from owned elements.
    pub fn from_header_slice(header: H, slice: impl AsRef<[T]>) -> Self {
        let slice = slice.as_ref();
        let mut hv = Self::with_capacity(slice.len(), header);
        hv.extend_from_slice_intern(slice, &mut None);
        hv
    }

    /// Adds items from a slice to the end of the list.
    pub fn extend_from_slice(&mut self, slice: impl AsRef<[T]>) {
        self.extend_from_slice_intern(slice.as_ref(), &mut None)
    }

    /// Adds items from a slice to the end of the list.
    /// This method must be used when `HeaderVecWeak` are used. It takes a closure that is responsible for
    /// updating the weak references as additional parameter.
    pub fn extend_from_slice_with_weakfix(
        &mut self,
        slice: impl AsRef<[T]>,
        weak_fixup: WeakFixupFn,
    ) {
        self.extend_from_slice_intern(slice.as_ref(), &mut Some(weak_fixup));
    }

    #[inline(always)]
    fn extend_from_slice_intern(&mut self, slice: &[T], weak_fixup: &mut Option<WeakFixupFn>) {
        self.reserve_intern(slice.len(), false, weak_fixup);

        // copy data
        let end_ptr = self.end_ptr_mut();
        for (index, item) in slice.iter().enumerate() {
            unsafe {
                core::ptr::write(end_ptr.add(index), item.clone());
            }
        }
        // correct the len
        self.header_mut().len = (self.len_exact() + slice.len()).into();
    }
}

#[cfg(feature = "atomic_append")]
/// The atomic append API is only enabled when the `atomic_append` feature flag is set (which
/// is the default). The [`HeaderVec::push_atomic()`] or [`HeaderVec::extend_from_slice_atomic()`] methods then
/// become available and some internals using atomic operations.
///
/// This API implements interior-mutable appending to a shared `HeaderVec`. To other threads
/// the appended elements are either not seen or all seen at once. Without additional
/// synchronization these appends are racy but memory safe. The intention behind this API is to
/// provide facilities for building other container abstractions the benefit from the shared
/// non blocking nature while being unaffected from the racy semantics or provide synchronization
/// on their own (Eg: reference counted data, interners, streaming parsers, etc). Since the
/// `HeaderVec` is a shared object and we have only a `&self`, it can not be reallocated and moved,
/// therefore appending can only be done within the reserved capacity.
///
/// # Safety
///
/// Only one single thread must try to [`HeaderVec::push_atomic()`] or [`HeaderVec::extend_from_slice_atomic()`] the
/// `HeaderVec` at at time using the atomic append API's. The actual implementations of this
/// restriction is left to the caller.  This can be done by mutexes or guard objects. Or
/// simply by staying single threaded or ensuring somehow else that there is only a single
/// thread using the atomic_appending API.
impl<H, T> HeaderVec<H, T> {
    /// Atomically adds an item to the end of the list without reallocation.
    ///
    /// # Errors
    ///
    /// If the vector is full, the item is returned.
    ///
    /// # Safety
    ///
    /// There must be only one thread calling this method at any time. Synchronization has to
    /// be provided by the user.
    pub unsafe fn push_atomic(&self, item: T) -> Result<(), T> {
        // relaxed is good enough here because this should be the only thread calling this method.
        let len = self.len_atomic_relaxed();
        if len < self.capacity() {
            unsafe {
                core::ptr::write(self.end_ptr_atomic_mut(), item);
            };
            let len_again = self.len_atomic_add_release(1);
            // in debug builds we check for races, the chance to catch these are still pretty minimal
            debug_assert_eq!(len_again, len, "len was updated by another thread");
            Ok(())
        } else {
            Err(item)
        }
    }

    /// Get the length of the vector with `Ordering::Acquire`. This ensures that the length is
    /// properly synchronized after it got atomically updated.
    #[inline(always)]
    fn len_atomic_acquire(&self) -> usize {
        self.header().len.load(Ordering::Acquire)
    }

    /// Get the length of the vector with `Ordering::Relaxed`. This is useful for when you don't
    /// need exact synchronization semantic.
    #[inline(always)]
    fn len_atomic_relaxed(&self) -> usize {
        self.header().len.load(Ordering::Relaxed)
    }

    /// Add `n` to the length of the vector atomically with `Ordering::Release`.
    ///
    /// # Safety
    ///
    /// Before incrementing the length of the vector, you must ensure that new elements are
    /// properly initialized.
    #[inline(always)]
    unsafe fn len_atomic_add_release(&self, n: usize) -> usize {
        self.header().len.fetch_add(n, Ordering::Release)
    }

    /// Gets the pointer to the end of the slice. This returns a mutable pointer to
    /// uninitialized memory behind the last element.
    #[inline(always)]
    fn end_ptr_atomic_mut(&self) -> *mut T {
        unsafe { self.as_ptr().add(self.len_atomic_acquire()) as *mut T }
    }
}

#[cfg(feature = "atomic_append")]
impl<H, T: Clone> HeaderVec<H, T> {
    /// Atomically add items from a slice to the end of the list. without reallocation
    ///
    /// # Errors
    ///
    /// If the vector is full, the item is returned.
    ///
    /// # Safety
    ///
    /// There must be only one thread calling this method at any time. Synchronization has to
    /// be provided by the user.
    pub unsafe fn extend_from_slice_atomic<'a>(&self, slice: &'a [T]) -> Result<(), &'a [T]> {
        #[cfg(debug_assertions)] // only for the race check later
        let len = self.len_atomic_relaxed();
        if self.spare_capacity() >= slice.len() {
            // copy data
            let end_ptr = self.end_ptr_atomic_mut();
            for (index, item) in slice.iter().enumerate() {
                unsafe {
                    core::ptr::write(end_ptr.add(index), item.clone());
                }
            }
            // correct the len
            let _len_again = self.len_atomic_add_release(slice.len());
            // in debug builds we check for races, the chance to catch these are still pretty minimal
            #[cfg(debug_assertions)]
            debug_assert_eq!(_len_again, len, "len was updated by another thread");
            Ok(())
        } else {
            Err(slice)
        }
    }
}

#[cfg(feature = "std")]
/// The methods that depend on stdlib features.
impl<H, T> HeaderVec<H, T> {
    /// Removes the specified range from a `HeaderVec` in bulk, returning all
    /// removed elements as an iterator. If the iterator is dropped before
    /// being fully consumed, it drops the remaining removed elements.
    ///
    /// The returned iterator keeps a mutable borrow on the `HeaderVec` to optimize
    /// its implementation.
    ///
    /// # Feature compatibility
    ///
    /// The `drain()` API and `Drain` iterator are only available when the `std` feature is
    /// enabled.
    ///
    /// # Panics
    ///
    /// Panics if the starting point is greater than the end point or if
    /// the end point is greater than the length of the vector.
    ///
    /// # Leaking
    ///
    /// If the returned iterator goes out of scope without being dropped (due to
    /// [`mem::forget`], for example), the vector may have lost and leaked
    /// elements arbitrarily, including elements outside the range.
    ///
    /// # Examples
    ///
    /// ```
    /// use header_vec::HeaderVec;
    /// let mut v: HeaderVec<(), _> = HeaderVec::from(&[1, 2, 3]);
    /// let u: Vec<_> = v.drain(1..).collect();
    /// assert_eq!(v.as_slice(), &[1]);
    /// assert_eq!(u.as_slice(), &[2, 3]);
    ///
    /// // A full range clears the vector, like `clear()` does
    /// v.drain(..);
    /// assert_eq!(v.as_slice(), &[]);
    /// ```
    pub fn drain<R>(&mut self, range: R) -> Drain<'_, H, T>
    where
        R: RangeBounds<usize>,
    {
        // Memory safety
        //
        // When the Drain is first created, it shortens the length of
        // the source vector to make sure no uninitialized or moved-from elements
        // are accessible at all if the Drain's destructor never gets to run.
        //
        // Drain will ptr::read out the values to remove.
        // When finished, remaining tail of the vec is copied back to cover
        // the hole, and the vector length is restored to the new length.
        //
        let len = self.len();
        let Range { start, end } = future_slice::range(range, ..len);

        unsafe {
            // set self.vec length's to start, to be safe in case Drain is leaked
            self.set_len(start);
            let range_slice = slice::from_raw_parts(self.as_ptr().add(start), end - start);
            Drain {
                tail_start: end,
                tail_len: len - end,
                iter: range_slice.iter(),
                vec: NonNull::from(self),
            }
        }
    }

    /// Creates a splicing iterator that replaces the specified range in the vector
    /// with the given `replace_with` iterator and yields the removed items.
    /// `replace_with` does not need to be the same length as `range`.
    ///
    /// `range` is removed even if the iterator is not consumed until the end.
    ///
    /// It is unspecified how many elements are removed from the vector
    /// if the `Splice` value is leaked.
    ///
    /// The input iterator `replace_with` is only consumed when the `Splice` value is dropped.
    ///
    /// This is optimal if:
    ///
    /// * The tail (elements in the vector after `range`) is empty,
    /// * or `replace_with` yields fewer or equal elements than `range`’s length
    /// * or the lower bound of its `size_hint()` is exact.
    ///
    /// Otherwise, a temporary vector is allocated to store the tail elements which are in the way.
    ///
    /// # Panics
    ///
    /// Panics if the starting point is greater than the end point or if
    /// the end point is greater than the length of the vector.
    ///
    /// # Examples
    ///
    /// ```
    /// use header_vec::HeaderVec;
    /// let mut hv: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3, 4]);
    /// let new = [7, 8, 9];
    /// let u: Vec<_> = hv.splice(1..3, new).collect();
    /// assert_eq!(hv.as_slice(), [1, 7, 8, 9, 4]);
    /// assert_eq!(u, [2, 3]);
    /// ```
    #[inline]
    pub fn splice<R, I>(&mut self, range: R, replace_with: I) -> Splice<'_, H, I::IntoIter>
    where
        R: RangeBounds<usize>,
        I: IntoIterator<Item = T>,
    {
        self.splice_internal(range, replace_with, None)
    }

    /// Creates a splicing iterator like [`HeaderVec::splice()`].
    /// This method must be used when `HeaderVecWeak` are used. It takes a closure that is responsible for
    /// updating the weak references as additional parameter.
    #[inline]
    pub fn splice_with_weakfix<'a, R, I>(
        &'a mut self,
        range: R,
        replace_with: I,
        weak_fixup: WeakFixupFn<'a>,
    ) -> Splice<'a, H, I::IntoIter>
    where
        R: RangeBounds<usize>,
        I: IntoIterator<Item = T>,
    {
        self.splice_internal(range, replace_with, Some(weak_fixup))
    }

    #[inline(always)]
    fn splice_internal<'a, R, I>(
        &'a mut self,
        range: R,
        replace_with: I,
        weak_fixup: Option<WeakFixupFn<'a>>,
    ) -> Splice<'a, H, I::IntoIter>
    where
        R: RangeBounds<usize>,
        I: IntoIterator<Item = T>,
    {
        Splice {
            drain: self.drain(range),
            replace_with: replace_with.into_iter(),
            weak_fixup,
        }
    }
}

impl<H, T> Drop for HeaderVec<H, T> {
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(self.as_mut_slice());
            ptr::drop_in_place(&mut self.header_mut().head);
            alloc::alloc::dealloc(self.ptr() as *mut u8, Self::layout(self.capacity()));
        }
    }
}

impl<H, T> Deref for HeaderVec<H, T> {
    type Target = H;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.header().head
    }
}

impl<H, T> DerefMut for HeaderVec<H, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.header_mut().head
    }
}

impl<H, T, I> Index<I> for HeaderVec<H, T>
where
    I: SliceIndex<[T]>,
{
    type Output = I::Output;

    #[inline(always)]
    fn index(&self, index: I) -> &I::Output {
        self.as_slice().index(index)
    }
}

impl<H, T, I> IndexMut<I> for HeaderVec<H, T>
where
    I: SliceIndex<[T]>,
{
    #[inline(always)]
    fn index_mut(&mut self, index: I) -> &mut I::Output {
        self.as_mut_slice().index_mut(index)
    }
}

impl<H, T> PartialEq for HeaderVec<H, T>
where
    H: PartialEq,
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.header().head == other.header().head && self.as_slice() == other.as_slice()
    }
}

impl<H, T> Clone for HeaderVec<H, T>
where
    H: Clone,
    T: Clone,
{
    fn clone(&self) -> Self {
        let mut new_vec = Self::with_capacity(self.len_strict(), self.header().head.clone());
        for e in self.as_slice() {
            new_vec.push(e.clone());
        }
        new_vec
    }
}

impl<H, T> Debug for HeaderVec<H, T>
where
    H: Debug,
    T: Debug,
{
    #[mutants::skip]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HeaderVec")
            .field("header", &self.header().head)
            .field("vec", &self.as_slice())
            .finish()
    }
}

impl<H: Default, T: Clone, U> From<U> for HeaderVec<H, T>
where
    U: AsRef<[T]>,
{
    fn from(from: U) -> Self {
        HeaderVec::from_header_slice(H::default(), from)
    }
}

impl<H, T> HeaderVec<H, T> {
    pub fn iter(&self) -> slice::Iter<'_, T> {
        self.as_slice().iter()
    }

    pub fn iter_mut(&mut self) -> slice::IterMut<'_, T> {
        self.as_mut_slice().iter_mut()
    }
}

impl<'a, H, T> IntoIterator for &'a HeaderVec<H, T> {
    type Item = &'a T;
    type IntoIter = slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, H, T> IntoIterator for &'a mut HeaderVec<H, T> {
    type Item = &'a mut T;
    type IntoIter = slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<H, T> Extend<T> for HeaderVec<H, T> {
    #[inline]
    #[track_caller]
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        let iter = iter.into_iter();
        self.reserve(iter.size_hint().0);
        iter.for_each(|item| self.push(item));
    }
}

/// Extend implementation that copies elements out of references before pushing them onto the Vec.
impl<'a, H, T: Copy + 'a> Extend<&'a T> for HeaderVec<H, T> {
    #[track_caller]
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        let iter = iter.into_iter();
        self.reserve(iter.size_hint().0);
        iter.for_each(|item| self.push(*item));
    }
}

/// Creates a HeaderVec with an optional header and elements. Similar to what the stdlib
/// `vec!()` macro does for `Vec`. When no header is provided, the unit `()` is used.
/// Note that the syntax differs slightly from the `vec!()` macro. When a header is provided, it
/// must be followed by a semicolon before the elements are in a square bracket list. This is to
/// distinguish between the header and the elements.
///
/// # Examples
///
/// ```
/// # use header_vec::header_vec;
/// // Create a HeaderVec with default header `()`
/// let v = header_vec![1, 2, 3];
/// assert_eq!(*v, ());
/// assert_eq!(v.as_slice(), &[1, 2, 3]);
///
/// // Create a HeaderVec with a custom header
/// let v = header_vec!("header"; [1, 2, 3]);
/// assert_eq!(*v, "header");
/// assert_eq!(v.as_slice(), &[1, 2, 3]);
///
/// // Create a HeaderVec with repetition (default header `()`)
/// let v = header_vec![42; 5];
/// assert_eq!(*v, ());
/// assert_eq!(v.as_slice(), &[42, 42, 42, 42, 42]);
///
/// // Create a HeaderVec with custom header and repetition
/// let v = header_vec!("header"; [42; 5]);
/// assert_eq!(*v, "header");
/// assert_eq!(v.as_slice(), &[42, 42, 42, 42, 42]);
/// ```
#[macro_export]
macro_rules! header_vec {
    () => {
        $crate::HeaderVec::new(())
    };

    ($header:expr; []) => {
        $crate::HeaderVec::new($header)
    };

    ($header:expr; [$($elem:expr),+ $(,)?]) => {{
        let mut vec = $crate::HeaderVec::new($header);
        vec.extend(IntoIterator::into_iter([$($elem),+]));
        vec
    }};

    ($header:expr; [$elem:expr; $n:expr]) => {{
        let mut vec = $crate::HeaderVec::new($header);
        vec.extend(std::iter::repeat($elem).take($n));
        vec
    }};

    ($elem:expr; $n:expr) => {{
        let mut vec = $crate::HeaderVec::new(());
        vec.extend(std::iter::repeat($elem).take($n));
        vec
    }};

    ($($elem:expr),+ $(,)?) => {{
        let mut vec = $crate::HeaderVec::new(());
        vec.extend(IntoIterator::into_iter([$($elem),+]));
        vec
    }};
}
