//! An RDMA library consisting of safe RDMA wrappings and several useful
//! functionalities to build RDMA connections.
//!
//! This library uses existing installation of MLNX_OFED or ibverbs library.
//!
//! # Example
//!
//! This example sends and receives a message via RDMA RC QPs.
//!
//! ```rust
#![doc = include_str!("../examples/local_rc_sendrecv.rs")]
//! ```

#![cfg_attr(docsrs, feature(doc_cfg))]

// This must be placed before any other modules because of its macros.
mod utils;

pub mod bindings;
pub mod ctrl;
pub mod hi;
pub mod lo;
