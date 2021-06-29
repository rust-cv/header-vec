#![no_std]

extern crate alloc;

use core::{
    cmp,
    marker::PhantomData,
    mem,
    ops::{Deref, DerefMut, Index, IndexMut},
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
        // Allocate the initial memory, which is unititialized.
        let layout = Self::layout(1);
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
        header.capacity = 1;
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
        unsafe { core::slice::from_raw_parts(self.ptr.add(Self::offset()), self.len()) }
    }

    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr.add(Self::offset()), self.len()) }
    }

    /// Adds an item to the end of the list.
    ///
    /// Returns `true` if the memory was moved to a new location.
    /// In this case, you are responsible for updating the weak nodes.
    pub fn push(&mut self, item: T) -> bool {
        let old_len = self.len();
        let new_len = old_len + 1;
        self.header_mut().len = new_len;
        let old_capacity = self.capacity();
        // If it isn't big enough.
        let different = if new_len > old_capacity {
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
            let different = ptr != self.ptr;
            // Assign the new pointer.
            self.ptr = ptr;

            different
        } else {
            false
        };
        unsafe {
            core::ptr::write(self.ptr.add(Self::offset()).add(old_len), item);
        }
        different
    }

    /// Gives the offset in units of T (as if the pointer started at an array of T) that the slice actually starts at.
    #[inline(always)]
    fn offset() -> usize {
        // We need to first compute the first location we can start in align units.
        // Then we go from align units to offset units using mem::align_of::<T>() / mem::size_of::<T>().
        (mem::size_of::<HeaderVecHeader<H>>() + mem::align_of::<T>() - 1) / mem::align_of::<T>()
            * mem::align_of::<T>()
            / mem::size_of::<T>()
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
