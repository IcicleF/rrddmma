//! Queue pair and related types.

mod builder;
mod peer;
mod state;
mod ty;

use std::io::{self, Error as IoError, ErrorKind as IoErrorKind};
use std::ptr::NonNull;
use std::sync::Arc;
use std::{fmt, mem, ptr};

use thiserror::Error;

pub use self::builder::*;
pub use self::peer::*;
pub use self::state::*;
pub use self::ty::*;
use crate::bindings::*;
use crate::rdma::{context::Context, cq::Cq, mr::*, nic::*, pd::Pd, type_alias::*};
use crate::utils::{interop::*, select::*};

/// Wrapper for `*mut ibv_qp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub(crate) struct IbvQp(NonNull<ibv_qp>);

impl IbvQp {
    /// Destroy the QP.
    ///
    /// # Safety
    ///
    /// - A QP must not be destroyed more than once.
    /// - Destroyed QPs must not be used anymore.
    pub unsafe fn destroy(self) -> io::Result<()> {
        // SAFETY: FFI.
        let ret = ibv_destroy_qp(self.as_ptr());
        from_c_ret(ret)
    }

    /// Get the QP type.
    ///
    /// # Panics
    ///
    /// Panic if the QP contains an unknown type, which shouldn't happen.
    #[inline]
    pub fn qp_type(&self) -> QpType {
        // SAFETY: `self` points to a valid `ibv_qp` instance.
        let ty = unsafe { (*self.as_ptr()).qp_type };
        ty.into()
    }

    /// Get the QP number.
    #[inline]
    pub fn qp_num(&self) -> u32 {
        // SAFETY: `self` points to a valid `ibv_qp` instance.
        unsafe { (*self.as_ptr()).qp_num }
    }

    /// Get the QP state.
    #[inline]
    pub fn qp_state(&self) -> QpState {
        // SAFETY: `self` points to a valid `ibv_qp` instance.
        let state = unsafe { (*self.as_ptr()).state };
        state.into()
    }
}

impl_ibv_wrapper_traits!(ibv_qp, IbvQp);

/// Queue pair creation error type.
#[derive(Debug, Error)]
pub enum QpCreationError {
    /// `libibverbs` interfaces returned an error.
    #[error("I/O error from ibverbs")]
    IoError(#[from] io::Error),

    /// Specified capabilities are not supported by the device.
    /// The three fields are for the capability name, the maximum supported
    /// value, and the required value.
    #[error("capability not enough: {0} supports up to {1}, {2} required")]
    CapabilityNotEnough(String, u32, u32),
}

/// Ownership holder of queue pair.
struct QpInner {
    pd: Pd,
    qp: IbvQp,
    init_attr: QpInitAttr,
}

impl Drop for QpInner {
    fn drop(&mut self) {
        // SAFETY: call only once, and no UAF since I will be dropped.
        unsafe { self.qp.destroy() }.expect("cannot destroy QP on drop");
    }
}

/// Queue pair.
pub struct Qp {
    /// Cached queue pair pointer.
    qp: IbvQp,

    /// Queue pair body.
    inner: Arc<QpInner>,

    /// Local port that this QP is bound to.
    local_port: Option<(Port, GidIndex)>,

    /// Remote peer that this QP is connected to.
    peer: Option<QpPeer>,
}

impl fmt::Debug for Qp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("Qp<{:p}>", self.as_raw()))
    }
}

impl Qp {
    /// Check whether the given capabilities are supported by the device.
    fn check_caps(ctx: &Context, caps: &QpCaps) -> Result<(), QpCreationError> {
        let attr = ctx.attr();
        if caps.max_send_wr > attr.max_qp_wr as _ {
            return Err(QpCreationError::CapabilityNotEnough(
                "max_send_wr".to_string(),
                attr.max_qp_wr as _,
                caps.max_send_wr,
            ));
        }
        if caps.max_recv_wr > attr.max_qp_wr as _ {
            return Err(QpCreationError::CapabilityNotEnough(
                "max_recv_wr".to_string(),
                attr.max_qp_wr as _,
                caps.max_recv_wr,
            ));
        }
        if caps.max_send_sge > attr.max_sge as _ {
            return Err(QpCreationError::CapabilityNotEnough(
                "max_send_sge".to_string(),
                attr.max_sge as _,
                caps.max_send_sge,
            ));
        }
        if caps.max_recv_sge > attr.max_sge as _ {
            return Err(QpCreationError::CapabilityNotEnough(
                "max_recv_sge".to_string(),
                attr.max_sge as _,
                caps.max_recv_sge,
            ));
        }
        Ok(())
    }

