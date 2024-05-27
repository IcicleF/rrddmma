use std::{io, mem};

use crate::bindings::*;
use crate::prelude::QpCaps;
use crate::rdma::{context::Context, cq::Cq, mr::Permission, nic::*, pd::Pd, srq::Srq};

use super::{Dct, DctCreationError};

/// DCT builder.
#[derive(Clone, Default)]
pub struct DctBuilder<'a> {
    /// Protection domain of this DCT.
    pub(super) pd: Option<&'a Pd>,

    /// Receive completion queue for this DCT.
    pub(super) cq: Option<&'a Cq>,

    /// The local port to use.
    pub(super) port: Option<Port>,

    /// The GID index to use.
    /// If not specified, use the recommended GID.
    pub(super) gid_index: u8,

    /// The maximum message size (in bytes) that can be inline received by the DCT.
    pub(super) inline_size: u32,
}

impl<'a> DctBuilder<'a> {
    /// Create a new DCT builder.
    pub fn new() -> Self {
        Default::default()
    }

    /// Set the protection domain of this DCT.
    pub fn pd(mut self, pd: &'a Pd) -> Self {
        self.pd = Some(pd);
        self
    }

    /// Set the receive completion queue for this DCT.
    pub fn cq(mut self, cq: &'a Cq) -> Self {
        self.cq = Some(cq);
        self
    }

    /// Set the local port and GID index for this DCT.
    ///
    /// If no GID index is specified (i.e., `gid_index` is `None`), this
    /// method will use the recommended GID. See documentation of
    /// [`Port::recommended_gid()`] for more information.
    pub fn port(mut self, port: &Port, gid_index: Option<u8>) -> Self {
        self.port = Some(port.clone());
        self.gid_index = gid_index.unwrap_or(port.recommended_gid().1);
        self
    }

    /// Set the maximum message size (in bytes) that can be inline received
    /// by this DCT.
    pub fn inline_size(mut self, len: u32) -> Self {
        self.inline_size = len;
        self
    }

    /// Build the DCT.
    ///
    /// # Panics
    ///
    /// Panic if any mandatory field (except `inline_size`) is not set.
    pub fn build(self, context: &Context) -> Result<Dct, DctCreationError> {
        Dct::new(context, self)
    }
}

impl<'a> DctBuilder<'a> {
    /// Unwrap the builder and return the set attributes.
    pub(crate) fn unwrap(self) -> io::Result<DctInitAttr> {
        let pd = self.pd.expect("PD must be set").clone();
        let cq = self.cq.expect("CQ must be set");
        let srq = Srq::new(
            &pd,
            Some(cq),
            QpCaps::default().max_recv_wr,
            QpCaps::default().max_recv_sge,
        )?;
        Ok(DctInitAttr {
            pd,
            cq: cq.clone(),
            srq,
            port: self.port.expect("local port must be set"),
            gid_index: self.gid_index,
            inline_size: self.inline_size,
        })
    }
}

/// Initialization attributes of a DCT endpoint.
pub(crate) struct DctInitAttr {
    pub pd: Pd,
    pub cq: Cq,
    pub srq: Srq,
    pub port: Port,
    pub gid_index: u8,
    pub inline_size: u32,
}

impl DctInitAttr {
    /// Create an [`ibv_exp_dct_init_attr`] from the attributes.
    pub fn to_init_attr(&self) -> ibv_exp_dct_init_attr {
        ibv_exp_dct_init_attr {
            pd: self.pd.as_raw(),
            cq: self.cq.as_raw(),
            srq: self.srq.as_raw(),
            dc_key: Dct::GLOBAL_DC_KEY,
            port: self.port.num(),
            access_flags: Permission::default().into(),
            min_rnr_timer: 12,
            mtu: self.port.mtu() as _,
            gid_index: self.gid_index,
            hop_limit: 0xFF,
            inline_size: self.inline_size,

            // SAFETY: POD type.
            ..unsafe { mem::zeroed() }
        }
    }
}
