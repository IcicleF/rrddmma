use std::io;
use std::ptr::NonNull;

use super::context::Context;

use anyhow;
use rdma_sys::*;

/// Protection domain.
#[allow(dead_code)]
#[derive(Debug)]
pub struct Pd<'a> {
    ctx: &'a Context,
    pd: NonNull<ibv_pd>,
}

unsafe impl<'a> Sync for Pd<'a> {}

impl<'a> Pd<'a> {
    pub fn alloc(ctx: &'a Context) -> anyhow::Result<Self> {
        let pd = NonNull::new(unsafe { ibv_alloc_pd(ctx.as_ptr()) })
            .ok_or_else(|| anyhow::anyhow!(io::Error::last_os_error()))?;

        Ok(Self { ctx, pd })
    }

    pub fn as_ptr(&self) -> *mut ibv_pd {
        self.pd.as_ptr()
    }

    pub fn context(&self) -> &Context {
        self.ctx
    }
}

impl<'a> Drop for Pd<'a> {
    fn drop(&mut self) {
        unsafe { ibv_dealloc_pd(self.as_ptr()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alloc() {
        let ctx = Context::open(Some("mlx5_0"), 1, 0).unwrap();
        let pd = Pd::alloc(&ctx).unwrap();
        assert_eq!(pd.context().as_ptr(), ctx.as_ptr());
    }
}
