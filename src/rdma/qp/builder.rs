use std::mem;

use super::{Qp, QpCreationError, QpType};
use crate::bindings::*;
use crate::rdma::cq::*;
use crate::rdma::pd::*;

/// Queue pair capability attributes.
///
/// Documentation heavily borrowed from [RDMAmojo](https://www.rdmamojo.com/2012/12/21/ibv_create_qp/).
#[derive(Clone, Copy, Debug)]
pub struct QpCaps {
    /// The maximum number of outstanding Work Requests that can be posted to
    /// the Send Queue in that Queue Pair.
    ///
    /// Value can be [0..`dev_cap.max_qp_wr`].
    ///
    /// **NOTE:** There may be RDMA devices that for specific transport types
    /// may support less outstanding Work Requests than the maximum reported
    /// value.
    pub max_send_wr: u32,

    /// The maximum number of outstanding Work Requests that can be posted to
    /// the Receive Queue in that Queue Pair.
    ///
    /// Value can be [0..`dev_cap.max_qp_wr`].
    ///
    /// **NOTE:** There may be RDMA devices that for specific transport types
    /// may support less outstanding Work Requests than the maximum reported
    /// value. This value is ignored if the Queue Pair is associated with an SRQ.
    pub max_recv_wr: u32,

    /// The maximum number of scatter/gather elements in any Work Request that
    /// can be posted to the Send Queue in that Queue Pair.
    ///
    /// Value can be [0..`dev_cap.max_sge`].
    ///
    /// **NOTE:** There may be RDMA devices that for specific transport types
    /// may support less scatter/gather elements than the maximum reported value.
    pub max_send_sge: u32,

    /// The maximum number of scatter/gather elements in any Work Request that
    /// can be posted to the Receive Queue in that Queue Pair.
    ///
    /// Value can be [0..`dev_cap.max_sge`].
    ///
    /// **NOTE:** There may be RDMA devices that for specific transport types
    /// may support less scatter/gather elements than the maximum reported value.
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

/// Queue pair builder.
#[derive(Clone)]
pub struct QpBuilder<'a> {
    /// Send completion queue for this QP.
    pub send_cq: Option<&'a Cq>,

    /// Receive completion queue for this QP. Can be the same to send CQ.
    pub recv_cq: Option<&'a Cq>,

    /// Capabilities of this QP.
    pub caps: QpCaps,

    /// Queue pair type.
    pub qp_type: Option<QpType>,

    /// Whether to signal for all send work requests.
    pub sq_sig_all: Option<bool>,
}

impl<'a> QpBuilder<'a> {
    /// Unwrap the builder and return the underlying attributes.
    #[inline]
    pub(super) fn unwrap(self) -> QpInitAttr {
        QpInitAttr {
            send_cq: self.send_cq.expect("send CQ must be set").clone(),
            recv_cq: self.recv_cq.expect("recv CQ must be set").clone(),
            caps: self.caps,
            qp_type: self.qp_type.expect("QP type must be set"),
            sq_sig_all: self.sq_sig_all.expect("sq_sig_all must be explicitly set"),
        }
    }
}

impl<'a> QpBuilder<'a> {
    /// Create a new queue pair builder.
    #[inline]
    pub fn new() -> Self {
        Self {
            send_cq: None,
            recv_cq: None,

            // SAFETY: POD type.
            caps: unsafe { mem::zeroed() },
            qp_type: None,
            sq_sig_all: None,
        }
    }

    /// Set the send completion queue for this QP.
    #[inline]
    pub fn send_cq(mut self, send_cq: &'a Cq) -> Self {
        self.send_cq = Some(send_cq);
        self
    }

    /// Set the receive completion queue for this QP.
    #[inline]
    pub fn recv_cq(mut self, recv_cq: &'a Cq) -> Self {
        self.recv_cq = Some(recv_cq);
        self
    }

    /// Set the capabilities of this QP.
    /// If not set, the QP will be unable to send or receive any work request.
    #[inline]
    pub fn caps(mut self, caps: QpCaps) -> Self {
        self.caps = caps;
        self
    }

    /// Set the type of this QP.
    #[inline]
    pub fn qp_type(mut self, qp_type: QpType) -> Self {
        self.qp_type = Some(qp_type);
        self
    }

    /// Set whether to signal for all send work requests.
    #[inline]
    pub fn sq_sig_all(mut self, sq_sig_all: bool) -> Self {
        self.sq_sig_all = Some(sq_sig_all);
        self
    }

    /// Build the queue pair.
    ///
    /// # Panics
    ///
    /// Panic if any mandatory fields (except QP capabilities) are not set.
    #[inline]
    pub fn build(self, pd: &Pd) -> Result<Qp, QpCreationError> {
        Qp::new(pd, self)
    }
}

impl Default for QpBuilder<'_> {
    fn default() -> Self {
        Self::new()
    }
}

/// [`QpBuilder`] with mandatory fields.
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
}

impl From<&'_ QpInitAttr> for ibv_qp_init_attr {
    /// Translate the initialization attributes into [`ibv_qp_init_attr`].
    fn from(value: &QpInitAttr) -> Self {
        ibv_qp_init_attr {
            send_cq: value.send_cq.as_raw(),
            recv_cq: value.recv_cq.as_raw(),
            cap: ibv_qp_cap {
                max_send_wr: value.caps.max_send_wr,
                max_recv_wr: value.caps.max_recv_wr,
                max_send_sge: value.caps.max_send_sge,
                max_recv_sge: value.caps.max_recv_sge,
                max_inline_data: value.caps.max_inline_data,
            },
            qp_type: u32::from(value.qp_type),
            sq_sig_all: value.sq_sig_all as i32,
            ..(unsafe { mem::zeroed() })
        }
    }
}