    /// Create a new queue pair with the given initialization attributes.
    pub(crate) fn new(pd: &Pd, init_attr: QpBuilder<'_>) -> Result<Self, QpCreationError> {
        let init_attr = init_attr.unwrap();
        Self::check_caps(pd.context(), &init_attr.caps)?;

        let qp = unsafe {
            let mut init_attr = (&init_attr).into();
            ibv_create_qp(pd.as_raw(), &mut init_attr)
        };
        let qp = NonNull::new(qp).ok_or_else(io::Error::last_os_error)?;
        let qp = IbvQp(qp);

        let qp = Qp {
            inner: Arc::new(QpInner {
                pd: pd.clone(),
                qp,
                init_attr,
            }),
            qp,
            local_port: None,
            peer: None,
        };
        Ok(qp)
    }

    /// Modify the queue pair to RESET.
    fn modify_2reset(&self) -> io::Result<()> {
        // SAFETY: POD type.
        let mut attr = unsafe { mem::zeroed::<ibv_qp_attr>() };
        let attr_mask = ibv_qp_attr_mask::IBV_QP_STATE;
        attr.qp_state = ibv_qp_state::IBV_QPS_RESET;

        // SAFETY: FFI.
        let ret = unsafe { ibv_modify_qp(self.as_raw(), &mut attr, attr_mask.0 as i32) };
        from_c_ret(ret)
    }

    /// Modify the queue pair from RESET to INIT.
    fn modify_reset2init(&self) -> io::Result<()> {
        // SAFETY: POD type.
        let mut attr = unsafe { mem::zeroed::<ibv_qp_attr>() };
        let mut attr_mask = ibv_qp_attr_mask::IBV_QP_STATE
            | ibv_qp_attr_mask::IBV_QP_PKEY_INDEX
            | ibv_qp_attr_mask::IBV_QP_PORT;
        attr.qp_state = ibv_qp_state::IBV_QPS_INIT;
        attr.pkey_index = 0;
        attr.port_num = self.local_port.as_ref().unwrap().0.num();

        if self.qp_type() == QpType::Rc {
            attr.qp_access_flags = (ibv_access_flags::IBV_ACCESS_REMOTE_WRITE
                | ibv_access_flags::IBV_ACCESS_REMOTE_READ
                | ibv_access_flags::IBV_ACCESS_REMOTE_ATOMIC)
                .0 as _;
            attr_mask |= ibv_qp_attr_mask::IBV_QP_ACCESS_FLAGS;
        } else {
            attr_mask |= ibv_qp_attr_mask::IBV_QP_QKEY;
            attr.qkey = Self::GLOBAL_QKEY;
        }

        // SAFETY: FFI.
        let ret = unsafe { ibv_modify_qp(self.as_raw(), &mut attr, attr_mask.0 as i32) };
        from_c_ret(ret)
    }

    /// Modify the queue pair from INIT to RTR.
    fn modify_init2rtr(&self) -> io::Result<()> {
        // SAFETY: POD type.
        let mut attr = unsafe { mem::zeroed::<ibv_qp_attr>() };
        let mut attr_mask = ibv_qp_attr_mask::IBV_QP_STATE;
        attr.qp_state = ibv_qp_state::IBV_QPS_RTR;

        if self.qp_type() == QpType::Rc {
            let (port, gid_idx) = self.local_port.as_ref().unwrap();
            let peer = self.peer.as_ref().unwrap();
            let gid_idx = *gid_idx;
            let ep = peer.endpoint();

            attr.path_mtu = port.mtu() as _;
            attr.dest_qp_num = ep.qpn;
            attr.rq_psn = Self::GLOBAL_INIT_PSN;
            attr.max_dest_rd_atomic = 16;
            attr.min_rnr_timer = 12;

            attr.ah_attr.grh.dgid = ep.gid.into();
            attr.ah_attr.grh.flow_label = 0;
            attr.ah_attr.grh.sgid_index = gid_idx;
            attr.ah_attr.grh.hop_limit = 0xFF;
            attr.ah_attr.grh.traffic_class = 0;
            attr.ah_attr.dlid = ep.lid;
            attr.ah_attr.sl = 0;
            attr.ah_attr.src_path_bits = 0;
            attr.ah_attr.port_num = port.num();
            attr.ah_attr.is_global = 1;

            attr_mask |= ibv_qp_attr_mask::IBV_QP_AV
                | ibv_qp_attr_mask::IBV_QP_PATH_MTU
                | ibv_qp_attr_mask::IBV_QP_DEST_QPN
                | ibv_qp_attr_mask::IBV_QP_RQ_PSN
                | ibv_qp_attr_mask::IBV_QP_MAX_DEST_RD_ATOMIC
                | ibv_qp_attr_mask::IBV_QP_MIN_RNR_TIMER;
        }

        // SAFETY: FFI.
        let ret = unsafe { ibv_modify_qp(self.as_raw(), &mut attr, attr_mask.0 as i32) };
        from_c_ret(ret)
    }

