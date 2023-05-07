//! An RDMA library consisting of a safe RDMA wrapping and several useful functionalities to build RDMA connections.
//!
//! # Example
//!
//! ```rust
//! use rrddmma::*;
//! use anyhow::Result;
//!
//! fn main() -> Result<()> {
//!     let context = rrddmma::Context::open(Some("mlx5_0"), 1, 0)?;
//!     let pd = rrddmma::Pd::new(context.clone())?;
//!
//!     let buf = vec![0u8; 4096];
//!     let mr = rrddmma::Mr::reg_slice(pd.clone(), &buf)?;
//!
//!     Ok(())
//! }
//! ```

mod rdma;
pub use rdma::{context::Context, cq::*, mr::*, pd::Pd, qp::*, wr::*};

/// Connection management utilities.
pub mod ctrl;
