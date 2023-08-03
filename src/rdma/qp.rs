use std::ptr::NonNull;
use std::sync::Arc;
use std::{fmt, io, mem, ptr};

use super::context::Context;
use super::cq::Cq;
use super::gid::Gid;
use super::mr::*;
use super::pd::Pd;
use super::remote_mem::*;
use super::types::*;
use super::wr::*;
use crate::utils::{interop::*, select::*};

use anyhow::{Context as _, Result};
use libc;
use rdma_sys::*;

/// Queue pair type.
#[derive(fmt::Debug, Clone, Copy, PartialEq, Eq)]
pub enum QpType {
    /// Reliable connection
    Rc,
    /// Unreliable datagram
    Ud,
}

impl From<QpType> for u32 {
    fn from(qp_type: QpType) -> Self {
        match qp_type {
            QpType::Rc => ibv_qp_type::IBV_QPT_RC,
            QpType::Ud => ibv_qp_type::IBV_QPT_UD,
        }
    }
}

impl From<u32> for QpType {
    fn from(qp_type: u32) -> Self {
        match qp_type {
            ibv_qp_type::IBV_QPT_RC => QpType::Rc,
            ibv_qp_type::IBV_QPT_UD => QpType::Ud,
            _ => panic!("invalid qp type"),
        }
    }
}

/// Queue pair state.
#[derive(fmt::Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QpState {
    /// Reset.
    Reset = ibv_qp_state::IBV_QPS_RESET as _,
    /// Initialized.
    Init = ibv_qp_state::IBV_QPS_INIT as _,
    /// Ready To Receive.
    Rtr = ibv_qp_state::IBV_QPS_RTR as _,
    /// Ready To Send.
    Rts = ibv_qp_state::IBV_QPS_RTS as _,
    /// Send Queue Drain.
    Sqd = ibv_qp_state::IBV_QPS_SQD as _,
    /// Send Queue Error.
    Sqe = ibv_qp_state::IBV_QPS_SQE as _,
    /// Error.
    Error = ibv_qp_state::IBV_QPS_ERR as _,
    /// Unknown.
    Unknown = ibv_qp_state::IBV_QPS_UNKNOWN as _,
}

impl From<u32> for QpState {
    fn from(qp_state: u32) -> Self {
        match qp_state {
            ibv_qp_state::IBV_QPS_RESET => QpState::Reset,
            ibv_qp_state::IBV_QPS_INIT => QpState::Init,
            ibv_qp_state::IBV_QPS_RTR => QpState::Rtr,
            ibv_qp_state::IBV_QPS_RTS => QpState::Rts,
            ibv_qp_state::IBV_QPS_SQD => QpState::Sqd,
            ibv_qp_state::IBV_QPS_SQE => QpState::Sqe,
            ibv_qp_state::IBV_QPS_ERR => QpState::Error,
            ibv_qp_state::IBV_QPS_UNKNOWN => QpState::Unknown,
            x => panic!("invalid QP state: {}", x),
        }
    }
}

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

/// Endpoint (NIC port & queue pair) data.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct QpEndpoint {
    pub gid: Gid,
    pub port_num: PortNum,
    pub lid: Lid,
    pub qpn: Qpn,
    pub psn: Psn,
    pub qkey: QKey,
}

impl QpEndpoint {
    pub fn new(gid: Gid, port_num: PortNum, lid: Lid, qpn: Qpn, psn: Psn, qkey: QKey) -> Self {
        QpEndpoint {
            gid,
            port_num,
            lid,
            qpn,
            psn,
            qkey,
        }
    }
}

/// Peer queue pair information that can be used in sends.
pub struct QpPeer {
    pub ah: NonNull<ibv_ah>,
    pub ep: QpEndpoint,
}

unsafe impl Sync for QpPeer {}

impl fmt::Debug for QpPeer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QpPeer")
            .field("endpoint", &self.ep)
            .finish()
    }
}

impl QpPeer {
    pub fn new(pd: Pd, ep: QpEndpoint) -> Result<Self> {
        let mut ah_attr = ibv_ah_attr {
            grh: ibv_global_route {
                dgid: ibv_gid::from(ep.gid),
                flow_label: 0,
                sgid_index: pd.context().gid_index(),
                hop_limit: 0xFF,
                traffic_class: 0,
            },
            is_global: 1,
            dlid: ep.lid,
            sl: 0,
            src_path_bits: 0,
            static_rate: 0,
            port_num: ep.port_num,
        };
        let ah = NonNull::new(unsafe { ibv_create_ah(pd.as_raw(), &mut ah_attr) })
            .ok_or(anyhow::anyhow!(io::Error::last_os_error()))
            .with_context(|| "failed to create address handle")?;
        Ok(QpPeer { ah, ep })
    }

