use std::ptr::NonNull;
use std::sync::Arc;
use std::{fmt, io, mem, ptr};

use super::context::Context;

use anyhow::Result;
use rdma_sys::*;
use thiserror::Error;

/// Opcode of a completion queue entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WcOpcode {
    /// Send request.
    Send = ibv_wc_opcode::IBV_WC_SEND as _,
    /// RDMA write request.
    RdmaWrite = ibv_wc_opcode::IBV_WC_RDMA_WRITE as _,
    /// RDMA read request.
    RdmaRead = ibv_wc_opcode::IBV_WC_RDMA_READ as _,
    /// RDMA compare-and-swap request.
    CompSwap = ibv_wc_opcode::IBV_WC_COMP_SWAP as _,
    /// RDMA fetch-and-add request.
    FetchAdd = ibv_wc_opcode::IBV_WC_FETCH_ADD as _,
    /// Memory window bind request.
    BindMw = ibv_wc_opcode::IBV_WC_BIND_MW as _,
    /// Receive request.
    Recv = ibv_wc_opcode::IBV_WC_RECV as _,
    /// Receive request with immediate data.
    RecvRdmaImm = ibv_wc_opcode::IBV_WC_RECV_RDMA_WITH_IMM as _,
}

impl From<u32> for WcOpcode {
    fn from(wc_opcode: u32) -> Self {
        match wc_opcode {
            ibv_wc_opcode::IBV_WC_SEND => WcOpcode::Send,
            ibv_wc_opcode::IBV_WC_RDMA_WRITE => WcOpcode::RdmaWrite,
            ibv_wc_opcode::IBV_WC_RDMA_READ => WcOpcode::RdmaRead,
            ibv_wc_opcode::IBV_WC_COMP_SWAP => WcOpcode::CompSwap,
            ibv_wc_opcode::IBV_WC_FETCH_ADD => WcOpcode::FetchAdd,
            ibv_wc_opcode::IBV_WC_BIND_MW => WcOpcode::BindMw,
            ibv_wc_opcode::IBV_WC_RECV => WcOpcode::Recv,
            ibv_wc_opcode::IBV_WC_RECV_RDMA_WITH_IMM => WcOpcode::RecvRdmaImm,
            _ => panic!("invalid opcode: {}", wc_opcode),
        }
    }
}

/// Status of a completion queue entry.
///
/// The documentation and error messages are heavily borrowed from [RDMAmojo](https://www.rdmamojo.com/2013/02/15/ibv_poll_cq/).
/// My biggest thanks to the author, Dotan Barak.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[repr(u32)]
pub enum WcStatus {
    /// **Operation completed successfully:** this means that the corresponding
    /// Work Request (and all of the unsignaled Work Requests that were posted
    /// previous to it) ended and the memory buffers that this Work Request
    /// refers to are ready to be (re)used.
    #[error("success")]
    Success = ibv_wc_status::IBV_WC_SUCCESS as _,

    /// **Local Length Error:** this happens if a Work Request that was posted
    /// in a local Send Queue contains a message that is greater than the maximum
    /// message size that is supported by the RDMA device port that should send
    /// the message or an Atomic operation which its size is different than 8
    /// bytes was sent. This also may happen if a Work Request that was posted
    /// in a local Receive Queue isn't big enough for holding the incoming
    /// message or if the incoming message size if greater the maximum message
    /// size supported by the RDMA device port that received the message.
    #[error("local length error")]
    LocLenErr = ibv_wc_status::IBV_WC_LOC_LEN_ERR as _,

    /// **Local QP Operation Error:** an internal QP consistency error was
    /// detected while processing this Work Request: this happens if a Work
    /// Request that was posted in a local Send Queue of a UD QP contains an
    /// Address Handle that is associated with a Protection Domain to a QP which
    /// is associated with a different Protection Domain or an opcode which isn't
    /// supported by the transport type of the QP isn't supported (for example:
    /// RDMA Write over a UD QP).
    #[error("local QP operation error")]
    LocQpOpErr = ibv_wc_status::IBV_WC_LOC_QP_OP_ERR as _,

    /// **Local EE Context Operation Error:** an internal EE Context
    /// consistency error was detected while processing this Work Request
    /// (**unused**, since it is relevant only to RD QPs or EE Context, which
    /// aren’t supported).
    #[error("local EE context operation error")]
    LocEecOpErr = ibv_wc_status::IBV_WC_LOC_EEC_OP_ERR as _,

