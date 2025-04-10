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
pub(crate) struct IbvPd(Option<NonNull<ibv_pd>>);

impl IbvPd {
    /// Deallocate the PD.
    ///
    /// # Safety
    ///
    /// - A PD must not be deallocated more than once.
    /// - Deallocated PDs must not be used anymore.
    pub(crate) unsafe fn dealloc(self) -> io::Result<()> {
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
#[derive(Clone)]
pub struct Pd {
    /// Cached protection domain pointer.
    pd: IbvPd,

    /// Protection domain body.
    inner: Arc<PdInner>,
}

impl Pd {
    /// Allocate a protection domain for the given RDMA device context.
    pub fn new(ctx: &Context) -> io::Result<Self> {
        // SAFETY: FFI
        let pd = unsafe { ibv_alloc_pd(ctx.as_raw()) };
        let pd = NonNull::new(pd).ok_or_else(IoError::last_os_error)?;
        let pd = IbvPd::from(pd);

        Ok(Self {
            pd,
            inner: Arc::new(PdInner {
                ctx: ctx.clone(),
                pd,
            }),
        })
    }

    /// Get the underlying `ibv_pd` structure.
    #[inline]
    pub fn as_raw(&self) -> *mut ibv_pd {
        self.pd.as_ptr()
    }

    /// Get the RDMA device context of the protection domain.
    #[inline]
    pub fn context(&self) -> &Context {
        &self.inner.ctx
    }

    /// Consume and leak the `Pd`, returning the underlying `ibv_pd` pointer.
    /// The method receiver must be the only instance of the same protection domain, i.e.,
    ///
    /// - none of its clones may be alive
    /// - no [`Mr`](crate::lo::mr::Mr)s, [`Qp`](crate::lo::qp::Qp)s, or [`Srq`](crate::lo::srq::Srq)s created from it may be alive.
    ///
    /// otherwise, this method fails.
    pub fn leak(mut self) -> Result<*mut ibv_pd, Self> {
        let mut inner = {
            let inner = Arc::try_unwrap(self.inner);
            match inner {
                Ok(inner) => inner,
                Err(inner) => {
                    self.inner = inner;
                    return Err(self);
                }
            }
        };
        let pd = inner.pd.0.take().unwrap();
        Ok(pd.as_ptr())
    }
}
