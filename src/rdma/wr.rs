use std::marker::PhantomData;
use std::{fmt, mem, ptr};

use rdma_sys::*;

use crate::utils::select::Select;

use super::mr::*;
use super::qp::{build_sgl, QpPeer};
use super::remote_mem::*;

/// This type has the same memory layout with [`ibv_sge`] but with a [`Debug`]
/// implementation.
#[derive(Debug)]
#[repr(C)]
struct IbvSgeDebuggable {
    pub addr: u64,
    pub length: u32,
    pub lkey: u32,
}

/// Wrapper of basic parameters of an RDMA work request.
struct WrBase<'mem> {
    local: Vec<ibv_sge>,
    wr_id: u64,
    signal: bool,

    /// Pretend to hold a reference to the original memory regions even if we
    /// have already transformed the slices into a scatter-gather list.
    /// This prevents the SGL from being invalid.
    marker: PhantomData<&'mem Mr<'mem>>,
}

/// Send work request details.
///
/// Aside from the necessities of every RDMA work request:
/// - a list of registered memory areas (can be empty) as the data resource
///   or target,
/// - a work request ID, and
/// - a set of flags (currently, only to signal or not),
///
/// this type holds the remaining parameters for each type of send work request.
///
/// **NOTE:** this type intentionally discriminates RDMA send via RC and UD QPs.
/// This is to improve performance when the user only uses RC.
#[derive(Debug)]
pub enum SendWrDetails<'a> {
    /// Send via RC QP, requires specifying an optional immediate and whether
    /// to inline.
    SendRc(
        /// [`Some`] to send with an immediate value, or [`None`] to send without.
        Option<u32>,
        /// Indicate whether to inline the send.
        bool,
    ),

    /// Send via UD QP, requires specifying the target, an optional immediate,
    /// and whether to inline.
    SendUd(
        /// The target QP.
        &'a QpPeer,
        /// [`Some`] to send with an immediate value, or [`None`] to send without.
        Option<u32>,
        /// Indicate whether to inline the send.
        bool,
    ),

    /// Read, requires a remote memory area to read from.
    Read(
        /// The remote memory area to read from.
        RemoteMem,
    ),

    /// Write, requires a remote memory area to write to and an optional immediate.
    Write(
        /// The remote memory area to write to.
        RemoteMem,
        /// [`Some`] to write with an immediate value, or [`None`] to write without.
        Option<u32>,
    ),

    /// Atomic compare-and-swap, requires a remote memory area to operate on, a
    /// value to compare against, and a value to swap with.
    CompareSwap(
        /// The remote memory area to operate on.
        /// This must be an aligned 8B memory area.
        RemoteMem,
        /// The value to compare against.
        u64,
        /// The value to swap with.
        u64,
    ),

    /// Atomic fetch-and-add, requires a remote memory area to operate on and a
    /// value to add.
    FetchAdd(
        /// The remote memory area to operate on.
        /// This must be an aligned 8B memory area.
        RemoteMem,
        /// The value to add.
        u64,
    ),
}

/// Send work request.
///
/// Use this type when you want to post multiple send work requests to a
/// queue pair at once (which can reduce doorbell ringing overheads).
///
/// **NOTE:** when using this type for RDMA atomics, the library will not
/// check for you whether the provided memory slices are 8B-sized and
/// 8B-aligned. It is your responsibility to ensure that they are properly
/// sized and aligned. However, there won't be an UB even if you don't:
/// the RDMA hardware will report an error for you.
pub struct SendWr<'a, 'b>(WrBase<'a>, SendWrDetails<'b>);

impl fmt::Debug for SendWr<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sge = self
            .0
            .local
            .iter()
            .map(|sge| unsafe { mem::transmute(*sge) })
            .collect::<Vec<IbvSgeDebuggable>>();
        f.debug_struct("SendWr")
            .field("sge", &sge)
            .field("wr_id", &self.0.wr_id)
            .field("signaled", &self.0.signal)
            .field("details", &self.1)
            .finish()
    }
}

impl<'a, 'b> SendWr<'a, 'b> {
    /// Create a new send work request with basic parameters and the details
    /// that specifies its concrete type.
    pub fn new(
        local: &[MrSlice<'a, '_>],
        wr_id: u64,
        signal: bool,
        additions: SendWrDetails<'b>,
    ) -> Self {
        Self(
            WrBase {
                local: build_sgl(local),
                wr_id,
                signal,
                marker: PhantomData,
            },
            additions,
        )
    }

    /// Get whether this work request is signaled.
    #[inline]
    pub fn signaled(&self) -> bool {
        self.0.signal
    }

