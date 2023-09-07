use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, RangeBounds};
use std::ptr;

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
        if len == 0 {
            return Err(anyhow::anyhow!("zero-length memory regions are disallowed"));
        }

        // This should use the global allocator
        let buf = vec![0u8; len].into_boxed_slice();
        Self::new_owned(pd, buf).map_err(|(_, e)| e)
    }

    /// Take ownership of the provided memory region and register MR on it.
    /// On error, the provided buffer will be returned along with the error.
    pub fn new_owned(
        pd: Pd,
        buf: Box<[u8]>,
    ) -> std::result::Result<Self, (Box<[u8]>, anyhow::Error)> {
        if buf.is_empty() {
            return Err((
                buf,
                anyhow::anyhow!("zero-length memory regions are disallowed"),
            ));
        }

        let mr = match unsafe {
            Mr::reg_with_ref(pd, buf.as_ptr() as *mut _, buf.len(), &PhantomData::<()>)
        } {
            Ok(mr) => mr,
            Err(e) => return Err((buf, e)),
        };
        Ok(Self { mr, buf })
    }

    /// Allocate memory that shares the same length and content with the provided
    /// slice, and then register MR on it.
    pub fn new_with_content(pd: Pd, content: &[u8]) -> Result<Self> {
        if content.is_empty() {
            return Err(anyhow::anyhow!("zero-length memory regions are disallowed"));
        }

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
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Get the underlying [`Mr`].
    #[inline]
    pub fn mr(&self) -> &Mr {
        &self.mr
    }

    /// Get a `MrSlice` that represents the whole memory region.
    #[inline]
    pub fn as_slice(&self) -> MrSlice {
        self.mr.as_slice()
    }

    /// Sub-slicing this slice. Return `None` if the range is out of bounds.
    #[inline]
    pub fn get_slice(&self, r: impl RangeBounds<usize>) -> Option<MrSlice> {
        self.mr.get_slice(r)
    }

    /// Zero the allocated buffer.
    #[inline]
    pub fn clear(&mut self) {
        unsafe {
            ptr::write_bytes(self.buf.as_mut_ptr(), 0, self.buf.len());
        }
    }

    /// Get a memory region slice from a pointer inside the represented memory
    /// area slice and a specified length.
    ///
    /// # Safety
    ///
    /// - The specified slice `(ptr..(ptr + len))` must be within the bounds of
    ///   the MR.
    #[inline]
    pub unsafe fn get_slice_from_ptr(&self, pointer: *const u8, len: usize) -> MrSlice {
        self.mr.get_slice_from_ptr(pointer, len)
    }

    /// Get a memory region slice that represents the specified range of the
    /// the memory area within this memory slice.
    ///
    /// # Safety
    ///
    /// - The specified range must be within the bounds of the MR.
    #[inline]
    pub unsafe fn get_slice_unchecked(&self, r: impl RangeBounds<usize>) -> MrSlice {
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
