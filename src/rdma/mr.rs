use std::ffi::c_void;
use std::fmt;
use std::ops::Range;
use std::ptr::NonNull;

use super::pd::Pd;

use anyhow;
use rdma_sys::*;

/// Local memory region.
///
/// A memory region is a contiguous region of memory that is registered with the RDMA device.
#[allow(dead_code)]
pub struct Mr<'a> {
    pd: &'a Pd<'a>,
    mr: NonNull<ibv_mr>,

    addr: *mut u8,
    len: usize,
}

unsafe impl<'a> Sync for Mr<'a> {}

impl<'a> fmt::Debug for Mr<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mr")
            .field("addr", &self.addr)
            .field("len", &self.len)
            .finish()
    }
}

impl<'a> Mr<'a> {
    pub fn reg(pd: &'a Pd, addr: *mut u8, len: usize) -> anyhow::Result<Self> {
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
        Ok(Self { pd, mr, addr, len })
    }

    pub fn from_slice(pd: &'a Pd, buf: &[u8]) -> anyhow::Result<Self> {
        Self::reg(pd, buf.as_ptr() as *mut u8, buf.len())
    }

    #[inline]
    pub fn as_ptr(&self) -> *mut ibv_mr {
        self.mr.as_ptr()
    }

    #[inline]
    pub fn addr(&self) -> *mut u8 {
        self.addr
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn lkey(&self) -> u32 {
        unsafe { (*self.mr.as_ptr()).lkey }
    }

    #[inline]
    pub fn rkey(&self) -> u32 {
        unsafe { (*self.mr.as_ptr()).rkey }
    }

    #[inline]
    pub fn as_slice(&self) -> MrSlice {
        MrSlice::new(self, 0..self.len())
    }

    #[inline]
    pub fn get(&self, r: Range<usize>) -> Option<MrSlice> {
        if r.start <= r.end && r.end <= self.len() {
            Some(MrSlice::new(self, r))
        } else {
            None
        }
    }

    #[inline]
    unsafe fn get_unchecked(&self, r: Range<usize>) -> MrSlice {
        MrSlice::new(self, r)
    }
}

impl<'a> Drop for Mr<'a> {
    fn drop(&mut self) {
        unsafe {
            ibv_dereg_mr(self.mr.as_ptr());
        }
    }
}

pub struct MrSlice<'a> {
    mr: &'a Mr<'a>,
    range: Range<usize>,
}

impl<'a> fmt::Debug for MrSlice<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MrSlice")
            .field("mr", &self.mr)
            .field("range", &self.range)
            .finish()
    }
}

impl<'a> MrSlice<'a> {
    pub fn new(mr: &'a Mr, range: Range<usize>) -> Self {
        Self { mr, range }
    }

    #[inline]
    pub fn mr(&self) -> &'a Mr {
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
}

/// Remote Memory Region
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

    pub fn as_slice(&self) -> RemoteMrSlice {
        RemoteMrSlice::new(self, 0..self.len)
    }

    pub fn get(&self, r: Range<usize>) -> Option<RemoteMrSlice> {
        if r.start <= r.end && r.end <= self.len {
            Some(RemoteMrSlice::new(self, r))
        } else {
            None
        }
    }

    pub unsafe fn get_unchecked(&self, r: Range<usize>) -> RemoteMrSlice {
        RemoteMrSlice::new(self, r)
    }
}

impl<'a> From<&'a Mr<'a>> for RemoteMr {
    fn from(mr: &'a Mr) -> Self {
        Self {
            addr: mr.addr() as u64,
            len: mr.len(),
            rkey: mr.rkey(),
        }
    }
}

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
}

impl<'a> From<&RemoteMrSlice<'a>> for rdma_t {
    fn from(value: &RemoteMrSlice<'a>) -> Self {
        Self {
            remote_addr: value.mr.addr + value.range.start as u64,
            rkey: value.mr.rkey,
        }
    }
}
