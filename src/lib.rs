#![no_std]

extern crate alloc;

use core::{
    cmp,
    fmt::Debug,
    marker::PhantomData,
    mem::{self, ManuallyDrop},
    ops::{Deref, DerefMut, Index, IndexMut},
    ptr,
    slice::SliceIndex,
};

#[cfg(feature = "atomic_append")]
use core::sync::atomic::{AtomicUsize, Ordering};

struct HeaderVecHeader<H> {
    head: H,
    capacity: usize,
    #[cfg(feature = "atomic_append")]
    len: AtomicUsize,
    #[cfg(not(feature = "atomic_append"))]
    len: usize,
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
/// [`HeaderVec`] itself consists solely of a pointer, it's only 8 bytes big.
/// All of the data, like our header `OurHeaderType { a: 2 }`, the length of the vector: `2`,
/// and the contents of the vector `['x', 'z']` resides on the other side of the pointer.
pub struct HeaderVec<H, T> {
    ptr: *mut T,
    _phantom: PhantomData<H>,
}

impl<H, T> HeaderVec<H, T> {
    pub fn new(head: H) -> Self {
        Self::with_capacity(1, head)
    }

    pub fn with_capacity(capacity: usize, head: H) -> Self {
        assert!(capacity > 0, "HeaderVec capacity cannot be 0");
        // Allocate the initial memory, which is unititialized.
        let layout = Self::layout(capacity);
        let ptr = unsafe { alloc::alloc::alloc(layout) } as *mut T;

        // Handle out-of-memory.
        if ptr.is_null() {
            alloc::alloc::handle_alloc_error(layout);
        }

        // Create self.
        let mut this = Self {
            ptr,
            _phantom: PhantomData,
        };

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

    /// Get the length of the vector from a mutable reference.  When one has a `&mut
    /// HeaderVec`, this is the method is always exact and can be slightly faster than the non
    /// mutable `len()`.
    #[cfg(feature = "atomic_append")]
    #[inline(always)]
    pub fn len_exact(&mut self) -> usize {
        *self.header_mut().len.get_mut()
    }
    #[cfg(not(feature = "atomic_append"))]
    #[inline(always)]
    pub fn len_exact(&mut self) -> usize {
        self.header_mut().len
    }

    /// This gives the length of the `HeaderVec`. This is the non synchronized variant may
    /// produce racy results in case another thread atomically appended to
    /// `&self`. Nevertheless it is always safe to use.
    #[cfg(feature = "atomic_append")]
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len_atomic_relaxed()
    }
    #[cfg(not(feature = "atomic_append"))]
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.header().len
    }

