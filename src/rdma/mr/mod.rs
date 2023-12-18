mod mr_slice;
mod perm;
mod remote;
mod slicing;

use std::cell::UnsafeCell;
use std::ffi::c_void;
use std::io::{self, Error as IoError};
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::slice;

pub use self::mr_slice::*;
pub use self::perm::*;
pub use self::remote::*;
pub use self::slicing::*;
use super::pd::Pd;
use crate::bindings::*;
use crate::utils::interop::from_c_ret;

/// Wrapper for `*mut ibv_mr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct IbvMr(NonNull<ibv_mr>);

impl IbvMr {
    /// Get the start address of the registered memory area.
    pub fn addr(&self) -> *mut u8 {
        // SAFETY: the `ibv_mr` instance is valid.
        unsafe { (*self.as_ptr()).addr as *mut u8 }
    }

    /// Get the length of the registered memory area.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        // SAFETY: the `ibv_mr` instance is valid.
        unsafe { (*self.as_ptr()).length }
    }

    /// Get the local key of the memory region.
    pub fn lkey(&self) -> u32 {
        // SAFETY: the `ibv_mr` instance is valid.
        unsafe { (*self.as_ptr()).lkey }
    }

    /// Get the remote key of the memory region.
    pub fn rkey(&self) -> u32 {
        // SAFETY: the `ibv_mr` instance is valid.
        unsafe { (*self.as_ptr()).rkey }
    }

    /// Deregister the MR.
    ///
    /// # Safety
    ///
    /// - An MR must not be deregistered more than once.
    /// - Deregistered MRs must not be used anymore.
    pub unsafe fn dereg(self) -> io::Result<()> {
        // SAFETY: FFI.
        let ret = ibv_dereg_mr(self.as_ptr());
        from_c_ret(ret)
    }
}

impl_ibv_wrapper_traits!(ibv_mr, IbvMr);

/// Local memory region.
///
/// A memory region is a virtual memory space registered to the RDMA device.
/// The registered memory itself does not belong to this type, but it must
/// outlive this type's lifetime (`'mem`) or there can be dangling pointers.
///
/// **Subtyping:** [`Mr<'a>`] is *covariant* over `'a`.
pub struct Mr<'a> {
    pd: &'a Pd<'a>,
    mr: IbvMr,
    _marker: PhantomData<&'a UnsafeCell<[u8]>>,
}

impl<'a> Mr<'a> {
    /// Register a memory region with the given protection domain.
    pub fn reg(pd: &'a Pd, buf: &'a [u8], perm: Permission) -> io::Result<Self> {
        // SAFETY: FFI.
        let mr = unsafe {
            ibv_reg_mr(
                pd.as_raw(),
                buf.as_ptr() as *mut c_void,
                buf.len(),
                perm.into(),
            )
        };
        let mr = NonNull::new(mr).ok_or_else(IoError::last_os_error)?;
        Ok(Self {
            pd,
            mr: IbvMr::from(mr),
            _marker: PhantomData,
        })
    }

    /// Get the underlying `ibv_mr` structure.
    #[inline]
    pub fn as_raw(&self) -> *mut ibv_mr {
        self.mr.as_ptr()
    }

    /// Get the local key of the memory region.
    #[inline]
    pub fn lkey(&self) -> u32 {
        self.mr.lkey()
    }

    /// Get the remote key of the memory region.
    #[inline]
    pub fn rkey(&self) -> u32 {
        self.mr.rkey()
    }

    /// Retrieve the registered memory area as a slice.
    ///
    /// # Safety
    ///
    /// See the safety documentation of [`std::slice::from_raw_parts_mut`].
    #[inline]
    pub unsafe fn mem(&self) -> &'a mut [u8] {
        slice::from_raw_parts_mut(self.addr(), self.len())
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

impl<'a, 's> Slicing<'s> for Mr<'a>
where
    'a: 's,
{
    type Output = MrSlice<'s>;

    #[inline]
    fn addr(&'s self) -> *mut u8 {
        self.mr.addr()
    }

    #[inline]
    fn len(&'s self) -> usize {
        self.mr.len()
    }

    #[inline]
    unsafe fn slice_unchecked(&'s self, offset: usize, len: usize) -> Self::Output {
        MrSlice::new(self, offset, len)
    }
}

impl Drop for Mr<'_> {
    fn drop(&mut self) {
        // SAFETY: call only once, and no UAF since I will be dropped.
        unsafe { self.mr.dereg() }.expect("cannot dereg MR on drop");
    }
}
