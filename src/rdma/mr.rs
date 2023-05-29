use std::ffi::c_void;
use std::marker::PhantomData;
use std::ops::Range;
use std::ptr::NonNull;
use std::sync::Arc;
use std::{io, mem};

use super::pd::Pd;

use anyhow::Result;
use rdma_sys::*;

#[allow(dead_code)]
#[derive(Debug)]
struct MrInner<'mem> {
    pd: Pd,
    mr: NonNull<ibv_mr>,
    marker: PhantomData<&'mem [u8]>,
}

unsafe impl Send for MrInner<'_> {}
unsafe impl Sync for MrInner<'_> {}

impl Drop for MrInner<'_> {
    fn drop(&mut self) {
        unsafe {
            ibv_dereg_mr(self.mr.as_ptr());
        }
    }
}

/// Local memory region.
///
/// This type is a simple wrapper of an `Arc` and is guaranteed to have the
/// same memory layout with it.
///
/// A memory region is a virtual memory space registered to the RDMA device.
/// The registered memory itself does not belong to this type, but it must
/// outlive this type's lifetime (`'mem`) or there can be dangling pointers.
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct Mr<'mem> {
    inner: Arc<MrInner<'mem>>,
}

impl<'mem> Mr<'mem> {
    /// Register a memory region with the given protection domain.
    ///
    /// *Memory region* is an RDMA concept representing a handle to a
    /// registry of a contiguous *virtual memory area*. In other words,
    /// *region* and *area* are different things.
    ///
    /// This function is intentionally named `reg` instead of `new` to avoid
    /// the possible confusion that the created `Mr` holds the ownership
    /// of the memory area and that it will deallocate the memory when dropped
    /// (*actually, it won't!*). In fact, the `Mr` only acquires its lifetime
    /// and ensures that the memory area outlives the `Mr`.
    ///
    /// The memory region is registered with the following access flags:
    /// - `IBV_ACCESS_LOCAL_WRITE` for recv,
    /// - `IBV_ACCESS_REMOTE_WRITE` for remote RDMA write,
    /// - `IBV_ACCESS_REMOTE_READ` for remote RDMA read, and
    /// - `IBV_ACCESS_REMOTE_ATOMIC` for remote atomics.
    ///
    /// **NOTE:** although non-64-bit machines can hardly be seen nowadays, if
    /// you managed to run this crate on one, this function will error as remote
    /// memory addresses are represented by the `u64` type.
    pub fn reg(pd: Pd, buf: &'mem [u8]) -> Result<Self> {
        unsafe { Self::reg_with_ref(pd, buf.as_ptr() as *mut u8, buf.len(), buf) }
    }

    /// Register a memory region with the given protection domain and a raw
    /// pointer. An extra lifetime provider is required to get the lifetime
    /// parameter for the created instance.
    ///
    /// The caller must ensure that the memory area `[addr..(addr + len))`
    /// outlives the lifetime provided by the `_marker`.
    pub unsafe fn reg_with_ref(
        pd: Pd,
        addr: *mut u8,
        len: usize,
        _marker: &'mem impl ?Sized,
    ) -> Result<Self> {
        if mem::size_of::<usize>() != mem::size_of::<u64>() {
            return Err(anyhow::anyhow!("non-64-bit platforms are not supported"));
        }

        let mr = NonNull::new(unsafe {
            ibv_reg_mr(
                pd.as_ptr(),
                addr as *mut c_void,
                len,
                (ibv_access_flags::IBV_ACCESS_LOCAL_WRITE
                    | ibv_access_flags::IBV_ACCESS_REMOTE_WRITE
                    | ibv_access_flags::IBV_ACCESS_REMOTE_READ
                    | ibv_access_flags::IBV_ACCESS_REMOTE_ATOMIC)
                    .0 as i32,
            )
        });

        match mr {
            Some(mr) => Ok(Self {
                inner: Arc::new(MrInner {
                    pd,
                    mr,
                    marker: PhantomData::<&'mem [u8]>,
                }),
            }),
            None => Err(anyhow::anyhow!(io::Error::last_os_error())),
        }
    }

    /// Get the underlying `ibv_mr` structure.
    #[inline]
    pub fn as_ptr(&self) -> *mut ibv_mr {
        self.inner.mr.as_ptr()
    }

    /// Get the start address of the registered memory area.
    #[inline]
    pub fn addr(&self) -> *mut u8 {
        unsafe { (*self.inner.mr.as_ptr()).addr as *mut u8 }
    }

    /// Get the length of the registered memory area.
    #[inline]
    pub fn len(&self) -> usize {
        unsafe { (*self.inner.mr.as_ptr()).length }
    }

    /// Get the local key of the memory region.
    #[inline]
    pub fn lkey(&self) -> u32 {
        unsafe { (*self.inner.mr.as_ptr()).lkey }
    }

