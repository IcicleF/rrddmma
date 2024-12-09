#![cfg(mlnx4)]

use super::*;
use std::{io, mem};

/// Experimental work completion entry.
#[derive(Clone, Copy, Default)]
#[repr(transparent)]
pub struct ExpWc(pub ibv_exp_wc);

impl ExpWc {
    /// Read the timestamp of the work completion.
    pub fn timestamp(&self) -> Option<u64> {
        (self.0.exp_wc_flags & IBV_EXP_WC_WITH_TIMESTAMP != 0).then(|| self.0.timestamp)
    }
}

/// Experimental completion queue.
pub trait ExpCq {
    /// Non-blockingly poll into the given buffer. Return the number of work
    /// completions polled.
    ///
    /// It is the caller's responsibility to check the status codes of the
    /// returned work completion entries.
    ///
    /// **NOTE:** It is possible that the number of polled work completions is
    /// less than `wc.len()` or even zero. The validity of work completions
    /// beyond the number of polled work completions is not guaranteed.
    fn exp_poll_into(&self, wc: &mut [ExpWc]) -> io::Result<u32>;
}

impl ExpCq for Cq {
    fn exp_poll_into(&self, wc: &mut [ExpWc]) -> io::Result<u32> {
        if wc.is_empty() {
            return Ok(0);
        }

        // SAFETY: FFI, and that `Wc` is transparent over `ibv_wc`.
        let num = unsafe {
            ibv_exp_poll_cq(
                self.as_raw(),
                wc.len() as i32,
                wc.as_mut_ptr().cast(),
                mem::size_of::<ExpWc>() as _,
            )
        };
        if num >= 0 {
            for i in 0..num as usize {
                if wc[i].0.exp_wc_flags & IBV_EXP_WC_WITH_TIMESTAMP != 0 {
                    // SAFETY: FFI.
                    wc[i].0.timestamp = unsafe {
                        ibv_exp_cqe_ts_to_ns(self.context().clock_info(), wc[i].0.timestamp)
                    };
                }
            }
            Ok(num as u32)
        } else {
            Err(io::Error::from_raw_os_error(num))
        }
    }
}