    /// Modify the queue pair from RTR to RTS.
    fn modify_rtr2rts(&self) -> io::Result<()> {
        // SAFETY: POD type.
        let mut attr = unsafe { mem::zeroed::<ibv_qp_attr>() };
        let mut attr_mask = ibv_qp_attr_mask::IBV_QP_STATE | ibv_qp_attr_mask::IBV_QP_SQ_PSN;
        attr.qp_state = ibv_qp_state::IBV_QPS_RTS;
        attr.sq_psn = Self::GLOBAL_INIT_PSN;

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

        // SAFETY: FFI.
        let ret = unsafe { ibv_modify_qp(self.as_raw(), &mut attr, attr_mask.0 as i32) };
        from_c_ret(ret)
    }

    /// Explain [`ibv_post_recv`] errors.
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

    /// Explain [`ibv_post_send`] errors.
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
}

impl Qp {
    /// Global initial packet sequence number.
    pub const GLOBAL_INIT_PSN: Psn = 0;

    /// Global QKey.
    pub const GLOBAL_QKEY: QKey = 0x114514;

    /// UD header size.
    pub const GRH_SIZE: usize = 40;

    /// Create a new QP builder.
    pub fn builder<'a>() -> QpBuilder<'a> {
        Default::default()
    }

    /// Get the underlying `ibv_qp` pointer.
    #[inline]
    pub fn as_raw(&self) -> *mut ibv_qp {
        self.qp.as_ptr()
    }

    /// Get the protection domain of the queue pair.
    pub fn pd(&self) -> &Pd {
        &self.inner.pd
    }

    /// Get the context of the queue pair.
    pub fn context(&self) -> &Context {
        self.inner.pd.context()
    }

    /// Get the type of the queue pair.
    #[inline]
    pub fn qp_type(&self) -> QpType {
        self.qp.qp_type()
    }

    /// Get the queue pair number.
    #[inline]
    pub(crate) fn qp_num(&self) -> u32 {
        self.qp.qp_num()
    }

    /// Get the current state of the queue pair.
    #[inline]
    pub fn state(&self) -> QpState {
        self.qp.qp_state()
    }

    /// Get the capabilities of this QP.
    pub fn caps(&self) -> &QpCaps {
        &self.inner.init_attr.caps
    }

    /// Get the information of the local port that this QP is bound to.
    #[inline]
    pub fn port(&self) -> Option<&(Port, GidIndex)> {
        self.local_port.as_ref()
    }

    /// Get the information of the remote peer that this QP is connected to.
    #[inline]
    pub fn peer(&self) -> Option<&QpPeer> {
        self.peer.as_ref()
    }

