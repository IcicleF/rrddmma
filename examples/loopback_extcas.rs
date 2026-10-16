//! Loopback hardware verification of 16B masked atomic CAS/FAA posted as
//! MLX5 raw WQEs (`mlx5dv_wr_raw_wqe`).
//!
//! Tests (target memory is read in host order; result buffer is BE per 8B slot
//! for 16B ops, and native byte order for 8B ops):
//! 1. 16B masked CAS, compare matches  -> swap happens, result = old value
//! 2. 16B masked CAS, compare mismatch -> no swap, result = current value
//! 3. 16B masked FAA, mask = 0         -> plain add (+1 both slots), result = old value
//! 4. 16B masked FAA, per-bit ADD/XOR  -> slot0 arithmetic +1, slot1 XOR toggle,
//!                                        result = old value
//! 5. 8B masked CAS, compare matches   -> swap happens, result = old value
//! 6. 8B masked FAA, mask = 0          -> plain add (+1), result = old value

use std::ptr::{self, NonNull};

use rrddmma::hi::RegisteredMem;
use rrddmma::lo::prelude::*;
use rrddmma::lo::qp::ExtCompareSwapParams;

const N: usize = 16;

fn nn(v: &[u64]) -> NonNull<u64> {
    NonNull::new(v.as_ptr() as *mut u64).unwrap()
}

