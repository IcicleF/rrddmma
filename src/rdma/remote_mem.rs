use std::ops::{Bound, RangeBounds};

use super::mr::*;
use crate::bindings::*;

/// Remote registered memory.
///
/// This structure contains remote memory region information and does not hold
/// any RDMA resources locally. Therefore, unlike `Mr`, `RemoteMem` does not
/// have a `RemoteMemSlice` counterpart, as this type itself can represent a
/// remote memory region slice by letting `addr` and `len` correspond to only
/// a part of the entire remote memory region.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct RemoteMem {
    pub addr: u64,
    pub len: usize,
    pub rkey: u32,
}

impl RemoteMem {
    /// Create a new piece of remote registered memory data.
    pub fn new(addr: u64, len: usize, rkey: u32) -> Self {
        Self { addr, len, rkey }
    }

    /// Create a dummy remote registered memory data that can be used as a
    /// placeholder.
    pub fn dummy() -> Self {
        Self::new(0, 0, 0)
    }

    /// Get a pointer at the given offset.
    #[inline]
    pub fn at(&self, offset: usize) -> u64 {
        self.addr + offset as u64
    }

    /// Get a remote memory region slice that represents the specified range of
    /// the remote memory area. Return `None` if the range is out of bounds.
    #[inline]
    pub fn get_slice(&self, r: impl RangeBounds<usize>) -> Option<Self> {
        let start = match r.start_bound() {
            Bound::Included(&s) => s,
            Bound::Excluded(&s) => s + 1,
            Bound::Unbounded => 0,
        };
        let end = match r.end_bound() {
            Bound::Included(&e) => e + 1,
            Bound::Excluded(&e) => e,
            Bound::Unbounded => self.len,
        };

        if start <= end && end <= self.len {
            Some(Self {
                addr: self.addr + start as u64,
                len: end - start,
                rkey: self.rkey,
            })
        } else {
            None
        }
    }

    /// Get a remote memory region slice from a pointer inside the remote memory
    /// area and a specified length.
    ///
    /// # Safety
    ///
    /// - The specified slice `(ptr..(ptr + len))` must be within the bounds of
    ///   the remote memory slice.
    ///
    /// In fact, out-of-bound ranges of remote memory can do no harm to the local
    /// machine, and RDMA requests containing such ranges will be rejected by the
    /// remote RDMA NIC. The `unsafe` signature of this method is only to remind
    /// the caller that if the specified range is out of bound and an RDMA request
    /// unfortunately uses it, then the request will fail and the QP will get into
    /// error state.
    #[inline]
    pub unsafe fn get_slice_from_ptr(&self, ptr: u64, len: usize) -> Self {
        let offset = (ptr - self.addr) as usize;
        self.get_slice_unchecked(offset..(offset + len))
    }

    /// Get a remote memory region slice that represents the specified range of
    /// the remote memory area.
    ///
    /// # Safety
    ///
    /// - The specified range must be within the bounds of the remote memory slice.
    ///
    /// In fact, out-of-bound ranges of remote memory can do no harm to the local
    /// machine, and RDMA requests containing such ranges will be rejected by the
    /// remote RDMA NIC. The `unsafe` signature of this method is only to remind
    /// the caller that if the specified range is out of bound and an RDMA request
    /// unfortunately uses it, then the request will fail and the QP will get into
    /// error state.
    #[inline]
    pub unsafe fn get_slice_unchecked(&self, r: impl RangeBounds<usize>) -> Self {
        let start = match r.start_bound() {
            Bound::Included(&s) => s,
            Bound::Excluded(&s) => s + 1,
            Bound::Unbounded => 0,
        };
        let end = match r.end_bound() {
            Bound::Included(&e) => e + 1,
            Bound::Excluded(&e) => e,
            Bound::Unbounded => self.len,
        };
        Self {
            addr: self.addr + start as u64,
            len: end - start,
            rkey: self.rkey,
        }
    }

    /// Generate a [`rdma_t`] instance for RDMA one-sided operations to this
    /// piece of remote memory.
    #[inline]
    pub(crate) fn as_rdma_t(&self) -> rdma_t {
        rdma_t {
            remote_addr: self.addr,
            rkey: self.rkey,
        }
    }
}

/// Pack necessary information of a `MrSlice` into a `RemoteMr` so that it can
/// be sent to the remote side. This is useful when you only want to expose a
/// specific part of a local memory region to the remote side.
impl From<MrSlice<'_>> for RemoteMem {
    fn from(slice: MrSlice<'_>) -> Self {
        Self {
            addr: slice.addr() as u64,
            len: slice.len(),
            rkey: slice.mr().rkey(),
        }
    }
}

/// Convert an [`Option<RemoteMem>`] into a [`RemoteMem`]. If the input is
/// `None`, a dummy `RemoteMem` will be returned.
impl From<Option<RemoteMem>> for RemoteMem {
    fn from(opt: Option<RemoteMem>) -> Self {
        opt.unwrap_or_else(RemoteMem::dummy)
    }
}
