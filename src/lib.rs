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

struct HeaderVecHeader<H> {
    head: H,
    capacity: usize,
    len: usize,
}

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
        header.len = 0;

        this
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.header().len
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.header().capacity
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[T] {
        unsafe { core::slice::from_raw_parts(self.start_ptr(), self.len()) }
    }

    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { core::slice::from_raw_parts_mut(self.start_ptr_mut(), self.len()) }
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

    /// Adds an item to the end of the list.
    ///
    /// Returns `true` if the memory was moved to a new location.
    /// In this case, you are responsible for updating the weak nodes.
    pub fn push(&mut self, item: T) -> Option<*const ()> {
        let old_len = self.len();
        let new_len = old_len + 1;
        self.header_mut().len = new_len;
        let old_capacity = self.capacity();
        // If it isn't big enough.
        let previous_pointer = if new_len > old_capacity {
            // Compute the new capacity.
            let new_capacity = old_capacity * 2;
            // Set the new capacity.
            self.header_mut().capacity = new_capacity;
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

            previous_pointer
        } else {
            None
        };
        unsafe {
            core::ptr::write(self.start_ptr_mut().add(old_len), item);
        }
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
        let original_len = self.len();
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
        self.header_mut().len = head;
    }

    /// Gives the offset in units of T (as if the pointer started at an array of T) that the slice actually starts at.
    #[inline(always)]
    fn offset() -> usize {
        // We need to first compute the first location we can start in align units.
        // Then we go from align units to offset units using mem::align_of::<T>() / mem::size_of::<T>().
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

impl<H, T> Drop for HeaderVec<H, T> {
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(&mut self.header_mut().head);
            for ix in 0..self.len() {
                ptr::drop_in_place(self.start_ptr_mut().add(ix));
            }
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
        let mut new_vec = Self::with_capacity(self.len(), self.header().head.clone());
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
