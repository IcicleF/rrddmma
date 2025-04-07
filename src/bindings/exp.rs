#[repr(C)]
#[derive(Clone, Copy)]
pub union imm_data_invalidated_rkey_union_t {
    pub imm_data: u32,
    pub invalidated_rkey: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct cqe_wait_t {
    pub cq: *mut ibv_cq,
    pub cq_count: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct wqe_enable_t {
    pub qp: *mut ibv_qp,
    pub wqe_count: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union task_t {
    pub rdma: rdma_t,
    pub atomic: atomic_t,
    pub cqe_wait: cqe_wait_t,
    pub wqe_enable: wqe_enable_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct calc_t {
    pub calc_op: ibv_exp_calc_op::Type,
    pub data_type: ibv_exp_calc_data_type::Type,
    pub data_size: ibv_exp_calc_data_size::Type,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union calc_op_t {
    pub calc: calc_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct dc_t {
    pub ah: *mut ibv_ah,
    pub dct_access_key: u64,
    pub dct_number: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct exp_bind_mw_t {
    pub mw: *mut ibv_mw,
    pub rkey: u32,
    pub bind_info: ibv_exp_mw_bind_info,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union bind_mw_tso_union_t {
    pub bind_mw: exp_bind_mw_t,
    pub tso: tso_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct rb_t {
    pub mem_repeat_block_list: *mut ibv_exp_mem_repeat_block,
    pub repeat_count: *mut size_t,
    pub stride_dim: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union mem_list_t {
    pub mem_reg_list: *mut ibv_exp_mem_region,
    pub rb: rb_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct umr_t {
    pub umr_type: u32,
    pub memory_objects: *mut ibv_exp_mkey_list_container,
    pub exp_access: u64,
    pub modified_mr: *mut ibv_mr,
    pub base_addr: u64,
    pub num_mrs: u32,
    pub mem_list: mem_list_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union inline_data_op_t {
    pub cmp_swap: ibv_exp_cmp_swap,
    pub fetch_add: ibv_exp_fetch_add,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct inline_data_t {
    pub op: inline_data_op_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union wr_data_t {
    pub inline_data: inline_data_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct masked_atomics_t {
    pub log_arg_sz: u32,
    pub remote_addr: u64,
    pub rkey: u32,
    pub wr_data: wr_data_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union ext_op_t {
    pub umr: umr_t,
    pub masked_atomics: masked_atomics_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ibv_exp_send_wr {
    pub wr_id: u64,
    pub next: *mut Self,
    pub sg_list: *mut ibv_sge,
    pub num_sge: c_int,
    pub exp_opcode: ibv_exp_wr_opcode::Type,
    pub reserved: c_int,
    pub ex: imm_data_invalidated_rkey_union_t,
    pub wr: wr_t,
    pub qp_type_xrc_remote_srq_num_union: qp_type_xrc_remote_srq_num_union_t,
    pub task: task_t,
    pub op: calc_op_t,
    pub dc: dc_t,
    pub bind_mw_tso_union: bind_mw_tso_union_t,
    pub exp_send_flags: u64,
    pub comp_mask: u32,
    pub ext_op: ext_op_t,
}

impl ibv_exp_send_wr {
    /// Set the immediate data.
    #[inline(always)]
    pub fn set_imm(&mut self, imm: u32) {
        // SAFETY: union of two `u32`s.
        unsafe { self.ex.imm_data = imm };
    }
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
        IBV_EXP_RET_ON_INVALID_COMP_MASK_compat!($val, $valid_mask, ::std::ptr::null_mut(), $func)
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

/// Create an experimental queue pair.
#[inline]
pub unsafe fn ibv_exp_create_qp(
    context: *mut ibv_context,
    qp_init_attr: *mut ibv_exp_qp_init_attr,
) -> *mut ibv_qp {
    let mask = (*qp_init_attr).comp_mask;

    if mask == ibv_exp_qp_init_attr_comp_mask::IBV_EXP_QP_INIT_ATTR_PD.0 {
        return ibv_create_qp((*qp_init_attr).pd, qp_init_attr as *mut ibv_qp_init_attr);
    }

    let vctx = verbs_get_exp_ctx_op!(context, lib_exp_create_qp);
    if vctx.is_null() {
        *__errno_location() = ENOSYS;
        std::ptr::null_mut()
    } else {
        IBV_EXP_RET_NULL_ON_INVALID_COMP_MASK_compat!(
            (*qp_init_attr).comp_mask,
            ibv_exp_qp_init_attr_comp_mask::IBV_EXP_QP_INIT_ATTR_RESERVED1.0 - 1,
            "ibv_exp_create_qp"
        );
        (*vctx).lib_exp_create_qp.unwrap()(context, qp_init_attr)
    }
}

/// Modify a queue pair.
#[inline]
pub unsafe fn ibv_exp_modify_qp(
    qp: *mut ibv_qp,
    attr: *mut ibv_exp_qp_attr,
    exp_attr_mask: u64,
) -> ::std::os::raw::c_int {
    let vctx = verbs_get_exp_ctx_op!((*qp).context, lib_exp_modify_qp);
    if vctx.is_null() {
        *__errno_location() = ENOSYS;
        ENOSYS
    } else {
        IBV_EXP_RET_EINVAL_ON_INVALID_COMP_MASK_compat!(
            (*attr).comp_mask,
            ibv_exp_qp_attr_comp_mask::IBV_EXP_QP_ATTR_RESERVED.0 - 1,
            "ibv_exp_modify_qp"
        );
        (*vctx).lib_exp_modify_qp.unwrap()(qp, attr, exp_attr_mask)
    }
}

/// Post a list of experimental work requests to a send queue.
#[inline]
pub unsafe fn ibv_exp_post_send(
    qp: *mut ibv_qp,
    wr: *mut ibv_exp_send_wr,
    bad_wr: *mut *mut ibv_exp_send_wr,
) -> ::std::os::raw::c_int {
    let vctx = verbs_get_exp_ctx_op!((*qp).context, drv_exp_post_send);
    if vctx.is_null() {
        -ENOSYS
    } else {
        (*vctx).drv_exp_post_send.unwrap()(qp, wr, bad_wr)
    }
}

/// Create an experimental shared receive queue.
#[inline]
pub unsafe fn ibv_exp_create_srq(
    context: *mut ibv_context,
    attr: *mut ibv_exp_create_srq_attr,
) -> *mut ibv_srq {
    let vctx = verbs_get_exp_ctx_op!(context, exp_create_srq);
    if vctx.is_null() {
        *__errno_location() = ENOSYS;
        std::ptr::null_mut()
    } else {
        IBV_EXP_RET_NULL_ON_INVALID_COMP_MASK_compat!(
            (*attr).comp_mask,
            IBV_EXP_CREATE_SRQ_RESERVED - 1,
            "ibv_exp_create_srq"
        );
        (*vctx).exp_create_srq.unwrap()(context, attr)
    }
}

/// Query device experimental attributes.
#[inline]
pub unsafe fn ibv_exp_query_device(
    context: *mut ibv_context,
    attr: *mut ibv_exp_device_attr,
) -> ::std::os::raw::c_int {
    let vctx = verbs_get_exp_ctx_op!(context, lib_exp_query_device);
    if vctx.is_null() {
        ENOSYS
    } else {
        if ((*attr).comp_mask
            & ibv_exp_device_attr_comp_mask::IBV_EXP_DEVICE_ATTR_COMP_MASK_2.0 as u32
            != 0)
        {
            IBV_EXP_RET_EINVAL_ON_INVALID_COMP_MASK_compat!(
                (*attr).comp_mask_2,
                ibv_exp_device_attr_comp_mask_2::IBV_EXP_DEVICE_ATTR_RESERVED_2.0 as u64 - 1,
                "ibv_exp_query_device"
            );
        }
        (*vctx).lib_exp_query_device.unwrap()(context, attr)
    }
}

/// Create a Dynamically-connected target.
#[inline]
pub unsafe fn ibv_exp_create_dct(
    context: *mut ibv_context,
    attr: *mut ibv_exp_dct_init_attr,
) -> *mut ibv_exp_dct {
    let vctx = verbs_get_exp_ctx_op!(context, create_dct);
    if vctx.is_null() {
        *__errno_location() = ENOSYS;
        std::ptr::null_mut()
    } else {
        IBV_EXP_RET_NULL_ON_INVALID_COMP_MASK_compat!(
            (*attr).comp_mask,
            ibv_exp_dct_init_attr_comp_mask::IBV_EXP_DCT_INIT_ATTR_RESERVED.0 - 1,
            "ibv_exp_create_dct"
        );
        pthread_mutex_lock(&mut (*context).mutex);
        let dct = (*vctx).create_dct.unwrap()(context, attr);
        if !dct.is_null() {
            (*dct).context = context;
        }
        pthread_mutex_unlock(&mut (*context).mutex);
        dct
    }
}

/// Destroy a Dynamically-connected target.
#[inline]
pub unsafe fn ibv_exp_destroy_dct(dct: *mut ibv_exp_dct) -> ::std::os::raw::c_int {
    let context = (*dct).context;
    let vctx = verbs_get_exp_ctx_op!(context, destroy_dct);
    if vctx.is_null() {
        *__errno_location() = ENOSYS;
        ENOSYS
    } else {
        pthread_mutex_lock(&mut (*context).mutex);
        let err = (*vctx).destroy_dct.unwrap()(dct);
        pthread_mutex_unlock(&mut (*context).mutex);
        err
    }
}

/// Query a experimental Dynamically-connected target.
#[inline]
pub unsafe fn ibv_exp_query_dct(
    dct: *mut ibv_exp_dct,
    attr: *mut ibv_exp_dct_attr,
) -> ::std::os::raw::c_int {
    let context = (*dct).context;
    let vctx = verbs_get_exp_ctx_op!(context, query_dct);
    if vctx.is_null() {
        *__errno_location() = ENOSYS;
        ENOSYS
    } else {
        IBV_EXP_RET_EINVAL_ON_INVALID_COMP_MASK_compat!(
            (*attr).comp_mask,
            ibv_exp_dct_attr_comp_mask::IBV_EXP_DCT_ATTR_RESERVED.0 - 1,
            "ibv_exp_query_dct"
        );
        pthread_mutex_lock(&mut (*context).mutex);
        let err = (*vctx).query_dct.unwrap()(dct, attr);
        pthread_mutex_unlock(&mut (*context).mutex);
        err
    }
}

/// Create an experimental CQ.
#[inline]
pub unsafe fn ibv_exp_create_cq(
    context: *mut ibv_context,
    cqe: ::std::os::raw::c_int,
    cq_context: *mut ::std::os::raw::c_void,
    channel: *mut ibv_comp_channel,
    comp_vector: ::std::os::raw::c_int,
    attr: *mut ibv_exp_cq_init_attr,
) -> *mut ibv_cq {
    let vctx = verbs_get_exp_ctx_op!(context, exp_create_cq);
    if vctx.is_null() {
        *__errno_location() = ENOSYS;
        std::ptr::null_mut()
    } else {
        IBV_EXP_RET_NULL_ON_INVALID_COMP_MASK_compat!(
            (*attr).comp_mask,
            IBV_EXP_CQ_INIT_ATTR_RESERVED1 - 1,
            "ibv_exp_create_cq"
        );
        pthread_mutex_lock(&mut (*context).mutex);
        let cq = (*vctx).exp_create_cq.unwrap()(context, cqe, channel, comp_vector, attr);
        if !cq.is_null() {
            (*cq).context = context;
            (*cq).channel = channel;
            if !channel.is_null() {
                (*channel).refcnt += 1;
            }
            (*cq).cq_context = cq_context;
            (*cq).comp_events_completed = 0;
            (*cq).async_events_completed = 0;
            pthread_mutex_init(&mut (*cq).mutex, std::ptr::null());
            pthread_cond_init(&mut (*cq).cond, std::ptr::null());
        }

        pthread_mutex_unlock(&mut (*context).mutex);
        cq
    }
}

/// Poll a CQ for an experimental WC.
#[inline]
pub unsafe fn ibv_exp_poll_cq(
    ibcq: *mut ibv_cq,
    num_entries: ::std::os::raw::c_int,
    wc: *mut ibv_exp_wc,
    wc_size: u32,
) -> ::std::os::raw::c_int {
    let vctx = verbs_get_exp_ctx_op!((*ibcq).context, drv_exp_ibv_poll_cq);
    if vctx.is_null() {
        -ENOSYS
    } else {
        (*vctx).drv_exp_ibv_poll_cq.unwrap()(ibcq, num_entries, wc, wc_size)
    }
}

/// Query device values.
#[inline]
pub unsafe fn ibv_exp_query_values(
    context: *mut ibv_context,
    q_values: ::std::os::raw::c_int,
    values: *mut ibv_exp_values,
) -> ::std::os::raw::c_int {
    let vctx = verbs_get_exp_ctx_op!(context, drv_exp_query_values);
    if vctx.is_null() {
        -ENOSYS
    } else {
        IBV_EXP_RET_EINVAL_ON_INVALID_COMP_MASK_compat!(
            (*values).comp_mask,
            IBV_EXP_VALUES_RESERVED - 1,
            "ibv_exp_query_values"
        );
        (*vctx).drv_exp_query_values.unwrap()(context, q_values, values)
    }
}

/// Convert device timestamp to system clock.
#[inline]
pub unsafe fn ibv_exp_cqe_ts_to_ns(clock_info: *const ibv_exp_clock_info, ts: u64) -> u64 {
    IBV_EXP_RET_ZERO_ON_INVALID_COMP_MASK_compat!(
        (*clock_info).comp_mask,
        IBV_EXP_CLOCK_INFO_RESERVED - 1,
        "ibv_exp_cqe_ts_to_ns"
    );

    let mut delta = (ts - (*clock_info).cycles) & (*clock_info).mask;
    let mut nsec = (*clock_info).nsec;

    if delta > (*clock_info).mask / 2 {
        delta -= ((*clock_info).cycles - ts) & (*clock_info).mask;
        nsec -= ((delta * (*clock_info).mult as u64) - (*clock_info).frac) >> (*clock_info).shift;
    } else {
        nsec += ((delta * (*clock_info).mult as u64) + (*clock_info).frac) >> (*clock_info).shift;
    }
    nsec
}

pub const IBV_EXP_DCT_STATE_ACTIVE: u8 = 0;
pub const IBV_EXP_DCT_STATE_DRAINING: u8 = 1;
pub const IBV_EXP_DCT_STATE_DRAINED: u8 = 2;
