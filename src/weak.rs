//! Weak reference to a `HeaderVec`.

use core::{
    fmt::Debug,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
};

use crate::HeaderVec;

pub struct HeaderVecWeak<H, T> {
    pub(crate) header_vec: ManuallyDrop<HeaderVec<H, T>>,
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
