//! An RDMA library consisting of a safe RDMA wrapping and several useful
//! functionalities to build RDMA connections.
//! It is built atop the [`rdma-sys`] crate.
//!
//! `rrddmma` provides safe wrappings with `Arc`-based custom types. Therefore,
//! all RDMA resource holder types ([`Context`], [`Pd`], [`Cq`], [`Mr`], and
//! [`Qp`]) should be viewed as references to the true underlying resources.
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
//! # Example
//!
//! ```rust
//! use rrddmma::*;
//! use anyhow::Result;
//!
//! fn main() -> Result<()> {
//!     let context = Context::open(Some("mlx5_0"), 1, 0)?;
//!     let pd = Pd::new(context.clone())?;
//!
//!     let buf = vec![0u8; 4096];
//!     let mr = Mr::reg_slice(pd.clone(), &buf)?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Warning
//!
//! **The interfaces are unstable and up to change!**
//!
//! [`rdma-sys`]: https://docs.rs/rdma-sys/latest/rdma_sys/

mod rdma;
pub use rdma::gid::Gid;
pub use rdma::{context::Context, cq::*, mr::*, pd::Pd, qp::*, wr::*};

/// Connection management utilities.
pub mod ctrl;
