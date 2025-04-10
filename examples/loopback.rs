use quanta::Instant;
use rrddmma::{hi::RegisteredMem, lo::prelude::*};

fn make_qp(dev: &str) -> anyhow::Result<Qp> {
    let Nic { context, ports } = Nic::open(dev)?;
    let pd = Pd::new(&context)?;
    let cq = Cq::new(&context, Cq::DEFAULT_CQ_DEPTH)?;
    let mut qp = Qp::builder()
        .qp_type(QpType::Rc)
        .caps(QpCaps::default())
        .send_cq(&cq)
        .recv_cq(&cq)
        .sq_sig_all(true)
        .build(&pd)?;
    qp.bind_local_port(&ports[0], None)?;
    Ok(qp)
}

fn main() -> anyhow::Result<()> {
    let mut qp = make_qp("mlx5_0")?;
    let ep = qp.endpoint().unwrap();
    qp.bind_peer(ep)?;

    // Receive a message from the client.
    let mut mem0 = RegisteredMem::new(qp.pd(), 4096)?;
    let mut mem1 = RegisteredMem::new(qp.pd(), 4096)?;
    unsafe { std::ptr::write_bytes(mem0.as_mut_ptr(), 0x14, 4096) };

    let time = Instant::now();
    unsafe { std::ptr::copy_nonoverlapping(mem0.as_ptr(), mem1.as_mut_ptr(), 4096) };
    println!("Time elapsed (memcpy): {:?}", time.elapsed());

    assert_eq!(
        unsafe { std::slice::from_raw_parts(mem1.as_ptr(), 4096) },
        &[0x14; 4096]
    );

    unsafe { std::ptr::write_bytes(mem1.as_mut_ptr(), 0x0, 4096) };
    let tgt = mem1.mr().as_remote();

    let time = Instant::now();

    qp.write(&[mem0.as_slice()], &tgt, 0, None, false)?;
    unsafe {
        qp.read(
            &[mem1.slice_unchecked(0, 1)],
            &tgt.slice_unchecked(0, 1),
            0,
            true,
        )?;
    }
    qp.scq().poll_one_blockingly_consumed();

    println!("Time elapsed (RDMA): {:?}", time.elapsed());
    assert_eq!(
        unsafe { std::slice::from_raw_parts(mem1.as_ptr(), 4096) },
        &[0x14; 4096]
    );

    Ok(())
}
