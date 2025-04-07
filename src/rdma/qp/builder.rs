#[cfg(feature = "legacy")]
use std::collections::HashSet;
use std::mem;

use crate::bindings::*;
use crate::rdma::cq::*;
use crate::rdma::pd::*;

use super::{Qp, QpCreationError, QpType};

/// Experimental features available in MLNX_OFED v4.x drivers.
#[cfg(feature = "legacy")]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum ExpFeature {
    /// Enable extended atomic compare-and-swap & fetch-and-add.
    ExtendedAtomics,
}

/// Queue pair capability attributes.
///
/// Documentation heavily borrowed from [RDMAmojo](https://www.rdmamojo.com/2012/12/21/ibv_create_qp/).
#[derive(Clone, Copy, Debug)]
pub struct QpCaps {
    /// The maximum number of outstanding work Requests that can be posted to
    /// the Send Queue in that Queue Pair.
    ///
    /// Value can be [0..`dev_cap.max_qp_wr`].
    ///
    /// **NOTE:** There may be RDMA devices that for specific transport types
    /// may support fewer outstanding Work Requests than the maximum reported
    /// value.
    pub max_send_wr: u32,

    /// The maximum number of outstanding Work Requests that can be posted to
    /// the Receive Queue in that Queue Pair.
    ///
    /// Value can be [0..`dev_cap.max_qp_wr`].
    ///
    /// **NOTE:** There may be RDMA devices that for specific transport types
    /// may support fewer outstanding Work Requests than the maximum reported
    /// value. This value is ignored if the Queue Pair is associated with an SRQ.
    pub max_recv_wr: u32,

    /// The maximum number of scatter/gather elements in any Work Request that
    /// can be posted to the Send Queue in that Queue Pair.
    ///
    /// Value can be [0..`dev_cap.max_sge`].
    ///
    /// **NOTE:** There may be RDMA devices that for specific transport types
    /// may support fewer scatter/gather elements than the maximum reported value.
    pub max_send_sge: u32,

    /// The maximum number of scatter/gather elements in any Work Request that
    /// can be posted to the Receive Queue in that Queue Pair.
    ///
    /// Value can be [0..`dev_cap.max_sge`].
    ///
    /// **NOTE:** There may be RDMA devices that for specific transport types
    /// may support fewer scatter/gather elements than the maximum reported value.
    /// This value is ignored if the Queue Pair is associated with an SRQ.
    pub max_recv_sge: u32,

    /// The maximum message size (in bytes) that can be posted inline to the
    /// Send Queue. If no inline message is requested, the value can be 0.
    pub max_inline_data: u32,
}

impl Default for QpCaps {
    /// Generate a default RDMA queue pair capabilities setting.
    /// The queue pair capabilities are set to:
    /// - 128 outstanding send/recv work requests,
    /// - 16 SGEs per send/recv work request, and
    /// - 64B inline data.
    ///
    /// **NOTE:** Such a setting might *not* be supported by the underlying
    /// RDMA device.
    fn default() -> Self {
        QpCaps {
            max_send_wr: 128,
            max_recv_wr: 128,
            max_send_sge: 16,
            max_recv_sge: 16,
            max_inline_data: 64,
        }
    }
}

impl QpCaps {
    /// Generate a default RDMA queue pair capabilities setting for DC initiator.
    /// The queue pair capabilities are set to:
    /// - 128 outstanding send work requests,
    /// - **8** SGEs per send work request, and
    /// - 64B inline data.
    ///
    /// Note that the SGL length limit is not 16 as in the [`Self::default()`] setting
    /// due to the hardware limit. The hard limit on ConnectX-5 NICs is probably 11;
    /// for safety, we set it to 8.
    ///
    /// **ALSO NOTE:** Such a setting might *not* be supported by the underlying
    /// RDMA device.
    pub fn for_dc_ini() -> Self {
        QpCaps {
            max_send_wr: 128,
            max_recv_wr: 0,
            max_send_sge: 8,
            max_recv_sge: 0,
            max_inline_data: 64,
        }
    }
}

