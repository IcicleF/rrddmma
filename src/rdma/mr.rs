use std::ffi::c_void;
use std::marker::PhantomData;
use std::mem;
use std::ops::Range;
use std::ptr::NonNull;
use std::sync::Arc;

use super::pd::Pd;

use anyhow;
use rdma_sys::*;

#[allow(dead_code)]
#[derive(Debug)]
struct MrInner<'a> {
    pd: Pd,
    mr: NonNull<ibv_mr>,
    marker: PhantomData<&'a [u8]>,
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
    /// Register a memory region with the given protection domain and a raw
    /// pointer. An extra lifetime provider is required to get the lifetime
    /// parameter for the created instance.
    pub fn reg_with_ref<Ref: ?Sized>(
        pd: Pd,
        addr: *mut u8,
        len: usize,
        _marker: &'mem Ref,
    ) -> anyhow::Result<Self> {
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
        })
        .ok_or_else(|| anyhow::anyhow!("ibv_reg_mr failed: {}", std::io::Error::last_os_error()))?;
        Ok(Self {
            inner: Arc::new(MrInner {
                pd,
                mr,
                marker: PhantomData::<&'mem [u8]>,
            }),
        })
    }

    /// Register a memory region with the given protection domain.
    ///
    /// *Memory region* is an RDMA concept representing a handle to a
    /// registry of a contiguous *virtual memory area*. In other words,
    /// *region* and *area* are different things.
    ///
    /// This function is intentionally named `reg` instead of `new` to avoid
    /// the possible confusion that the created `Mr` holds the ownership
    /// of the memory area and that it will deallocate the memory when dropped
    /// (*actually, it won't!*).
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
    pub fn reg(pd: Pd, buf: &'mem [u8]) -> anyhow::Result<Self> {
        Self::reg_with_ref(pd, buf.as_ptr() as *mut u8, buf.len(), buf)
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
    /// `(ptr .. (ptr + len))` is out of bounds.
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
pub struct MrSlice<'mem> {
    pub(crate) addr: *mut u8,
    pub(crate) len: usize,
    pub(crate) lkey: u32,
    pub(crate) rkey: u32,
    pub(crate) marker: PhantomData<&'mem [u8]>,
}

impl<'mem> MrSlice<'mem> {
    /// Create a new memory region slice of the given MR and range.
    pub(crate) fn new(mr: &'mem Mr, range: Range<usize>) -> Self {
        Self {
            addr: unsafe { mr.addr().add(range.start) },
            len: range.end - range.start,
            lkey: mr.lkey(),
            rkey: mr.rkey(),
            marker: PhantomData,
        }
    }

    /// Get the starting address of the slice.
    #[inline]
    pub fn addr(&self) -> *mut u8 {
        self.addr
    }

    /// Get the length of the slice.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Get the local key of the memory region.
    #[inline]
    pub fn lkey(&self) -> u32 {
        self.lkey
    }

    /// Get the remote key of the memory region.
    #[inline]
    pub fn rkey(&self) -> u32 {
        self.rkey
    }

    /// Sub-slicing this slice. Return `None` if the range is out of bounds.
    #[inline]
    pub fn get_slice(&self, r: Range<usize>) -> Option<MrSlice<'mem>> {
        if r.start <= r.end && r.end <= self.len() {
            Some(MrSlice {
                addr: unsafe { self.addr.add(r.start) },
                len: r.end - r.start,
                ..*self
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
    pub unsafe fn get_slice_from_ptr(&self, ptr: *const u8, len: usize) -> MrSlice<'mem> {
        let offset = ptr as usize - self.addr() as usize;
        self.get_slice_unchecked(offset..(offset + len))
    }

    /// Get a memory region slice that represents the specified range of the
    /// the memory area within this memory slice. The behavior is undefined
    /// if the range is out of bounds.
    #[inline]
    pub unsafe fn get_slice_unchecked(&self, r: Range<usize>) -> MrSlice<'mem> {
        MrSlice {
            addr: self.addr.add(r.start),
            len: r.end - r.start,
            ..*self
        }
    }
}
