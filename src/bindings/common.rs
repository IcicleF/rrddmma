#![macro_use]

use super::*;
use libc::*;

#[repr(C)]
#[derive(Clone, Copy)]
pub union ibv_gid {
    pub raw: [u8; 16],
    pub global: ibv_gid_global_t,
}

#[repr(C)]
pub struct ibv_async_event {
    pub element: ibv_async_event_element_union_t,
    pub event_type: ibv_event_type,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ibv_global_route {
    pub dgid: ibv_gid,
    pub flow_label: u32,
    pub sgid_index: u8,
    pub hop_limit: u8,
    pub traffic_class: u8,
}

// ibv_send_wr related union and struct types
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct rdma_t {
    pub remote_addr: u64,
    pub rkey: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct atomic_t {
    pub remote_addr: u64,
    pub compare_add: u64,
    pub swap: u64,
    pub rkey: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ud_t {
    pub ah: *mut ibv_ah,
    pub remote_qpn: u32,
    pub remote_qkey: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union wr_t {
    pub rdma: rdma_t,
    pub atomic: atomic_t,
    pub ud: ud_t,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct xrc_t {
    pub remote_srqn: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union qp_type_t {
    pub xrc: xrc_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union qp_type_xrc_remote_srq_num_union_t {
    pub qp_type: qp_type_t,
    pub xrc_remote_srq_num: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct bind_mw_t {
    pub mw: *mut ibv_mw,
    pub rkey: u32,
    pub bind_info: ibv_mw_bind_info,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct tso_t {
    pub hdr: *mut c_void,
    pub hdr_sz: u16,
    pub mss: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union mw_rkey_bind_info_union_t {
    pub mw: *mut ibv_mw,
    pub rkey: u32,
    pub bind_info: ibv_mw_bind_info,
}

// ibv_flow_spec related union and struct types
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct hdr_t {
    pub type_: ibv_flow_spec_type::Type,
    pub size: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union ibv_flow_spec_union_t {
    pub hdr: hdr_t,
    pub eth: ibv_flow_spec_eth,
    pub ipv4: ibv_flow_spec_ipv4,
    pub tcp_udp: ibv_flow_spec_tcp_udp,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ibv_ah_attr {
    pub grh: ibv_global_route,
    pub dlid: u16,
    pub sl: u8,
    pub src_path_bits: u8,
    pub static_rate: u8,
    pub is_global: u8,
    pub port_num: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ibv_flow_spec {
    pub ibv_flow_spec_union: ibv_flow_spec_union_t,
}

pub const IBV_LINK_LAYER_UNSPECIFIED: ::std::os::raw::c_int = 0;
pub const IBV_LINK_LAYER_INFINIBAND: ::std::os::raw::c_int = 1;
pub const IBV_LINK_LAYER_ETHERNET: ::std::os::raw::c_int = 2;
pub const IBV_EXP_LINK_LAYER_START: ::std::os::raw::c_int = 32;
pub const IBV_EXP_LINK_LAYER_SCIF: ::std::os::raw::c_int = IBV_EXP_LINK_LAYER_START;

macro_rules! container_of {
    ($ptr:expr, $container:path, $field:ident) => {{
        ($ptr as *const _ as *const u8 as *mut u8).sub(memoffset::offset_of!($container, $field))
            as *mut $container
    }};
}

macro_rules! verbs_get_ctx_op {
    ($ctx:expr, $op:ident) => {{
        let vctx = verbs_get_ctx($ctx);
        if vctx.is_null()
            || (*vctx).sz
                < ::std::mem::size_of_val(&*vctx) - memoffset::offset_of!(verbs_context, $op)
            || (*vctx).$op.is_none()
        {
            std::ptr::null_mut()
        } else {
            vctx
        }
    }};
}

/// Close an extended connection domain.
#[inline]
pub unsafe fn ibv_close_xrcd(xrcd: *mut ibv_xrcd) -> ::std::os::raw::c_int {
    let vctx = verbs_get_ctx((*xrcd).context);
    (*vctx).close_xrcd.unwrap()(xrcd)
}

/// Free a memory window.
#[inline]
pub unsafe fn ibv_dealloc_mw(mw: *mut ibv_mw) -> ::std::os::raw::c_int {
    (*(*mw).context).ops.dealloc_mw.unwrap()(mw)
}

/// Increment the 8 lsb in the given rkey.
#[inline]
pub fn ibv_inc_rkey(rkey: u32) -> u32 {
    const MASK: u32 = 0x000000FF;
    let newtag = ((rkey + 1) & MASK) as u8;

    (rkey & !MASK) | newtag as u32
}

/// Bind a memory window to a region.
#[inline]
pub unsafe fn ibv_bind_mw(
    qp: *mut ibv_qp,
    mw: *mut ibv_mw,
    mw_bind: *mut ibv_mw_bind,
) -> ::std::os::raw::c_int {
    if (*mw).type_ != ibv_mw_type::IBV_MW_TYPE_1 {
        EINVAL
    } else {
        (*(*mw).context).ops.bind_mw.unwrap()(qp, mw, mw_bind)
    }
}

/// Poll a CQ for work completions.
///
/// Poll a CQ for (possibly multiple) completions. If the return value
/// is < 0, an error occurred. If the return value is >= 0, it is the
/// number of completions returned. If the return value is
/// non-negative and strictly less than num_entries, then the CQ was
/// emptied.
///
/// # Arguments
///
/// - `cq`: the CQ being polled
/// - `num_entries`: maximum number of completions to return
/// - `wc`: array of at least @num_entries of &struct ibv_wc where completions
///   will be returned
#[inline]
pub unsafe fn ibv_poll_cq(
    cq: *mut ibv_cq,
    num_entries: ::std::os::raw::c_int,
    wc: *mut ibv_wc,
) -> ::std::os::raw::c_int {
    (*(*cq).context).ops.poll_cq.unwrap()(cq, num_entries, wc)
}

/// Post a list of work requests to a send queue.
///
/// If IBV_SEND_INLINE flag is set, the data buffers can be reused
/// immediately after the call returns.
#[inline]
pub unsafe fn ibv_post_send(
    qp: *mut ibv_qp,
    wr: *mut ibv_send_wr,
    bad_wr: *mut *mut ibv_send_wr,
) -> ::std::os::raw::c_int {
    (*(*qp).context).ops.post_send.unwrap()(qp, wr, bad_wr)
}

/// Post a list of work requests to a receive queue.
#[inline]
pub unsafe fn ibv_post_recv(
    qp: *mut ibv_qp,
    wr: *mut ibv_recv_wr,
    bad_wr: *mut *mut ibv_recv_wr,
) -> ::std::os::raw::c_int {
    (*(*qp).context).ops.post_recv.unwrap()(qp, wr, bad_wr)
}
