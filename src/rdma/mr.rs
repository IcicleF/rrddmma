use std::cell::UnsafeCell;
use std::ffi::c_void;
use std::marker::PhantomData;
use std::ops::{Bound, Index, Range, RangeBounds};
use std::ptr::NonNull;
use std::slice::{self, SliceIndex};
use std::sync::Arc;
use std::{fmt, io, mem};

use super::pd::Pd;
use super::remote_mem::RemoteMem;

use anyhow::Result;
use rdma_sys::*;

#[allow(dead_code)]
#[derive(Debug)]
struct MrInner<'mem> {
    pd: Pd,
    mr: NonNull<ibv_mr>,
    marker: PhantomData<&'mem UnsafeCell<[u8]>>,
}

unsafe impl Send for MrInner<'_> {}
unsafe impl Sync for MrInner<'_> {}

impl Drop for MrInner<'_> {
    fn drop(&mut self) {
        // SAFETY: FFI.
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
#[derive(Clone)]
#[repr(transparent)]
pub struct Mr<'mem> {
    inner: Arc<MrInner<'mem>>,
}

impl fmt::Debug for Mr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("Mr<{:p}>", self.as_raw()))
    }
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
        // SAFETY: this call simply decouples the reference to a long pointer
        // into an address, a length, and a lifetime.
        unsafe { Self::reg_with_ref(pd, buf.as_ptr() as *mut u8, buf.len(), buf) }
    }

    /// Register a memory region with the given protection domain with the
    /// memory area reference decoupled into a raw pointer, a length, and an
    /// extra lifetime provider is required to get the lifetime parameter
    /// for the created memory region instance.
    ///
    /// The caller must ensure that the memory area `[addr..(addr + len))`
    /// outlives the lifetime provided by the `_marker`.
    pub unsafe fn reg_with_ref<Marker>(
        pd: Pd,
        addr: *mut u8,
        len: usize,
        _marker: &'mem Marker,
    ) -> Result<Self>
    where
        Marker: ?Sized,
    {
        if mem::size_of::<usize>() != mem::size_of::<u64>() {
            return Err(anyhow::anyhow!("non-64-bit platforms are not supported"));
        }

        // SAFETY: FFI.
        let mr = NonNull::new(unsafe {
            ibv_reg_mr(
                pd.as_raw(),
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
                    marker: PhantomData::<&'mem UnsafeCell<[u8]>>,
                }),
            }),
            None => Err(anyhow::anyhow!(io::Error::last_os_error())),
        }
    }

    /// Get the underlying `ibv_mr` structure.
    #[inline]
    pub fn as_raw(&self) -> *mut ibv_mr {
        self.inner.mr.as_ptr()
    }

    /// Get the start address of the registered memory area.
    #[inline]
    pub fn addr(&self) -> *mut u8 {
        // SAFETY: the pointer is valid as long as the `Mr` is alive.
        unsafe { (*self.inner.mr.as_ptr()).addr as *mut u8 }
    }

    /// Get the length of the registered memory area.
    #[inline]
    pub fn len(&self) -> usize {
        // SAFETY: the pointer is valid as long as the `Mr` is alive.
        unsafe { (*self.inner.mr.as_ptr()).length }
    }

    /// Get the local key of the memory region.
    #[inline]
    pub fn lkey(&self) -> u32 {
        // SAFETY: the pointer is valid as long as the `Mr` is alive.
        unsafe { (*self.inner.mr.as_ptr()).lkey }
    }

    /// Get the remote key of the memory region.
    #[inline]
    pub fn rkey(&self) -> u32 {
        // SAFETY: the pointer is valid as long as the `Mr` is alive.
        unsafe { (*self.inner.mr.as_ptr()).rkey }
    }

    /// Get a memory region slice that represents the entire memory area.
    #[inline]
    pub fn as_slice(&self) -> MrSlice {
        // SAFETY: the range is naturally contained in the memory area.
        unsafe { MrSlice::new(self, 0..self.len()) }
    }

    /// Get a memory region slice that represents the specified range of
    /// the memory area. Return `None` if the range is out of bounds.
    #[inline]
    pub fn get_slice<R>(&self, r: R) -> Option<MrSlice>
    where
        R: RangeBounds<usize>,
    {
        let start = match r.start_bound() {
            Bound::Included(&s) => s,
            Bound::Excluded(&s) => s + 1,
            Bound::Unbounded => 0,
        };
        let end = match r.end_bound() {
            Bound::Included(&e) => e + 1,
            Bound::Excluded(&e) => e,
            Bound::Unbounded => self.len(),
        };

        if start <= end && end <= self.len() {
            // SAFETY: the range is guaranteed to be contained in the memory area.
            Some(unsafe { MrSlice::new(self, start..end) })
        } else {
            None
        }
    }

    /// Get a memory region slice from a pointer inside the memory area
    /// and a specified length. The behavior is undefined if the pointer
    /// is not contained within the MR or the specified slice
    /// `(ptr..(ptr + len))` is out of bounds.
    #[inline]
    pub unsafe fn get_slice_from_ptr(&self, pointer: *const u8, len: usize) -> MrSlice {
        let offset = pointer as usize - self.addr() as usize;
        self.get_slice_unchecked(offset..(offset + len))
    }

    /// Get a memory region slice that represents the specified range of
    /// the memory area. The behavior is undefined if the range is out of
    /// bounds.
    #[inline]
    pub unsafe fn get_slice_unchecked<R>(&self, r: R) -> MrSlice
    where
        R: RangeBounds<usize>,
    {
        let start = match r.start_bound() {
            Bound::Included(&s) => s,
            Bound::Excluded(&s) => s + 1,
            Bound::Unbounded => 0,
        };
        let end = match r.end_bound() {
            Bound::Included(&e) => e + 1,
            Bound::Excluded(&e) => e,
            Bound::Unbounded => self.len(),
        };

        MrSlice::new(self, start..end)
    }

    /// View this local memory region as a remote memory region for RDMA access
    /// from remote peers.
    #[inline]
    pub fn as_remote(&self) -> RemoteMem {
        RemoteMem {
            addr: self.addr() as u64,
            len: self.len(),
            rkey: self.rkey(),
        }
    }
}

