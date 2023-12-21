#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(deref_nullptr)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::too_many_arguments)]
#![allow(unused)]

mod common;

#[cfg(mlnx4)]
mod mlnx4;

#[cfg(mlnx5)]
mod mlnx5;

mod private {
    use libc::*;
    include!(concat!(env!("OUT_DIR"), "/verbs_bindings.rs"));

    #[cfg(mlnx4)]
    pub use super::mlnx4::*;

    #[cfg(mlnx5)]
    pub use super::mlnx5::*;
}

pub(crate) use self::private::*;

#[cfg(not(manual_mlx5))]
mod __ibv_get_device_list_mod {
    use super::ibv_device;

    extern "C" {
        /// Get list of IB devices currently available.
        /// Returns a NULL-terminated array of IB devices.
        /// The array can be released with `ibv_free_device_list()``.
        ///
        /// # Arguments
        ///
        /// * `num_devices` - Optional. If non-NULL, set to the number of devices.
        pub(crate) fn ibv_get_device_list(
            num_devices: *mut ::std::os::raw::c_int,
        ) -> *mut *mut ibv_device;
    }
}

#[cfg(manual_mlx5)]
mod __ibv_get_device_list_mod {
    use super::ibv_device;

    #[repr(C)]
    struct verbs_device_ops {
        _private: [u8; 0],
    }

    extern "C" {
        #[no_mangle]
        static mut verbs_provider_mlx5: verbs_device_ops;
    }

    mod external {
        extern "C" {
            pub(super) fn ibv_get_device_list(
                num_devices: *mut ::std::os::raw::c_int,
            ) -> *mut *mut super::ibv_device;
        }

        extern "C" {
            pub(super) fn ibv_static_providers(unused: *mut ::std::os::raw::c_void, ...);
        }
    }

    /// Get list of IB devices currently available.
    /// Returns a NULL-terminated array of IB devices.
    /// The array can be released with `ibv_free_device_list()``.
    ///
    /// # Arguments
    ///
    /// * `num_devices` - Optional. If non-NULL, set to the number of devices.
    pub(crate) unsafe fn ibv_get_device_list(
        num_devices: *mut ::std::os::raw::c_int,
    ) -> *mut *mut ibv_device {
        self::external::ibv_static_providers(
            std::ptr::null_mut(),
            &mut verbs_provider_mlx5 as *mut verbs_device_ops,
            0,
        );
        self::external::ibv_get_device_list(num_devices)
    }
}

pub(crate) use self::__ibv_get_device_list_mod::ibv_get_device_list;
