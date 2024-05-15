//! The RDMA prelude.
//!
//! The purpose of this module is to alleviate imports of common RDMA
//! functionalities.

pub use crate::rdma::context::Context;
pub use crate::rdma::cq::{Cq, Wc};
#[cfg(mlnx4)]
pub use crate::rdma::dct::Dct;
pub use crate::rdma::mr::{Mr, MrRemote, MrSlice, Slicing};
pub use crate::rdma::nic::{Nic, Port};
pub use crate::rdma::pd::Pd;
pub use crate::rdma::qp::{Qp, QpCaps, QpEndpoint, QpPeer, QpType};
pub use crate::rdma::srq::Srq;
pub use crate::rdma::wr::*;

