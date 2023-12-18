//! Provide interoperability with C return values.

use std::io::{self, Error as IoError};

/// Converts a C return value to a Rust `Result`.
#[inline(always)]
pub(crate) fn from_c_ret(ret: i32) -> io::Result<()> {
    match ret {
        0 => Ok(()),
        _ => from_c_err(ret),
    }
}

/// Converts a C return value to a Rust `Result`, with optional customized error message.
#[inline(always)]
pub(crate) fn from_c_ret_explained(
    ret: i32,
    f: impl FnOnce(i32) -> Option<&'static str>,
) -> io::Result<()> {
    if ret == 0 {
        Ok(())
    } else {
        let msg = f(ret);
        match msg {
            Some(msg) => Err(IoError::new(
                IoError::from_raw_os_error(ret).kind(),
                msg.to_owned(),
            )),
            None => from_c_err(ret),
        }
    }
}

/// Converts a non-zero C return value to a Rust `Result`.
#[inline(always)]
pub(crate) fn from_c_err<T>(code: i32) -> io::Result<T> {
    Err(IoError::from_raw_os_error(code))
}
