//! An RDMA library consisting of a safe RDMA wrapping and several useful
//! functionalities to build RDMA connections.
//! It is built atop the [`rdma-sys`] crate.
//!
//! `rrddmma` provides safe wrappings with `Arc`-based custom types. All RDMA
//! resource holder types ([`Context`], [`Pd`], [`Cq`], [`Mr`], and [`Qp`])
//! should be viewed as references to the true underlying resources.
//! You can share these resources simply by `clone()`-ing the abovementioned
//! types' instances.
//! While this does add an extra layer of indirection, it also drastically
//! simplifies the system's design when it comes to multi-threading.
//!
//! Aside from RDMA functionalities, there are also some TCP-based connection
//! management utilities in the [`ctrl`] mod. Most commonly-used ones include
//! distributed barriers ([`ctrl::Barrier`]) and connection builders
//! ([`ctrl::Connecter`]).
//!
//! **WARNING: The interfaces are unstable and up to change!**
//!
//! # Example
//!
//! ```rust
#![doc = include_str!("../examples/local_sendrecv.rs")]
//! ```
//!
//! It should print: `Hello, rrddmma!`
//!
//! [`rdma-sys`]: https://docs.rs/rdma-sys/latest/rdma_sys/

mod rdma;

pub use rdma::context::Context;
pub use rdma::cq::*;
pub use rdma::gid::Gid;
pub use rdma::mr::*;
pub use rdma::pd::Pd;
pub use rdma::qp::*;
pub use rdma::remote_mem::*;
pub use rdma::wr::*;

/// Connection management utilities.
pub mod ctrl;
