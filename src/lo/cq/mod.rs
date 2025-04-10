//! Completion queue and Work completion.

mod exp_wc;
mod wc;

use std::io::{self, Error as IoError};
use std::mem::MaybeUninit;
use std::ptr::{self, NonNull};
use std::sync::Arc;
use std::{fmt, hint};

#[cfg(feature = "legacy")]
pub use self::exp::*;
pub use self::wc::*;
use super::context::Context;
use crate::bindings::*;
use crate::utils::interop::from_c_ret;

/// Wrapper for `*mut ibv_cq`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub(crate) struct IbvCq(Option<NonNull<ibv_cq>>);

impl IbvCq {
    /// Destroy the CQ.
    ///
    /// # Safety
    ///
    /// - A CQ must not be destroyed more than once.
    /// - Destroyed CQs must not be used anymore.
    pub unsafe fn destroy(self) -> io::Result<()> {
        // SAFETY: FFI.
        let ret = ibv_destroy_cq(self.as_ptr());
        from_c_ret(ret)
    }
}

impl_ibv_wrapper_traits!(ibv_cq, IbvCq);

/// Ownership holder of completion queue.
struct CqInner {
    ctx: Context,
    cq: IbvCq,
}

impl Drop for CqInner {
    fn drop(&mut self) {
        // SAFETY: call only once, and no UAF since I will be dropped.
        unsafe { self.cq.destroy() }.expect("cannot destroy CQ on drop");
    }
}

/// Completion queue.
#[derive(Clone)]
pub struct Cq {
    /// Cached CQ pointer.
    cq: IbvCq,

    /// CQ body.
    inner: Arc<CqInner>,
}

impl fmt::Debug for Cq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("Cq<{:p}>", self.as_raw()))
    }
}

impl Cq {
    /// The default CQ depth.
    pub const DEFAULT_CQ_DEPTH: u32 = 128;

