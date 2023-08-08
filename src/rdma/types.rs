//! Type aliases and re-exports for RDMA-related operations.

/// Port number is a [`u8`] that identifies a port on a local switch or an HCA.
pub type PortNum = u8;

/// Local identifier (LID) is a [`u16`] that identifies a port on a switch or an HCA in the cluster.
pub type Lid = u16;

/// QP number (QPN) is a [`u32`] that identifies a local queue pair.
pub type Qpn = u32;

/// Packet sequence number (PSN) is a [`u32`] that identifies a packet in a flow.
pub type Psn = u32;

/// Q_Key (QKey) is a [`u32`] identifier for a UD queue pair.
pub type QKey = u32;

/// Local key (LKey) is a [`u32`] that identifies a local memory region.
pub type LKey = u32;

/// Remote key (RKey) is a [`u32`] that identifies a remote memory region.
pub type RKey = u32;

/// Work request Identifier (WrId) is a [`u64`] that can be designated by the user to identify a work request.
pub type WrId = u64;

/// Immediate data (ImmData) is a [`u32`] that can be carried in RDMA send-type work requests.
pub type ImmData = u32;

/// Re-export of [`rdma_sys`] types.
pub mod sys {
    pub use rdma_sys::*;
}
