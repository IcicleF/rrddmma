//! Bindings of libibverbs C interfaces.

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
        ///
        /// Returns a NULL-terminated array of IB devices.
        /// The array can be released with `ibv_free_device_list()`.
        pub fn ibv_get_device_list(num_devices: *mut ::std::os::raw::c_int)
            -> *mut *mut ibv_device;
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
    ///
    /// Returns a NULL-terminated array of IB devices.
    /// The array can be released with `ibv_free_device_list()`.
    pub unsafe fn ibv_get_device_list(
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

pub use self::__ibv_get_device_list_mod::ibv_get_device_list;

/// Free the list of ibv_device structs provided by [`ibv_get_device_list`].
///
/// Any desired devices should be opened prior to calling this command.
/// Once the list is freed, all [`ibv_device`] structs that were on the list
/// become invalid and can no longer be used.
pub use self::private::ibv_free_device_list;

/// Get the device name.
pub use self::private::ibv_get_device_name;

/// RDMA device information.
pub use self::private::ibv_device;

/// Open the device and create a context for further use.
pub use self::private::ibv_open_device;

/// Close the device context.
pub use self::private::ibv_close_device;

/// RDMA device context.
pub use self::private::ibv_context;

/// Query the device attributes.
pub use self::private::ibv_query_device;

/// Device attributes.
pub use self::private::ibv_device_attr;

/// Query the port attributes.
pub use self::private::ibv_query_port;

/// Port attributes.
pub use self::private::ibv_port_attr;

/// Allocate a protection domain.
pub use self::private::ibv_alloc_pd;

/// Deallocate a protection domain.
pub use self::private::ibv_dealloc_pd;

/// Protection domain.
pub use self::private::ibv_pd;

/// Create a completion queue.
pub use self::private::ibv_create_cq;

/// Destroy a completion queue.
pub use self::private::ibv_destroy_cq;

/// Completion queue.
pub use self::private::ibv_cq;

/// Poll for work completions.
pub use self::private::ibv_poll_cq;

/// Work completion.
pub use self::private::ibv_wc;

/// Register a memory region.
pub use self::private::ibv_reg_mr;

/// Deregister a memory region.
pub use self::private::ibv_dereg_mr;

/// Memory region.
pub use self::private::ibv_mr;

/// Memory region permissions.
pub use self::private::ibv_access_flags;

/// Create a queue pair.
pub use self::private::ibv_create_qp;

/// Destroy a queue pair.
pub use self::private::ibv_destroy_qp;

/// Queue pair.
pub use self::private::ibv_qp;

/// Queue pair initialization attributes.
pub use self::private::ibv_qp_init_attr;

/// Queue pair capabilities.
pub use self::private::ibv_qp_cap;

/// Queue pair type.
pub use self::private::ibv_qp_type;

/// Modify the queue pair state.
pub use self::private::ibv_modify_qp;

/// Query the queue pair attributes.
pub use self::private::ibv_query_qp;

/// Queue pair attributes.
pub use self::private::ibv_qp_attr;

/// Queue pair state.
pub use self::private::ibv_qp_state;

pub use self::private::ibv_post_send;

/// Send work request.
pub use self::private::ibv_send_wr;

/// Scatter-gather entry.
pub use self::private::ibv_sge;

/// Send flags.
pub use self::private::ibv_send_flags;

/// Send opcode.
pub use self::private::ibv_wr_opcode;

pub use self::private::ibv_post_recv;

/// Receive work request.
pub use self::private::ibv_recv_wr;

/// Create an address handle.
pub use self::private::ibv_create_ah;

/// Destroy an address handle.
pub use self::private::ibv_destroy_ah;

/// Address handle.
pub use self::private::ibv_ah;