    /// **Local Protection Error:** the locally posted Work Request’s buffers
    /// in the scatter/gather list does not reference a Memory Region that is
    /// valid for the requested operation.
    #[error("local protection error")]
    LocProtErr = ibv_wc_status::IBV_WC_LOC_PROT_ERR as _,

    /// **Work Request Flushed Error:** a Work Request was in process or
    /// outstanding when the QP transitioned into the Error State.
    #[error("WR flush error")]
    WrFlushErr = ibv_wc_status::IBV_WC_WR_FLUSH_ERR as _,

    /// **Memory Window Binding Error:** a failure happened when trying to bind
    /// a memory window to a MR.
    #[error("memory window bind error")]
    MwBindErr = ibv_wc_status::IBV_WC_MW_BIND_ERR as _,

    /// **Bad Response Error:** an unexpected transport layer opcode was returned
    /// by the responder. *Relevant for RC QPs.*
    #[error("bad response error")]
    BadRespErr = ibv_wc_status::IBV_WC_BAD_RESP_ERR as _,

    /// **Local Access Error:** a protection error occurred on a local data buffer
    /// during the processing of a RDMA Write with Immediate operation sent from
    /// the remote node. *Relevant for RC QPs.*
    #[error("local access error")]
    LocAccessErr = ibv_wc_status::IBV_WC_LOC_ACCESS_ERR as _,

    /// **Remote Invalid Request Error:** the responder detected an invalid message
    /// on the channel. Possible causes include the operation is not supported by
    /// this receive queue (qp_access_flags in remote QP wasn't configured to
    /// support this operation), insufficient buffering to receive a new RDMA or
    /// Atomic Operation request, or the length specified in a RDMA request is
    /// greater than 2^31 bytes. *Relevant for RC QPs.*
    #[error("remote invalid request error")]
    RemInvReqErr = ibv_wc_status::IBV_WC_REM_INV_REQ_ERR as _,

    /// **Remote Access Error:** a protection error occurred on a remote data
    /// buffer to be read by an RDMA Read, written by an RDMA Write or accessed
    /// by an atomic operation. This error is reported only on RDMA operations
    /// or atomic operations. *Relevant for RC QPs.*
    #[error("remote access error")]
    RemAccessErr = ibv_wc_status::IBV_WC_REM_ACCESS_ERR as _,

    /// **Remote Operation Error:** the operation could not be completed
    /// successfully by the responder. Possible causes include a responder QP
    /// related error that prevented the responder from completing the request
    /// or a malformed WQE on the Receive Queue. *Relevant for RC QPs.*
    #[error("remote operation error")]
    RemOpErr = ibv_wc_status::IBV_WC_REM_OP_ERR as _,

    /// **Transport Retry Counter Exceeded:** the local transport timeout retry
    /// counter was exceeded while trying to send this message. This means that
    /// the remote side didn't send any Ack or Nack.
    /// - If this happens when sending the first message, usually this mean that
    ///   the connection attributes are wrong or the remote side isn't in a state
    ///   that it can respond to messages.
    /// - If this happens after sending the first message, usually it means that
    /// the remote QP isn't available anymore.
    ///
    /// *Relevant for RC QPs.*
    #[error("transport retry counter exceeded")]
    RetryExcErr = ibv_wc_status::IBV_WC_RETRY_EXC_ERR as _,

    /// **RNR Retry Counter Exceeded:** the RNR NAK retry count was exceeded.
    /// This usually means that the remote side didn't post any WR to its Receive
    /// Queue. *Relevant for RC QPs.*
    #[error("RNR retry counter exceeded")]
    RnrRetryExcErr = ibv_wc_status::IBV_WC_RNR_RETRY_EXC_ERR as _,

    /// **Local RDD Violation Error:** the RDD associated with the QP does not
    /// match the RDD associated with the EE Context (**unused**, since it is
    /// relevant only to RD QPs or EE Context, which aren't supported).
    #[error("local RDD violation error")]
    LocRddViolErr = ibv_wc_status::IBV_WC_LOC_RDD_VIOL_ERR as _,

    /// **Remote Invalid RD Request Error:** the responder detected an invalid
    /// incoming RD message. Causes include a Q_Key or RDD violation (**unused**,
    /// since it is relevant only to RD QPs or EE Context, which aren't supported).
    #[error("remote invalid RD request")]
    RemInvRdReqErr = ibv_wc_status::IBV_WC_REM_INV_RD_REQ_ERR as _,

    /// **Remote Aborted Error:** for UD or UC QPs associated with a SRQ, the
    /// responder aborted the operation.
    #[error("remote aborted error")]
    RemAbortErr = ibv_wc_status::IBV_WC_REM_ABORT_ERR as _,

