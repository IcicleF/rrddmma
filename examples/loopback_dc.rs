//! Loopback hardware verification of DCT + DCI on the rdma-core path.
//!
//! The DCT is created via `mlx5dv_create_qp` (MLX5DV_DCTYPE_DCT) and the DCI
//! as a raw-WQE-capable driver QP (MLX5DV_DCTYPE_DCI). Tests:
//! 1. SEND over DC  -> received via the DCT's SRQ
//! 2. RDMA WRITE over DC
//! 3. 16B masked CAS over DC (raw WQE + AV segment)
//! 4. 16B masked FAA over DC (raw WQE + AV segment)

use std::ptr::{self, NonNull};

use rrddmma::hi::RegisteredMem;
use rrddmma::lo::prelude::*;
use rrddmma::lo::qp::ExtCompareSwapParams;

const N: usize = 16;

fn nn(v: &[u64]) -> NonNull<u64> {
    NonNull::new(v.as_ptr() as *mut u64).unwrap()
}

/// Poll one completion and demand success + the expected `wr_id`.
fn expect_wc(cq: &Cq, wr_id: u64, what: &str) -> Wc {
    let wc = cq
        .poll_one(true)
        .unwrap()
        .unwrap_or_else(|| panic!("{what}: no completion"));
    if wc.status() != WcStatus::Success {
        panic!(
            "{what}: wr_id={} status={:?} vendor_err={:#x}",
            wc.wr_id(),
            wc.status(),
            wc.0.vendor_err
        );
    }
    if wc.wr_id() != wr_id {
        panic!("{what}: unexpected wr_id {}", wc.wr_id());
    }
    wc
}

