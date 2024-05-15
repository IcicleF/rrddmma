//! Shared Receive Queues.

use std::{fmt, ptr};
use std::io::{self, Error as IoError};
use std::ptr::NonNull;
use std::sync::Arc;

use crate::bindings::*;
use crate::rdma::{context::Context, mr::*, pd::Pd, qp::build_sgl};
use crate::utils::interop::*;

/// Wrapper for `*mut ibv_srq`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub(crate) struct IbvSrq(NonNull<ibv_srq>);

impl IbvSrq {
    /// Destroy the SRQ.
    ///
    /// # Safety
    ///
    /// - An SRQ must not be destroyed more than once.
    /// - Destroyed SRQs must not be used anymore.
    pub unsafe fn destroy(self) -> io::Result<()> {
        // SAFETY: FFI.
        let ret = ibv_destroy_srq(self.as_ptr());
        from_c_ret(ret)
    }
}

impl_ibv_wrapper_traits!(ibv_srq, IbvSrq);

/// Ownership holder of the SRQ.
struct SrqInner {
    pd: Pd,
    srq: IbvSrq,
    num: u32,
}

impl Drop for SrqInner {
    fn drop(&mut self) {
        // SAFETY: call only once, and no UAF since I will be dropped.
        unsafe { self.srq.destroy() }.expect("cannot destroy SRQ on drop");
    }
}

/// Shared receive queue.
#[derive(Clone)]
pub struct Srq {
    /// Cached SRQ pointer.
    srq: IbvSrq,

    /// SRQ body.
    inner: Arc<SrqInner>,
}

impl fmt::Debug for Srq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("Srq<{:p}>", self.as_raw()))
    }
}

impl Srq {
    /// Create a shared receive queue on the given RDMA protection domain.
    pub fn new(pd: &Pd, max_wr: u32, max_sge: u32) -> io::Result<Self> {
        let mut init_attr = ibv_srq_init_attr {
            srq_context: ptr::null_mut(),
            attr: ibv_srq_attr {
                max_wr,
                max_sge,
                srq_limit: 0,
            },
        };
        // SAFETY: FFI.
        let srq = unsafe { ibv_create_srq(pd.as_raw(), &mut init_attr) };
        let srq = NonNull::new(srq).ok_or_else(IoError::last_os_error)?;
        let srq = IbvSrq(srq);

        // Query srq_num.
        let mut num = 0;
        // SAFETY: FFI.
        let ret = unsafe { ibv_get_srq_num(srq.as_ptr(), &mut num) };
        from_c_ret(ret)?;

        let srq = Srq {
            inner: Arc::new(SrqInner {
                pd: pd.clone(),
                srq,
                num,
            }),
            srq,
        };
        Ok(srq)
    }

    /// Get the underlying `ibv_srq` pointer.
    #[inline]
    pub fn as_raw(&self) -> *mut ibv_srq {
        self.srq.as_ptr()
    }

    /// Get the protection domain of the SRQ.
    pub fn pd(&self) -> &Pd {
        &self.inner.pd
    }

    /// Get the RDMA device context of the SRQ.
    pub fn context(&self) -> &Context {
        self.inner.pd.context()
    }

    /// Get the SRQ number.
    pub fn srq_num(&self) -> u32 {
        self.inner.num
    }

    /// Post a receive work request to the SRQ.
    pub fn recv(&self, local: &[MrSlice], wr_id: u64) -> io::Result<()> {
        let mut sgl = build_sgl(local);
        let mut wr = ibv_recv_wr {
            wr_id,
            next: ptr::null_mut(),
            sg_list: if local.is_empty() {
                ptr::null_mut()
            } else {
                sgl.as_mut_ptr()
            },
            num_sge: local.len() as i32,
        };
        let ret = {
            let mut bad_wr = ptr::null_mut();
            // SAFETY: FFI.
            unsafe { ibv_post_srq_recv(self.as_raw(), &mut wr, &mut bad_wr) }
        };
        from_c_ret(ret)
    }
}