    /// Create a new completion queue.
    pub fn new(ctx: &Context, capacity: u32) -> io::Result<Self> {
        let max_capacity = ctx.attr().max_cqe as u32;
        if capacity > max_capacity {
            return Err(IoError::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "CQ capacity {} too large (maximum: {})",
                    capacity, max_capacity
                ),
            ));
        }

        // SAFETY: FFI.
        let cq = unsafe {
            ibv_create_cq(
                ctx.as_raw(),
                capacity as i32,
                ptr::null_mut(),
                ptr::null_mut(),
                0,
            )
        };
        let cq = NonNull::new(cq).ok_or_else(IoError::last_os_error)?;
        let cq = IbvCq::from(cq);

        Ok(Self {
            cq,
            inner: Arc::new(CqInner {
                ctx: ctx.clone(),
                cq,
            }),
        })
    }

    /// Get the underlying [`ibv_cq`] pointer.
    pub fn as_raw(&self) -> *mut ibv_cq {
        self.cq.as_ptr()
    }

    /// Get the underlying [`Context`].
    pub fn context(&self) -> &Context {
        &self.inner.ctx
    }

    /// Get the capacity of the completion queue.
    pub fn capacity(&self) -> usize {
        // SAFETY: the pointer is valid as long as the `Cq` is alive.
        (unsafe { (*self.cq.as_ptr()).cqe }) as _
    }

    /// Non-blockingly poll the completion queue for up to `num` work completions.
    /// Results will be filled in contiguous memory starting at `wc`.
    ///
    /// # Safety
    ///
    /// - `wc` must be a valid pointer to a buffer that can hold at least `num` work completions.
    pub unsafe fn poll(&self, wc: *mut Wc, num: usize) -> io::Result<usize> {
        // SAFETY: FFI, and that `Wc` is transparent over `ibv_wc`.
        let num = unsafe { ibv_poll_cq(self.as_raw(), num as i32, wc.cast()) };
        if num >= 0 {
            Ok(num as usize)
        } else {
            Err(io::Error::from_raw_os_error(num))
        }
    }

    /// Non-blockingly poll for as many as possible existing CQ entries.
    /// Return the work completions polled.
    ///
    /// It is the caller's responsibility to check the status codes of the
    /// returned work completion entries.
    pub fn poll_all(&self) -> io::Result<Vec<Wc>> {
        self.poll_some(self.capacity(), false)
    }

    /// Poll for work completions with an upper bound. Return the work completions polled.
    /// If `blocking` is set to `true`, this function will block until there are exactly
    /// `num` work completions available.
    ///
    /// It is the caller's responsibility to check the status codes of the
    /// returned work completion entries.
    pub fn poll_some(&self, num: usize, blocking: bool) -> io::Result<Vec<Wc>> {
        if num == 0 {
            return Ok(Vec::new());
        }

        let mut wc = <Vec<Wc>>::with_capacity(num);
        let n = self.poll_into(wc.spare_capacity_mut(), blocking)?;
        // SAFETY: `self.poll_into` guarantees that the returned number of elements
        // are initialized.
        unsafe { wc.set_len(n) };
        Ok(wc)
    }

    /// Poll one work completion. Return the work completion polled.
    /// If `blocking` is set to `true`, this function will block until there is a
    /// work completion available.
    ///
    /// It is the caller's responsibility to check the status codes of the
    /// returned work completion entry.
    pub fn poll_one(&self, blocking: bool) -> io::Result<Option<Wc>> {
        let mut wc = MaybeUninit::<Wc>::uninit();
        loop {
            // SAFETY: `wc.as_mut_ptr()` is a valid pointer to a buffer of one `Wc` object.
            let num = unsafe { self.poll(wc.as_mut_ptr(), 1) }?;
            if !blocking || num != 0 {
                // SAFETY: `wc` is initialized if `num != 0`.
                return Ok((num != 0).then(|| unsafe { wc.assume_init() }));
            }
            hint::spin_loop();
        }
    }

    /// Poll for work completions into the given buffer. Return the number of work
    /// completions polled.
    /// If `blocking` is set to `true`, this function will block until there are exactly
    /// `num` work completions available.
    ///
    /// It is the caller's responsibility to check the status codes of the
    /// returned work completion entries.
    pub fn poll_into(&self, wc: &mut [MaybeUninit<Wc>], blocking: bool) -> io::Result<usize> {
        if wc.is_empty() {
            return Ok(0);
        }

        let mut pwc = wc.as_mut_ptr() as *mut Wc;
        let mut n = 0;

        loop {
            // SAFETY: `pwc` points to a valid buffer of `num - n` remaining unfilled elements.
            n += unsafe { self.poll(pwc, wc.len() - n) }?;
            pwc = unsafe { wc.as_mut_ptr().add(n) as _ };

            if !blocking || n == wc.len() {
                return Ok(n);
            }
            hint::spin_loop();
        }
    }

    /// Poll one work completion into the given buffer. Return whether a work completion
    /// was polled.
    /// If `blocking` is set to `true`, this function will block until there are exactly
    /// `num` work completions available.
    ///
    /// It is the caller's responsibility to check the status codes of the
    /// returned work completion entry.
    pub fn poll_one_into(&self, wc: &mut MaybeUninit<Wc>, blocking: bool) -> io::Result<bool> {
        loop {
            // SAFETY: `wc` points to a valid buffer of one `Wc` object.
            let num = unsafe { self.poll(wc.as_mut_ptr(), 1) }?;
            if !blocking || num != 0 {
                return Ok(num != 0);
            }
            hint::spin_loop();
        }
    }

    /// Poll one work completion blockingly and return it only when successful.
    ///
    /// # Panics
    ///
    /// - If the poll fails or the completion indicates an error, this function will panic with the error message.
    pub fn poll_one_blockingly_consumed(&self) -> Wc {
        match self.poll_one(true) {
            Ok(Some(wc)) => {
                wc.ok().unwrap();
                wc
            }
            Err(e) => panic!("poll_one_blockingly_consumed: {}", e),
            _ => unreachable!("bug: a blocking poll returns no completion"),
        }
    }
}
