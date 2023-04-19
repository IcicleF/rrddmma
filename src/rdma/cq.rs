use std::ptr::NonNull;
use std::{fmt, io, mem, ptr};

use super::context::Context;

use anyhow::Result;
use rdma_sys::*;

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
    pub fn status(&self) -> u32 {
        self.0.status
    }

    /// Get the completion status as a `Result`.
    ///
    /// - If the status is `IBV_WC_SUCCESS`, return the number of bytes processed or transferred.
    /// - Otherwise, return an error.
    #[inline]
    pub fn result(&self) -> Result<usize> {
        if self.status() == ibv_wc_status::IBV_WC_SUCCESS {
            Ok(self.0.byte_len as usize)
        } else {
            Err(anyhow::anyhow!(self.0.status))
        }
    }

    /// Get the opcode of the work request.
    #[inline]
    pub fn opcode(&self) -> u32 {
        self.0.opcode
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