    /// Generate a [`ud_t`] instance for RDMA sends to this peer.
    #[inline]
    pub(crate) fn as_ud_t(&self) -> ud_t {
        ud_t {
            ah: self.ah.as_ptr(),
            remote_qpn: self.ep.qpn,
            remote_qkey: self.ep.qkey,
        }
    }
}

impl Drop for QpPeer {
    fn drop(&mut self) {
        unsafe { ibv_destroy_ah(self.ah.as_ptr()) };
    }
}

struct QpInner {
    pd: Pd,
    qp: NonNull<ibv_qp>,
    init_attr: QpInitAttr,
}

unsafe impl Send for QpInner {}
unsafe impl Sync for QpInner {}

impl Drop for QpInner {
    fn drop(&mut self) {
        unsafe { ibv_destroy_qp(self.qp.as_ptr()) };
    }
}

impl PartialEq for QpInner {
    fn eq(&self, other: &Self) -> bool {
        self.qp.as_ptr() == other.qp.as_ptr()
    }
}

impl Eq for QpInner {}

/// Queue pair.
///
/// This type is a simple wrapper of an `Arc` and is guaranteed to have the
/// same memory layout with it.
#[derive(Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct Qp {
    inner: Arc<QpInner>,
}

impl fmt::Debug for Qp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("Qp<{:p}>", self.as_raw()))
    }
}

impl Qp {
    /// Create a new queue pair with the given initialization attributes.
    pub fn new(pd: Pd, init_attr: QpInitAttr) -> Result<Self> {
        let qp = NonNull::new(unsafe {
            let mut init_attr = init_attr.to_actual_init_attr();
            ibv_create_qp(pd.as_raw(), &mut init_attr)
        })
        .ok_or(anyhow::anyhow!(io::Error::last_os_error()))
        .with_context(|| "failed to create queue pair")?;
        Ok(Qp {
            inner: Arc::new(QpInner { pd, qp, init_attr }),
        })
    }

    /// Get the underlying `ibv_qp` pointer.
    #[inline]
    pub fn as_raw(&self) -> *mut ibv_qp {
        self.inner.qp.as_ptr()
    }

    /// Get the protection domain of the queue pair.
    #[inline]
    pub fn pd(&self) -> Pd {
        self.inner.pd.clone()
    }

    /// Get the context of the queue pair.
    #[inline]
    pub fn context(&self) -> Context {
        self.inner.pd.context()
    }

    /// Get the type of the queue pair.
    #[inline]
    pub fn qp_type(&self) -> QpType {
        let ty = unsafe { (*self.inner.qp.as_ptr()).qp_type };
        match ty {
            ibv_qp_type::IBV_QPT_RC => QpType::Rc,
            ibv_qp_type::IBV_QPT_UD => QpType::Ud,
            _ => panic!("unknown qp type"),
        }
    }

    /// Get the queue pair number.
    #[inline]
    pub(crate) fn qp_num(&self) -> u32 {
        unsafe { (*self.inner.qp.as_ptr()).qp_num }
    }

    /// Get the current state of the queue pair.
    #[inline]
    pub fn state(&self) -> QpState {
        let state = unsafe { (*self.inner.qp.as_ptr()).state };
        match state {
            ibv_qp_state::IBV_QPS_RESET => QpState::Reset,
            ibv_qp_state::IBV_QPS_INIT => QpState::Init,
            ibv_qp_state::IBV_QPS_RTR => QpState::Rtr,
            ibv_qp_state::IBV_QPS_RTS => QpState::Rts,
            ibv_qp_state::IBV_QPS_ERR => QpState::Error,
            _ => panic!("unknown qp state"),
        }
    }

    /// Get the capabilities of this QP.
    #[inline]
    pub fn caps(&self) -> &QpCaps {
        &self.inner.init_attr.cap
    }

