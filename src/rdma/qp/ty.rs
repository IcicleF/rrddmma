use crate::bindings::*;

/// Queue pair type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QpType {
    /// Reliable connection.
    Rc = ibv_qp_type::IBV_QPT_RC as _,

    /// Unreliable connection.
    Uc = ibv_qp_type::IBV_QPT_UC as _,

    /// Unreliable datagram.
    Ud = ibv_qp_type::IBV_QPT_UD as _,

    /// Extended reliable connection (deprecated).
    #[cfg(mlnx4)]
    Xrc = ibv_qp_type::IBV_QPT_XRC as _,

    /// Raw packet (or Raw Ethernet).
    RawPacket = ibv_qp_type::IBV_QPT_RAW_PACKET as _,

    /// Extended reliable connection initiator.
    XrcIni = ibv_qp_type::IBV_QPT_XRC_SEND as _,

    /// Extended reliable connection target.
    XrcTgt = ibv_qp_type::IBV_QPT_XRC_RECV as _,

    /// Dynamically-connected QP initiator.
    #[cfg(mlnx4)]
    DcIni = ibv_qp_type::IBV_EXP_QPT_DC_INI as _,

    /// Driver-specific QP type.
    #[cfg(mlnx5)]
    Driver = ibv_qp_type::IBV_QPT_DRIVER as _,
}

#[cfg(mlnx4)]
impl QpType {
    const fn is_reliable_impl(self) -> bool {
        matches!(self, Self::Rc | Self::Xrc | Self::XrcIni | Self::XrcTgt)
    }

    const fn is_target_impl(self) -> bool {
        !matches!(self, Self::XrcIni | Self::DcIni)
    }
}

#[cfg(mlnx5)]
impl QpType {
    const fn is_reliable_impl(self) -> bool {
        matches!(self, Self::Rc | Self::XrcIni | Self::XrcTgt)
    }

    const fn is_target_impl(self) -> bool {
        !matches!(self, Self::XrcIni)
    }
}

impl QpType {
    /// Determine whether the QP type is reliable.
    pub const fn is_reliable(self) -> bool {
        self.is_reliable_impl()
    }

    /// Determine whether the QP type is datagram.
    pub const fn is_connected(self) -> bool {
        !matches!(self, Self::Ud | Self::RawPacket)
    }

    /// Determine whether the QP type can be a transmission initiator.
    pub const fn is_initiator(self) -> bool {
        !matches!(self, Self::XrcTgt)
    }

    /// Determine whether the QP type can be a transmission target.
    pub const fn is_target(self) -> bool {
        self.is_target_impl()
    }
}

impl From<QpType> for u32 {
    fn from(qp_type: QpType) -> Self {
        qp_type as _
    }
}

impl From<u32> for QpType {
    fn from(qp_type: u32) -> Self {
        match qp_type {
            ibv_qp_type::IBV_QPT_RC => QpType::Rc,
            ibv_qp_type::IBV_QPT_UC => QpType::Uc,
            ibv_qp_type::IBV_QPT_UD => QpType::Ud,
            #[cfg(mlnx4)]
            ibv_qp_type::IBV_QPT_XRC => QpType::Xrc,
            ibv_qp_type::IBV_QPT_RAW_PACKET => QpType::RawPacket,
            ibv_qp_type::IBV_QPT_XRC_SEND => QpType::XrcIni,
            ibv_qp_type::IBV_QPT_XRC_RECV => QpType::XrcTgt,
            #[cfg(mlnx4)]
            ibv_qp_type::IBV_EXP_QPT_DC_INI => QpType::DcIni,
            #[cfg(mlnx5)]
            ibv_qp_type::IBV_QPT_DRIVER => QpType::Driver,
            _ => panic!("unrecognized QP type: {}", qp_type),
        }
    }
}