    /// Translate the `SendWr` into a `ibv_send_wr` that can be passed to
    /// `ibv_post_send`.
    pub fn to_wr(&self) -> ibv_send_wr {
        let mut wr = unsafe { mem::zeroed::<ibv_send_wr>() };

        wr.wr_id = self.0.wr_id;
        wr.sg_list = self.0.local.as_ptr() as *mut _;
        wr.num_sge = self.0.local.len() as i32;
        wr.send_flags = self
            .0
            .signal
            .select_val(ibv_send_flags::IBV_SEND_SIGNALED.0, 0);
        wr.next = ptr::null_mut();

        // Fill in work request details
        fn fill_opcode_with_imm(
            wr: &mut ibv_send_wr,
            imm: &Option<u32>,
            op: ibv_wr_opcode::Type,
            op_with_imm: ibv_wr_opcode::Type,
        ) {
            if let Some(imm) = imm {
                wr.opcode = op_with_imm;
                wr.imm_data_invalidated_rkey_union =
                    imm_data_invalidated_rkey_union_t { imm_data: *imm };
            } else {
                wr.opcode = op;
            }
        }
        match &self.1 {
            SendWrDetails::SendRc(imm, inl) => {
                fill_opcode_with_imm(
                    &mut wr,
                    imm,
                    ibv_wr_opcode::IBV_WR_SEND,
                    ibv_wr_opcode::IBV_WR_SEND_WITH_IMM,
                );
                if *inl {
                    wr.send_flags |= ibv_send_flags::IBV_SEND_INLINE.0;
                }
            }
            SendWrDetails::SendUd(peer, imm, inl) => {
                wr.wr.ud = ud_t::from(*peer);
                fill_opcode_with_imm(
                    &mut wr,
                    imm,
                    ibv_wr_opcode::IBV_WR_SEND,
                    ibv_wr_opcode::IBV_WR_SEND_WITH_IMM,
                );
                if *inl {
                    wr.send_flags |= ibv_send_flags::IBV_SEND_INLINE.0;
                }
            }
            SendWrDetails::Read(remote) => {
                wr.wr.rdma = rdma_t::from(remote);
                wr.opcode = ibv_wr_opcode::IBV_WR_RDMA_READ;
            }
            SendWrDetails::Write(remote, imm) => {
                wr.wr.rdma = rdma_t::from(remote);
                fill_opcode_with_imm(
                    &mut wr,
                    imm,
                    ibv_wr_opcode::IBV_WR_RDMA_WRITE,
                    ibv_wr_opcode::IBV_WR_RDMA_WRITE_WITH_IMM,
                );
            }
            SendWrDetails::CompareSwap(remote, current, new) => {
                wr.wr.atomic = atomic_t {
                    compare_add: *current,
                    swap: *new,
                    remote_addr: remote.addr,
                    rkey: remote.rkey,
                };
                wr.opcode = ibv_wr_opcode::IBV_WR_ATOMIC_CMP_AND_SWP;
            }
            SendWrDetails::FetchAdd(remote, add) => {
                wr.wr.atomic = atomic_t {
                    compare_add: *add,
                    swap: 0,
                    remote_addr: remote.addr,
                    rkey: remote.rkey,
                };
                wr.opcode = ibv_wr_opcode::IBV_WR_ATOMIC_FETCH_AND_ADD;
            }
        };

        wr
    }
}

/// Receive work request.
///
/// Equivalent to the work request basics.
///
/// Use this type when you want to post multiple recv work requests to a
/// queue pair at once (which can reduce doorbell ringing overheads).
pub struct RecvWr<'a>(WrBase<'a>);

impl fmt::Debug for RecvWr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sge = self
            .0
            .local
            .iter()
            .map(|sge| unsafe { mem::transmute(*sge) })
            .collect::<Vec<IbvSgeDebuggable>>();
        f.debug_struct("RecvWr")
            .field("sge", &sge)
            .field("wr_id", &self.0.wr_id)
            .field("signaled", &self.0.signal)
            .finish()
    }
}

impl<'a> RecvWr<'a> {
    pub fn new(local: &[MrSlice<'a, '_>], wr_id: u64, signal: bool) -> Self {
        Self(WrBase {
            local: build_sgl(local),
            wr_id,
            signal,
            marker: PhantomData,
        })
    }

    /// Translate the `RecvWr` into a `ibv_recv_wr` that can be passed to
    /// `ibv_post_recv`.
    pub fn to_wr(&self) -> ibv_recv_wr {
        ibv_recv_wr {
            wr_id: self.0.wr_id,
            sg_list: self.0.local.as_ptr() as *mut _,
            num_sge: self.0.local.len() as i32,
            next: ptr::null_mut(),
        }
    }
}
