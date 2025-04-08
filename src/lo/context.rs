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
pub(crate) struct IbvContext(Option<NonNull<ibv_context>>);

impl IbvContext {
    /// Get the underlying [`IbvDevice`] pointer.
    pub(crate) fn dev(&self) -> IbvDevice {
        // SAFETY: the pointed-to `ibv_context` instance is valid.
        unsafe { IbvDevice::from(NonNull::new_unchecked(self.as_ref().device)) }
    }

    /// Query device attributes.
    pub(crate) fn query_device(&self) -> io::Result<ibv_device_attr> {
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
    pub(crate) unsafe fn close(self) -> io::Result<()> {
        // SAFETY: FFI.
        match self.0 {
            Some(ctx) => {
                let ret = ibv_close_device(ctx.as_ptr());
                from_c_ret(ret)
            }
            None => Ok(()),
        }
    }
}

impl_ibv_wrapper_traits!(ibv_context, IbvContext);

/// Ownership holder of device context.
struct ContextInner {
    ctx: IbvContext,
    attr: ibv_device_attr,

    #[cfg(feature = "exp")]
    clock_info: ibv_exp_clock_info,
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
    #[cfg(feature = "exp")]
    pub(crate) fn new(ctx: IbvContext, attr: ibv_device_attr) -> Self {
        // SAFETY: FFI.
        let clock_info = unsafe {
            let mut values = std::mem::zeroed();
            ibv_exp_query_values(ctx.as_ptr(), IBV_EXP_VALUES_CLOCK_INFO as _, &mut values);
            values.clock_info
        };
        Self {
            inner: Arc::new(ContextInner {
                ctx,
                attr,
                clock_info,
            }),
            ctx,
        }
    }

    /// Create a context from an opened device and its attributes.
    #[cfg(not(feature = "exp"))]
    pub(crate) fn new(ctx: IbvContext, attr: ibv_device_attr) -> Self {
        Self {
            ctx,
            inner: Arc::new(ContextInner { ctx, attr }),
        }
    }
}

impl Context {
    /// Get the underlying [`ibv_context`] raw pointer.
    pub fn as_raw(&self) -> *mut ibv_context {
        self.ctx.as_ptr()
    }

    /// Consume and leak the `Context`, returning the underlying [`ibv_context`] raw pointer.
    /// The method receiver must be the only instance of the same context, i.e.,
    ///
    /// - None of its clones may be alive.
    /// - No [`Pd`](crate::prelude::Pd)s or [`Cq`](crate::prelude::Cq)s created from it may be alive.
    ///
    /// Otherwise, this method fails.
    pub fn leak(mut self) -> Result<*mut ibv_context, Self> {
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
        let ctx = inner.ctx.0.take().unwrap();
        Ok(ctx.as_ptr())
    }

    /// Get the underlying device attributes.
    pub fn attr(&self) -> &ibv_device_attr {
        &self.inner.attr
    }

    /// Get the clock information.
    #[cfg(feature = "exp")]
    pub fn clock_info(&self) -> &ibv_exp_clock_info {
        &self.inner.clock_info
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
