use std::io::Error as IoError;
use std::mem;
use std::net::Ipv6Addr;

use serde::Serialize;
use thiserror::Error;

use super::raw::Gid;
use crate::bindings::*;
use crate::rdma::{context::*, types::*};

/// GID type.
#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub enum GidType {
    /// RoCEv1.
    RoceV1,

    /// RoCEv2.
    RoceV2,

    /// Infiniband.
    Infiniband,
}

/// GID query result error type.
#[derive(Debug, Error)]
pub enum GidQueryError {
    /// **I/O error:** `libibverbs` interfaces returned an error.
    #[error("I/O error from ibverbs")]
    IoError(#[from] IoError),

    /// **Unrecognized GID type:** the GID type is not Infiniband,
    /// RoCEv1, or RoCEv2, and is not recognized by this library.
    #[error("unregonized GID type")]
    Unrecognized,
}

/// GID with type.
#[derive(Debug, Clone, Copy)]
pub struct GidTyped {
    pub gid: Gid,
    pub ty: GidType,
}

impl GidTyped {
    #[cfg(mlnx4)]
    fn query_impl(
        ctx: IbvContext,
        port_num: u8,
        port_attr: &ibv_port_attr,
        gid_index: GidIndex,
    ) -> Result<Self, GidQueryError> {
        let is_ethernet = port_attr.link_layer == (IBV_LINK_LAYER_ETHERNET as u8);

        // SAFETY: POD type.
        let mut attr = ibv_exp_gid_attr {
            comp_mask: IBV_EXP_QUERY_GID_ATTR_TYPE | IBV_EXP_QUERY_GID_ATTR_GID,
            ..unsafe { mem::zeroed() }
        };

        // SAFETY: FFI.
        let ret =
            unsafe { ibv_exp_query_gid_attr(ctx.as_ptr(), port_num, gid_index as _, &mut attr) };
        if ret != 0 {
            return Err(IoError::from_raw_os_error(ret).into());
        }

        let gid = attr.gid;
        let ty = match (is_ethernet, attr.type_) {
            (false, _) => GidType::Infiniband,
            (true, ibv_exp_roce_gid_type::IBV_EXP_IB_ROCE_V1_GID_TYPE) => GidType::RoceV1,
            (true, ibv_exp_roce_gid_type::IBV_EXP_ROCE_V2_GID_TYPE) => GidType::RoceV2,

            // SAFETY: enum constraints of `libibverbs`.
            _ => {
                return Err(GidQueryError::Unrecognized);
            }
        };
        Ok(GidTyped::new(Gid(gid), ty))
    }

    #[cfg(mlnx5)]
    fn query_impl(
        ctx: IbvContext,
        port_num: u8,
        _: &ibv_port_attr,
        gid_index: i32,
    ) -> Result<Self, GidQueryError> {
        // SAFETY: POD type.
        let mut entry = unsafe { mem::zeroed() };

        // SAFETY: FFI.
        let ret = unsafe { ibv_query_gid_ex(ctx.as_ptr(), port_num, gid_index as _, &mut entry) };
        if ret != 0 {
            return Err(io::Error::from_raw_os_error(ret).into());
        }

        Ok(match attr.gid_type {
            IBV_GID_TYPE_IB => GidType::Infiniband,
            IBV_GID_TYPE_ROCE_V1 => GidType::RoceV1,
            IBV_GID_TYPE_ROCE_V2 => GidType::RoceV2,

            // SAFETY: enum constraints of `libibverbs`.
            _ => unsafe { hint::unreachable_unchecked() },
        })
    }
}

impl GidTyped {
    /// Create a new GID.
    #[inline]
    pub fn new(gid: Gid, ty: GidType) -> Self {
        Self { gid, ty }
    }

    /// Query the GID from the specified device port.
    pub(crate) fn query(
        ctx: IbvContext,
        port_num: u8,
        port_attr: &ibv_port_attr,
        gid_index: GidIndex,
    ) -> Result<Self, GidQueryError> {
        Self::query_impl(ctx, port_num, port_attr, gid_index)
    }
}

impl PartialEq for GidTyped {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.gid.eq(&other.gid)
    }
}

impl Eq for GidTyped {}

impl From<GidTyped> for Gid {
    #[inline]
    fn from(gid: GidTyped) -> Self {
        gid.gid
    }
}

impl From<GidTyped> for Ipv6Addr {
    #[inline]
    fn from(gid: GidTyped) -> Self {
        Self::from(gid.gid)
    }
}

impl From<GidTyped> for [u8; 16] {
    #[inline]
    fn from(gid: GidTyped) -> Self {
        Self::from(gid.gid)
    }
}

impl Serialize for GidTyped {
    #[inline]
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.gid.serialize(serializer)
    }
}