    /// **Invalid EE Context Number:** an invalid EE Context number was detected
    /// (**unused**, since it is relevant only to RD QPs or EE Context, which
    /// aren't supported).
    #[error("invalid EE context number")]
    InvEecnErr = ibv_wc_status::IBV_WC_INV_EECN_ERR as _,

    /// **Invalid EE Context State Error:** operation is not legal for the
    /// specified EE Context state (**unused**, since it is relevant only to RD
    /// QPs or EE Context, which aren't supported).
    #[error("invalid EE context state error")]
    InvEecStateErr = ibv_wc_status::IBV_WC_INV_EEC_STATE_ERR as _,

    /// **Fatal error:** a fatal error that may not be recoverable.
    #[error("fatal error")]
    FatalErr = ibv_wc_status::IBV_WC_FATAL_ERR as _,

    /// **Response Timeout Error:** a response timed out.
    #[error("response timeout error")]
    RespTimeoutErr = ibv_wc_status::IBV_WC_RESP_TIMEOUT_ERR as _,

    /// **Tag Matching Error:** a failure occurred when trying to issue an
    /// `ibv_post_srq_ops` with opcode `IBV_WR_TAG_DEL` to remove a previously
    /// added tag entry, due to concurrent tag consumption.
    #[error("tag matching error")]
    TmErr = ibv_wc_status::IBV_WC_TM_ERR as _,

    /// **Rendezvous Request Tagged Buffer Insufficient:** this is due to
    /// a posted tagged buffer is insufficient to hold the data of a
    /// rendezvous request.
    #[error("rendezvous request tagged buffer insufficient")]
    TmRndvIncomplete = ibv_wc_status::IBV_WC_TM_RNDV_INCOMPLETE as _,

    /// **General Error:** other error which isn't one of the above errors.
    #[error("general error")]
    GeneralErr = ibv_wc_status::IBV_WC_GENERAL_ERR as _,
}

/// For better performance, the cast from `u32` to `WcStatus` is implemented as
/// a direct `mem::transmute` for valid values from 0 to 23 instead of a verbose
/// `match` statement for each possibility.
impl From<u32> for WcStatus {
    fn from(wc_status: u32) -> Self {
        match wc_status {
            x if x <= ibv_wc_status::IBV_WC_TM_RNDV_INCOMPLETE => unsafe { std::mem::transmute(x) },
            x => panic!("invalid wc status: {}", x),
        }
    }
}

/// Work completion entry.
///
/// This structure transparently wraps an `ibv_wc` structure, representing
/// an entry polled from the completion queue.
#[repr(transparent)]
pub struct Wc(ibv_wc);

/// Work completions are [trivially copyable](https://en.cppreference.com/w/cpp/named_req/TriviallyCopyable)
/// and is thus safe to send to another thread.
unsafe impl Send for Wc {}
/// Work completions are [trivially copyable](https://en.cppreference.com/w/cpp/named_req/TriviallyCopyable)
/// and is thus safe to share between threads.
unsafe impl Sync for Wc {}

impl Wc {
    /// Get the work request ID.
    #[inline]
    pub fn wr_id(&self) -> u64 {
        self.0.wr_id
    }

    /// Get the completion status.
    #[inline]
    pub fn status(&self) -> WcStatus {
        WcStatus::from(self.0.status)
    }

    /// Get the completion status as a `Result`.
    ///
    /// - If the status is `IBV_WC_SUCCESS`, return the number of bytes processed or transferred.
    /// - Otherwise, return an error.
    #[inline]
    pub fn result(&self) -> Result<usize> {
        if self.status() == WcStatus::Success {
            Ok(self.0.byte_len as usize)
        } else {
            Err(self.status().into())
        }
    }

    /// Get the opcode of the work request.
    #[inline]
    pub fn opcode(&self) -> WcOpcode {
        WcOpcode::from(self.0.opcode)
    }

    /// Get the number of bytes processed or transferred.
    #[inline]
    pub fn bytes(&self) -> usize {
        self.0.byte_len as usize
    }

    /// Get the immediate data.
    #[inline]
    pub fn imm(&self) -> Option<u32> {
        if (self.0.wc_flags & ibv_wc_flags::IBV_WC_WITH_IMM.0) != 0 {
            unsafe { self.0.imm_data_invalidated_rkey_union.imm_data as u32 }.into()
        } else {
            None
        }
    }

    /// Get the immediate data, without checking the validity.
    #[inline]
    pub unsafe fn imm_unchecked(&self) -> u32 {
        self.0.imm_data_invalidated_rkey_union.imm_data as u32
    }
}

