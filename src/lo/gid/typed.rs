use std::io::Error as IoError;
#[cfg(not(feature = "legacy"))]
use std::io::{self, ErrorKind as IoErrorKind};
use std::mem;
use std::net::Ipv6Addr;

use serde::Serialize;
use thiserror::Error;

use crate::bindings::*;
use crate::lo::{context::*, type_alias::*};

use super::raw::Gid;

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

impl GidType {
    pub fn is_roce(self) -> bool {
        matches!(self, GidType::RoceV1 | GidType::RoceV2)
    }

    pub fn is_infiniband(self) -> bool {
        matches!(self, GidType::Infiniband)
    }
}

/// GID query result error type.
#[derive(Debug, Error)]
pub enum GidQueryError {
    /// `libibverbs` interfaces returned an error.
    #[error("I/O error from ibverbs")]
    IoError(#[from] IoError),

    /// Failed to query the attributes of the GID. This usually means that the
    /// GID itself does not exist in the first place.
    #[error("attribute query error")]
    AttributeQueryError,

    /// The GID type is not Infiniband, RoCEv1, or RoCEv2.
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
    #[cfg(feature = "legacy")]
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

    #[cfg(not(feature = "legacy"))]
    fn query_impl(
        ctx: IbvContext,
        port_num: u8,
        port_attr: &ibv_port_attr,
        gid_index: GidIndex,
    ) -> Result<Self, GidQueryError> {
        // SAFETY: POD type.
        let mut gid = unsafe { mem::zeroed() };

        // SAFETY: FFI.
        let ret = unsafe { ibv_query_gid(ctx.as_ptr(), port_num, gid_index as _, &mut gid) };
        if ret != 0 {
            return Err(IoError::from_raw_os_error(ret).into());
        }

        // Extract GID type from ibsysfs.
        let gid_is_rocev2 = {
            use std::fs::File;
            use std::io::prelude::*;
            use std::path::{Path, PathBuf};

            let path = Path::new("/sys/class/infiniband")
                .join(ctx.dev().name()?)
                .join("ports")
                .join(port_num.to_string())
                .join("gid_attrs/types")
                .join(gid_index.to_string());
            let mut buf = String::new();

            #[inline]
            fn local_read(path: PathBuf, buf: &mut String) -> io::Result<usize> {
                File::open(path)?.read_to_string(buf)
            }
            if let Err(e) = local_read(path, &mut buf) {
                return if e.kind() == IoErrorKind::InvalidInput {
                    Err(GidQueryError::AttributeQueryError)
                } else {
                    Err(e.into())
                };
            }

            matches!(buf.trim(), "RoCE v2")
        };

        let ty = match (port_attr.link_layer as i32, gid_is_rocev2) {
            (IBV_LINK_LAYER_ETHERNET, false) => GidType::RoceV1,
            (IBV_LINK_LAYER_ETHERNET, true) => GidType::RoceV2,
            (_, false) => GidType::Infiniband,
            _ => return Err(GidQueryError::Unrecognized),
        };
        Ok(GidTyped::new(Gid(gid), ty))
    }
}

impl GidTyped {
    /// Create a typed GID.
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
