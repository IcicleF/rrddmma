use super::{MrSlice, Slicing};
use crate::bindings::*;

/// Remote registered memory.
///
/// This structure contains remote memory region information and does not hold
/// any RDMA resources locally. Therefore, unlike `Mr`, `RemoteMem` does not
/// have a `RemoteMemSlice` counterpart, as this type itself can represent a
/// remote memory region slice by letting `addr` and `len` correspond to only
/// a part of the entire remote memory region.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct MrRemote {
    pub addr: u64,
    pub len: usize,
    pub rkey: u32,
}

impl MrRemote {
    /// Create a new piece of remote registered memory data.
    pub fn new(addr: u64, len: usize, rkey: u32) -> Self {
        Self { addr, len, rkey }
    }

    /// Create a dummy `MrRemote` with all fields set to zero.
    pub fn dummy() -> Self {
        Self::new(0, 0, 0)
    }

    /// Get a pointer at the given offset.
    #[inline]
    pub fn at(&self, offset: usize) -> u64 {
        self.addr + offset as u64
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

impl Default for MrRemote {
    /// Create a dummy `MrRemote` with all fields set to zero.
    fn default() -> Self {
        Self::dummy()
    }
}

unsafe impl<'s> Slicing<'s> for MrRemote {
    type Output = Self;

    #[inline]
    fn addr(&'s self) -> *mut u8 {
        self.addr as _
    }

    #[inline]
    fn len(&'s self) -> usize {
        self.len
    }

    #[inline]
    unsafe fn slice_unchecked(&'s self, offset: usize, len: usize) -> Self::Output {
        Self::new(self.addr + offset as u64, len, self.rkey)
    }
}

/// Pack necessary information of a `MrSlice` into a `RemoteMr` so that it can
/// be sent to the remote side. This is useful when you only want to expose a
/// specific part of a local memory region to the remote side.
impl From<MrSlice<'_>> for MrRemote {
    fn from(slice: MrSlice<'_>) -> Self {
        Self {
            addr: slice.addr() as u64,
            len: slice.len(),
            rkey: slice.mr().rkey(),
        }
    }
}
