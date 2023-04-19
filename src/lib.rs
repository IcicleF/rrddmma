//! An RDMA library consisting of a safe RDMA wrapping and several useful functionalities to build RDMA connections.

mod rdma;
pub use rdma::{context::Context, cq::*, mr::*, pd::Pd, qp::*};

pub mod ctrl;
