use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, Range};

use anyhow::Result;

use crate::rdma::mr::*;
use crate::rdma::pd::*;

/// A wrapper around an owned memory area that is registered as an RDMA MR.
/// The memory area is allocated on the heap with `Box<[u8]>` and will be
/// deallocated when this structure is dropped.
///
/// **WARNING:** Since Rust disallows self-referencing, this type deceives
/// the borrow checker by storing a `Mr<'static>` inside. Generally, this
/// should not cause any safety problems since the `Mr` references memory
/// allocated on the heap, which isn't movable, and that it will definitely
/// get dropped before the referenced memory.
/// However, you should still use this type with some care.
pub struct RegisteredMem {
    /// The memory region, dropped first.
    mr: Mr<'static>,

    /// The allocated buffer, dropped after the `Mr`.
    buf: Box<[u8]>,
}

impl RegisteredMem {
    /// Allocate memory with the given length and register MR on it.
    pub fn new(pd: Pd, len: usize) -> Result<Self> {
        // This should use the global allocator
        let buf = vec![0u8; len].into_boxed_slice();
        let mr =
            unsafe { Mr::reg_with_ref(pd, buf.as_ptr() as *mut _, buf.len(), &PhantomData::<()>)? };

        Ok(Self { mr, buf })
    }

    /// Allocate memory that shares the same length and content with the provided
    /// slice, and then register MR on it.
    pub fn new_with_content(pd: Pd, content: &[u8]) -> Result<Self> {
        let mut ret = Self::new(pd, content.len())?;
        ret.buf.copy_from_slice(content);
        Ok(ret)
    }

    /// Get the address of the allocated memory.
    #[inline]
    pub fn addr(&self) -> *mut u8 {
        self.buf.as_ptr() as *mut u8
    }

    /// Get the length of the allocated memory.
    #[inline]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Get a `MrSlice` that represents the whole memory region.
    #[inline]
    pub fn as_slice(&self) -> MrSlice {
        self.mr.as_slice()
    }

    /// Sub-slicing this slice. Return `None` if the range is out of bounds.
    #[inline]
    pub fn get_slice(&self, r: Range<usize>) -> Option<MrSlice> {
        self.mr.get_slice(r)
    }

    /// Get a memory region slice from a pointer inside the represented memory
    /// area slice and a specified length. The behavior is undefined if the
    /// pointer is not contained within this slice or `(ptr..(ptr + len))`
    /// is out of bounds with regard to this slice.
    #[inline]
    pub unsafe fn get_slice_from_ptr(&self, ptr: *const u8, len: usize) -> MrSlice {
        self.mr.get_slice_from_ptr(ptr, len)
    }

    /// Get a memory region slice that represents the specified range of the
    /// the memory area within this memory slice. The behavior is undefined
    /// if the range is out of bounds.
    #[inline]
    pub unsafe fn get_slice_unchecked(&self, r: Range<usize>) -> MrSlice {
        self.mr.get_slice_unchecked(r)
    }
}

impl Deref for RegisteredMem {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.buf.as_ref()
    }
}

impl DerefMut for RegisteredMem {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buf.as_mut()
    }
}
