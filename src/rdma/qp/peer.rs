use std::fmt;
use std::io::{self, Error as IoError};
use std::ptr::NonNull;
use std::sync::Arc;

use crate::bindings::*;
use crate::rdma::{dct::Dct, gid::Gid, pd::Pd, qp::Qp, type_alias::*};
use crate::utils::interop::from_c_ret;

/// Endpoint (NIC port & queue pair / DCT) data.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct QpEndpoint {
    /// Endpoint GID.
    pub gid: Gid,

    /// Port LID.
    pub lid: Lid,

    /// Port index.
    pub port_num: PortNum,

    /// QP or DCT number.
    pub num: Qpn,
}

impl QpEndpoint {
    /// Create an endpoint reprensenting a regular queue pair.
    /// Return `None` if the Qp is not yet bound to a local port.
    pub fn of_qp(qp: &Qp) -> Option<Self> {
        let (port, gid_idx) = qp.port()?;
        let gid = port.gids()[*gid_idx as usize];

        Some(Self {
            gid: gid.gid,
            port_num: port.num(),
            lid: port.lid(),
            num: qp.qp_num(),
        })
    }

    /// Create an endpoint representing a DCT.
    pub fn of_dct(dct: &Dct) -> Self {
        let init_attr = dct.init_attr();
        let gid = init_attr.port.gids()[init_attr.gid_index as usize];
        Self {
            gid: gid.gid,
            port_num: init_attr.port.num(),
            lid: init_attr.port.lid(),
            num: dct.dct_num(),
        }
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
#[derive(Clone)]
pub struct QpPeer {
    /// Cached address handle pointer.
    ah: IbvAh,

    /// QP or DCT number.
    num: Qpn,

    /// Peer information body.
    inner: Arc<QpPeerInner>,
}

impl fmt::Debug for QpPeer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QpPeer")
            .field("endpoint", &self.inner.ep)
            .finish()
    }
}

impl QpPeer {
    /// Create a new peer that represents a regular QP or a DCT.
    pub(crate) fn new(pd: &Pd, sgid_index: GidIndex, ep: QpEndpoint) -> io::Result<Self> {
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

        Ok(Self {
            inner: Arc::new(QpPeerInner {
                _pd: pd.clone(),
                ah,
                ep,
            }),
            ah,
            num: ep.num,
        })
    }

    /// Return a handle that can be used in RDMA UD sends to this peer.
    /// The return type is opaque to the user; you may only copy assign it to [`ibv_send_wr::wr`].
    #[inline]
    pub fn ud(&self) -> ud_t {
        ud_t {
            ah: self.ah.as_ptr(),
            remote_qpn: self.num,
            remote_qkey: Qp::GLOBAL_QKEY,
        }
    }

    /// Return a handle that can be used in RDMA DC send-type verbs to this peer.
    /// The return type is opaque to the user; you may only copy assign it to [`ibv_exp_send_wr::dc`].
    #[inline]
    pub fn dc(&self) -> dc_t {
        dc_t {
            ah: self.ah.as_ptr(),
            dct_number: self.num,
            dct_access_key: Dct::GLOBAL_DC_KEY,
        }
    }
}

impl QpPeer {
    /// Get the endpoint data of this peer.
    #[inline]
    pub fn endpoint(&self) -> &QpEndpoint {
        &self.inner.ep
    }

    /// Fill in a send work request for UD sending to this peer.
    #[inline]
    pub fn set_ud_peer(&self, wr: &mut ibv_send_wr) {
        wr.wr.ud = self.ud();
    }
}
