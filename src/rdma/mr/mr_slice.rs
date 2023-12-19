use std::ptr::NonNull;

use super::{Mr, Slicing};
use crate::bindings::*;

/// Slice of a local memory region.
///
/// A slice corresponds to an RDMA scatter-gather list entry, which can be used
/// in RDMA data-plane verbs.
///
/// **Subtyping:** [`MrSlice<'a>`] is *covariant* over `'a`.
#[derive(Clone)]
pub struct MrSlice<'a> {
    mr: &'a Mr,
    offset: usize,
    len: usize,
}

impl<'a> MrSlice<'a> {
    /// Create a new memory region slice of the given MR, offset, and length.
    pub(crate) fn new(mr: &'a Mr, offset: usize, len: usize) -> Self {
        Self { mr, offset, len }
    }

    /// Get the underlying `Mr`.
    #[inline]
    pub fn mr(&self) -> &Mr {
        self.mr
    }
    /// Attempt to resize the memory region slice to the specified length.
    /// This attempt has no effect if the desired length is greater
    /// than the largest possible length of the slice.
    /// Return whether the resize was successful.
    #[must_use = "must check if the resize was successful"]
    #[inline]
    pub fn resize(&mut self, len: usize) -> bool {
        let max_len = self.mr.len() - self.offset;
        if len <= max_len {
            self.len = len;
            true
        } else {
            false
        }
    }
}

unsafe impl<'a, 's> Slicing<'s> for MrSlice<'a>
where
    'a: 's,
{
    type Output = MrSlice<'s>;

    #[inline]
    fn addr(&'s self) -> *mut u8 {
        (self.mr.addr() as usize + self.offset) as *mut u8
    }

    #[inline]
    fn len(&'s self) -> usize {
        self.len
    }

    #[inline]
    unsafe fn slice_unchecked(&'s self, offset: usize, len: usize) -> Self::Output {
        Self {
            mr: self.mr,
            offset: self.offset + offset,
            len,
        }
    }
}

impl From<MrSlice<'_>> for ibv_sge {
    fn from(slice: MrSlice<'_>) -> Self {
        Self {
            addr: slice.addr() as u64,
            length: slice.len() as u32,
            lkey: slice.mr.lkey(),
        }
    }
}

impl From<MrSlice<'_>> for NonNull<u8> {
    fn from(slice: MrSlice<'_>) -> Self {
        // SAFETY: the address is guaranteed to be non-null.
        unsafe { NonNull::new_unchecked(slice.addr()) }
    }
}
