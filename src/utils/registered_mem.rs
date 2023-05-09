use std::ffi::c_void;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, Range};
use std::ptr::NonNull;

use anyhow::Result;
use rdma_sys::*;

use crate::rdma::mr::MrSlice;
use crate::rdma::pd::*;

/// A wrapper around an owned memory area that is registered as an RDMA MR.
/// The memory area is allocated on the heap with `Box<[u8]>` and will be
/// deallocated when this structure is dropped.
pub struct RegisteredMem {
    mr: NonNull<ibv_mr>,
    buf: Box<[u8]>,
}

impl RegisteredMem {
    /// Allocate memory with the given length and register a memory region on
    /// it.
    pub fn new(pd: Pd, len: usize) -> Result<Self> {
        // This should use the global allocator
        let buf = vec![0u8; len].into_boxed_slice();
        let mr = NonNull::new(unsafe {
            ibv_reg_mr(
                pd.as_ptr(),
                buf.as_ptr() as *mut c_void,
                len,
                (ibv_access_flags::IBV_ACCESS_LOCAL_WRITE
                    | ibv_access_flags::IBV_ACCESS_REMOTE_WRITE
                    | ibv_access_flags::IBV_ACCESS_REMOTE_READ
                    | ibv_access_flags::IBV_ACCESS_REMOTE_ATOMIC)
                    .0 as i32,
            )
        })
        .ok_or_else(|| anyhow::anyhow!("ibv_reg_mr failed: {}", std::io::Error::last_os_error()))?;

        Ok(Self { mr, buf })
    }

    /// Allocate memory that shares the same length and content with the provided
    /// slice and register a memory region on it.
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
        MrSlice {
            addr: self.buf.as_ptr() as *mut u8,
            len: self.buf.len(),
            lkey: unsafe { (*self.mr.as_ptr()).lkey },
            rkey: unsafe { (*self.mr.as_ptr()).rkey },
            marker: PhantomData,
        }
    }

    /// Sub-slicing this slice. Return `None` if the range is out of bounds.
    #[inline]
    pub fn get_slice(&self, r: Range<usize>) -> Option<MrSlice> {
        if r.start <= r.end && r.end <= self.len() {
            Some(MrSlice {
                addr: unsafe { self.addr().add(r.start) },
                len: r.end - r.start,
                lkey: unsafe { (*self.mr.as_ptr()).lkey },
                rkey: unsafe { (*self.mr.as_ptr()).rkey },
                marker: PhantomData,
            })
        } else {
            None
        }
    }

    /// Get a memory region slice from a pointer inside the represented memory
    /// area slice and a specified length. The behavior is undefined if the
    /// pointer is not contained within this slice or `(ptr .. (ptr + len))`
    /// is out of bounds with regard to this slice.
    #[inline]
    pub unsafe fn get_slice_from_ptr(&self, ptr: *const u8, len: usize) -> MrSlice {
        let offset = ptr as usize - self.addr() as usize;
        self.get_slice_unchecked(offset..(offset + len))
    }

    /// Get a memory region slice that represents the specified range of the
    /// the memory area within this memory slice. The behavior is undefined
    /// if the range is out of bounds.
    #[inline]
    pub unsafe fn get_slice_unchecked(&self, r: Range<usize>) -> MrSlice {
        MrSlice {
            addr: self.addr().add(r.start),
            len: r.end - r.start,
            lkey: unsafe { (*self.mr.as_ptr()).lkey },
            rkey: unsafe { (*self.mr.as_ptr()).rkey },
            marker: PhantomData,
        }
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
