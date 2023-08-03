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
//! management utilities in the [`ctrl`] mod. Most commonly-used ones include
//! distributed barriers ([`ctrl::Barrier`]) and connection builders
//! ([`ctrl::Connecter`]). Some higher-level wrappings of RDMA resources are
//! in the [`wrap`] mod and is under continuous development.
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
//! It should print: `Hello, rrddmma!`
//!
//! [`rdma-sys`]: https://docs.rs/rdma-sys/latest/rdma_sys/

/// Shared util functions.
mod utils;

/// RDMA data-plane functionalities.
/// Not to be publicly exposed, instead `pub use` necessary items.
mod rdma;

#[cfg(not(feature = "full_name"))]
mod rdma_export {
    pub use super::rdma::context::Context;
    pub use super::rdma::cq::{Cq, Wc, WcOpcode, WcStatus};
    pub use super::rdma::gid::Gid;
    pub use super::rdma::mr::{Mr, MrSlice};
    pub use super::rdma::pd::Pd;
    pub use super::rdma::qp::{Qp, QpCaps, QpEndpoint, QpInitAttr, QpPeer, QpState, QpType};
    pub use super::rdma::remote_mem::RemoteMem;
    pub use super::rdma::wr::{RawRecvWr, RawSendWr, RecvWr, SendWr, SendWrDetails};
}

#[cfg(feature = "full_name")]
mod rdma_export {
    pub use super::rdma::context::Context;
    pub use super::rdma::cq::{
        Cq as CompletionQueue, Wc as WorkCompletion, WcOpcode as WorkCompletionOpcode,
        WcStatus as WorkCompletionStatus,
    };
    pub use super::rdma::gid::Gid;
    pub use super::rdma::mr::{Mr as MemoryRegion, MrSlice as MemoryRegionSlice};
    pub use super::rdma::pd::Pd as ProtectionDomain;
    pub use super::rdma::qp::{
        Qp as QueuePair, QpCaps as QueuePairCapabilities, QpEndpoint as QueuePairEndpoint,
        QpInitAttr as QueuePairInitAttributes, QpPeer as QueuePairPeer, QpState as QueuePairState,
        QpType as QueuePairType,
    };
    pub use super::rdma::remote_mem::RemoteMem as RemoteMemory;
    pub use super::rdma::wr::{
        RawRecvWr as RawReceiveWorkRequest, RawSendWr as RawSendWorkRequest,
        RecvWr as ReceiveWorkRequest, SendWr as SendWorkRequest,
        SendWrDetails as SendWorkRequestDetails,
    };
}

/// Export RDMA data-plane functionalities to the top-level.
pub use rdma_export::*;

/// Export types to the top-level.
pub use rdma::types;

/// Connection management utilities.
pub mod ctrl;

/// Higher-level wrappings of RDMA resources.
pub mod wrap;
