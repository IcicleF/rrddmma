use std::ops::Range;

use super::mr::*;
use rdma_sys::*;

/// Remote registered memory.
///
/// This structure contains remote memory region information and does not hold
/// any RDMA resources locally. Therefore, unlike `Mr`, `RemoteMem` does not
/// have a `RemoteMemSlice` counterpart, as this type itself can represent a
/// remote memory region slice by letting `addr` and `len` correspond to only
/// a part of the entire remote memory region.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
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
    pub fn get_slice(&self, r: Range<usize>) -> Option<Self> {
        if r.start <= r.end && r.end <= self.len {
            Some(unsafe { self.get_slice_unchecked(r) })
        } else {
            None
        }
    }

    /// Get a remote memory region slice from a pointer inside the remote memory
    /// area and a specified length. The behavior is undefined if the pointer
    /// is not contained within the remote MR or the specified slice
    /// `(ptr..(ptr + len))` is out of bounds.
    #[inline]
    pub unsafe fn get_slice_from_ptr(&self, ptr: u64, len: usize) -> Self {
        let offset = (ptr - self.addr) as usize;
        self.get_slice_unchecked(offset..(offset + len))
    }

    /// Get a remote memory region slice that represents the specified range of
    /// the remote memory area. The behavior is undefined if the range is out of
    /// bounds.
    #[inline]
    pub unsafe fn get_slice_unchecked(&self, r: Range<usize>) -> Self {
        Self {
            addr: self.addr + r.start as u64,
            len: r.end - r.start,
            rkey: self.rkey,
        }
    }
}

impl<'a> From<&RemoteMem> for rdma_t {
    fn from(value: &RemoteMem) -> Self {
        Self {
            remote_addr: value.addr,
            rkey: value.rkey,
        }
    }
}

/// Pack necessary information of a `Mr` into a `RemoteMr` so that it can be
/// sent to the remote side.
impl From<&'_ Mr<'_>> for RemoteMem {
    fn from(mr: &'_ Mr<'_>) -> Self {
        Self {
            addr: mr.addr() as u64,
            len: mr.len(),
            rkey: mr.rkey(),
        }
    }
}

/// Pack necessary information of a `MrSlice` into a `RemoteMr` so that it can
/// be sent to the remote side. This is useful when you only want to expose a
/// specific part of a local memory region to the remote side.
impl From<MrSlice<'_, '_>> for RemoteMem {
    fn from(slice: MrSlice<'_, '_>) -> Self {
        Self {
            addr: slice.addr() as u64,
            len: slice.len(),
            rkey: slice.mr().rkey(),
        }
    }
}