    /// Get endpoint information of this QP.
    #[inline]
    pub fn endpoint(&self) -> QpEndpoint {
        const GLOBAL_QKEY: u32 = 0x11111111;
        QpEndpoint {
            gid: self.context().gid(),
            port_num: self.context().port_num(),
            lid: self.context().lid(),
            qpn: self.qp_num(),
            psn: 0,
            qkey: GLOBAL_QKEY,
        }
    }

    /// Get the associated send completion queue.
    #[inline]
    pub fn scq(&self) -> &Cq {
        &self.inner.init_attr.send_cq
    }

    /// Get the associated receive completion queue.
    #[inline]
    pub fn rcq(&self) -> &Cq {
        &self.inner.init_attr.recv_cq
    }

    fn modify_reset_to_init(&self, ep: &QpEndpoint) -> Result<()> {
        let mut attr = unsafe { mem::zeroed::<ibv_qp_attr>() };
        let mut attr_mask = ibv_qp_attr_mask::IBV_QP_STATE
            | ibv_qp_attr_mask::IBV_QP_PKEY_INDEX
            | ibv_qp_attr_mask::IBV_QP_PORT;
        attr.qp_state = ibv_qp_state::IBV_QPS_INIT;
        attr.pkey_index = 0;
        attr.port_num = ep.port_num;

        if self.qp_type() == QpType::Rc {
            attr.qp_access_flags = (ibv_access_flags::IBV_ACCESS_REMOTE_WRITE
                | ibv_access_flags::IBV_ACCESS_REMOTE_READ
                | ibv_access_flags::IBV_ACCESS_REMOTE_ATOMIC)
                .0;
            attr_mask = attr_mask | ibv_qp_attr_mask::IBV_QP_ACCESS_FLAGS;
        } else {
            attr_mask = attr_mask | ibv_qp_attr_mask::IBV_QP_QKEY;
            attr.qkey = ep.qkey;
        }

        let ret = unsafe { ibv_modify_qp(self.inner.qp.as_ptr(), &mut attr, attr_mask.0 as i32) };
        from_c_ret(ret)
    }

    fn modify_init_to_rtr(&self, ep: &QpEndpoint) -> Result<()> {
        let mut attr = unsafe { mem::zeroed::<ibv_qp_attr>() };
        let mut attr_mask = ibv_qp_attr_mask::IBV_QP_STATE;
        attr.qp_state = ibv_qp_state::IBV_QPS_RTR;

        if self.qp_type() == QpType::Rc {
            let ctx = self.inner.pd.context();

            attr.path_mtu = ctx.mtu_raw();
            attr.dest_qp_num = ep.qpn;
            attr.rq_psn = ep.psn;
            attr.max_dest_rd_atomic = 16;
            attr.min_rnr_timer = 12;

            attr.ah_attr.grh.dgid = ibv_gid::from(ep.gid);
            attr.ah_attr.grh.flow_label = 0;
            attr.ah_attr.grh.sgid_index = ctx.gid_index();
            attr.ah_attr.grh.hop_limit = 0xFF;
            attr.ah_attr.grh.traffic_class = 0;
            attr.ah_attr.dlid = ep.lid;
            attr.ah_attr.sl = 0;
            attr.ah_attr.src_path_bits = 0;
            attr.ah_attr.port_num = ctx.port_num();
            attr.ah_attr.is_global = 1;

            attr_mask = attr_mask
                | ibv_qp_attr_mask::IBV_QP_AV
                | ibv_qp_attr_mask::IBV_QP_PATH_MTU
                | ibv_qp_attr_mask::IBV_QP_DEST_QPN
                | ibv_qp_attr_mask::IBV_QP_RQ_PSN
                | ibv_qp_attr_mask::IBV_QP_MAX_DEST_RD_ATOMIC
                | ibv_qp_attr_mask::IBV_QP_MIN_RNR_TIMER;
        }

        let ret = unsafe { ibv_modify_qp(self.inner.qp.as_ptr(), &mut attr, attr_mask.0 as i32) };
        from_c_ret(ret)
    }

