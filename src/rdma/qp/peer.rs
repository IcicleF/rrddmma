use std::mem;
use std::ptr::NonNull;
use std::sync::Arc;
use std::{fmt, io};

use crate::rdma::gid::Gid;
use crate::rdma::pd::Pd;
use crate::rdma::qp::Qp;
use crate::rdma::types::*;

use crate::bindings::*;
use anyhow::{Context as _, Result};

/// Endpoint (NIC port & queue pair) data.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct QpEndpoint {
    pub gid: Gid,
    pub lid: Lid,
    pub port_num: PortNum,
    pub qpn: Qpn,
}

impl QpEndpoint {
    /// Create a new endpoint from a queue pair.
    pub fn new(qp: &Qp) -> Self {
        let ctx = qp.context();
        QpEndpoint {
            gid: ctx.gid(),
            port_num: ctx.port_num(),
            lid: ctx.lid(),
            qpn: qp.qp_num(),
        }
    }

    /// Create a dummy endpoint.
    /// This is useful when modifying a UD QP to RTS.
    pub(crate) unsafe fn dummy() -> Self {
        mem::zeroed()
    }
}

struct QpPeerInner {
    ah: NonNull<ibv_ah>,
    ep: QpEndpoint,
}

unsafe impl Send for QpPeerInner {}
unsafe impl Sync for QpPeerInner {}

impl Drop for QpPeerInner {
    fn drop(&mut self) {
        // SAFETY: FFI.
        unsafe { ibv_destroy_ah(self.ah.as_ptr()) };
    }
}

/// Peer queue pair information that can be used in sends.
#[derive(Clone)]
#[repr(transparent)]
pub struct QpPeer {
    inner: Arc<QpPeerInner>,
}

unsafe impl Send for QpPeer {}
unsafe impl Sync for QpPeer {}

impl fmt::Debug for QpPeer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QpPeer")
            .field("endpoint", &self.inner.ep)
            .finish()
    }
}

impl QpPeer {
    pub fn new(pd: &Pd, ep: QpEndpoint) -> Result<Self> {
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
        Ok(Self {
            inner: Arc::new(QpPeerInner { ah, ep }),
        })
    }

    /// Get the underlying [`ibv_ah`] pointer.
    #[inline]
    pub fn as_raw(&self) -> *mut ibv_ah {
        self.inner.ah.as_ptr()
    }

    /// Get the endpoint data of this peer.
    #[inline]
    pub fn endpoint(&self) -> &QpEndpoint {
        &self.inner.ep
    }

    /// Generate a [`ud_t`] instance for RDMA sends to this peer.
    #[inline]
    pub fn ud(&self) -> ud_t {
        ud_t {
            ah: self.inner.ah.as_ptr(),
            remote_qpn: self.inner.ep.qpn,
            remote_qkey: Qp::GLOBAL_QKEY,
        }
    }
}
