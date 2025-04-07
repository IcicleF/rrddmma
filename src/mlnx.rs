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
/// Note that this value is not always the same as the document -- it depends on your deployment environment.
#[cfg(not(feature = "legacy"))]
pub const MLNX_VERSION: MlnxVersion = MlnxVersion::Mlnx5;

/// Contains the current MLNX_OFED version detected by rrddmma.
/// Note that this value is not always the same as the document -- it depends on your deployment environment.
#[cfg(feature = "legacy")]
pub const MLNX_VERSION: MlnxVersion = MlnxVersion::Mlnx4;
