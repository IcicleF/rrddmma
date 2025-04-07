use std::marker::PhantomData;
use std::pin::Pin;
use std::{io, mem};

use crate::bindings::*;
use crate::lo::{mr::*, qp::Qp};
use crate::utils::interop::from_c_ret;

include!("macros.rs");

/// Send work request with a compile-time determined number of SGEs.
pub struct RecvWr<'a, const N: usize> {
    wr: ibv_recv_wr,
    sgl: Pin<Box<[ibv_sge; N]>>,
    _marker: PhantomData<&'a Mr>,
}

/// Create a new send work request.
pub fn recv_wr<'a, const N: usize>() -> RecvWr<'a, N> {
    Default::default()
}

impl<const N: usize> Default for RecvWr<'_, N> {
    fn default() -> Self {
        let mut this = Self {
            // SAFETY: POD type.
            wr: unsafe { mem::zeroed() },

            // SAFETY: POD type.
            sgl: Box::pin([unsafe { mem::zeroed() }; N]),
            _marker: PhantomData,
        };
        this.wr.sg_list = this.sgl.as_mut_ptr();
        this
    }
}

impl_wr_basic_setters!(RecvWr);
impl_wr_raw_accessors!(RecvWr, ibv_recv_wr);

impl<const N: usize> RecvWr<'_, N> {
    /// Post the work request to the send queue.
    #[inline]
    pub fn post(&mut self, qp: &Qp) -> io::Result<()> {
        let mut bad_wr = std::ptr::null_mut();
        // SAFETY: FFI.
        let ret = unsafe { ibv_post_recv(qp.as_raw(), &mut self.wr, &mut bad_wr) };
        from_c_ret(ret)
    }
}
