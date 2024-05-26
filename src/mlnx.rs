//! MLNX_OFED version-based feature flag.

/// Possibble MLNX_OFED versions.
pub enum MlnxVersion {
    /// MLNX_OFED v5.x or newer.
    /// RDMA-Core features are available.
    Mlnx5,

    /// MLNX_OFED v4.x.
    /// Experimental verbs are available.
    Mlnx4,
}

/// Contains the current MLNX_OFED version detected by rrddmma.
#[cfg(mlnx5)]
pub const MLNX_VERSION: MlnxVersion = MlnxVersion::Mlnx5;

/// Contains the current MLNX_OFED version detected by rrddmma.
#[cfg(mlnx4)]
pub const MLNX_VERSION: MlnxVersion = MlnxVersion::Mlnx4;
