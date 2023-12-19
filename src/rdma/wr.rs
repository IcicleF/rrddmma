use std::marker::PhantomData;
use std::{fmt, mem, ptr};

use crate::bindings::*;

use super::mr::*;
use super::qp::{build_sgl, QpPeer};
use super::types::*;

use crate::utils::select::Select;

/// This type has the same memory layout with [`ibv_sge`] but with a [`Debug`]
/// implementation.
#[repr(C)]
struct IbvSgeDebuggable {
    pub addr: u64,
    pub length: u32,
    pub lkey: u32,
}

impl fmt::Debug for IbvSgeDebuggable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            "Sge [{:p}, {:p})",
            self.addr as *const u8,
            (self.addr + self.length as u64) as *const u8
        ))
    }
}

/// Wrapper of basic parameters of an RDMA work request.
struct WrBase<'mem> {
    local: Vec<ibv_sge>,
    wr_id: WrId,
    signal: bool,

    /// Pretend to hold a reference to the original memory regions even if we
    /// have already transformed the slices into a scatter-gather list.
    /// This prevents the SGL from being invalid.
    marker: PhantomData<&'mem Mr>,
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
    /// Send via RC QP.
    SendRc {
        /// [`Some`] to send with an immediate value, or [`None`] to send without.
        imm: Option<ImmData>,
        /// Indicate whether to inline the send.
        inline: bool,
    },

    /// Send via UD QP.
    SendUd {
        /// Information of the receiver.
        peer: &'a QpPeer,
        /// [`Some`] to send with an immediate value, or [`None`] to send without.
        imm: Option<ImmData>,
        /// Indicate whether to inline the send.
        inline: bool,
    },

    /// Read.
    Read {
        /// The remote memory area to read from.
        src: RemoteMem,
    },

    /// Write.
    Write {
        /// The remote memory area to write to.
        dst: RemoteMem,
        /// [`Some`] to write with an immediate value, or [`None`] to write without.
        imm: Option<ImmData>,
    },

    /// Atomic compare-and-swap.
    CompareSwap {
        /// The remote memory area to operate on.
        /// This must be an aligned 8B memory area.
        dst: RemoteMem,
        /// The value to compare against.
        current: u64,
        /// The value to swap with.
        new: u64,
    },

    /// Atomic fetch-and-add.
    FetchAdd {
        /// The remote memory area to operate on.
        /// This must be an aligned 8B memory area.
        dst: RemoteMem,
        /// The value to add.
        add: u64,
    },
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
pub struct SendWr<'a>(WrBase<'a>, SendWrDetails<'a>);

impl fmt::Debug for SendWr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sge = self
            .0
            .local
            .iter()
            // SAFETY: `IbvSgeDebuggable` has the same memory layout with `ibv_sge`.
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

impl<'a> SendWr<'a> {
    /// Create a new send work request with basic parameters and the details
    /// that specifies its concrete type.
    pub fn new(
        local: &[MrSlice<'a>],
        wr_id: u64,
        signal: bool,
        details: SendWrDetails<'a>,
    ) -> Self {
        Self(
            WrBase {
                local: build_sgl(local),
                wr_id,
                signal,
                marker: PhantomData,
            },
            details,
        )
    }

    /// Set whether this work request is signaled. Return `self` for chaining.
    #[inline]
    pub fn set_signaled(&mut self, signaled: bool) -> &mut Self {
        self.0.signal = signaled;
        self
    }

    /// Get whether this work request is signaled.
    #[inline]
    pub fn is_signaled(&self) -> bool {
        self.0.signal
    }

    /// Translate the `SendWr` into a `ibv_send_wr` that can be passed to
    /// `ibv_post_send`.
    pub fn to_wr(&self) -> ibv_send_wr {
        // SAFETY: this is safe in C.
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
                wr.set_imm(*imm);
            } else {
                wr.opcode = op;
            }
        }