    /// Get the remote key of the memory region.
    #[inline]
    pub fn rkey(&self) -> u32 {
        unsafe { (*self.inner.mr.as_ptr()).rkey }
    }

    /// Get a memory region slice that represents the entire memory area.
    #[inline]
    pub fn as_slice(&self) -> MrSlice {
        MrSlice::new(self, 0..self.len())
    }

    /// Get a memory region slice that represents the specified range of
    /// the memory area. Return `None` if the range is out of bounds.
    #[inline]
    pub fn get_slice(&self, r: Range<usize>) -> Option<MrSlice> {
        if r.start <= r.end && r.end <= self.len() {
            Some(MrSlice::new(self, r))
        } else {
            None
        }
    }

    /// Get a memory region slice from a pointer inside the memory area
    /// and a specified length. The behavior is undefined if the pointer
    /// is not contained within the MR or the specified slice
    /// `(ptr..(ptr + len))` is out of bounds.
    #[inline]
    pub unsafe fn get_slice_from_ptr(&self, ptr: *const u8, len: usize) -> MrSlice {
        let offset = ptr as usize - self.addr() as usize;
        self.get_slice_unchecked(offset..(offset + len))
    }

    /// Get a memory region slice that represents the specified range of
    /// the memory area. The behavior is undefined if the range is out of
    /// bounds.
    #[inline]
    pub unsafe fn get_slice_unchecked(&self, r: Range<usize>) -> MrSlice {
        MrSlice::new(self, r)
    }
}

/// Slice of a local memory region.
///
/// A slice corresponds to an RDMA scatter-gather list entry, which can be used
/// in RDMA data-plane verbs.
#[derive(Debug, Clone)]
pub struct MrSlice<'a, 'mem> {
    mr: &'a Mr<'mem>,
    range: Range<usize>,
}

impl<'a, 'mem> MrSlice<'a, 'mem> {
    /// Create a new memory region slice of the given MR and range.
    pub fn new(mr: &'a Mr<'mem>, range: Range<usize>) -> Self {
        Self { mr, range }
    }

    /// Get the starting address of the slice.
    #[inline]
    pub fn addr(&self) -> *mut u8 {
        unsafe { self.mr.addr().add(self.range.start) }
    }

    /// Get the length of the slice.
    #[inline]
    pub fn len(&self) -> usize {
        self.range.end - self.range.start
    }

    /// Get the underlying `Mr`.
    #[inline]
    pub fn mr(&self) -> &Mr<'mem> {
        &self.mr
    }

    /// Sub-slice this slice. Return `None` if the range is out of bounds.
    #[inline]
    pub fn get_slice(&self, r: Range<usize>) -> Option<MrSlice<'a, 'mem>> {
        if r.start <= r.end && r.end <= self.len() {
            Some(MrSlice::new(
                self.mr,
                (self.range.start + r.start)..(self.range.start + r.end),
            ))
        } else {
            None
        }
    }

    /// Get a memory region slice from a pointer inside the represented memory
    /// area slice and a specified length. The behavior is undefined if the
    /// pointer is not contained within this slice or `(ptr..(ptr + len))`
    /// is out of bounds with regard to this slice.
    #[inline]
    pub unsafe fn get_slice_from_ptr(&self, ptr: *const u8, len: usize) -> MrSlice<'a, 'mem> {
        let offset = ptr as usize - self.addr() as usize;
        self.get_slice_unchecked(offset..(offset + len))
    }

    /// Get a memory region slice that represents the specified range of the
    /// the memory area within this memory slice. The behavior is undefined
    /// if the range is out of bounds.
    #[inline]
    pub unsafe fn get_slice_unchecked(&self, r: Range<usize>) -> MrSlice<'a, 'mem> {
        MrSlice::new(
            self.mr,
            (self.range.start + r.start)..(self.range.start + r.end),
        )
    }

    /// Resize the memory region slice to the specified length. Return whether
    /// the resize was successful.
    #[must_use = "must check if the resize was successful"]
    #[inline]
    pub fn resize(&mut self, len: usize) -> bool {
        let max_len = self.mr.len() - self.range.start;
        if len <= max_len {
            self.range.end = self.range.start + len;
            true
        } else {
            false
        }
    }
}

impl From<MrSlice<'_, '_>> for ibv_sge {
    fn from(slice: MrSlice<'_, '_>) -> Self {
        Self {
            addr: slice.addr() as u64,
            length: slice.len() as u32,
            lkey: slice.mr.lkey(),
        }
    }
}

impl From<MrSlice<'_, '_>> for NonNull<u8> {
    fn from(slice: MrSlice<'_, '_>) -> Self {
        unsafe { NonNull::new_unchecked(slice.addr()) }
    }
}
