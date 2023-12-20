use std::marker::PhantomData;
use std::pin::Pin;
use std::{io, mem};

use crate::bindings::*;
use crate::rdma::{mr::*, qp::Qp};
use crate::utils::interop::from_c_ret;

include!("macros.rs");

/// Send work request with a compile-time determined number of SGEs.
pub struct SendWr<'a, const N: usize> {
    wr: ibv_send_wr,
    sgl: Pin<Box<[ibv_sge; N]>>,
    _marker: PhantomData<&'a Mr>,
}

/// Create a new send work request.
pub fn send_wr<'a, const N: usize>() -> SendWr<'a, N> {
    Default::default()
}

impl<const N: usize> Default for SendWr<'_, N> {
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

impl_wr_basic_setters!(SendWr);
impl_wr_flags_setters!(SendWr, send_flags);
impl_wr_raw_accessors!(SendWr, ibv_send_wr);

impl<const N: usize> SendWr<'_, N> {
    /// Set the work request to an RDMA send.
    #[inline]
    pub fn set_wr_send(&mut self, imm: Option<u32>) -> &mut Self {
        match imm {
            Some(imm) => {
                self.wr.opcode = ibv_wr_opcode::IBV_WR_SEND_WITH_IMM;
                self.wr.set_imm(imm);
            }
            None => self.wr.opcode = ibv_wr_opcode::IBV_WR_SEND,
        };
        self
    }

    /// Set the work request to an RDMA read.
    #[inline]
    pub fn set_wr_read(&mut self, remote: MrRemote) -> &mut Self {
        self.wr.opcode = ibv_wr_opcode::IBV_WR_RDMA_READ;
        self.wr.wr.rdma = (&remote).into();
        self
    }

    /// Set the work request to an RDMA write.
    #[inline]
    pub fn set_wr_write(&mut self, remote: MrRemote, imm: Option<u32>) -> &mut Self {
        self.wr.wr.rdma = (&remote).into();
        match imm {
            Some(imm) => {
                self.wr.opcode = ibv_wr_opcode::IBV_WR_RDMA_WRITE_WITH_IMM;
                self.wr.set_imm(imm);
            }
            None => self.wr.opcode = ibv_wr_opcode::IBV_WR_RDMA_WRITE,
        };
        self
    }

    /// Set the work request to an RDMA atomic compare-and-swap.
    #[inline]
    pub fn set_wr_cas(&mut self, remote: MrRemote, compare: u64, swap: u64) -> &mut Self {
        self.wr.opcode = ibv_wr_opcode::IBV_WR_ATOMIC_CMP_AND_SWP;
        self.wr.wr.atomic = atomic_t {
            remote_addr: remote.addr,
            compare_add: compare,
            swap,
            rkey: remote.rkey,
        };
        self
    }

    /// Set the work request to an RDMA atomic fetch-and-add.
    #[inline]
    pub fn set_wr_faa(&mut self, remote: MrRemote, add: u64) -> &mut Self {
        self.wr.opcode = ibv_wr_opcode::IBV_WR_ATOMIC_FETCH_AND_ADD;
        self.wr.wr.atomic = atomic_t {
            remote_addr: remote.addr,
            compare_add: add,
            swap: 0,
            rkey: remote.rkey,
        };
        self
    }

    /// Post the work request to the send queue.
    #[inline]
    pub fn post_on(&mut self, qp: &Qp) -> io::Result<()> {
        let mut bad_wr = std::ptr::null_mut();
        // SAFETY: FFI.
        let ret = unsafe { ibv_post_send(qp.as_raw(), &mut self.wr, &mut bad_wr) };
        from_c_ret(ret)
    }
}
