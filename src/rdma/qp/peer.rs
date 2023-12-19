use std::fmt;
use std::io::{self, Error as IoError};
use std::ptr::NonNull;
use std::sync::Arc;

use crate::rdma::{gid::Gid, pd::Pd, qp::Qp, type_alias::*};

use crate::bindings::*;
use crate::utils::interop::from_c_ret;

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
    pub fn new(qp: &Qp) -> Option<Self> {
        let (port, gid_idx) = qp.port()?;
        let gid = port.gids()[*gid_idx as usize];

        Some(QpEndpoint {
            gid: gid.gid,
            port_num: port.num(),
            lid: port.lid(),
            qpn: qp.qp_num(),
        })
    }
}

/// Wrapper of [`*mut ibv_ah`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct IbvAh(NonNull<ibv_ah>);

impl IbvAh {
    /// Destroy the address handle.
    ///
    /// # Safety
    ///
    /// - An AH must not be destroyed more than once.
    /// - Destroyed AHs must not be used anymore.
    pub unsafe fn destroy(self) -> io::Result<()> {
        // SAFETY: FFI.
        let ret = ibv_destroy_ah(self.as_ptr());
        from_c_ret(ret)
    }
}

impl_ibv_wrapper_traits!(ibv_ah, IbvAh);

/// Ownership holder of address handle.
struct QpPeerInner {
    _pd: Pd,
    ah: IbvAh,
    ep: QpEndpoint,
}

impl Drop for QpPeerInner {
    fn drop(&mut self) {
        // SAFETY: call only once, and no UAF since I will be dropped.
        unsafe { self.ah.destroy() }.expect("cannot destroy AH on drop");
    }
}

/// Remote peer information that can be used in sends.
pub struct QpPeer {
    /// Peer information body.
    inner: Arc<QpPeerInner>,

    /// Cached address handle pointer.
    ah: IbvAh,

    /// Cached QPN.
    qpn: Qpn,
}

impl fmt::Debug for QpPeer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QpPeer")
            .field("endpoint", &self.inner.ep)
            .finish()
    }
}

impl QpPeer {
    /// Create a new peer
    pub(crate) fn new(pd: &Pd, sgid_index: u8, ep: QpEndpoint) -> io::Result<Self> {
        let mut ah_attr = ibv_ah_attr {
            grh: ibv_global_route {
                dgid: ep.gid.into(),
                flow_label: 0,
                sgid_index,
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

        // SAFETY: FFI.
        let ah = unsafe { ibv_create_ah(pd.as_raw(), &mut ah_attr) };
        let ah = NonNull::new(ah).ok_or_else(IoError::last_os_error)?;
        let ah = IbvAh(ah);

        let qpn = ep.qpn;
        Ok(Self {
            inner: Arc::new(QpPeerInner {
                _pd: pd.clone(),
                ah,
                ep,
            }),
            ah,
            qpn,
        })
    }

    /// Get the underlying [`ibv_ah`] pointer.
    #[inline]
    pub fn as_raw(&self) -> *mut ibv_ah {
        self.ah.as_ptr()
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
            ah: self.ah.as_ptr(),
            remote_qpn: self.qpn,
            remote_qkey: Qp::GLOBAL_QKEY,
        }
    }
}
