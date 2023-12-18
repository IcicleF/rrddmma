use super::*;
use std::ops::{Bound, Range, RangeBounds};

/// Clip a range to the given upper-bound.
#[inline]
fn clip_range(r: impl RangeBounds<usize>, upper: usize) -> Range<usize> {
    let start = match r.start_bound() {
        Bound::Included(&s) => s,
        Bound::Excluded(&s) => s + 1,
        Bound::Unbounded => 0,
    };
    let end = match r.end_bound() {
        Bound::Included(&e) => e + 1,
        Bound::Excluded(&e) => e,
        Bound::Unbounded => upper,
    };

    start..end
}

/// A slicable memory region.
///
/// The trait is sealed and cannot be implemented outside of this crate.
pub trait Slicing<'s>: Sealed {
    type Output: 's;

    /// Get the starting address of the memory region.
    fn addr(&'s self) -> *mut u8;

    /// Get the length of the memory region.
    fn len(&'s self) -> usize;

    /// Get a slice from an offset and a length.
    /// Return `None` if the range is out of bounds.
    fn slice(&'s self, offset: usize, len: usize) -> Option<Self::Output> {
        if offset < self.len() && len <= self.len() - offset {
            Some(unsafe { self.slice_unchecked(offset, len) })
        } else {
            None
        }
    }

    /// Get a slice from a range.
    /// Return `None` if the range is out of bounds.
    fn slice_by_range(&'s self, range: impl RangeBounds<usize>) -> Option<Self::Output> {
        let r = clip_range(range, self.len());
        self.slice(r.start, r.len())
    }

    /// Get a slice from a pointer and a length.
    /// Return `None` if the range is out of bounds.
    fn slice_by_ptr(&'s self, pointer: *mut u8, len: usize) -> Option<Self::Output> {
        if pointer >= self.addr() {
            self.slice((pointer as usize) - (self.addr() as usize), len)
        } else {
            None
        }
    }

    /// Get a slice from an offset and a length.
    ///
    /// # Safety
    ///
    /// - The specified range must be within the bounds of the memory region.
    unsafe fn slice_unchecked(&'s self, offset: usize, len: usize) -> Self::Output;

    /// Get a slice from a range.
    ///
    /// # Safety
    ///
    /// - The specified range must be within the bounds of the memory region.
    unsafe fn slice_by_range_unchecked(&'s self, range: impl RangeBounds<usize>) -> Self::Output {
        let r = clip_range(range, self.len());
        self.slice_unchecked(r.start, r.len())
    }

    /// Get a slice from a pointer and a length.
    ///
    /// # Safety
    ///
    /// - The specified range must be within the bounds of the memory region.
    unsafe fn slice_by_ptr_unchecked(&'s self, pointer: *mut u8, len: usize) -> Self::Output {
        self.slice_unchecked((pointer as usize) - (self.addr() as usize), len)
    }
}

trait Sealed {}

impl Sealed for Mr<'_> {}
impl Sealed for MrSlice<'_> {}
impl Sealed for RemoteMem {}
impl Sealed for crate::wrap::RegisteredMem<'_> {}
