use std::ptr::NonNull;
use std::{fmt, io, mem, ptr};

use super::context::Context;

use anyhow::Result;
use rdma_sys::*;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CqeOpcode {
    Send = 0,
    RdmaWrite = 1,
    RdmaRead = 2,
    CompSwap = 3,
    FetchAdd = 4,
    BindMw = 5,
    Recv = 128,
    RecvRdmaImm = 129,
}

impl From<u32> for CqeOpcode {
    fn from(n: u32) -> Self {
        match n {
            0 => CqeOpcode::Send,
            1 => CqeOpcode::RdmaWrite,
            2 => CqeOpcode::RdmaRead,
            3 => CqeOpcode::CompSwap,
            4 => CqeOpcode::FetchAdd,
            5 => CqeOpcode::BindMw,
            128 => CqeOpcode::Recv,
            129 => CqeOpcode::RecvRdmaImm,
            _ => panic!("invalid opcode: {}", n),
        }
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CqeStatus {
    #[error("success")]
    Success = 0,

    #[error("local length error")]
    LocLenErr = 1,

    #[error("local QP operation error")]
    LocQpOpErr = 2,

    #[error("local EE context operation error")]
    LocEecOpErr = 3,

    #[error("local protection error")]
    LocProtErr = 4,

    #[error("WR flush error")]
    WrFlushErr = 5,

    #[error("memory window bind error")]
    MwBindErr = 6,

    #[error("bad response error")]
    BadRespErr = 7,

    #[error("local access error")]
    LocAccessErr = 8,

    #[error("remote invalid request error")]
    RemInvReqErr = 9,

    #[error("remote access error")]
    RemAccessErr = 10,

    #[error("remote operation error")]
    RemOpErr = 11,

    #[error("transport retry counter exceeded")]
    RetryExcErr = 12,

    #[error("RNR retry counter exceeded")]
    RnrRetryExcErr = 13,

    #[error("local RDD violation error")]
    LocRddViolErr = 14,

    #[error("remote invalid RD request")]
    RemInvRdReqErr = 15,

    #[error("remote aborted error")]
    RemAbortErr = 16,

    #[error("invalid EE context number")]
    InvEecnErr = 17,

    #[error("invalid EE context state error")]
    InvEecStateErr = 18,

    #[error("fatal error")]
    FatalErr = 19,

    #[error("response timeout error")]
    RespTimeoutErr = 20,

    #[error("general error")]
    GeneralErr = 21,
}

impl From<u32> for CqeStatus {
    fn from(n: u32) -> Self {
        match n {
            0 => CqeStatus::Success,
            1 => CqeStatus::LocLenErr,
            2 => CqeStatus::LocQpOpErr,
            3 => CqeStatus::LocEecOpErr,
            4 => CqeStatus::LocProtErr,
            5 => CqeStatus::WrFlushErr,
            6 => CqeStatus::MwBindErr,
            7 => CqeStatus::BadRespErr,
            8 => CqeStatus::LocAccessErr,
            9 => CqeStatus::RemInvReqErr,
            10 => CqeStatus::RemAccessErr,
            11 => CqeStatus::RemOpErr,
            12 => CqeStatus::RetryExcErr,
            13 => CqeStatus::RnrRetryExcErr,
            14 => CqeStatus::LocRddViolErr,
            15 => CqeStatus::RemInvRdReqErr,
            16 => CqeStatus::RemAbortErr,
            17 => CqeStatus::InvEecnErr,
            18 => CqeStatus::InvEecStateErr,
            19 => CqeStatus::FatalErr,
            20 => CqeStatus::RespTimeoutErr,
            21 => CqeStatus::GeneralErr,
            _ => panic!("invalid status: {}", n),
        }
    }
}

/// Work completion entry.
///
/// This structure transparently wraps a `ibv_wc` structure and thus represents an entry in the completion queue.
///
/// Work completions are [trivially copyable](https://en.cppreference.com/w/cpp/named_req/TriviallyCopyable).
/// Therefore, this structure is `Send` and `Sync`, and can be safely cloned.
#[repr(transparent)]
pub struct Wc(ibv_wc);

unsafe impl Send for Wc {}
unsafe impl Sync for Wc {}

impl Wc {
    /// Get the work request ID.
    #[inline]
    pub fn wr_id(&self) -> u64 {
        self.0.wr_id
    }

    /// Get the completion status.
    #[inline]
    pub fn status(&self) -> CqeStatus {
        CqeStatus::from(self.0.status)
    }

    /// Get the completion status as a `Result`.
    ///
    /// - If the status is `IBV_WC_SUCCESS`, return the number of bytes processed or transferred.
    /// - Otherwise, return an error.
    #[inline]
    pub fn result(&self) -> Result<usize> {
        if self.status() == CqeStatus::Success {
            Ok(self.0.byte_len as usize)
        } else {
            Err(self.status().into())
        }
    }

    /// Get the opcode of the work request.
    #[inline]
    pub fn opcode(&self) -> CqeOpcode {
        CqeOpcode::from(self.0.opcode)
    }

    /// Get the number of bytes processed or transferred.
    #[inline]
    pub fn bytes(&self) -> usize {
        self.0.byte_len as usize
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

/// Completion queue.
///
/// This structure owns a completion queue (`ibv_cq`) and holds a reference to the device context of the queue.
/// It is responsible of destroying the completion queue when dropped.
///
/// The underlying device context must live longer than the completion queue.
///
/// Because the inner `ibv_cq` is owned by this structure, `Cq` is `!Send` and cannot be cloned.
/// However, although `Cq` is `Sync` because thread-safety is guaranteed by the ibverbs userspace driver, it is
/// still unrecommended to use the same `Cq` in multiple threads for performance reasons.
#[derive(Debug)]
pub struct Cq<'a> {
    ctx: &'a Context,
    cq: NonNull<ibv_cq>,
}

unsafe impl<'a> Sync for Cq<'a> {}

impl<'a> Cq<'a> {
    pub fn new(ctx: &'a Context, size: Option<i32>) -> Result<Self> {
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

        Ok(Self { ctx, cq })
    }

    /// Get the underlying `ibv_cq` pointer.
    pub fn as_ptr(&self) -> *mut ibv_cq {
        self.cq.as_ptr()
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
        let num = unsafe { ibv_poll_cq(self.cq.as_ptr(), wc.len() as i32, wc.as_mut_ptr().cast()) };

        if num < 0 {
            Err(anyhow::anyhow!(io::Error::last_os_error()))
        } else {
            Ok(num)
        }
    }

    /// Blocking poll.
    ///
    /// Block until `wc.len()` work completions are polled.
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

impl<'a> Drop for Cq<'a> {
    fn drop(&mut self) {
        unsafe { ibv_destroy_cq(self.cq.as_ptr()) };
    }
}
