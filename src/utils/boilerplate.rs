#![macro_use]

macro_rules! impl_ibv_wrapper_traits {
    ($ibv_ty:ty, $wrapper_ty:ty) => {
        impl ::std::ops::Deref for $wrapper_ty {
            type Target = ::std::ptr::NonNull<$ibv_ty>;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl From<::std::ptr::NonNull<$ibv_ty>> for $wrapper_ty {
            fn from(pointer: ::std::ptr::NonNull<$ibv_ty>) -> Self {
                Self(pointer)
            }
        }

        unsafe impl Send for $wrapper_ty {}
        unsafe impl Sync for $wrapper_ty {}
    };
}
