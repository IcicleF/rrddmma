use std::io;
use std::ptr::NonNull;
use std::sync::Arc;

use super::context::Context;

use anyhow;
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
    pub fn new(ctx: Context) -> anyhow::Result<Self> {
        let pd = NonNull::new(unsafe { ibv_alloc_pd(ctx.as_ptr()) })
            .ok_or_else(|| anyhow::anyhow!(io::Error::last_os_error()))?;

        Ok(Self {
            inner: Arc::new(PdInner { ctx, pd }),
        })
    }

    /// Get the underlying `ibv_pd` structure.
    pub fn as_ptr(&self) -> *mut ibv_pd {
        self.inner.pd.as_ptr()
    }

    /// Get the underlying `Context`.
    pub fn context(&self) -> Context {
        self.inner.ctx.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alloc() {
        let ctx = Context::open(Some("mlx5_0"), 1, 0).unwrap();
        let pd = Pd::new(&ctx).unwrap();
        assert_eq!(pd.context().as_ptr(), ctx.as_ptr());
    }
}