/// Queue pair builder.
#[derive(Clone)]
pub struct QpBuilder<'a> {
    /// Send completion queue for this QP.
    pub(super) send_cq: Option<&'a Cq>,

    /// Receive completion queue for this QP. Can be the same to send CQ.
    pub(super) recv_cq: Option<&'a Cq>,

    /// Capabilities of this QP.
    pub(super) caps: QpCaps,

    /// Queue pair type.
    pub(super) qp_type: Option<QpType>,

    /// Whether to signal for all send work requests.
    pub(super) sq_sig_all: Option<bool>,

    /// Whether to use global routing. Default is `true`.
    pub(super) global_routing: bool,

    /// Enabled experimental features.
    #[cfg(feature = "legacy")]
    pub(super) features: HashSet<ExpFeature>,
}

impl<'a> QpBuilder<'a> {
    /// Create a new queue pair builder.
    pub fn new() -> Self {
        Self {
            send_cq: None,
            recv_cq: None,
            // SAFETY: POD type.
            caps: unsafe { mem::zeroed() },
            qp_type: None,
            sq_sig_all: None,
            global_routing: true,

            #[cfg(feature = "legacy")]
            features: Default::default(),
        }
    }

    /// Set the send completion queue for this QP.
    pub fn send_cq(mut self, send_cq: &'a Cq) -> Self {
        self.send_cq = Some(send_cq);
        self
    }

    /// Set the receive completion queue for this QP.
    pub fn recv_cq(mut self, recv_cq: &'a Cq) -> Self {
        self.recv_cq = Some(recv_cq);
        self
    }

    /// Set the capabilities of this QP.
    /// If not set, the QP will be unable to send or receive any work request by default.
    pub fn caps(mut self, caps: QpCaps) -> Self {
        self.caps = caps;
        self
    }

    /// Set the type of this QP.
    pub fn qp_type(mut self, qp_type: QpType) -> Self {
        self.qp_type = Some(qp_type);
        self
    }

    /// Set whether to signal for all send work requests.
    pub fn sq_sig_all(mut self, sq_sig_all: bool) -> Self {
        self.sq_sig_all = Some(sq_sig_all);
        self
    }

    /// Set whether to use global routing.
    /// If not set, the QP will use global routing by default.
    ///
    /// Global routing is used to enable routing between different Infiniband subnets,
    /// and to enable routing within subnets in RoCE networks. As such, it is mandatory
    /// to set this attribute to `true` in RoCE networks, and if you don't do so, the QP
    /// will err when you bind a local port to it.
    pub fn global_routing(mut self, global_routing: bool) -> Self {
        self.global_routing = global_routing;
        self
    }

    /// Enable experimental features for the QP.
    #[cfg(feature = "legacy")]
    pub fn enable_feature(mut self, feature: ExpFeature) -> Self {
        self.features.insert(feature);
        self
    }

    /// Disable experimental features for the QP.
    #[cfg(feature = "legacy")]
    pub fn disable_feature(mut self, feature: ExpFeature) -> Self {
        self.features.remove(&feature);
        self
    }

    /// Build the queue pair on the given protection domain.
    ///
    /// # Panics
    ///
    /// Panic if any mandatory field (except QP capabilities) is not set.
    pub fn build(self, pd: &Pd) -> Result<Qp, QpCreationError> {
        Qp::new(pd, self)
    }
}

impl<'a> QpBuilder<'a> {
    /// Unwrap the builder and return the set attributes.
    #[inline]
    pub(super) fn unwrap(self) -> QpInitAttr {
        QpInitAttr {
            send_cq: self.send_cq.expect("send CQ must be set").clone(),
            recv_cq: self.recv_cq.expect("recv CQ must be set").clone(),
            caps: self.caps,
            qp_type: self.qp_type.expect("QP type must be set"),
            sq_sig_all: self.sq_sig_all.expect("sq_sig_all must be explicitly set"),
            global_routing: self.global_routing,

            #[cfg(feature = "legacy")]
            features: self.features,
        }
    }
}

impl Default for QpBuilder<'_> {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialization attributes of a queue pair.
pub(super) struct QpInitAttr {
    /// Send completion queue for this QP.
    pub send_cq: Cq,

    /// Receive completion queue for this QP. Can be the same to send CQ.
    pub recv_cq: Cq,

    /// Capabilities of this QP.
    pub caps: QpCaps,

    /// Queue pair type.
    pub qp_type: QpType,

