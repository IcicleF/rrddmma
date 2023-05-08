use std::ffi::c_void;
use std::ops::Range;
use std::ptr::NonNull;
use std::sync::Arc;

use super::pd::Pd;

use anyhow;
use rdma_sys::*;

#[allow(dead_code)]
#[derive(Debug)]
struct MrInner {
    pd: Pd,
    mr: NonNull<ibv_mr>,

    addr: *mut u8,
    len: usize,
}

unsafe impl Send for MrInner {}
unsafe impl Sync for MrInner {}

impl Drop for MrInner {
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
/// The registered memory itself does not belong to this type.
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct Mr {
    inner: Arc<MrInner>,
}

impl Mr {
    /// Register a memory region with the given protection domain.
    ///
    /// *Memory region* is an RDMA concept representing a handle to a
    /// registry of a contiguous *virtual memory area*. In other words,
    /// *region* and *area* are different things.
    ///
    /// This function is intentionally named `reg` instead of `new` to avoid
    /// the possible confusion that the produced `Mr<'a>` holds the ownership
    /// of the memory area and that it will deallocate the memory when dropped.
    ///
    /// The memory region is registered with the following access flags:
    /// - `IBV_ACCESS_LOCAL_WRITE` for recv
    /// - `IBV_ACCESS_REMOTE_WRITE` for remote RDMA write
    /// - `IBV_ACCESS_REMOTE_READ` for remote RDMA read
    /// - `IBV_ACCESS_REMOTE_ATOMIC` for remote atomics
    pub fn reg(pd: Pd, addr: *mut u8, len: usize) -> anyhow::Result<Self> {
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
            inner: Arc::new(MrInner { pd, mr, addr, len }),
        })
    }

    /// Register a memory region with the given protection domain.
    /// It simply calls `reg` with the pointer of the given slice.
    pub fn reg_slice(pd: Pd, buf: &[u8]) -> anyhow::Result<Self> {
        Self::reg(pd, buf.as_ptr() as *mut u8, buf.len())
    }

    /// Get the underlying `ibv_mr` structure.
    #[inline]
    pub fn as_ptr(&self) -> *mut ibv_mr {
        self.inner.mr.as_ptr()
    }

    /// Get the start address of the registered memory area.
    #[inline]
    pub fn addr(&self) -> *mut u8 {
        self.inner.addr
    }

    /// Get the length of the registered memory area.
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len
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
    /// is not contained within the MR or the specified slice (ptr..(ptr + len))
    /// is out of bounds.
    #[inline]
    pub unsafe fn get_slice_from_ptr(&self, ptr: *const u8, len: usize) -> MrSlice {
        let offset = ptr as usize - self.inner.addr as usize;
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
/// Data-plane verbs accept local memory region slices.
/// In other words, a slice corresponds to an RDMA scatter-gather list entry.
#[derive(Debug, Clone)]
pub struct MrSlice<'a> {
    mr: &'a Mr,
    range: Range<usize>,
}

impl<'a> MrSlice<'a> {
    /// Create a new memory region slice of the given MR and range.
    pub fn new(mr: &'a Mr, range: Range<usize>) -> Self {
        Self { mr, range }
    }

    /// Get the underlying `Mr` structure.
    #[inline]
    pub fn mr(&self) -> &'a Mr {
        &self.mr
    }

    /// Get the starting offset of the slice with regard to the original MR.
    #[inline]
    pub fn offset(&self) -> usize {
        self.range.start
    }

    /// Get the length of the slice.
    #[inline]
    pub fn len(&self) -> usize {
        self.range.end - self.range.start
    }

    /// Get the starting address of the slice.
    #[inline]
    pub fn as_ptr(&self) -> *mut u8 {
        unsafe { self.mr.addr().add(self.range.start) }
    }

    /// Sub-slicing this slice. Return `None` if the range is out of bounds.
    #[inline]
    pub fn get_slice(&self, r: Range<usize>) -> Option<MrSlice> {
        if r.start <= r.end && r.end <= self.len() {
            Some(MrSlice::new(
                self.mr,
                (self.range.start + r.start)..(self.range.start + r.end),
            ))
        } else {
            None
        }
    }

    #[inline]
    pub unsafe fn get_slice_from_ptr(&self, ptr: *const u8, len: usize) -> MrSlice {
        let offset = ptr as usize - self.as_ptr() as usize;
        self.get_slice_unchecked(offset..(offset + len))
    }

    #[inline]
    pub unsafe fn get_slice_unchecked(&self, r: Range<usize>) -> MrSlice {
        MrSlice::new(
            self.mr,
            (self.range.start + r.start)..(self.range.start + r.end),
        )
    }
}

/// Remote memory region.
///
/// This structure contains remote memory region information and does not hold any resources locally.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct RemoteMr {
    pub addr: u64,
    pub len: usize,
    pub rkey: u32,
}

impl RemoteMr {
    pub fn new(addr: u64, len: usize, rkey: u32) -> Self {
        Self { addr, len, rkey }
    }

    #[inline]
    pub fn as_slice(&self) -> RemoteMrSlice {
        RemoteMrSlice::new(self, 0..self.len)
    }

    #[inline]
    pub fn get(&self, r: Range<usize>) -> Option<RemoteMrSlice> {
        if r.start <= r.end && r.end <= self.len {
            Some(RemoteMrSlice::new(self, r))
        } else {
            None
        }
    }

    #[inline]
    pub unsafe fn get_unchecked(&self, r: Range<usize>) -> RemoteMrSlice {
        RemoteMrSlice::new(self, r)
    }
}

impl<'a> From<&'a Mr> for RemoteMr {
    fn from(mr: &'a Mr) -> Self {
        Self {
            addr: mr.addr() as u64,
            len: mr.len(),
            rkey: mr.rkey(),
        }
    }
}

/// Slice of a remote memory region.
///
/// RDMA one-sided verbs accept remote memory region slices.
/// In other words, a slice corresponds to a `wr.wr.rdma` field in `ibv_send_wr`.
#[derive(Debug, Clone)]
pub struct RemoteMrSlice<'a> {
    mr: &'a RemoteMr,
    range: Range<usize>,
}

impl<'a> RemoteMrSlice<'a> {
    pub fn new(mr: &'a RemoteMr, range: Range<usize>) -> Self {
        Self { mr, range }
    }

    #[inline]
    pub fn mr(&self) -> &RemoteMr {
        &self.mr
    }

    #[inline]
    pub fn offset(&self) -> usize {
        self.range.start
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.range.end - self.range.start
    }

    #[inline]
    pub fn get(&self, r: Range<usize>) -> Option<RemoteMrSlice> {
        if r.start <= r.end && r.end <= self.len() {
            Some(RemoteMrSlice::new(
                self.mr,
                (self.range.start + r.start)..(self.range.start + r.end),
            ))
        } else {
            None
        }
    }

    #[inline]
    pub unsafe fn get_unchecked(&self, r: Range<usize>) -> RemoteMrSlice {
        RemoteMrSlice::new(
            self.mr,
            (self.range.start + r.start)..(self.range.start + r.end),
        )
    }
}

impl<'a> From<&RemoteMrSlice<'a>> for rdma_t {
    fn from(value: &RemoteMrSlice<'a>) -> Self {
        Self {
            remote_addr: value.mr.addr + value.range.start as u64,
            rkey: value.mr.rkey,
        }
    }
}