impl<'mem, I> Index<I> for Mr<'mem>
where
    I: SliceIndex<[u8]>,
{
    type Output = <I as SliceIndex<[u8]>>::Output;

    /// Index into the memory region as a `[u8]`.
    #[inline]
    fn index(&self, index: I) -> &Self::Output {
        // SAFETY: TODO: is this really safe?
        let s = unsafe { slice::from_raw_parts(self.addr(), self.len()) };
        &s[index]
    }
}

/// Slice of a local memory region.
///
/// A slice corresponds to an RDMA scatter-gather list entry, which can be used
/// in RDMA data-plane verbs.
#[derive(Debug, Clone)]
pub struct MrSlice<'a> {
    mr: &'a Mr<'a>,
    range: Range<usize>,
}

impl<'a> MrSlice<'a> {
    /// Create a new memory region slice of the given MR and range.
    pub unsafe fn new(mr: &'a Mr<'a>, range: Range<usize>) -> Self {
        Self { mr, range }
    }

    /// Get the starting address of the slice.
    #[inline]
    pub fn addr(&self) -> *mut u8 {
        // SAFETY: the address is guaranteed to be contained within the
        // registered memory area as it can only be created from safe
        // [`Mr`] interfaces where the range will be checked, or from
        // unsafe [`Mr`] interfaces where the user is responsible for
        // ensuring the range's correctness.
        unsafe { self.mr.addr().add(self.range.start) }
    }

    /// Get the length of the slice.
    #[inline]
    pub fn len(&self) -> usize {
        self.range.end - self.range.start
    }

    /// Get the underlying `Mr`.
    #[inline]
    pub fn mr(&self) -> &Mr<'a> {
        &self.mr
    }

    /// Sub-slice this slice. Return `None` if the range is out of bounds.
    #[inline]
    pub fn get_slice(&self, r: impl RangeBounds<usize>) -> Option<MrSlice<'a>> {
        let start = match r.start_bound() {
            Bound::Included(&s) => s,
            Bound::Excluded(&s) => s + 1,
            Bound::Unbounded => 0,
        };
        let end = match r.end_bound() {
            Bound::Included(&e) => e + 1,
            Bound::Excluded(&e) => e,
            Bound::Unbounded => self.len(),
        };

        if start <= end && end <= self.len() {
            // SAFETY: the range is guaranteed to be contained in the slice.
            Some(unsafe {
                MrSlice::new(
                    self.mr,
                    (self.range.start + start)..(self.range.start + end),
                )
            })
        } else {
            None
        }
    }

    /// Get a memory region slice from a pointer inside the represented memory
    /// area slice and a specified length. The behavior is undefined if the
    /// pointer is not contained within this slice or `(ptr..(ptr + len))`
    /// is out of bounds with regard to this slice.
    #[inline]
    pub unsafe fn get_slice_from_ptr(&self, pointer: *const u8, len: usize) -> MrSlice<'a> {
        let offset = pointer as usize - self.addr() as usize;
        self.get_slice_unchecked(offset..(offset + len))
    }

    /// Get a memory region slice that represents the specified range of the
    /// the memory area within this memory slice. The behavior is undefined
    /// if the range is out of bounds.
    #[inline]
    pub unsafe fn get_slice_unchecked(&self, r: impl RangeBounds<usize>) -> MrSlice<'a> {
        let start = match r.start_bound() {
            Bound::Included(&s) => s,
            Bound::Excluded(&s) => s + 1,
            Bound::Unbounded => 0,
        };
        let end = match r.end_bound() {
            Bound::Included(&e) => e + 1,
            Bound::Excluded(&e) => e,
            Bound::Unbounded => self.len(),
        };

        MrSlice::new(
            self.mr,
            (self.range.start + start)..(self.range.start + end),
        )
    }

    /// Attempt to resize the memory region slice to the specified length.
    /// This attempt can have no effect if the desired length is greater
    /// than the largest possible length of the slice.
    /// Return whether the resize was successful.
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

impl<'a, I> Index<I> for MrSlice<'a>
where
    I: SliceIndex<[u8]>,
{
    type Output = <I as SliceIndex<[u8]>>::Output;

    /// Index
    #[inline]
    fn index(&self, index: I) -> &Self::Output {
        // SAFETY: TODO: is this really safe?
        let s = unsafe { slice::from_raw_parts(self.addr(), self.len()) };
        &s[index]
    }
}

impl From<MrSlice<'_>> for ibv_sge {
    fn from(slice: MrSlice<'_>) -> Self {
        Self {
            addr: slice.addr() as u64,
            length: slice.len() as u32,
            lkey: slice.mr.lkey(),
        }
    }
}

impl From<MrSlice<'_>> for NonNull<u8> {
    fn from(slice: MrSlice<'_>) -> Self {
        // SAFETY: the address is guaranteed to be non-null.
        unsafe { NonNull::new_unchecked(slice.addr()) }
    }
}