    fn modify_rtr_to_rts(&self, ep: &QpEndpoint) -> Result<()> {
        let mut attr = unsafe { mem::zeroed::<ibv_qp_attr>() };
        let mut attr_mask = ibv_qp_attr_mask::IBV_QP_STATE | ibv_qp_attr_mask::IBV_QP_SQ_PSN;
        attr.qp_state = ibv_qp_state::IBV_QPS_RTS;
        attr.sq_psn = ep.psn;

        if self.qp_type() == QpType::Rc {
            attr.max_rd_atomic = 16;
            attr.timeout = 14;
            attr.retry_cnt = 6;
            attr.rnr_retry = 6;
            attr_mask = attr_mask
                | ibv_qp_attr_mask::IBV_QP_MAX_QP_RD_ATOMIC
                | ibv_qp_attr_mask::IBV_QP_TIMEOUT
                | ibv_qp_attr_mask::IBV_QP_RETRY_CNT
                | ibv_qp_attr_mask::IBV_QP_RNR_RETRY;
        }

        let ret = unsafe { ibv_modify_qp(self.inner.qp.as_ptr(), &mut attr, attr_mask.0 as i32) };
        from_c_ret(ret)
    }

    /// Establish connection with the remote endpoint.
    /// If the QP type is UD, only modify the QP to RTS.
    pub fn connect(&self, ep: &QpEndpoint) -> Result<()> {
        if self.state() == QpState::Reset {
            self.modify_reset_to_init(ep)?;
        }
        if self.state() == QpState::Init {
            self.modify_init_to_rtr(ep)?;
        }
        if self.state() == QpState::Rtr {
            self.modify_rtr_to_rts(ep)?;
        }
        Ok(())
    }