    /// Get the endpoint information of this QP.
    /// Return `None` if the QP is not yet bound to a local port.
    #[inline]
    pub fn endpoint(&self) -> Option<QpEndpoint> {
        QpEndpoint::new(self)
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

    /// Bind the queue pair to an active local port.
    /// Will modify the QP to RTS state if it is an unreliable datagram QP at
    /// RESET state.
    ///
    /// This method is *not* commutative with [`Self::bind_peer()`]. You must
    /// bind the QP to a local port before binding it to a remote peer.
    ///
    /// If no GID index is specified (i.e., `gid_index` is `None`), this
    /// method will use the recommended GID. See documentation of
    /// [`Port::recommended_gid()`] for more information.
    ///
    /// # Panics
    ///
    /// Panic if the QP is already bound to a local port.
    /// If you really wish to rebind the QP to another port, call [`Self::reset()`] first.
    pub fn bind_local_port(&mut self, port: &Port, gid_index: Option<u8>) -> io::Result<()> {
        assert!(
            self.local_port.is_none(),
            "QP already bound to a local port"
        );
        if port.state() != PortState::Active {
            return Err(IoError::new(
                IoErrorKind::NotConnected,
                "port is not active",
            ));
        }

        let gid_index = gid_index.unwrap_or(port.recommended_gid().1);
        self.local_port = Some((port.clone(), gid_index));

        // Bring up QP if UD.
        if !self.qp_type().is_connected() {
            self.modify_reset2init()?;
            self.modify_init2rtr()?;
            self.modify_rtr2rts()?;
        }
        Ok(())
    }

    /// Bind the queue pair to a remote peer.
    /// Will modify the QP to RTS state if it is an connected QP at RESET state
    /// and already bound to a local port.
    ///
    /// If the QP is UD, this method will not modify the QP. Instead, it sets
    /// the default target for all sends.
    ///
    /// This method is *not* commutative with [`Self::bind_local_port()`].
    /// You must bind the QP to a local port before binding it to a remote peer.
    ///
    /// # Panics
    ///
    /// - Panic if the QP is not yet bound to a local port.
    /// - Panic if the QP is connected and already bound to a remote peer.
    pub fn bind_peer(&mut self, ep: QpEndpoint) -> io::Result<()> {
        assert!(
            self.local_port.is_some(),
            "QP not yet bound to a local port"
        );
        assert!(
            !(self.qp_type().is_connected() && self.peer.is_some()),
            "QP already bound to a remote peer"
        );

        self.peer = Some(QpPeer::new(
            self.pd(),
            self.local_port.as_ref().unwrap().1,
            ep,
        )?);

        // Bring up QP.
        if self.qp_type().is_connected() {
            self.modify_reset2init()?;
            self.modify_init2rtr()?;
            self.modify_rtr2rts()?;
        }
        Ok(())
    }

    /// Reset the QP.
    /// Modify the QP to RESET state and clear any local port or remote peer
    /// bindings.
    pub fn reset(&mut self) -> io::Result<()> {
        self.modify_2reset()?;
        self.local_port.take();
        self.peer.take();
        Ok(())
    }

    /// Create a new peer that is reachable from this QP.
    /// The QP must be bound to a local port.
    ///
    /// # Panics
    ///
    /// Panic if this QP is not bound to a local port.
    pub fn make_peer(&self, ep: &QpEndpoint) -> io::Result<QpPeer> {
        QpPeer::new(self.pd(), self.local_port.as_ref().unwrap().1, *ep)
    }

    /// Post a RDMA recv request.
    ///
    /// **NOTE:** This method has no mutable borrows to its parameters, but can
    /// cause the content of the buffers to be modified!
    pub fn recv(&self, local: &[MrSlice], wr_id: u64) -> io::Result<()> {
        let mut sgl = build_sgl(local);
        let mut wr = ibv_recv_wr {
            wr_id,
            next: ptr::null_mut(),
            sg_list: if local.is_empty() {
                ptr::null_mut()
            } else {
                sgl.as_mut_ptr()
            },
            num_sge: local.len() as i32,
        };
        let ret = unsafe {
            let mut bad_wr = ptr::null_mut();
            ibv_post_recv(self.as_raw(), &mut wr, &mut bad_wr)
        };
        from_c_ret_explained(ret, Self::recv_err_explanation)
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
    ) -> io::Result<()> {
        let mut sgl = build_sgl(local);
        let mut wr = ibv_send_wr {
            wr_id,
            next: ptr::null_mut(),
            sg_list: if local.is_empty() {
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
            ..(unsafe { mem::zeroed() })
        };
        wr.set_imm(imm.unwrap_or(0));

        if let Some(peer) = peer.or(self.peer.as_ref()) {
            wr.wr.ud = peer.ud();
        }
        let ret = unsafe {
            let mut bad_wr = ptr::null_mut();
            ibv_post_send(self.as_raw(), &mut wr, &mut bad_wr)
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
        remote: &MrRemote,
        wr_id: WrId,
        signal: bool,
    ) -> io::Result<()> {
        let mut sgl = build_sgl(local);
        let mut wr = unsafe { mem::zeroed::<ibv_send_wr>() };
        wr = ibv_send_wr {
            wr_id,
            next: ptr::null_mut(),
            sg_list: if local.is_empty() {
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
            ibv_post_send(self.as_raw(), &mut wr, &mut bad_wr)
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
        remote: &MrRemote,
        wr_id: WrId,
        imm: Option<ImmData>,
        signal: bool,
    ) -> io::Result<()> {
        let mut sgl = build_sgl(local);
        let mut wr = ibv_send_wr {
            wr_id,
            next: ptr::null_mut(),
            sg_list: if local.is_empty() {
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
            wr: wr_t {
                rdma: remote.as_rdma_t(),
            },
            ..(unsafe { mem::zeroed() })
        };
        wr.set_imm(imm.unwrap_or(0));

        let ret = unsafe {
            let mut bad_wr = ptr::null_mut();
            ibv_post_send(self.as_raw(), &mut wr, &mut bad_wr)
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
        remote: &MrRemote,
        current: u64,
        new: u64,
        wr_id: WrId,
        signal: bool,
    ) -> io::Result<()> {
        if local.len() != mem::size_of::<u64>() || remote.len != mem::size_of::<u64>() {
            return Err(IoError::new(
                IoErrorKind::InvalidInput,
                format!(
                    "expected 8B buffers for compare-and-swap, got ({}, {})",
                    local.len(),
                    remote.len
                ),
            ));
        }
        if (local.addr() as u64) % (mem::align_of::<u64>() as u64) != 0
            || remote.addr % (mem::align_of::<u64>() as u64) != 0
        {
            return Err(IoError::new(
                IoErrorKind::InvalidInput,
                format!(
                    "expected 8B-aligned buffers for compare-and-swap, got ({:p}, {:p})",
                    local.addr(),
                    remote.addr as *const u8
                ),
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
            ibv_post_send(self.as_raw(), &mut wr, &mut bad_wr)
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
        remote: &MrRemote,
        add: u64,
        wr_id: WrId,
        signal: bool,
    ) -> io::Result<()> {
        if local.len() != mem::size_of::<u64>() || remote.len != mem::size_of::<u64>() {
            return Err(IoError::new(
                IoErrorKind::InvalidInput,
                format!(
                    "expected 8B buffers for compare-and-swap, got ({}, {})",
                    local.len(),
                    remote.len
                ),
            ));
        }
        if (local.addr() as u64) % (mem::align_of::<u64>() as u64) != 0
            || remote.addr % (mem::align_of::<u64>() as u64) != 0
        {
            return Err(IoError::new(
                IoErrorKind::InvalidInput,
                format!(
                    "expected 8B-aligned buffers for compare-and-swap, got ({:p}, {:p})",
                    local.addr(),
                    remote.addr as *const u8
                ),
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
            ibv_post_send(self.as_raw(), &mut wr, &mut bad_wr)
        };
        from_c_ret_explained(ret, Self::send_err_explanation)
    }

    /// Post a list of recv work requests without any checks.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the work request list is valid, including:
    /// - all work request entries in the linked list
    /// - length of the work request list
    /// - scatter/gather lists and their lengths
    #[inline(always)]
    pub unsafe fn post_raw_recv(&self, wr: &ibv_recv_wr) -> io::Result<()> {
        let ret = {
            let mut bad_wr = ptr::null_mut();
            ibv_post_recv(self.as_raw(), wr as *const _ as *mut _, &mut bad_wr)
        };
        from_c_ret_explained(ret, Self::recv_err_explanation)
    }

    /// Post a list of send-type work requests without any checks.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the work request list is valid, including:
    /// - all work request entries in the linked list
    /// - length of the work request list
    /// - scatter/gather lists and their lengths
    #[inline(always)]
    pub unsafe fn post_raw_send(&self, wr: &ibv_send_wr) -> io::Result<()> {
        let ret = {
            let mut bad_wr = ptr::null_mut();
            ibv_post_send(self.as_raw(), wr as *const _ as *mut _, &mut bad_wr)
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