fn main() -> anyhow::Result<()> {
    let Nic { context, ports } = Nic::open("ibp160s0f1")?;
    let pd = Pd::new(&context)?;
    let cq = Cq::new(&context, Cq::DEFAULT_CQ_DEPTH)?;
    let mut qp = Qp::builder()
        .qp_type(QpType::Rc)
        .caps(QpCaps::default())
        .send_cq(&cq)
        .recv_cq(&cq)
        .sq_sig_all(false)
        .build(&pd)?;
    qp.bind_local_port(&ports[0], None)?;
    let ep = qp.endpoint().unwrap();
    qp.bind_peer(ep)?;

    let mut tgt = RegisteredMem::new(qp.pd(), 4096)?;
    let mut res = RegisteredMem::new(qp.pd(), 4096)?;
    let remote = unsafe { tgt.mr().as_remote().slice_unchecked(0, N) };

    macro_rules! set_tgt {
        ($a:expr, $b:expr) => {{
            unsafe { ptr::write_volatile(tgt.as_mut_ptr() as *mut u64, $a) };
            unsafe { ptr::write_volatile(tgt.as_mut_ptr().add(8) as *mut u64, $b) };
        }};
    }
    macro_rules! read_tgt {
        () => {{
            (
                unsafe { ptr::read_volatile(tgt.as_ptr() as *const u64) },
                unsafe { ptr::read_volatile(tgt.as_ptr().add(8) as *const u64) },
            )
        }};
    }
    macro_rules! read_res {
        () => {{
            (
                unsafe { ptr::read_volatile(res.as_ptr() as *const u64).swap_bytes() },
                unsafe { ptr::read_volatile(res.as_ptr().add(8) as *const u64).swap_bytes() },
            )
        }};
    }
    macro_rules! clear_res {
        () => {{
            unsafe { ptr::write_bytes(res.as_mut_ptr(), 0, N) };
        }};
    }
    macro_rules! post {
        ($wr_id:expr, $call:expr) => {{
            let local = res.slice(0, N).unwrap();
            unsafe { $call(&qp, local, remote, $wr_id)? };
            let wc = cq.poll_one_blockingly_consumed();
            assert_eq!(wc.wr_id(), $wr_id);
        }};
    }

    // -------- Test 1: 16B masked CAS, positive --------
    set_tgt!(0x0123456789abcdef, 0x1122334455667788);
    clear_res!();
    let cmp = [0x0123456789abcdef, 0x1122334455667788];
    let swap = [0xdeadbeefcafebabe, 0xfeedfacefeedface];
    let cmp_mask = [!0u64; 2];
    let swap_mask = [!0u64; 2];
    let params = ExtCompareSwapParams {
        compare: nn(&cmp),
        swap: nn(&swap),
        compare_mask: nn(&cmp_mask),
        swap_mask: nn(&swap_mask),
    };
    post!(1, |qp: &Qp, local, remote, id| qp.ext_compare_swap::<N>(
        local, remote, params, id, true
    ));
    let (r0, r1) = read_res!();
    let (t0, t1) = read_tgt!();
    assert_eq!(r0, 0x0123456789abcdef, "test1 result slot0");
    assert_eq!(r1, 0x1122334455667788, "test1 result slot1");
    assert_eq!(t0, 0xdeadbeefcafebabe, "test1 target slot0");
    assert_eq!(t1, 0xfeedfacefeedface, "test1 target slot1");
    println!("test1 16B masked CAS (positive): PASSED");

    // -------- Test 2: 16B masked CAS, negative --------
    clear_res!();
    let cmp = [0u64, 0u64]; // mismatch
    let swap = [0x1111111111111111, 0x2222222222222222];
    let params = ExtCompareSwapParams {
        compare: nn(&cmp),
        swap: nn(&swap),
        compare_mask: nn(&cmp_mask),
        swap_mask: nn(&swap_mask),
    };
    post!(2, |qp: &Qp, local, remote, id| qp.ext_compare_swap::<N>(
        local, remote, params, id, true
    ));
    let (r0, r1) = read_res!();
    let (t0, t1) = read_tgt!();
    assert_eq!(r0, 0xdeadbeefcafebabe, "test2 result slot0");
    assert_eq!(r1, 0xfeedfacefeedface, "test2 result slot1");
    assert_eq!(t0, 0xdeadbeefcafebabe, "test2 target slot0 unchanged");
    assert_eq!(t1, 0xfeedfacefeedface, "test2 target slot1 unchanged");
    println!("test2 16B masked CAS (negative): PASSED");

    // -------- Test 3: 16B masked FAA, plain add (mask = 0 -> arithmetic) --------
    set_tgt!(0x0123456789abcdef, 0x1122334455667788);
    clear_res!();
    let add = [1u64, 1u64];
    let fa_mask = [0u64; 2];
    post!(3, |qp: &Qp, local, remote, id| qp.ext_fetch_add::<N>(
        local,
        remote,
        nn(&add),
        nn(&fa_mask),
        id,
        true
    ));
    let (r0, r1) = read_res!();
    let (t0, t1) = read_tgt!();
    assert_eq!(r0, 0x0123456789abcdef, "test3 result slot0");
    assert_eq!(r1, 0x1122334455667788, "test3 result slot1");
    assert_eq!(t0, 0x0123456789abcdf0, "test3 target slot0 +1");
    assert_eq!(t1, 0x1122334455667789, "test3 target slot1 +1");
    println!("test3 16B masked FAA (plain add): PASSED");

    // -------- Test 4: 16B masked FAA, per-bit ADD/XOR selection --------
    // mask bit = 0 -> the add bit is added arithmetically (with carry);
    // mask bit = 1 -> the add bit is XOR-ed in. Slot0: +1, slot1: toggle bit0.
    set_tgt!(0x0123456789abcdef, 0x1122334455667789);
    clear_res!();
    let fa_mask = [0u64, !0u64];
    post!(4, |qp: &Qp, local, remote, id| qp.ext_fetch_add::<N>(
        local,
        remote,
        nn(&add),
        nn(&fa_mask),
        id,
        true
    ));
    let (r0, r1) = read_res!();
    let (t0, t1) = read_tgt!();
    assert_eq!(r0, 0x0123456789abcdef, "test4 result slot0");
    assert_eq!(r1, 0x1122334455667789, "test4 result slot1");
    assert_eq!(t0, 0x0123456789abcdf0, "test4 target slot0 +1 (arithmetic)");
    // XOR toggle: ...89 -> ...88, whereas an arithmetic +1 would give ...8a.
    assert_eq!(t1, 0x1122334455667788, "test4 target slot1 XOR bit0");
    println!("test4 16B masked FAA (per-bit ADD/XOR): PASSED");

    // -------- Test 5: 8B masked CAS, positive --------
    unsafe { ptr::write_volatile(tgt.as_mut_ptr() as *mut u64, 0x0123456789abcdef) };
    clear_res!();
    let remote8 = unsafe { tgt.mr().as_remote().slice_unchecked(0, 8) };
    let cmp8 = [0x0123456789abcdefu64];
    let swap8 = [0xdeadbeefcafebabeu64];
    let cmp_mask8 = [!0u64];
    let swap_mask8 = [!0u64];
    let params8 = ExtCompareSwapParams {
        compare: nn(&cmp8),
        swap: nn(&swap8),
        compare_mask: nn(&cmp_mask8),
        swap_mask: nn(&swap_mask8),
    };
    {
        let local = res.slice(0, 8).unwrap();
        unsafe { qp.ext_compare_swap::<8>(local, remote8, params8, 5, true)? };
    }
    let wc = cq.poll_one_blockingly_consumed();
    assert_eq!(wc.wr_id(), 5);
    // NOTE: 8B atomics return the old value in *native* byte order (the
    // legacy mlx5 atomic response path), unlike 16B ops (BE per 8B slot).
    let r0 = unsafe { ptr::read_volatile(res.as_ptr() as *const u64) };
    let t0 = unsafe { ptr::read_volatile(tgt.as_ptr() as *const u64) };
    assert_eq!(r0, 0x0123456789abcdef, "test5 result");
    assert_eq!(t0, 0xdeadbeefcafebabe, "test5 target swapped");
    println!("test5 8B masked CAS (positive): PASSED");

    // -------- Test 6: 8B masked FAA, plain add --------
    unsafe { ptr::write_volatile(tgt.as_mut_ptr() as *mut u64, 0x0123456789abcdee) };
    let add8 = [1u64];
    let fa_mask8 = [0u64];
    {
        let local = res.slice(0, 8).unwrap();
        unsafe { qp.ext_fetch_add::<8>(local, remote8, nn(&add8), nn(&fa_mask8), 6, true)? };
    }
    let wc = cq.poll_one_blockingly_consumed();
    assert_eq!(wc.wr_id(), 6);
    let r0 = unsafe { ptr::read_volatile(res.as_ptr() as *const u64) };
    let t0 = unsafe { ptr::read_volatile(tgt.as_ptr() as *const u64) };
    assert_eq!(r0, 0x0123456789abcdee, "test6 result");
    assert_eq!(t0, 0x0123456789abcdef, "test6 target +1");
    println!("test6 8B masked FAA (plain add): PASSED");

    println!("all 6 tests PASSED");
    Ok(())
}