    /// Explain [`ibv_post_recv`] errors, for internal use.
    fn recv_err_explanation(ret: i32) -> Option<&'static str> {
        match ret {
            libc::EINVAL => Some("invalid work request"),
            libc::ENOMEM => {
                Some("recv queue is full, or not enough resources to complete this operation")
            }
            libc::EFAULT => Some("invalid QP"),
            _ => None,
        }
    }

    /// Post a RDMA recv request.
    ///
    /// **NOTE:** This method has no mutable borrows to its parameters, but can
    /// cause the content of the buffers to be modified!
    pub fn recv(&self, local: &[MrSlice], wr_id: u64) -> Result<()> {
        let mut sgl = build_sgl(local);
        let mut wr = ibv_recv_wr {
            wr_id,
            next: ptr::null_mut(),
            sg_list: if local.len() == 0 {
                ptr::null_mut()
            } else {
                sgl.as_mut_ptr()
            },
            num_sge: local.len() as i32,
        };
        let ret = unsafe {
            let mut bad_wr = ptr::null_mut();
            ibv_post_recv(self.inner.qp.as_ptr(), &mut wr, &mut bad_wr)
        };
        from_c_ret_explained(ret, Self::recv_err_explanation)
    }

    /// Post a list of receive work requests to the queue pair in order.
    /// This enables doorbell batching and can reduce doorbell ringing overheads.
    #[inline]
    pub fn post_recv(&self, ops: &[RecvWr]) -> Result<()> {
        if ops.len() == 0 {
            return Ok(());
        }

        // Safety: we only hold references to the `RecvWr`s, whose lifetimes
        // can only outlive this function. `ibv_post_recv` is used inside this
        // function, so the work requests are guaranteed to be valid.
        let mut wrs = ops.iter().map(|op| op.to_wr()).collect::<Vec<_>>();
        for i in 0..(wrs.len() - 1) {
            wrs[i].next = &mut wrs[i + 1];
        }

        let ret = unsafe {
            let mut bad_wr = ptr::null_mut();
            ibv_post_recv(self.inner.qp.as_ptr(), wrs.as_mut_ptr(), &mut bad_wr)
        };
        from_c_ret_explained(ret, Self::recv_err_explanation)
    }

    /// Post a list of raw receive work requests.
    /// This enables doorbell batching and can reduce doorbell ringing overheads.
    ///
    /// ### Safety
    ///
    /// - Every work request must refer to valid memory address.
    /// - `head` must lead a valid chain of work requests of valid length.
    #[inline]
    pub unsafe fn post_raw_recv(&self, head: &RawRecvWr) -> Result<()> {
        let ret = unsafe {
            let mut bad_wr = ptr::null_mut();
            ibv_post_recv(
                self.inner.qp.as_ptr(),
                &head as *const _ as *mut _,
                &mut bad_wr,
            )
        };
        from_c_ret_explained(ret, Self::recv_err_explanation)
    }

    /// Explain [`ibv_post_send`] errors, for internal use.
    fn send_err_explanation(ret: i32) -> Option<&'static str> {
        match ret {
            libc::EINVAL => Some("invalid work request"),
            libc::ENOMEM => {
                Some("send queue is full, or not enough resources to complete this operation")
            }
            libc::EFAULT => Some("invalid QP"),
            _ => None,
        }
    }

    /// Post an RDMA send request.
    ///
    /// If `peer` is `None`, this QP is expected to be connected and the send
    /// will go to the remote end of the connection. Otherwise, this QP is
    /// expected to be UD and the send will go to the specified peer.
    ///
    /// **NOTE:** this function is only equivalent to calling `ibv_post_send`.
    /// It is the caller's responsibility to ensure the completion of the send
    /// by some means, for example by polling the send CQ.
    pub fn send(
        &self,
        local: &[MrSlice],
        peer: Option<&QpPeer>,
        imm: Option<ImmData>,
        wr_id: WrId,
        signal: bool,
        inline: bool,
    ) -> Result<()> {
        if !signal && self.inner.init_attr.sq_sig_all {
            log::warn!("QP configured to signal all sends despite this send ask to not signal");
        }

        let mut sgl = build_sgl(local);
        let mut wr = unsafe { mem::zeroed::<ibv_send_wr>() };
        wr = ibv_send_wr {
            wr_id,
            next: ptr::null_mut(),
            sg_list: if local.len() == 0 {
                ptr::null_mut()
            } else {
                sgl.as_mut_ptr()
            },
            num_sge: local.len() as i32,
            opcode: imm.is_none().select_val(
                ibv_wr_opcode::IBV_WR_SEND,
                ibv_wr_opcode::IBV_WR_SEND_WITH_IMM,
            ),
            send_flags: signal.select_val(ibv_send_flags::IBV_SEND_SIGNALED.0, 0)
                | inline.select_val(ibv_send_flags::IBV_SEND_INLINE.0, 0),
            imm_data_invalidated_rkey_union: imm_data_invalidated_rkey_union_t {
                imm_data: imm.unwrap_or(0),
            },
            ..wr
        };
        if let Some(peer) = peer {
            wr.wr.ud = peer.as_ud_t();
        }
        let ret = unsafe {
            let mut bad_wr = ptr::null_mut();
            ibv_post_send(self.inner.qp.as_ptr(), &mut wr, &mut bad_wr)
        };
        from_c_ret_explained(ret, Self::send_err_explanation)
    }

    /// Post an RDMA read request.
    /// Only valid for RC QPs.
    ///
    /// **NOTE:** this function is only equivalent to calling `ibv_post_send`.
    /// It is the caller's responsibility to ensure the completion of the write
    /// by some means, for example by polling the send CQ. Also, this method has
    /// no mutable borrows to its parameters, but can cause the content of the
    /// buffers to be modified!
    pub fn read(
        &self,
        local: &[MrSlice],
        remote: &RemoteMem,
        wr_id: WrId,
        signal: bool,
    ) -> Result<()> {
        if !signal && self.inner.init_attr.sq_sig_all {
            log::warn!(
                "QP configured to signal all sends despite this RDMA read ask to not signal"
            );
        }

        let mut sgl = build_sgl(local);
        let mut wr = unsafe { mem::zeroed::<ibv_send_wr>() };
        wr = ibv_send_wr {
            wr_id,
            next: ptr::null_mut(),
            sg_list: if local.len() == 0 {
                ptr::null_mut()
            } else {
                sgl.as_mut_ptr()
            },
            num_sge: local.len() as i32,
            opcode: ibv_wr_opcode::IBV_WR_RDMA_READ,
            send_flags: signal.select_val(ibv_send_flags::IBV_SEND_SIGNALED.0, 0),
            wr: wr_t {
                rdma: remote.as_rdma_t(),
            },
            ..wr
        };
        let ret = unsafe {
            let mut bad_wr = ptr::null_mut();
            ibv_post_send(self.inner.qp.as_ptr(), &mut wr, &mut bad_wr)
        };
        from_c_ret_explained(ret, Self::send_err_explanation)
    }

    /// Post an RDMA write request.
    /// Only valid for RC QPs.
    ///
    /// **NOTE:** this function is only equivalent to calling `ibv_post_send`.
    /// It is the caller's responsibility to ensure the completion of the write
    /// by some means, for example by polling the send CQ.
    pub fn write(
        &self,
        local: &[MrSlice],
        remote: &RemoteMem,
        wr_id: WrId,
        imm: Option<ImmData>,
        signal: bool,
    ) -> Result<()> {
        if !signal && self.inner.init_attr.sq_sig_all {
            log::warn!(
                "QP configured to signal all sends despite this RDMA write ask to not signal"
            );
        }

        let mut sgl = build_sgl(local);
        let mut wr = unsafe { mem::zeroed::<ibv_send_wr>() };
        wr = ibv_send_wr {
            wr_id,
            next: ptr::null_mut(),
            sg_list: if local.len() == 0 {
                ptr::null_mut()
            } else {
                sgl.as_mut_ptr()
            },
            num_sge: local.len() as i32,
            opcode: imm.is_none().select_val(
                ibv_wr_opcode::IBV_WR_RDMA_WRITE,
                ibv_wr_opcode::IBV_WR_RDMA_WRITE_WITH_IMM,
            ),
            send_flags: signal.select_val(ibv_send_flags::IBV_SEND_SIGNALED.0, 0),
            imm_data_invalidated_rkey_union: imm_data_invalidated_rkey_union_t {
                imm_data: imm.unwrap_or(0),
            },
            wr: wr_t {
                rdma: remote.as_rdma_t(),
            },
            ..wr
        };
        let ret = unsafe {
            let mut bad_wr = ptr::null_mut();
            ibv_post_send(self.inner.qp.as_ptr(), &mut wr, &mut bad_wr)
        };
        from_c_ret_explained(ret, Self::send_err_explanation)
    }

    /// Post an RDMA atomic compare-and-swap (CAS) request.
    /// Only valid for RC QPs.
    ///
    /// **NOTE:** this function is only equivalent to calling `ibv_post_send`.
    /// It is the caller's responsibility to ensure the completion of the CAS
    /// by some means, for example by polling the send CQ.
    #[inline]
    pub fn compare_swap(
        &self,
        local: &MrSlice,
        remote: &RemoteMem,
        current: u64,
        new: u64,
        wr_id: WrId,
        signal: bool,
    ) -> Result<()> {
        if !signal && self.inner.init_attr.sq_sig_all {
            log::warn!("QP configured to signal all sends despite this RDMA CAS ask to not signal");
        }

        if local.len() != mem::size_of::<u64>() || remote.len != mem::size_of::<u64>() {
            return Err(anyhow::anyhow!(
                "expected 8B buffers for compare-and-swap, got ({}, {})",
                local.len(),
                remote.len
            ));
        }
        if (local.addr() as u64) % (mem::align_of::<u64>() as u64) != 0
            || remote.addr % (mem::align_of::<u64>() as u64) != 0
        {
            return Err(anyhow::anyhow!(
                "expected 8B-aligned buffers for compare-and-swap, got ({:p}, {:p})",
                local.addr(),
                remote.addr as *const u8
            ));
        }

        let mut sgl = [ibv_sge::from(local.clone())];
        let mut wr = unsafe { mem::zeroed::<ibv_send_wr>() };
        wr = ibv_send_wr {
            wr_id,
            next: ptr::null_mut(),
            sg_list: sgl.as_mut_ptr(),
            num_sge: 1,
            opcode: ibv_wr_opcode::IBV_WR_ATOMIC_CMP_AND_SWP,
            send_flags: signal.select_val(ibv_send_flags::IBV_SEND_SIGNALED.0, 0),
            wr: wr_t {
                atomic: atomic_t {
                    compare_add: current,
                    swap: new,
                    remote_addr: remote.addr,
                    rkey: remote.rkey,
                },
            },
            ..wr
        };
        let ret = unsafe {
            let mut bad_wr = ptr::null_mut();
            ibv_post_send(self.inner.qp.as_ptr(), &mut wr, &mut bad_wr)
        };
        from_c_ret_explained(ret, Self::send_err_explanation)
    }

    /// Post an RDMA fetch-and-add (FAA) request.
    /// Only valid for RC QPs.
    ///
    /// **NOTE:** this function is only equivalent to calling `ibv_post_send`.
    /// It is the caller's responsibility to ensure the completion of the FAA
    /// by some means, for example by polling the send CQ.
    #[inline]
    pub fn fetch_add(
        &self,
        local: &MrSlice,
        remote: &RemoteMem,
        add: u64,
        wr_id: WrId,
        signal: bool,
    ) -> Result<()> {
        if !signal && self.inner.init_attr.sq_sig_all {
            log::warn!("QP configured to signal all sends despite this RDMA FAA ask to not signal");
        }

        if local.len() != mem::size_of::<u64>() || remote.len != mem::size_of::<u64>() {
            return Err(anyhow::anyhow!(
                "expected 8B buffers for fetch-add, got ({}, {})",
                local.len(),
                remote.len
            ));
        }
        if (local.addr() as u64) % (mem::align_of::<u64>() as u64) != 0
            || remote.addr % (mem::align_of::<u64>() as u64) != 0
        {
            return Err(anyhow::anyhow!(
                "expected 8B-aligned buffers for compare-and-swap, got ({:p}, {:p})",
                local.addr(),
                remote.addr as *const u8
            ));
        }

        let mut sgl = [ibv_sge::from(local.clone())];
        let mut wr = unsafe { mem::zeroed::<ibv_send_wr>() };
        wr = ibv_send_wr {
            wr_id,
            next: ptr::null_mut(),
            sg_list: sgl.as_mut_ptr(),
            num_sge: 1,
            opcode: ibv_wr_opcode::IBV_WR_ATOMIC_FETCH_AND_ADD,
            send_flags: signal.select_val(ibv_send_flags::IBV_SEND_SIGNALED.0, 0),
            wr: wr_t {
                atomic: atomic_t {
                    compare_add: add,
                    swap: 0,
                    remote_addr: remote.addr,
                    rkey: remote.rkey,
                },
            },
            ..wr
        };
        let ret = unsafe {
            let mut bad_wr = ptr::null_mut();
            ibv_post_send(self.inner.qp.as_ptr(), &mut wr, &mut bad_wr)
        };
        from_c_ret_explained(ret, Self::send_err_explanation)
    }

    /// Post a list of send work requests to the queue pair in order.
    /// This enables doorbell batching and can reduce doorbell ringing overheads.
    #[inline]
    pub fn post_send(&self, ops: &[SendWr<'_>]) -> Result<()> {
        if ops.len() == 0 {
            return Ok(());
        }

        if ops.iter().any(|wr| !wr.is_signaled()) && self.inner.init_attr.sq_sig_all {
            log::warn!(
                "QP configured to signal all sends despite some work requests ask to not signal"
            );
        }

        // Safety: we only hold references to the `SendWr`s, whose lifetimes
        // can only outlive this function. `ibv_post_send` is used inside this
        // function, so the work requests are guaranteed to be valid.
        let mut wrs = ops.iter().map(|op| op.to_wr()).collect::<Vec<_>>();
        for i in 0..(wrs.len() - 1) {
            unsafe {
                wrs.get_unchecked_mut(i).next = wrs.get_unchecked_mut(i + 1);
            }
        }

        let mut bad_wr = ptr::null_mut();
        let ret = unsafe { ibv_post_send(self.inner.qp.as_ptr(), wrs.as_mut_ptr(), &mut bad_wr) };
        from_c_ret_explained(ret, Self::send_err_explanation).with_context(|| {
            let failed = wrs
                .iter()
                .enumerate()
                .filter(|(_, wr)| (*wr) as *const _ == bad_wr)
                .next();
            match failed {
                Some((i, _)) => format!("failed at send work request #{}", i),
                None => "failed at unknown send work request".to_string(),
            }
        })
    }

    /// Post a list of raw send work requests.
    /// This enables doorbell batching and can reduce doorbell ringing overheads.
    ///
    /// ### Safety
    ///
    /// - Every work request must refer to valid memory address.
    /// - `wr_head` must lead a valid chain of work requests of valid length.
    #[inline]
    pub unsafe fn post_raw_send(&self, head: &RawSendWr) -> Result<()> {
        let ret = unsafe {
            let mut bad_wr = ptr::null_mut();
            ibv_post_send(
                self.inner.qp.as_ptr(),
                &head as *const _ as *mut _,
                &mut bad_wr,
            )
        };
        from_c_ret_explained(ret, Self::send_err_explanation)
    }
}

#[inline]
pub(crate) fn build_sgl(slices: &[MrSlice]) -> Vec<ibv_sge> {
    slices
        .iter()
        .map(|slice| ibv_sge::from(slice.clone()))
        .collect()
}
