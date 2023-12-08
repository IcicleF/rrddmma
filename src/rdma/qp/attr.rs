use std::ptr;

use super::QpType;
use crate::rdma::cq::Cq;

use crate::bindings::*;

/// Queue pair capability attributes.
///
/// This type corresponds to `struct ibv_qp_cap` in the `ibverbs` C driver.
///
/// The documentation is heavily borrowed from [RDMAmojo](https://www.rdmamojo.com/2012/12/21/ibv_create_qp/).
/// My biggest thanks to the author, Dotan Barak.
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

/// Generate a default RDMA queue pair capabilities setting.
/// The queue pair can:
/// - maintain up to 128 outstanding send/recv work requests each,
/// - set a SGE of up to 16 entries per send/recv work request, and
/// - send up to 64 bytes of inline data.
impl Default for QpCaps {
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
    pub fn new(
        max_send_wr: u32,
        max_recv_wr: u32,
        max_send_sge: u32,
        max_recv_sge: u32,
        max_inline_data: u32,
    ) -> Self {
        QpCaps {
            max_send_wr,
            max_recv_wr,
            max_send_sge,
            max_recv_sge,
            max_inline_data,
        }
    }
}

/// Queue pair initialization attributes.
#[derive(Debug, Clone)]
pub struct QpInitAttr {
    /// Send completion queue for this QP.
    pub send_cq: Cq,

    /// Receive completion queue for this QP. Can be the same to send CQ.
    pub recv_cq: Cq,

    /// Capabilities of this QP.
    pub cap: QpCaps,

    /// Queue pair type.
    pub qp_type: QpType,

    /// Whether to signal for all send work requests.
    pub sq_sig_all: bool,
}

impl QpInitAttr {
    /// Generate a set of queue pair initialization attributes.
    pub fn new(send_cq: Cq, recv_cq: Cq, cap: QpCaps, qp_type: QpType, sq_sig_all: bool) -> Self {
        QpInitAttr {
            send_cq,
            recv_cq,
            cap,
            qp_type,
            sq_sig_all,
        }
    }

    /// Generate default RC queue pair initialization attributes, in which:
    ///
    /// - the send CQ and the receive CQ are the same,
    /// - QP capabilities are set to the default values, and
    /// - work requests are NOT signaled by default.
    pub fn default_rc(cq: Cq) -> Self {
        QpInitAttr {
            send_cq: cq.clone(),
            recv_cq: cq,
            cap: QpCaps::default(),
            qp_type: QpType::Rc,
            sq_sig_all: false,
        }
    }

    /// Generate default UD queue pair initialization attributes, in which:
    ///
    /// - the send CQ and the receive CQ are the same,
    /// - QP capabilities are set to the default values, and
    /// - work requests are NOT signaled by default.
    pub fn default_ud(cq: Cq) -> Self {
        QpInitAttr {
            send_cq: cq.clone(),
            recv_cq: cq,
            cap: QpCaps::default(),
            qp_type: QpType::Ud,
            sq_sig_all: false,
        }
    }

    /// Translate the initialization attributes into [`ibv_qp_init_attr`].
    #[cfg(mlnx4)]
    pub(crate) fn to_actual_init_attr(&self) -> ibv_qp_init_attr {
        ibv_qp_init_attr {
            qp_context: ptr::null_mut(),
            send_cq: self.send_cq.as_raw(),
            recv_cq: self.recv_cq.as_raw(),
            srq: ptr::null_mut(),
            cap: ibv_qp_cap {
                max_send_wr: self.cap.max_send_wr,
                max_recv_wr: self.cap.max_recv_wr,
                max_send_sge: self.cap.max_send_sge,
                max_recv_sge: self.cap.max_recv_sge,
                max_inline_data: self.cap.max_inline_data,
            },
            qp_type: u32::from(self.qp_type),
            sq_sig_all: self.sq_sig_all as i32,
            xrc_domain: ptr::null_mut(),
        }
    }

    /// Translate the initialization attributes into [`ibv_qp_init_attr`].
    #[cfg(mlnx5)]
    pub(crate) fn to_actual_init_attr(&self) -> ibv_qp_init_attr {
        ibv_qp_init_attr {
            qp_context: ptr::null_mut(),
            send_cq: self.send_cq.as_raw(),
            recv_cq: self.recv_cq.as_raw(),
            srq: ptr::null_mut(),
            cap: ibv_qp_cap {
                max_send_wr: self.cap.max_send_wr,
                max_recv_wr: self.cap.max_recv_wr,
                max_send_sge: self.cap.max_send_sge,
                max_recv_sge: self.cap.max_recv_sge,
                max_inline_data: self.cap.max_inline_data,
            },
            qp_type: u32::from(self.qp_type),
            sq_sig_all: self.sq_sig_all as i32,
        }
    }
}
