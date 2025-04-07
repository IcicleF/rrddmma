//! Bindings of libibverbs C interfaces.

#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(deref_nullptr)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::too_many_arguments)]
#![allow(unused)]

mod common;

#[cfg(feature = "legacy")]
mod legacy;

#[cfg(not(feature = "legacy"))]
mod rdma_core;

mod private {
    use libc::*;
    include!(concat!(env!("OUT_DIR"), "/verbs_bindings.rs"));

    #[cfg(feature = "legacy")]
    pub use super::legacy::*;

    #[cfg(not(feature = "legacy"))]
    pub use super::rdma_core::*;
}

pub(crate) use self::private::*;

extern "C" {
    /// Get list of IB devices currently available.
    ///
    /// Returns a NULL-terminated array of IB devices.
    /// The array can be released with `ibv_free_device_list()`.
    pub fn ibv_get_device_list(num_devices: *mut ::std::os::raw::c_int) -> *mut *mut ibv_device;
}

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

/// GID.
pub use self::private::ibv_gid;

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

#[cfg(feature = "legacy")]
pub use self::private::ibv_exp_create_qp;

#[cfg(feature = "legacy")]
pub use self::private::ibv_exp_modify_qp;

#[cfg(feature = "legacy")]
pub use self::private::ibv_exp_post_send;

#[cfg(feature = "legacy")]
/// Experimental QP attributes.
pub use self::private::ibv_exp_qp_attr;

#[cfg(feature = "legacy")]
/// Experimental QP initialization attributes.
pub use self::private::ibv_exp_qp_init_attr;

#[cfg(feature = "legacy")]
/// Experimental send flags.
pub use self::private::ibv_exp_send_flags;

#[cfg(feature = "legacy")]
/// Experimental send work request.
pub use self::private::ibv_exp_send_wr;

#[cfg(feature = "legacy")]
/// Experimental send opcode.
pub use self::private::ibv_exp_wr_opcode;