    /// This gives the length of the `HeaderVec`. With `atomic_append` enabled this gives a
    /// exact result *after* another thread atomically appended to this `HeaderVec`. It still
    /// requires synchronization because the length may become invalidated when another thread
    /// atomically appends data to this `HeaderVec` while we still work with the result of
    /// this method.
    #[cfg(not(feature = "atomic_append"))]
    #[inline(always)]
    pub fn len_strict(&self) -> usize {
        self.header().len
    }
    #[cfg(feature = "atomic_append")]
    #[inline(always)]
    pub fn len_strict(&self) -> usize {
        self.len_atomic_acquire()
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

    /// Check whenever a `HeaderVec` is empty. see [`len_strict()`] about the exactness guarantees.
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
        unsafe { core::slice::from_raw_parts(self.start_ptr(), self.len_strict()) }
    }

    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { core::slice::from_raw_parts_mut(self.start_ptr_mut(), self.len_exact()) }
    }

    /// This is useful to check if two nodes are the same. Use it with [`HeaderVec::is`].
    #[inline(always)]
    pub fn ptr(&self) -> *const () {
        self.ptr as *const ()
    }

    /// This is used to check if this is the `HeaderVec` that corresponds to the given pointer.
    /// This is useful for updating weak references after [`HeaderVec::push`] returns the pointer.
    #[inline(always)]
    pub fn is(&self, ptr: *const ()) -> bool {
        self.ptr as *const () == ptr
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
            header_vec: ManuallyDrop::new(Self {
                ptr: self.ptr,
                _phantom: PhantomData,
            }),
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
    #[inline(always)]
    pub fn reserve(&mut self, additional: usize) -> Option<*const ()> {
        if self.spare_capacity() < additional {
            let len = self.len_exact();
            unsafe { self.resize_cold(len + additional, false) }
        } else {
            None
        }
    }

    /// Reserves capacity for exactly `additional` more elements to be inserted in the given `HeaderVec`.
    #[inline]
    pub fn reserve_exact(&mut self, additional: usize) -> Option<*const ()> {
        if self.spare_capacity() < additional {
            let len = self.len_exact();
            unsafe { self.resize_cold(len + additional, true) }
        } else {
            None
        }
    }

    /// Shrinks the capacity of the `HeaderVec` to the `min_capacity` or `self.len()`, whichever is larger.
    #[inline]
    pub fn shrink_to(&mut self, min_capacity: usize) -> Option<*const ()> {
        let requested_capacity = self.len_exact().max(min_capacity);
        unsafe { self.resize_cold(requested_capacity, true) }
    }

    /// Resizes the vector hold exactly `self.len()` elements.
    #[inline(always)]
    pub fn shrink_to_fit(&mut self) -> Option<*const ()> {
        let len = self.len_exact();
        self.shrink_to(len)
    }

    /// Resize the vector to have at least room for `additional` more elements.
    /// does exact resizing if `exact` is true.
    ///
    /// Returns `Some(*const ())` if the memory was moved to a new location.
    ///
    /// # Safety
    ///
    /// `requested_capacity` must be greater or equal than `self.len()`
    #[cold]
    unsafe fn resize_cold(&mut self, requested_capacity: usize, exact: bool) -> Option<*const ()> {
        // For efficiency we do only a debug_assert here
        debug_assert!(
            self.len_exact() <= requested_capacity,
            "requested capacity is less than current length"
        );
        let old_capacity = self.capacity();
        debug_assert_ne!(old_capacity, 0, "capacity of 0 not yet supported");
        debug_assert_ne!(requested_capacity, 0, "capacity of 0 not yet supported");

        let new_capacity = if requested_capacity > old_capacity {
            if exact {
                // exact growing
                requested_capacity
            } else if requested_capacity <= old_capacity * 2 {
                // doubling the capacity is sufficient
                old_capacity * 2
            } else {
                // requested more than twice as much space, reserve the next multiple of
                // old_capacity that is greater than the requested capacity. This gives headroom
                // for new inserts while not doubling the memory requirement with bulk requests
                (requested_capacity / old_capacity + 1).saturating_mul(old_capacity)
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
                self.ptr as *mut u8,
                Self::layout(old_capacity),
                Self::elems_to_mem_bytes(new_capacity),
            ) as *mut T
        };
        // Handle out-of-memory.
        if ptr.is_null() {
            alloc::alloc::handle_alloc_error(Self::layout(new_capacity));
        }
        // Check if the new pointer is different than the old one.
        let previous_pointer = if ptr != self.ptr {
            // Give the user the old pointer so they can update everything.
            Some(self.ptr as *const ())
        } else {
            None
        };
        // Assign the new pointer.
        self.ptr = ptr;
        // And set the new capacity.
        self.header_mut().capacity = new_capacity;

        previous_pointer
    }

    /// Adds an item to the end of the list.
    ///
    /// Returns `Some(*const ())` if the memory was moved to a new location.
    /// In this case, you are responsible for updating the weak nodes.
    pub fn push(&mut self, item: T) -> Option<*const ()> {
        let old_len = self.len_exact();
        let new_len = old_len + 1;
        let previous_pointer = self.reserve(1);
        unsafe {
            core::ptr::write(self.start_ptr_mut().add(old_len), item);
        }
        self.header_mut().len = new_len.into();
        previous_pointer
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
        let start_ptr = self.start_ptr_mut();
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

    /// Gives the offset in units of T (as if the pointer started at an array of T) that the slice actually starts at.
    #[inline(always)]
    fn offset() -> usize {
        // The first location, in units of size_of::<T>(), that is after the header
        // It's the end of the header, rounded up to the nearest size_of::<T>()
        (mem::size_of::<HeaderVecHeader<H>>() + mem::size_of::<T>() - 1) / mem::size_of::<T>()
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
            cmp::max(mem::align_of::<H>(), mem::align_of::<T>()),
        )
        .expect("unable to produce memory layout with Hrc key type (is it a zero sized type? they are not permitted)")
    }

    /// Gets the pointer to the start of the slice.
    #[inline(always)]
    fn start_ptr(&self) -> *const T {
        unsafe { self.ptr.add(Self::offset()) }
    }

    /// Gets the pointer to the start of the slice.
    #[inline(always)]
    fn start_ptr_mut(&mut self) -> *mut T {
        unsafe { self.ptr.add(Self::offset()) }
    }

    #[inline(always)]
    fn header(&self) -> &HeaderVecHeader<H> {
        // The beginning of the memory is always the header.
        unsafe { &*(self.ptr as *const HeaderVecHeader<H>) }
    }

    #[inline(always)]
    fn header_mut(&mut self) -> &mut HeaderVecHeader<H> {
        // The beginning of the memory is always the header.
        unsafe { &mut *(self.ptr as *mut HeaderVecHeader<H>) }
    }
}

#[cfg(feature = "atomic_append")]
/// The atomic append API is only enabled when the `atomic_append` feature flag is set (which
/// is the default).
impl<H, T> HeaderVec<H, T> {
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

    #[inline(always)]
    pub fn is_empty_atomic_acquire(&self) -> bool {
        self.len_atomic_acquire() == 0
    }

    #[inline(always)]
    pub fn as_slice_atomic_acquire(&self) -> &[T] {
        unsafe { core::slice::from_raw_parts(self.start_ptr(), self.len_atomic_acquire()) }
    }

    /// Gets the pointer to the end of the slice. This returns a mutable pointer to
    /// uninitialized memory behind the last element.
    #[inline(always)]
    fn end_ptr_atomic_mut(&self) -> *mut T {
        unsafe { self.ptr.add(Self::offset()).add(self.len_atomic_acquire()) }
    }

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
}

impl<H, T> Drop for HeaderVec<H, T> {
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(&mut self.header_mut().head);
            for ix in 0..self.len_exact() {
                ptr::drop_in_place(self.start_ptr_mut().add(ix));
            }
            alloc::alloc::dealloc(self.ptr as *mut u8, Self::layout(self.capacity()));
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
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HeaderVec")
            .field("header", &self.header().head)
            .field("vec", &self.as_slice())
            .finish()
    }
}

pub struct HeaderVecWeak<H, T> {
    header_vec: ManuallyDrop<HeaderVec<H, T>>,
}

impl<H, T> Deref for HeaderVecWeak<H, T> {
    type Target = HeaderVec<H, T>;

    fn deref(&self) -> &Self::Target {
        &self.header_vec
    }
}

impl<H, T> DerefMut for HeaderVecWeak<H, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.header_vec
    }
}

impl<H, T> Debug for HeaderVecWeak<H, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HeaderVecWeak").finish()
    }
}