    /// Whether to signal for all send work requests.
    pub sq_sig_all: bool,

    /// Whether to use global routing.
    pub global_routing: bool,

    /// Experimental feature flags.
    #[cfg(feature = "legacy")]
    pub features: HashSet<ExpFeature>,
}

impl QpInitAttr {
    /// Create an [`ibv_qp_init_attr`] from the attributes.
    #[allow(unused)]
    pub fn to_init_attr(&self) -> ibv_qp_init_attr {
        ibv_qp_init_attr {
            send_cq: self.send_cq.as_raw(),
            recv_cq: self.recv_cq.as_raw(),
            cap: ibv_qp_cap {
                max_send_wr: self.caps.max_send_wr,
                max_recv_wr: self.caps.max_recv_wr,
                max_send_sge: self.caps.max_send_sge,
                max_recv_sge: self.caps.max_recv_sge,
                max_inline_data: self.caps.max_inline_data,
            },
            qp_type: u32::from(self.qp_type),
            sq_sig_all: self.sq_sig_all as i32,
            ..unsafe { mem::zeroed() }
        }
    }

    /// Create an [`ibv_exp_qp_init_attr`] from the attributes.
    #[cfg(feature = "legacy")]
    pub fn to_exp_init_attr(&self, pd: &Pd) -> ibv_exp_qp_init_attr {
        let mut attr = ibv_exp_qp_init_attr {
            send_cq: self.send_cq.as_raw(),
            recv_cq: self.recv_cq.as_raw(),
            cap: ibv_qp_cap {
                max_send_wr: self.caps.max_send_wr,
                max_recv_wr: self.caps.max_recv_wr,
                max_send_sge: self.caps.max_send_sge,
                max_recv_sge: self.caps.max_recv_sge,
                max_inline_data: self.caps.max_inline_data,
            },
            qp_type: u32::from(self.qp_type),
            sq_sig_all: self.sq_sig_all as i32,
            pd: pd.as_raw(),
            comp_mask: ibv_exp_qp_init_attr_comp_mask::IBV_EXP_QP_INIT_ATTR_PD.0,
            ..unsafe { mem::zeroed() }
        };

        // Digest experimental features.
        for feature in &self.features {
            match feature {
                ExpFeature::ExtendedAtomics => {
                    // SAFETY: POD type.
                    let mut dev_attr = unsafe { mem::zeroed::<ibv_exp_device_attr>() };
                    dev_attr.comp_mask =
                        (ibv_exp_device_attr_comp_mask::IBV_EXP_DEVICE_ATTR_EXT_ATOMIC_ARGS
                            | ibv_exp_device_attr_comp_mask::IBV_EXP_DEVICE_ATTR_EXP_CAP_FLAGS)
                            .0 as _;
                    // SAFETY: FFI.
                    unsafe { ibv_exp_query_device(pd.context().as_raw(), &mut dev_attr) };

                    attr.comp_mask |=
                        ibv_exp_qp_init_attr_comp_mask::IBV_EXP_QP_INIT_ATTR_ATOMICS_ARG.0;
                    attr.max_atomic_arg = 1 << dev_attr.ext_atom.log_max_atomic_inline;
                }
            }
        }
        attr
    }

    /// Create an [`ibv_qp_init_attr_ex`] from the attributes.
    #[allow(unused)]
    pub fn to_init_attr_ex(&self, pd: &Pd) -> ibv_qp_init_attr_ex {
        ibv_qp_init_attr_ex {
            send_cq: self.send_cq.as_raw(),
            recv_cq: self.recv_cq.as_raw(),
            cap: ibv_qp_cap {
                max_send_wr: self.caps.max_send_wr,
                max_recv_wr: self.caps.max_recv_wr,
                max_send_sge: self.caps.max_send_sge,
                max_recv_sge: self.caps.max_recv_sge,
                max_inline_data: self.caps.max_inline_data,
            },
            qp_type: u32::from(self.qp_type),
            sq_sig_all: self.sq_sig_all as i32,
            pd: pd.as_raw(),
            comp_mask: ibv_qp_init_attr_mask::IBV_QP_INIT_ATTR_PD.0,
            ..unsafe { mem::zeroed() }
        }
    }
}
