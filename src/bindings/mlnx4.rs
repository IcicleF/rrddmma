#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(deref_nullptr)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::too_many_arguments)]

use super::*;
use libc::*;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ibv_gid_global_t {
    pub subnet_prefix: u64,
    pub interface_id: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union ibv_gid {
    pub raw: [u8; 16],
    pub global: ibv_gid_global_t,
}

#[repr(C)]
pub union ibv_async_event_element_t {
    pub cq: *mut ibv_cq,
    pub qp: *mut ibv_qp,
    pub srq: *mut ibv_srq,
    pub dct: *mut ibv_exp_dct,
    pub port_num: c_int,
    pub xrc_qp_num: u32,
}

#[repr(C)]
pub struct ibv_async_event {
    pub element: ibv_async_event_element_t,
    pub event_type: ibv_event_type,
}

#[repr(C)]
pub struct ibv_wc {
    pub wr_id: u64,
    pub status: ibv_wc_status::Type,
    pub opcode: ibv_wc_opcode::Type,
    pub vendor_err: u32,
    pub byte_len: u32,
    pub imm_data: u32,
    pub qp_num: u32,
    pub src_qp: u32,
    pub wc_flags: c_uint,
    pub pkey_index: u16,
    pub slid: u16,
    pub sl: u8,
    pub dlid_path_bits: u8,
}

impl ibv_wc {
    /// Get the immediate data.
    #[inline(always)]
    pub fn imm(&self) -> u32 {
        self.imm_data
    }
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
#[derive(Copy, Clone)]
pub struct ibv_mw_bind_info {
    pub mr: *mut ibv_mr,
    pub addr: u64,
    pub length: u64,
    pub mw_access_flags: ::std::os::raw::c_uint,
}

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

#[repr(C)]
pub struct ibv_send_wr {
    pub wr_id: u64,
    pub next: *mut Self,
    pub sg_list: *mut ibv_sge,
    pub num_sge: c_int,
    pub opcode: ibv_wr_opcode::Type,
    pub send_flags: c_uint,
    pub imm_data: u32,
    pub wr: wr_t,
    pub qp_type_xrc_remote_srq_num: qp_type_xrc_remote_srq_num_union_t,
    pub bind_mw: mw_rkey_bind_info_union_t,
}

impl ibv_send_wr {
    /// Set the immediate data.
    #[inline(always)]
    pub fn set_imm(&mut self, imm: u32) {
        self.imm_data = imm;
    }
}

// ibv_flow_spec related union and struct types
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct hdr_t {
    pub type_: ibv_flow_spec_type::Type,
    pub size: u16,
}

#[repr(C)]
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

#[inline]
unsafe fn verbs_get_ctx(ctx: *const ibv_context) -> *mut verbs_context {
    const __VERBS_ABI_IS_EXTENDED: *mut ::std::os::raw::c_void =
        std::ptr::null_mut::<u8>().wrapping_sub(1) as _;
    if ctx.is_null() || (*ctx).abi_compat != __VERBS_ABI_IS_EXTENDED {
        std::ptr::null_mut()
    } else {
        container_of!(ctx, verbs_context, context)
    }
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

#[inline]
pub unsafe fn ___ibv_query_port(
    context: *mut ibv_context,
    port_num: u8,
    port_attr: *mut ibv_port_attr,
) -> ::std::os::raw::c_int {
    (*port_attr).link_layer = IBV_LINK_LAYER_UNSPECIFIED as u8;
    (*port_attr).reserved = 0;

    ibv_query_port(context, port_num, port_attr)
}

#[inline]
pub unsafe fn ibv_create_flow(qp: *mut ibv_qp, flow_attr: *mut ibv_flow_attr) -> *mut ibv_flow {
    let vctx = verbs_get_ctx_op!((*qp).context, create_flow);
    if vctx.is_null() {
        std::ptr::null_mut()
    } else {
        (*vctx).create_flow.unwrap()(qp, flow_attr)
    }
}

#[inline]
pub unsafe fn ibv_destroy_flow(flow_id: *mut ibv_flow) -> ::std::os::raw::c_int {
    let vctx = verbs_get_ctx_op!((*flow_id).context, destroy_flow);
    if vctx.is_null() {
        -ENOSYS
    } else {
        (*vctx).destroy_flow.unwrap()(flow_id)
    }
}

/// Open an extended connection domain.
#[inline]
pub unsafe fn ibv_open_xrcd(
    context: *mut ibv_context,
    xrcd_init_attr: *mut ibv_xrcd_init_attr,
) -> *mut ibv_xrcd {
    let vctx = verbs_get_ctx_op!(context, open_xrcd);
    if vctx.is_null() {
        *__errno_location() = ENOSYS;
        std::ptr::null_mut()
    } else {
        (*vctx).open_xrcd.unwrap()(context, xrcd_init_attr)
    }
}

/// Close an extended connection domain.
#[inline]
pub unsafe fn ibv_close_xrcd(xrcd: *mut ibv_xrcd) -> ::std::os::raw::c_int {
    let vctx = verbs_get_ctx((*xrcd).context);
    (*vctx).close_xrcd.unwrap()(xrcd)
}

/// Allocate a memory window.
#[inline]
pub unsafe fn ibv_alloc_mw(pd: *mut ibv_pd, type_: ibv_mw_type::Type) -> *mut ibv_mw {
    if let Some(alloc_mw) = (*(*pd).context).ops.alloc_mw {
        alloc_mw(pd, type_)
    } else {
        *__errno_location() = ENOSYS;
        std::ptr::null_mut()
    }
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
        -EINVAL
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

#[inline]
unsafe fn verbs_get_exp_ctx(ctx: *const ibv_context) -> *mut verbs_context_exp {
    let app_ex_ctx = verbs_get_ctx(ctx);
    if app_ex_ctx.is_null()
        || (*app_ex_ctx).has_comp_mask & verbs_context_mask::VERBS_CONTEXT_EXP.0 == 0
    {
        std::ptr::null_mut()
    } else {
        let actual_ex_ctx =
            ((ctx as usize) - ((*app_ex_ctx).sz - std::mem::size_of::<ibv_context>())) as *mut u8;
        (actual_ex_ctx as usize - std::mem::size_of::<verbs_context_exp>()) as *mut _
    }
}

macro_rules! IBV_EXP_RET_ON_INVALID_COMP_MASK_compat {
    ($val:expr, $valid_mask:expr, $ret:expr, $func:expr) => {{
        if (($val) > ($valid_mask)) {
            let __val: ::std::os::raw::c_ulonglong = ($val) as _;
            let __valid_mask: ::std::os::raw::c_ulonglong = ($valid_mask) as _;

            // NOTE: since we cannot easily acquire `stderr: *mut FILE`, we use `eprintln!` instead.
            // Compatibility issues may occur, but since this is debug info it should be fine.
            eprintln!(
                "{}: invalid comp_mask !!! (comp_mask = 0x{:x} valid_mask = 0x{:x})\n",
                $func, __val, __valid_mask,
            );
            *(::libc::__errno_location()) = ::libc::EINVAL;
            return $ret;
        }
    }};
}

#[allow(unused)]
macro_rules! IBV_EXP_RET_NULL_ON_INVALID_COMP_MASK_compat {
    ($val:expr, $valid_mask:expr, $func:expr) => {
        IBV_EXP_RET_ON_INVALID_COMP_MASK_compat!($val, $valid_mask, ::std::ptr::null_mut(), $func,)
    };
}

#[allow(unused)]
macro_rules! IBV_EXP_RET_EINVAL_ON_INVALID_COMP_MASK_compat {
    ($val:expr, $valid_mask:expr, $func:expr) => {
        IBV_EXP_RET_ON_INVALID_COMP_MASK_compat!($val, $valid_mask, ::libc::EINVAL, $func)
    };
}

#[allow(unused)]
macro_rules! IBV_EXP_RET_ZERO_ON_INVALID_COMP_MASK_compat {
    ($val:expr, $valid_mask:expr, $func:expr) => {
        IBV_EXP_RET_ON_INVALID_COMP_MASK_compat!($val, $valid_mask, 0, $func)
    };
}

macro_rules! verbs_get_exp_ctx_op {
    ($ctx:expr, $op:ident) => {{
        let vctx = verbs_get_exp_ctx($ctx);
        if vctx.is_null()
            || (*vctx).sz
                < ::std::mem::size_of_val(&*vctx) - memoffset::offset_of!(verbs_context_exp, $op)
            || (*vctx).$op.is_none()
        {
            std::ptr::null_mut()
        } else {
            vctx
        }
    }};
}

/// Query GID attributes.
#[inline]
pub unsafe fn ibv_exp_query_gid_attr(
    context: *mut ibv_context,
    port_num: u8,
    index: ::std::os::raw::c_uint,
    attr: *mut ibv_exp_gid_attr,
) -> ::std::os::raw::c_int {
    let vctx = verbs_get_exp_ctx_op!(context, exp_query_gid_attr);
    if vctx.is_null() {
        ENOSYS
    } else {
        IBV_EXP_RET_EINVAL_ON_INVALID_COMP_MASK_compat!(
            (*attr).comp_mask,
            IBV_EXP_QUERY_GID_ATTR_RESERVED - 1,
            "ibv_exp_query_gid_attr"
        );
        (*vctx).exp_query_gid_attr.unwrap()(context, port_num, index, attr)
    }
}
