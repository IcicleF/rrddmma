use anyhow::Result;
use std::io;

/// Implements a `select` method for primitive types that consumes two inputs
/// and returns one of them based on the value of the selector.
// Carbon language seems good at expressing this kind of thing :)
// so let's just borrow this from it!
mod select {
    pub trait Select {
        /// Selects one of two values based on the value of the selector.
        fn select<T>(self, a: T, b: T) -> T;
    }

    impl Select for bool {
        #[inline]
        fn select<T>(self, a: T, b: T) -> T {
            if self {
                a
            } else {
                b
            }
        }
    }

    macro_rules! impl_select_for_int {
        ($($t:ty)*) => ($(
            impl Select for $t {
                #[inline]
                fn select<T>(self, a: T, b: T) -> T {
                    if self != 0 {
                        a
                    } else {
                        b
                    }
                }
            }
        )*)
    }
    impl_select_for_int!(i8 i16 i32 i64 isize u8 u16 u32 u64 usize);
}
pub use select::*;

/// Converts a C return value to a Rust `Result`.
pub(crate) fn from_c_ret(ret: i32) -> Result<()> {
    (ret == 0).select(
        Ok(()),
        Err(anyhow::anyhow!(io::Error::from_raw_os_error(ret))),
    )
}

/// Converts a non-zero C return value to a Rust `Result`.
pub(crate) fn from_c_err<T>(code: i32) -> Result<T> {
    Err(anyhow::anyhow!(io::Error::from_raw_os_error(code)))
}
