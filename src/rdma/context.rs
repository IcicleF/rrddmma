//! Device context.

use std::io;
use std::os::fd::AsRawFd;
use std::ptr::NonNull;
use std::sync::Arc;

use super::nic::*;
use crate::bindings::*;
use crate::utils::interop::{from_c_err, from_c_ret};

/// Wrapper for `*mut ibv_context`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct IbvContext(NonNull<ibv_context>);

impl IbvContext {
    /// Get the underlying [`IbvDevice`] pointer.
    #[inline]
    pub fn dev(&self) -> IbvDevice {
        // SAFETY: the pointed-to `ibv_context` instance is valid.
        unsafe { IbvDevice::from(NonNull::new_unchecked(self.as_ref().device)) }
    }

    /// Query device attributes.
    pub fn query_device(&self) -> io::Result<ibv_device_attr> {
        // SAFETY: POD type.
        let mut dev_attr = Default::default();
        // SAFETY: FFI.
        let ret = unsafe { ibv_query_device(self.as_ptr(), &mut dev_attr) };
        match ret {
            0 => Ok(dev_attr),
            _ => from_c_err(ret),
        }
    }

    /// Close the context.
    ///
    /// # Safety
    ///
    /// - A context must not be closed more than once.
    /// - Closed contextes must not be used anymore.
    pub unsafe fn close(self) -> io::Result<()> {
        // SAFETY: FFI.
        let ret = ibv_close_device(self.as_ptr());
        from_c_ret(ret)
    }
}

impl_ibv_wrapper_traits!(ibv_context, IbvContext);

/// Ownership holder of device context.
struct ContextInner {
    ctx: IbvContext,
    attr: ibv_device_attr,
}

impl Drop for ContextInner {
    fn drop(&mut self) {
        // SAFETY: call only once, and no UAF since I will be dropped.
        unsafe { self.ctx.close() }.expect("cannot close context on drop");
    }
}

/// Device context.
#[derive(Clone)]
pub struct Context {
    /// Cached context pointer.
    ctx: IbvContext,

    /// Context body.
    inner: Arc<ContextInner>,
}

impl Context {
    /// Create a context from an opened device and its attributes.
    pub(crate) fn new(ctx: IbvContext, attr: ibv_device_attr) -> Self {
        Self {
            inner: Arc::new(ContextInner { ctx, attr }),
            ctx,
        }
    }
}

impl Context {
    /// Get the underlying [`ibv_context`] pointer.
    #[inline]
    pub fn as_raw(&self) -> *mut ibv_context {
        self.ctx.as_ptr()
    }

    /// Get the device attributes.
    #[inline]
    pub fn attr(&self) -> &ibv_device_attr {
        &self.inner.attr
    }
}

impl AsRawFd for Context {
    /// Get the `cmd_fd` of the context.
    fn as_raw_fd(&self) -> std::os::unix::prelude::RawFd {
        // SAFETY: the underlying `ibv_context` is valid.
        let ibv_ctx = unsafe { self.ctx.as_ref() };
        ibv_ctx.cmd_fd
    }
}