impl Default for Wc {
    /// Create a zeroed work completion entry.
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

impl fmt::Debug for Wc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Wc")
            .field("wr_id", &self.wr_id())
            .field("status", &self.status())
            .finish()
    }
}

impl Clone for Wc {
    fn clone(&self) -> Self {
        unsafe {
            let mut wc = mem::zeroed();
            ptr::copy_nonoverlapping(&self.0, &mut wc, 1);
            Wc(wc)
        }
    }
}

struct CqInner {
    ctx: Context,
    cq: NonNull<ibv_cq>,
}

unsafe impl Send for CqInner {}
unsafe impl Sync for CqInner {}

impl fmt::Debug for CqInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Context")
            .field("ctx", &self.ctx)
            .field("cq", &self.cq)
            .finish()
    }
}

impl Drop for CqInner {
    fn drop(&mut self) {
        unsafe { ibv_destroy_cq(self.cq.as_ptr()) };
    }
}

/// Completion queue.
///
/// This type is a simple wrapper of an `Arc` and is guaranteed to have the
/// same memory layout with it.
///
/// The underlying type behind the `Arc` owns a completion queue (`ibv_cq`)
/// and holds a reference to the device context of the queue to ensure the
/// context will not get dropped too early.
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct Cq {
    inner: Arc<CqInner>,
}

impl Cq {
    pub fn new(ctx: Context, size: Option<i32>) -> Result<Self> {
        const DEFAULT_CQ_SIZE: i32 = 128;
        let cq = NonNull::new(unsafe {
            ibv_create_cq(
                ctx.as_ptr(),
                size.unwrap_or(DEFAULT_CQ_SIZE),
                ptr::null_mut(),
                ptr::null_mut(),
                0,
            )
        })
        .ok_or_else(|| anyhow::anyhow!(io::Error::last_os_error()))?;

        Ok(Self {
            inner: Arc::new(CqInner { ctx, cq }),
        })
    }

    /// Get the underlying `ibv_cq` pointer.
    pub fn as_ptr(&self) -> *mut ibv_cq {
        self.inner.cq.as_ptr()
    }

    /// Get the underlying `Context`.
    pub fn context(&self) -> Context {
        self.inner.ctx.clone()
    }

    /// Non-blocking poll.
    ///
    /// Return the number of work completions polled.
    /// It is possible that the number of polled work completions is less than `wc.len()` or even zero.
    ///
    /// **NOTE:** The validity of work completions beyond the number of polled work completions is not guaranteed.
    /// For valid completions, it is not guaranteed that they are all success.
    /// It is the caller's responsibility to check the status of each work completion.
    pub fn poll(&self, wc: &mut [Wc]) -> Result<i32> {
        let num = unsafe {
            ibv_poll_cq(
                self.inner.cq.as_ptr(),
                wc.len() as i32,
                wc.as_mut_ptr().cast(),
            )
        };

        if num < 0 {
            Err(anyhow::anyhow!(io::Error::last_os_error()))
        } else {
            Ok(num)
        }
    }

    /// Blocking poll until `wc.len()` work completions are polled.
    ///
    /// **NOTE:** The validity of work completions beyond the number of polled work completions is not guaranteed.
    /// For valid completions, it is not guaranteed that they are all success.
    /// It is the caller's responsibility to check the status of each work completion.
    pub fn poll_blocking(&self, wc: &mut [Wc]) -> Result<()> {
        let num = wc.len();
        let mut polled = 0;
        while polled < num {
            let n = self.poll(&mut wc[polled..])?;
            if n == 0 {
                continue;
            }
            polled += n as usize;
        }
        Ok(())
    }

    /// Poll a specified number of work completions but consume them.
    ///
    /// Return the number of work completions polled.
    /// It is possible that the number of polled work completions is less than `num` or even zero.
    ///
    /// This method checks the status of each polled work completion and returns an error if any of them is not success.
    pub fn poll_nocqe(&self, num: usize) -> Result<i32> {
        let mut wc = vec![Wc::default(); num];
        let ret = self.poll(&mut wc)?;

        for i in 0..ret {
            wc[i as usize].result()?;
        }
        Ok(ret)
    }

    /// Blocking poll a specified number of work completions but consume them.
    ///
    /// This method checks the status of each polled work completion and returns an error if any of them is not success.
    pub fn poll_nocqe_blocking(&self, num: usize) -> Result<()> {
        let mut wc = vec![Wc::default(); num];
        self.poll_blocking(&mut wc)?;
        for i in 0..num {
            wc[i].result()?;
        }
        Ok(())
    }
}
