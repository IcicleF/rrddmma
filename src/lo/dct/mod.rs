//! Dynamically-connected target (DCT).
//!
//! The DCT implementation is split by the linked verbs library: the legacy
//! MLNX_OFED v4.x path (`legacy` feature) uses the `ibv_exp_dct` interface,
//! while the rdma-core path creates the DCT as a driver QP with the DC
//! extension via `mlx5dv_create_qp`.

mod builder;

#[cfg(feature = "legacy")]
#[path = "legacy.rs"]
mod imp;

#[cfg(not(feature = "legacy"))]
#[path = "rdmacore.rs"]
mod imp;

pub use self::builder::*;
pub use self::imp::*;
