//! Complex parameter types used in RDMA send operations.
#![cfg(feature = "exp")]

use std::ptr::NonNull;

/// Extended atomic compare-and-swap parameters.
#[derive(Debug, Clone, Copy)]
pub struct ExtCompareSwapParams {
    /// Pointer to the compare value.
    pub compare: NonNull<u64>,

    /// Pointer to the swap value.
    pub swap: NonNull<u64>,

    /// Pointer to the compare mask.
    pub compare_mask: NonNull<u64>,

    /// Pointer to the swap mask.
    pub swap_mask: NonNull<u64>,
}

unsafe impl Send for ExtCompareSwapParams {}

unsafe impl Sync for ExtCompareSwapParams {}
