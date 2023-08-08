use std::io;
use std::ptr::NonNull;
use std::sync::Arc;

use super::context::Context;
use super::mr::Mr;
use super::qp::{Qp, QpInitAttr};

use anyhow::{Context as _, Result};
use rdma_sys::*;

#[allow(dead_code)]
#[derive(Debug)]
struct PdInner {
    ctx: Context,
    pd: NonNull<ibv_pd>,
}

unsafe impl Send for PdInner {}
unsafe impl Sync for PdInner {}

impl Drop for PdInner {
    fn drop(&mut self) {
        // SAFETY: FFI.
        unsafe { ibv_dealloc_pd(self.pd.as_ptr()) };
    }
}

/// Protection domain.
///
/// This type is a simple wrapper of an `Arc` and is guaranteed to have the
/// same memory layout with it.
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct Pd {
    inner: Arc<PdInner>,
}

impl Pd {
    /// Allocate a protection domain for the given RDMA device context.
    pub fn new(ctx: Context) -> Result<Self> {
        // SAFETY: FFI
        let pd = NonNull::new(unsafe { ibv_alloc_pd(ctx.as_raw()) })
            .ok_or_else(|| anyhow::anyhow!(io::Error::last_os_error()))
            .with_context(|| "failed to create protection domain")?;

        Ok(Self {
            inner: Arc::new(PdInner { ctx, pd }),
        })
    }

    /// Get the underlying `ibv_pd` structure.
    #[inline]
    pub fn as_raw(&self) -> *mut ibv_pd {
        self.inner.pd.as_ptr()
    }

    /// Get the underlying `Context`.
    #[inline]
    pub fn context(&self) -> &Context {
        &self.inner.ctx
    }

    /// Register a memory region on this protection domain.
    pub fn reg_mr<'mem>(&self, buf: &'mem [u8]) -> Result<Mr<'mem>> {
        // SAFETY: this call simply decouples the reference to a long pointer
        // into an address, a length, and a lifetime.
        unsafe { Mr::reg_with_ref(self.clone(), buf.as_ptr() as *mut u8, buf.len(), buf) }
    }

    /// Create a queue pair on this protection domain.
    pub fn create_qp(&self, init_attr: QpInitAttr) -> Result<Qp> {
        Qp::new(self.clone(), init_attr)
    }
}
