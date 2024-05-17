//! Dynamically-connected target (DCT).
#![cfg(mlnx4)]

use std::ptr::NonNull;
use std::sync::Arc;
use std::{fmt, io, mem};

use thiserror::Error;

use crate::bindings::*;
use crate::rdma::{context::Context, cq::Cq, pd::Pd, qp::QpEndpoint, srq::Srq};
use crate::utils::interop::*;

pub use self::builder::*;

mod builder;

/// Wrapper for `*mut ibv_exp_dct`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub(crate) struct IbvExpDct(NonNull<ibv_exp_dct>);

impl IbvExpDct {
    /// Destroy the DCT.
    ///
    /// # Safety
    ///
    /// - A DCT must not be destroyed more than once.
    /// - Destroyed DCTs must not be used anymore.
    pub unsafe fn destroy(self) -> io::Result<()> {
        // SAFETY: FFI.
        let ret = ibv_exp_destroy_dct(self.as_ptr());
        from_c_ret(ret)
    }
}

impl_ibv_wrapper_traits!(ibv_exp_dct, IbvExpDct);

/// DCT creation error type.
#[derive(Debug, Error)]
pub enum DctCreationError {
    /// `libibverbs` interfaces returned an error.
    #[error("I/O error from ibverbs")]
    IoError(#[from] io::Error),

    /// Created DCT has a different DC key than the specified one.
    #[error("queried DCKey {0} is different from the default")]
    DifferentDcKey(u64),

    /// DCT state is not active.
    #[error("DCT is not in active state")]
    NotActive,
}

/// Ownership holder of the DCT.
struct DctInner {
    ctx: Context,
    dct: IbvExpDct,
    num: u32,
    init_attr: DctInitAttr,
}

impl Drop for DctInner {
    fn drop(&mut self) {
        // SAFETY: call only once, and no UAF since I will be dropped.
        unsafe { self.dct.destroy() }.expect("cannot destroy DCT on drop");
    }
}

/// Dynamically-connected target (DCT).
pub struct Dct {
    /// Cached DCT endpoint pointer.
    dct: IbvExpDct,

    /// DCT body.
    inner: Arc<DctInner>,
}

impl fmt::Debug for Dct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // SAFETY: `self` is a valid pointer to `ibv_exp_dct`.
        f.write_fmt(format_args!(
            "Dct<{:p}, num={}>",
            self.as_raw(),
            self.dct_num()
        ))
    }
}

impl Dct {
    /// Get the initialization attributes of the DCT.
    pub(crate) fn init_attr(&self) -> &DctInitAttr {
        &self.inner.init_attr
    }

    /// Create a new DCT.
    pub(crate) fn new(ctx: &Context, builder: DctBuilder) -> Result<Self, DctCreationError> {
        let init_attr = builder.unwrap()?;
        let dct = {
            let mut init_attr = init_attr.to_init_attr();
            // SAFETY: FFI.
            unsafe { ibv_exp_create_dct(ctx.as_raw(), &mut init_attr) }
        };
        let dct = NonNull::new(dct).ok_or_else(io::Error::last_os_error)?;
        let dct = IbvExpDct(dct);

        // Query the DCT to ensure things actually work.
        {
            // SAFETY: POD type.
            let mut attr = unsafe { mem::zeroed::<ibv_exp_dct_attr>() };

            // SAFETY: FFI.
            let ret = unsafe { ibv_exp_query_dct(dct.as_ptr(), &mut attr) };
            from_c_ret(ret)?;

            if attr.dc_key != Self::GLOBAL_DC_KEY {
                return Err(DctCreationError::DifferentDcKey(attr.dc_key));
            }
            if attr.state != IBV_EXP_DCT_STATE_ACTIVE {
                return Err(DctCreationError::NotActive);
            }
        }

        // SAFETY: `dct` points to a valid `ibv_exp_dct` instance.
        let num = unsafe { (*dct.as_ptr()).dct_num };
        let dct = Dct {
            inner: Arc::new(DctInner {
                ctx: ctx.clone(),
                dct,
                num,
                init_attr,
            }),
            dct,
        };
        Ok(dct)
    }
}

impl Dct {
    /// Global DC Key.
    pub const GLOBAL_DC_KEY: u64 = 0x1919810;

    /// Return a new DCT builder.
    pub fn builder<'a>() -> DctBuilder<'a> {
        Default::default()
    }

    /// Get the underlying `ibv_exp_dct` pointer.
    #[inline]
    pub fn as_raw(&self) -> *mut ibv_exp_dct {
        self.dct.as_ptr()
    }

    /// Get the RDMA device context of the DCT.
    pub fn context(&self) -> &Context {
        &self.inner.ctx
    }

    /// Get the protection domain of the DCT.
    pub fn pd(&self) -> &Pd {
        &self.inner.init_attr.pd
    }

    /// Get the DCT number.
    pub fn dct_num(&self) -> u32 {
        self.inner.num
    }

    /// Get the SRQ of this DCT.
    pub fn srq(&self) -> &Srq {
        &self.inner.init_attr.srq
    }

    /// Get the CQ of this DCT.
    pub fn cq(&self) -> &Cq {
        &self.inner.init_attr.cq
    }

    /// Get the endpoint information of this DCT.
    pub fn endpoint(&self) -> QpEndpoint {
        QpEndpoint::of_dct(self)
    }
}
