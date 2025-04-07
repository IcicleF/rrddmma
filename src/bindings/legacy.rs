use libc::*;

pub use super::common::*;
use super::*;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ibv_gid_global_t {
    pub subnet_prefix: u64,
    pub interface_id: u64,
}

#[repr(C)]
pub union ibv_async_event_element_union_t {
    pub cq: *mut ibv_cq,
    pub qp: *mut ibv_qp,
    pub srq: *mut ibv_srq,
    pub dct: *mut ibv_exp_dct,
    pub port_num: c_int,
    pub xrc_qp_num: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
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
pub struct ibv_send_wr {
    pub wr_id: u64,
    pub next: *mut Self,
    pub sg_list: *mut ibv_sge,
    pub num_sge: c_int,
    pub opcode: ibv_wr_opcode::Type,
    pub send_flags: c_uint,
    pub imm_data: u32,
    pub wr: wr_t,
    pub qp_type_xrc_remote_srq_num_union: qp_type_xrc_remote_srq_num_union_t,
    pub bind_mw: mw_rkey_bind_info_union_t,
}

impl ibv_send_wr {
    /// Set the immediate data.
    #[inline(always)]
    pub fn set_imm(&mut self, imm: u32) {
        self.imm_data = imm;
    }
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
pub(super) unsafe fn verbs_get_ctx(ctx: *const ibv_context) -> *mut verbs_context {
    const __VERBS_ABI_IS_EXTENDED: *mut ::std::os::raw::c_void =
        std::ptr::null_mut::<u8>().wrapping_sub(1) as _;
    if ctx.is_null() || (*ctx).abi_compat != __VERBS_ABI_IS_EXTENDED {
        std::ptr::null_mut()
    } else {
        container_of!(ctx, verbs_context, context)
    }
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
