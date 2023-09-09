//! An RDMA library consisting of a safe RDMA wrapping and several useful
//! functionalities to build RDMA connections.
//! It is built atop the [`rdma-sys`] crate and mainly designed for academic
//! research purposes.
//!
//! `rrddmma` provides safe wrappings with `Arc`-based custom types. All RDMA
//! resource holder types ([`Context`], [`Pd`], [`Cq`], [`Mr`], and [`Qp`])
//! should be viewed as references to the true underlying resources.
//! You can share these resources simply by `clone()`-ing the abovementioned
//! types' instances.
//! While this does add an extra layer of indirection, it also drastically
//! simplifies the system's design when it comes to multi-threading.
//!
//! Aside from RDMA functionalities, there are some TCP-based connection
//! management utilities in the [`ctrl`] mod. Currently there is only a
//! connection builder ([`ctrl::Connecter`]). Some higher-level wrappings
//! of RDMA resources are in the [`wrap`] mod and under continuous development.
//!
//! **WARNING: The interfaces are unstable and up to change!**
//!
//! # Example
//!
//! This example sends and receives a message via RDMA RC QPs.
//!
//! ```rust
#![doc = include_str!("../examples/local_sendrecv.rs")]
//! ```
//!
//! [`rdma-sys`]: https://docs.rs/rdma-sys/latest/rdma_sys/

/// Shared util functions.
mod utils;

/// RDMA data-plane functionalities.
/// Not to be publicly exposed, instead `pub use` necessary items.
mod rdma;

pub use rdma::context::Context;
pub use rdma::cq::{Cq, Wc, WcOpcode, WcStatus};
pub use rdma::gid::Gid;
pub use rdma::mr::{Mr, MrSlice};
pub use rdma::pd::Pd;
pub use rdma::qp::{Qp, QpCaps, QpEndpoint, QpInitAttr, QpPeer, QpState, QpType};
pub use rdma::remote_mem::RemoteMem;
pub use rdma::wr::{RawRecvWr, RawSendWr, RecvWr, SendWr, SendWrDetails};

/// Type aliases and re-exports for RDMA-related operations.
pub use rdma::types;

/// Connection management utilities.
pub mod ctrl;

/// Higher-level wrappings of RDMA resources.
pub mod wrap;

/// Re-export of [`rdma_sys`] types, modules, and functions.
/// If you seek to use the raw RDMA C API, you may want to use this module.
///
/// In the root of this module, only those used in the library are re-exported.
/// You can find the full list of re-exports in the [`sys::entire`] submodule.
pub mod sys {
    /// RDMA atomics work request parameters.
    pub use rdma_sys::atomic_t;

    /// Memory region access flags.
    pub use rdma_sys::ibv_access_flags;

    /// Global address handle.
    pub use rdma_sys::ibv_ah;

    /// Attributes (used in creation of address handles).
    pub use rdma_sys::ibv_ah_attr;

    /// Device context.
    pub use rdma_sys::ibv_context;

    /// Completion queue.
    pub use rdma_sys::ibv_cq;

    /// Physical device information.
    pub use rdma_sys::ibv_device;

    /// Physical device attributes.
    pub use rdma_sys::ibv_device_attr;

    /// GID.
    pub use rdma_sys::ibv_gid;

    /// Global routing information (used in creation of address handles).
    pub use rdma_sys::ibv_global_route;

    /// Memory region.
    pub use rdma_sys::ibv_mr;

    /// Protection domain.
    pub use rdma_sys::ibv_pd;

    /// Device port attributes.
    pub use rdma_sys::ibv_port_attr;

    /// Queue pair.
    pub use rdma_sys::ibv_qp;

    /// Queue pair attributes.
    pub use rdma_sys::ibv_qp_attr;

    /// Queue pair capabilities (used in creation of queue pairs).
    pub use rdma_sys::ibv_qp_cap;

    /// Queue pair initialization attributes (used in creation of queue pairs).
    pub use rdma_sys::ibv_qp_init_attr;

    /// Receive work request.
    pub use rdma_sys::ibv_recv_wr;

    /// Send work request flags.
    pub use rdma_sys::ibv_send_flags;

    /// Send work request.
    pub use rdma_sys::ibv_send_wr;

    /// Scatter-gather element.
    pub use rdma_sys::ibv_sge;

    /// Work completion entry.
    pub use rdma_sys::ibv_wc;

    /// Work completion entry flags.
    pub use rdma_sys::ibv_wc_flags;

    /// Immediate data union (used in send work requests).
    pub use rdma_sys::imm_data_invalidated_rkey_union_t;

    /// RDMA one-sided read/write work request parameters.
    pub use rdma_sys::rdma_t;

    /// RDMA UD QP send work request parameters.
    pub use rdma_sys::ud_t;

    /// Union type of [`rdma_t`], [`atomic_t`], and [`ud_t`].
    /// Specifies work request information.
    pub use rdma_sys::wr_t;

    /// Enum type of path active MTUs.
    pub use rdma_sys::ibv_mtu;

    /// Enum type of device port speeds.
    pub use rdma_sys::ibv_port_state;

    /// Mask of queue pair attributes (used in query of queue pair attributes).
    pub use rdma_sys::ibv_qp_attr_mask;

    /// Enum type of queue pair states.
    pub use rdma_sys::ibv_qp_state;

    /// Enum type of queue pair types.
    pub use rdma_sys::ibv_qp_type;

    /// Enum type of work request opcodes in completion entries.
    pub use rdma_sys::ibv_wc_opcode;

    /// Enum type of work completion statuses.
    pub use rdma_sys::ibv_wc_status;

    /// Enum type of work request opcodes.
    pub use rdma_sys::ibv_wr_opcode;

    /// Free device list.
    pub use rdma_sys::ibv_free_device_list;

    /// Get device list.
    pub use rdma_sys::ibv_get_device_list;

    /// Get device name.
    pub use rdma_sys::ibv_get_device_name;

    /// Open device context.
    pub use rdma_sys::ibv_open_device;

    /// Close device context.
    pub use rdma_sys::ibv_close_device;

    /// Query device port attributes.
    pub use rdma_sys::___ibv_query_port;

    /// Query device attributes.
    pub use rdma_sys::ibv_query_device;

    /// Query GID.
    pub use rdma_sys::ibv_query_gid;

    /// Allocate protection domain.
    pub use rdma_sys::ibv_alloc_pd;

    /// Deallocate protection domain.
    pub use rdma_sys::ibv_dealloc_pd;

    /// Create address handle.
    pub use rdma_sys::ibv_create_ah;

    /// Destroy address handle.
    pub use rdma_sys::ibv_destroy_ah;

    /// Create completion queue.
    pub use rdma_sys::ibv_create_cq;

    /// Destroy completion queue.
    pub use rdma_sys::ibv_destroy_cq;

    /// Register memory region.
    pub use rdma_sys::ibv_reg_mr;

    /// Deregister memory region.
    pub use rdma_sys::ibv_dereg_mr;

    /// Create queue pair.
    pub use rdma_sys::ibv_create_qp;

    /// Destroy queue pair.
    pub use rdma_sys::ibv_destroy_qp;

    /// Modify queue pair.
    pub use rdma_sys::ibv_modify_qp;

    /// Post receive work request.
    pub use rdma_sys::ibv_post_recv;

    /// Post send work request.
    pub use rdma_sys::ibv_post_send;

    /// Poll completion queue.
    pub use rdma_sys::ibv_poll_cq;

    /// All types, modules, and functions in [`rdma_sys`].
    pub mod entire {
        pub use rdma_sys::*;
    }
}
