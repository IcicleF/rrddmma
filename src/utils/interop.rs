use anyhow::Result;
use std::io;

use super::select::*;

/// Converts a C return value to a Rust `Result`.
#[inline(always)]
pub(crate) fn from_c_ret(ret: i32) -> Result<()> {
    (ret == 0).select(
        || Ok(()),
        || Err(anyhow::anyhow!(io::Error::from_raw_os_error(ret))),
    )
}

/// Converts a non-zero C return value to a Rust `Result`.
#[inline(always)]
pub(crate) fn from_c_err<T>(code: i32) -> Result<T> {
    Err(anyhow::anyhow!(io::Error::from_raw_os_error(code)))
}
