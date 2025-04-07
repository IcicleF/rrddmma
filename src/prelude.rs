//! The RDMA prelude.
//!
//! The purpose of this module is to alleviate imports of common RDMA
//! functionalities.

pub use crate::lo::context::Context;
pub use crate::lo::cq::{Cq, Wc, WcOpcode, WcStatus};
#[cfg(feature = "legacy")]
pub use crate::lo::cq::{ExpCq, ExpWc};
#[cfg(feature = "legacy")]
pub use crate::lo::dct::Dct;
pub use crate::lo::mr::{Mr, MrRemote, MrSlice, Slicing};
pub use crate::lo::nic::{Nic, Port};
pub use crate::lo::pd::Pd;
pub use crate::lo::qp::{Qp, QpCaps, QpEndpoint, QpPeer, QpType};
pub use crate::lo::srq::Srq;
pub use crate::lo::wr::*;