        match &self.1 {
            SendWrDetails::SendRc { imm, inline } => {
                fill_opcode_with_imm(
                    &mut wr,
                    imm,
                    ibv_wr_opcode::IBV_WR_SEND,
                    ibv_wr_opcode::IBV_WR_SEND_WITH_IMM,
                );
                if *inline {
                    wr.send_flags |= ibv_send_flags::IBV_SEND_INLINE.0;
                }
            }
            SendWrDetails::SendUd { peer, imm, inline } => {
                wr.wr.ud = peer.ud();
                fill_opcode_with_imm(
                    &mut wr,
                    imm,
                    ibv_wr_opcode::IBV_WR_SEND,
                    ibv_wr_opcode::IBV_WR_SEND_WITH_IMM,
                );
                if *inline {
                    wr.send_flags |= ibv_send_flags::IBV_SEND_INLINE.0;
                }
            }
            SendWrDetails::Read { src } => {
                wr.wr.rdma = src.as_rdma_t();
                wr.opcode = ibv_wr_opcode::IBV_WR_RDMA_READ;
            }
            SendWrDetails::Write { dst, imm } => {
                wr.wr.rdma = dst.as_rdma_t();
                fill_opcode_with_imm(
                    &mut wr,
                    imm,
                    ibv_wr_opcode::IBV_WR_RDMA_WRITE,
                    ibv_wr_opcode::IBV_WR_RDMA_WRITE_WITH_IMM,
                );
            }
            SendWrDetails::CompareSwap { dst, current, new } => {
                wr.wr.atomic = atomic_t {
                    compare_add: *current,
                    swap: *new,
                    remote_addr: dst.addr,
                    rkey: dst.rkey,
                };
                wr.opcode = ibv_wr_opcode::IBV_WR_ATOMIC_CMP_AND_SWP;
            }
            SendWrDetails::FetchAdd { dst, add } => {
                wr.wr.atomic = atomic_t {
                    compare_add: *add,
                    swap: 0,
                    remote_addr: dst.addr,
                    rkey: dst.rkey,
                };
                wr.opcode = ibv_wr_opcode::IBV_WR_ATOMIC_FETCH_AND_ADD;
            }
        };

        wr
    }
}

/// Raw send work request that owns a WR and its SGEs.
///
/// This type is useful when you really want to avoid any memory copy overheads
/// (aside from those in the driver) when posting a send work request.
pub struct RawSendWr(pub(crate) Vec<ibv_sge>, pub(crate) ibv_send_wr);

unsafe impl Send for RawSendWr {}
unsafe impl Sync for RawSendWr {}

impl From<SendWr<'_>> for RawSendWr {
    fn from(wr: SendWr<'_>) -> Self {
        let raw_wr = wr.to_wr();
        Self(wr.0.local, raw_wr)
    }
}

impl RawSendWr {
    /// Get the raw scatter-gather list.
    pub fn sgl(&self) -> &[ibv_sge] {
        &self.0
    }

    /// Set the raw scatter-gather list.
    pub fn set_sgl(&mut self, sgl: Vec<ibv_sge>) {
        self.0 = sgl;
        self.1.sg_list = self.0.as_ptr() as *mut _;
        self.1.num_sge = self.0.len() as i32;
    }

    /// Get the raw work request item for mutation.
    ///
    /// # Safety
    ///
    /// When mutating the raw work request, you must ensure that:
    ///
    /// - The `sg_list` field must point to a valid [`ibv_sge`] array.
    /// - The `num_sge` field must be a valid length of the SGE array.
    /// - The `next` field must be null or a valid pointer to another valid
    ///   [`ibv_send_wr`] object that outlives `self`.
    /// - Proper fields must be set according to the work request type.
    pub unsafe fn raw_wr(&mut self) -> &mut ibv_send_wr {
        &mut self.1
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
            // SAFETY: `IbvSgeDebuggable` has the same memory layout with `ibv_sge`.
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
    pub fn new(local: &[MrSlice<'a>], wr_id: u64) -> Self {
        Self(WrBase {
            local: build_sgl(local),
            wr_id,
            signal: true,
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

/// Raw receive work request that owns a WR and its SGEs.
///
/// This type is useful when you really want to avoid any memory copy overheads
/// (aside from those in the driver) when posting a receive work request.
pub struct RawRecvWr(pub(crate) Vec<ibv_sge>, pub(crate) ibv_recv_wr);

unsafe impl Send for RawRecvWr {}
unsafe impl Sync for RawRecvWr {}

impl RawRecvWr {
    /// Get the raw scatter-gather list.
    pub fn sgl(&self) -> &[ibv_sge] {
        &self.0
    }

    /// Set the raw scatter-gather list.
    pub fn set_sgl(&mut self, sgl: Vec<ibv_sge>) {
        self.0 = sgl;
        self.1.sg_list = self.0.as_ptr() as *mut _;
        self.1.num_sge = self.0.len() as i32;
    }

    /// Get the raw work request item for mutation.
    ///
    /// # Safety
    ///
    /// When mutating the raw work request, you must ensure that:
    ///
    /// - The `sg_list` field must point to a valid [`ibv_sge`] array.
    /// - The `num_sge` field must be a valid length of the SGE array.
    /// - The `next` field must be null or a valid pointer to another valid
    ///   [`ibv_recv_wr`] object that outlives `self`.
    pub unsafe fn raw_wr(&mut self) -> &mut ibv_recv_wr {
        &mut self.1
    }
}

impl From<RecvWr<'_>> for RawRecvWr {
    fn from(wr: RecvWr<'_>) -> Self {
        let raw_wr = wr.to_wr();
        Self(wr.0.local, raw_wr)
    }
}
