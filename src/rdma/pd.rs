//! Protection domain.

use std::io::{self, Error as IoError};
use std::ptr::NonNull;
use std::sync::Arc;

use super::context::Context;
use crate::bindings::*;
use crate::utils::interop::from_c_ret;

/// Wrapper for `*mut ibv_pd`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct IbvPd(NonNull<ibv_pd>);

impl IbvPd {
    /// Deallocate the PD.
    ///
    /// # Safety
    ///
    /// - A PD must not be deallocated more than once.
    /// - Deallocated PDs must not be used anymore.
    pub unsafe fn dealloc(self) -> io::Result<()> {
        // SAFETY: FFI.
        let ret = ibv_dealloc_pd(self.as_ptr());
        from_c_ret(ret)
    }
}

impl_ibv_wrapper_traits!(ibv_pd, IbvPd);

/// Ownership holder of protection domain.
struct PdInner {
    ctx: Context,
    pd: IbvPd,
}

impl Drop for PdInner {
    fn drop(&mut self) {
        // SAFETY: call only once, and no UAF since I will be dropped.
        unsafe { self.pd.dealloc() }.expect("cannot dealloc PD on drop");
    }
}

/// Protection domain.
pub struct Pd {
    inner: Arc<PdInner>,
    pd: IbvPd,
}

impl Pd {
    /// Make a clone of the `Arc` pointer.
    pub(crate) fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            pd: self.pd,
        }
    }
}

impl Pd {
    /// Allocate a protection domain for the given RDMA device context.
    pub fn new(ctx: &Context) -> io::Result<Self> {
        // SAFETY: FFI
        let pd = unsafe { ibv_alloc_pd(ctx.as_raw()) };
        let pd = NonNull::new(pd).ok_or_else(IoError::last_os_error)?;
        let pd = IbvPd(pd);

        Ok(Self {
            inner: Arc::new(PdInner {
                ctx: ctx.clone(),
                pd,
            }),
            pd,
        })
    }

    /// Get the underlying `ibv_pd` structure.
    #[inline]
    pub fn as_raw(&self) -> *mut ibv_pd {
        self.pd.as_ptr()
    }

    /// Get the underlying `Context`.
    #[inline]
    pub fn context(&self) -> &Context {
        &self.inner.ctx
    }
}
