//! Dynamically-connected target (DCT), rdma-core implementation.
//!
//! On rdma-core there is no dedicated DCT creation verb; the DCT is created
//! as an `IBV_QPT_DRIVER` QP with the DC extension (`MLX5DV_DCTYPE_DCT`) via
//! `mlx5dv_create_qp`, and is usable (RTS state) immediately after creation.

use std::ptr::NonNull;
use std::sync::Arc;
use std::{fmt, io, mem};

use thiserror::Error;

use crate::bindings::*;
use crate::lo::mr::Permission;
use crate::lo::qp::{IbvQp, QpEndpoint};
use crate::lo::{context::Context, cq::Cq, pd::Pd, srq::Srq};
use crate::utils::interop::from_c_ret;

use super::builder::{DctBuilder, DctInitAttr};

/// DCT creation error type.
#[derive(Debug, Error)]
pub enum DctCreationError {
    /// `libibverbs` interfaces returned an error.
    #[error("I/O error from ibverbs")]
    IoError(#[from] io::Error),
}

/// Ownership holder of the DCT.
struct DctInner {
    ctx: Context,
    qp: IbvQp,
    num: u32,
    init_attr: DctInitAttr,
}

impl Drop for DctInner {
    fn drop(&mut self) {
        // SAFETY: call only once, and no UAF since I will be dropped.
        unsafe { self.qp.destroy() }.expect("cannot destroy DCT on drop");
    }
}

/// Dynamically-connected target (DCT).
pub struct Dct {
    /// Cached QP pointer (the DCT is a driver QP underneath).
    qp: IbvQp,

    /// DCT body.
    inner: Arc<DctInner>,
}

impl fmt::Debug for Dct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            "Dct<{:p}, num={}>",
            self.as_raw(),
            self.dct_num()
        ))
    }
}

impl Dct {
    /// Create a new DCT.
    ///
    /// NOTE: unlike the legacy MLNX_OFED path, the rdma-core creation
    /// interface provides no way to set the DCT's MTU / RNR timer / inline
    /// receive size; device defaults are used instead.
    pub(crate) fn new(ctx: &Context, builder: DctBuilder) -> Result<Self, DctCreationError> {
        let init_attr = builder.unwrap()?;

        let qp = {
            // SAFETY: POD type.
            let mut attr = unsafe { mem::zeroed::<ibv_qp_init_attr_ex>() };
            attr.qp_type = ibv_qp_type::IBV_QPT_DRIVER;
            // NOTE: libibverbs' create_qp command path unconditionally
            // dereferences `send_cq`, so it must be set even for a DCT
            // (which only ever completes receives).
            attr.send_cq = init_attr.cq.as_raw();
            attr.recv_cq = init_attr.cq.as_raw();
            attr.pd = init_attr.pd.as_raw();
            attr.srq = init_attr.srq.as_raw();
            attr.cap.max_recv_wr = crate::lo::prelude::QpCaps::default().max_recv_wr;
            attr.cap.max_recv_sge = crate::lo::prelude::QpCaps::default().max_recv_sge;
            attr.comp_mask = ibv_qp_init_attr_mask::IBV_QP_INIT_ATTR_PD.0;

            // SAFETY: POD type.
            let mut mlx5_attr: mlx5dv_qp_init_attr = unsafe { mem::zeroed() };
            mlx5_attr.comp_mask = MLX5DV_QP_INIT_ATTR_MASK_DC as u64;
            mlx5_attr.dc_init_attr.dc_type = MLX5DV_DCTYPE_DCT;
            unsafe {
                *mlx5_attr
                    .dc_init_attr
                    .__bindgen_anon_1
                    .dct_access_key
                    .as_mut() = Self::GLOBAL_DC_KEY;
            }

            // SAFETY: FFI.
            unsafe { mlx5dv_create_qp(ctx.as_raw(), &mut attr, &mut mlx5_attr) }
        };
        let qp = NonNull::new(qp).ok_or_else(io::Error::last_os_error)?;
        let qp = IbvQp::from(qp);

        // NOTE: on the rdma-core path the DCT does not exist in the hardware
        // yet — the `CREATE_DCT` command is issued by the kernel only when
        // the QP is modified RESET→INIT→RTR, and the real DCT number comes
        // back in the RTR modify response (`qp_num` is updated there).
        Self::bring_up(&qp, &init_attr)?;

        // The DCT number equals the underlying QP number (now valid).
        let num = qp.qp_num();
        let dct = Dct {
            inner: Arc::new(DctInner {
                ctx: ctx.clone(),
                qp,
                num,
                init_attr,
            }),
            qp,
        };
        Ok(dct)
    }

