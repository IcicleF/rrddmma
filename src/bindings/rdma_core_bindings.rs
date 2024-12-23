use libc::*;

use super::*;
pub use super::common::*;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ibv_gid_global_t {
    pub subnet_prefix: __be64,
    pub interface_id: __be64,
}

#[repr(C)]
pub union ibv_async_event_element_union_t {
    pub cq: *mut ibv_cq,
    pub qp: *mut ibv_qp,
    pub srq: *mut ibv_srq,
    pub wq: *mut ibv_wq,
    pub port_num: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union imm_data_invalidated_rkey_union_t {
    pub imm_data: __be32,
    pub invalidated_rkey: u32,
}

impl std::fmt::Debug for imm_data_invalidated_rkey_union_t {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // SAFETY: union of two `u32`s.
        unsafe {
            f.debug_struct("imm_data_invalidated_rkey_union_t")
                .field("imm_data", &self.imm_data)
                .finish()
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ibv_wc {
    pub wr_id: u64,
    pub status: ibv_wc_status::Type,
    pub opcode: ibv_wc_opcode::Type,
    pub vendor_err: u32,
    pub byte_len: u32,
    pub imm_data_invalidated_rkey_union: imm_data_invalidated_rkey_union_t,
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
        // SAFETY: union of two `u32`s.
        unsafe { self.imm_data_invalidated_rkey_union.imm_data }
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
    pub imm_data_invalidated_rkey_union: imm_data_invalidated_rkey_union_t,
    pub wr: wr_t,
    pub qp_type_xrc_remote_srq_num: qp_type_xrc_remote_srq_num_union_t,
    pub bind_mw: mw_rkey_bind_info_union_t,
}

impl ibv_send_wr {
    /// Set the immediate data.
    #[inline(always)]
    pub fn set_imm(&mut self, imm: u32) {
        // SAFETY: union of two `u32`s.
        unsafe { self.imm_data_invalidated_rkey_union.imm_data = imm };
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct add_t {
    pub recv_wr_id: u64,
    pub sg_list: *mut ibv_sge,
    pub num_sge: c_int,
    pub tag: u64,
    pub mask: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct tm_t {
    pub unexpected_cnt: u32,
    pub handle: u32,
    pub add: add_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ibv_ops_wr {
    wr_id: u64,
    next: *mut Self,
    opcode: ibv_ops_wr_opcode::Type,
    flags: c_int,
    tm: tm_t,
}

#[repr(C)]
pub struct _compat_ibv_port_attr {
    state: ibv_port_state::Type,
    max_mtu: ibv_mtu::Type,
    active_mtu: ibv_mtu::Type,
    gid_tbl_len: c_int,
    port_cap_flags: u32,
    max_msg_sz: u32,
    bad_pkey_cntr: u32,
    qkey_viol_cntr: u32,
    pkey_tbl_len: u16,
    lid: u16,
    sm_lid: u16,
    lmc: u8,
    max_vl_num: u8,
    sm_sl: u8,
    subnet_timeout: u8,
    init_type_reply: u8,
    active_width: u8,
    active_speed: u8,
    phys_state: u8,
    link_layer: u8,
    flags: u8,
}

#[inline]
pub(super) unsafe fn verbs_get_ctx(ctx: *const ibv_context) -> *mut verbs_context {
    const __VERBS_ABI_IS_EXTENDED: *mut ::std::os::raw::c_void = usize::MAX as _;
    if ctx.is_null() || (*ctx).abi_compat != __VERBS_ABI_IS_EXTENDED {
        std::ptr::null_mut()
    } else {
        container_of!(ctx, verbs_context, context)
    }
}

#[inline]
pub unsafe fn ___ibv_query_port(
    context: *mut ibv_context,
    port_num: u8,
    port_attr: *mut ibv_port_attr,
) -> ::std::os::raw::c_int {
    let vctx = verbs_get_ctx_op!(context, query_port);
    if !vctx.is_null() {
        std::ptr::write_bytes(
            port_attr as *mut u8,
            0,
            ::std::mem::size_of::<ibv_port_attr>(),
        );
        ibv_query_port(context, port_num, port_attr as *mut _compat_ibv_port_attr)
    } else {
        (*vctx).query_port.unwrap()(
            context,
            port_num,
            port_attr,
            std::mem::size_of::<ibv_port_attr>(),
        )
    }
}
