use crate::bindings::*;

/// Queue pair state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            _ => panic!("invalid QP state: {}", qp_state),
        }
    }
}