fn main() -> anyhow::Result<()> {
    let Nic { context, ports } = Nic::open("ibp160s0f1")?;
    let pd = Pd::new(&context)?;
    let cq = Cq::new(&context, Cq::DEFAULT_CQ_DEPTH)?;

    // DCT: the target side. It owns an SRQ for its receive queue.
    let dct = Dct::builder()
        .pd(&pd)
        .cq(&cq)
        .port(&ports[0], None)
        .build(&context)?;
    println!("DCT created: {:?}, num={:#x}", dct, dct.dct_num());

    // DCI: the initiator side, with raw-WQE + extended send ops.
    let mut dci = Qp::builder()
        .qp_type(QpType::Driver)
        .caps(QpCaps::for_dc_ini())
        .send_cq(&cq)
        .recv_cq(&cq)
        .sq_sig_all(false)
        .build(&pd)?;
    dci.bind_local_port(&ports[0], None)?;
    dci.set_dc_peer(dci.make_peer(dct.endpoint())?);
    println!("DCI ready (state={:?})", dci.state());

    // -------- Test 1: SEND over DC --------
    let rx = RegisteredMem::new(&pd, 4096)?;
    dct.srq().recv(&[rx.slice(0, 64).unwrap()], 100)?;

    let mut tx = RegisteredMem::new(&pd, 4096)?;
    unsafe {
        ptr::write_volatile(tx.as_mut_ptr() as *mut u64, 0xcafecafecafecafe);
        ptr::write_volatile(tx.as_mut_ptr().add(8) as *mut u64, 0x1145141919810114);
    }
    dci.send(&[tx.slice(0, 32).unwrap()], None, None, 1, true, false)?;

    // Both the send completion (DCI) and the recv completion (DCT SRQ) land
    // on the same CQ; order between them is not assumed.
    let mut send_done = false;
    let mut recv_done = false;
    for _ in 0..2 {
        let wc = cq.poll_one(true).unwrap().unwrap();
        if wc.status() != WcStatus::Success {
            panic!(
                "test1 send/recv CQE: wr_id={} status={:?} vendor_err={:#x}",
                wc.wr_id(),
                wc.status(),
                wc.0.vendor_err
            );
        }
        match wc.wr_id() {
            1 => send_done = true,
            100 => recv_done = true,
            v => panic!("unexpected wr_id: {}", v),
        }
    }
    assert!(send_done && recv_done);
    let (r0, r1) = (
        unsafe { ptr::read_volatile(rx.as_ptr() as *const u64) },
        unsafe { ptr::read_volatile(rx.as_ptr().add(8) as *const u64) },
    );
    assert_eq!(r0, 0xcafecafecafecafe, "test1 recv slot0");
    assert_eq!(r1, 0x1145141919810114, "test1 recv slot1");
    println!("test1 SEND over DC: PASSED");

    // -------- Test 2: RDMA WRITE over DC --------
    let mut tgt = RegisteredMem::new(&pd, 4096)?;
    unsafe { ptr::write_bytes(tgt.as_mut_ptr(), 0, 64) };
    let remote = unsafe { tgt.mr().as_remote().slice_unchecked(0, 64) };
    dci.write(&[tx.slice(0, 64).unwrap()], &remote, 2, None, true)?;
    let wc = expect_wc(&cq, 2, "test2 write CQE");
    assert_eq!(wc.wr_id(), 2);
    assert_eq!(wc.status(), WcStatus::Success, "test2 write CQE");
    let (t0, t1) = (
        unsafe { ptr::read_volatile(tgt.as_ptr() as *const u64) },
        unsafe { ptr::read_volatile(tgt.as_ptr().add(8) as *const u64) },
    );
    assert_eq!(t0, 0xcafecafecafecafe, "test2 target slot0");
    assert_eq!(t1, 0x1145141919810114, "test2 target slot1");
    println!("test2 RDMA WRITE over DC: PASSED");

    // -------- Test 3: 16B masked CAS over DC --------
    let mut res = RegisteredMem::new(&pd, 4096)?;
    let remote16 = unsafe { tgt.mr().as_remote().slice_unchecked(0, N) };

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
    {
        let local = res.slice(0, N).unwrap();
        unsafe { dci.ext_compare_swap::<N>(local, remote16, params, 3, true)? };
    }
    let wc = expect_wc(&cq, 3, "test3 ext CAS CQE");
    assert_eq!(wc.wr_id(), 3);
    assert_eq!(wc.status(), WcStatus::Success, "test3 ext CAS CQE");
    let (r0, r1) = read_res!();
    let (t0, t1) = read_tgt!();
    assert_eq!(r0, 0x0123456789abcdef, "test3 result slot0");
    assert_eq!(r1, 0x1122334455667788, "test3 result slot1");
    assert_eq!(t0, 0xdeadbeefcafebabe, "test3 target slot0 swapped");
    assert_eq!(t1, 0xfeedfacefeedface, "test3 target slot1 swapped");
    println!("test3 16B masked CAS over DC: PASSED");

    // -------- Test 4: 16B masked FAA over DC --------
    set_tgt!(0x0123456789abcdef, 0x1122334455667788);
    clear_res!();
    let add = [1u64, 1u64];
    let fa_mask = [0u64; 2];
    {
        let local = res.slice(0, N).unwrap();
        unsafe {
            dci.ext_fetch_add::<N>(local, remote16, nn(&add), nn(&fa_mask), 4, true)?;
        }
    }
    let wc = expect_wc(&cq, 4, "test4 ext FAA CQE");
    assert_eq!(wc.wr_id(), 4);
    assert_eq!(wc.status(), WcStatus::Success, "test4 ext FAA CQE");
    let (r0, r1) = read_res!();
    let (t0, t1) = read_tgt!();
    assert_eq!(r0, 0x0123456789abcdef, "test4 result slot0");
    assert_eq!(r1, 0x1122334455667788, "test4 result slot1");
    assert_eq!(t0, 0x0123456789abcdf0, "test4 target slot0 +1");
    assert_eq!(t1, 0x1122334455667789, "test4 target slot1 +1");
    println!("test4 16B masked FAA over DC: PASSED");

    println!("all 4 tests PASSED");
    Ok(())
}