    /// Transition the freshly created DCT through RESET→INIT→RTR.
    ///
    /// The RTR transition is what actually instantiates the DCT in the
    /// hardware (and assigns its number). The AV carried by the RTR modify
    /// configures the DCT's local address (port / MTU / GID / hop limit).
    fn bring_up(qp: &IbvQp, init_attr: &DctInitAttr) -> Result<(), DctCreationError> {
        let port = &init_attr.port;

        // RESET -> INIT: enable the receive side on the given port.
        {
            // SAFETY: POD type.
            let mut attr = unsafe { mem::zeroed::<ibv_qp_attr>() };
            attr.qp_state = ibv_qp_state::IBV_QPS_INIT;
            attr.pkey_index = 0;
            attr.port_num = port.num();
            attr.qp_access_flags = Permission::default().into();

            let attr_mask = ibv_qp_attr_mask::IBV_QP_STATE
                | ibv_qp_attr_mask::IBV_QP_PKEY_INDEX
                | ibv_qp_attr_mask::IBV_QP_PORT
                | ibv_qp_attr_mask::IBV_QP_ACCESS_FLAGS;
            // SAFETY: FFI.
            let ret = unsafe { ibv_modify_qp(qp.as_ptr(), &mut attr, attr_mask.0 as i32) };
            from_c_ret(ret)?;
        }

        // INIT -> RTR: this creates the DCT in the hardware.
        {
            // SAFETY: POD type.
            let mut attr = unsafe { mem::zeroed::<ibv_qp_attr>() };
            attr.qp_state = ibv_qp_state::IBV_QPS_RTR;
            attr.path_mtu = port.mtu() as _;
            attr.min_rnr_timer = 12;
            attr.ah_attr.dlid = port.lid();
            attr.ah_attr.sl = 0;
            attr.ah_attr.src_path_bits = 0;
            attr.ah_attr.port_num = port.num();
            attr.ah_attr.is_global = 1;
            attr.ah_attr.grh.hop_limit = 0xFF;
            attr.ah_attr.grh.sgid_index = init_attr.gid_index;

            let attr_mask = ibv_qp_attr_mask::IBV_QP_STATE
                | ibv_qp_attr_mask::IBV_QP_PATH_MTU
                | ibv_qp_attr_mask::IBV_QP_MIN_RNR_TIMER
                | ibv_qp_attr_mask::IBV_QP_AV;
            // SAFETY: FFI.
            let ret = unsafe { ibv_modify_qp(qp.as_ptr(), &mut attr, attr_mask.0 as i32) };
            from_c_ret(ret)?;
        }

        Ok(())
    }
}

impl Dct {
    /// Global DC Key.
    pub const GLOBAL_DC_KEY: u64 = 0x1919810;

    /// Return a new DCT builder.
    pub fn builder<'a>() -> DctBuilder<'a> {
        Default::default()
    }

    /// Get the underlying `ibv_qp` raw pointer of the DCT.
    #[inline]
    pub fn as_raw(&self) -> *mut ibv_qp {
        self.qp.as_ptr()
    }

    /// Get the initialization attributes of the DCT.
    pub(crate) fn init_attr(&self) -> &DctInitAttr {
        &self.inner.init_attr
    }

    /// Get the RDMA device context of the DCT.
    pub fn context(&self) -> &Context {
        &self.inner.ctx
    }

    /// Get the protection domain of the DCT.
    pub fn pd(&self) -> &Pd {
        &self.inner.init_attr.pd
    }

    /// Get the DCT number.
    pub fn dct_num(&self) -> u32 {
        self.inner.num
    }

    /// Get the SRQ of this DCT.
    pub fn srq(&self) -> &Srq {
        &self.inner.init_attr.srq
    }

    /// Get the CQ of this DCT.
    pub fn cq(&self) -> &Cq {
        &self.inner.init_attr.cq
    }

    /// Get the endpoint information of this DCT.
    pub fn endpoint(&self) -> QpEndpoint {
        QpEndpoint::of_dct(self)
    }
}
