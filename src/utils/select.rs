/// Implements a `select` method for primitive types that consumes two inputs
/// and returns one of them based on the value of the selector.
// Carbon language seems good at expressing this kind of thing :)
// so let's just borrow this from it!
pub(crate) trait Select {
    /// Selects one of two values based on the value of the selector.
    fn select<T>(&self, a: T, b: T) -> T;
}

impl Select for bool {
    #[inline(always)]
    fn select<T>(&self, a: T, b: T) -> T {
        if *self {
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
            fn select<T>(&self, a: T, b: T) -> T {
                if *self != 0 {
                    a
                } else {
                    b
                }
            }
        }
    )*)
}

impl_select_for_int!(i8 i16 i32 i64 isize u8 u16 u32 u64 usize);

impl<T> Select for Option<T> {
    #[inline(always)]
    fn select<U>(&self, a: U, b: U) -> U {
        if self.is_some() {
            a
        } else {
            b
        }
    }
}

impl<T, E> Select for Result<T, E> {
    #[inline(always)]
    fn select<U>(&self, a: U, b: U) -> U {
        if self.is_ok() {
            a
        } else {
            b
        }
    }
}
