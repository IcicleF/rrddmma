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

#[cfg(not(target_os = "linux"))]
compile_error!("`rrddmma` currently only supports Linux");

/// C bindings.
mod bindings;

/// Shared util functions.
mod utils;

/// RDMA data-plane functionalities.
/// Not to be publicly exposed, instead `pub use` necessary items.
mod rdma;

pub use rdma::context::*;
pub use rdma::cq::*;
pub use rdma::gid::*;
pub use rdma::mr::*;
pub use rdma::nic::*;
pub use rdma::pd::*;
pub use rdma::qp::*;
pub use rdma::wr::*;

/// Type aliases and re-exports for RDMA-related operations.
pub use rdma::types;

/// Connection management utilities.
pub mod ctrl;

/// Higher-level wrappings of RDMA resources.
pub mod wrap;
