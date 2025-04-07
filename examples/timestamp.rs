#[cfg(not(feature = "legacy"))]
fn main() {
    eprintln!("cq_ex features is not yet implemented for MLNX_OFED v5.x+.");
}

#[cfg(feature = "legacy")]
fn main() -> anyhow::Result<()> {
    use rrddmma::{prelude::*, wrap::RegisteredMem};

    fn make_qp(dev: &str) -> anyhow::Result<Qp> {
        let Nic { context, ports } = Nic::finder().dev_name(dev).probe()?;
        let pd = Pd::new(&context)?;
        let cq = Cq::new_exp(&context, Cq::DEFAULT_CQ_DEPTH)?;
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

    let mut qp = make_qp("mlx5_0")?;
    let ep = qp.endpoint().unwrap();
    qp.bind_peer(ep)?;

    // Receive a message from the client.
    let mut mem0 = RegisteredMem::new(qp.pd(), 4096)?;
    let mut mem1 = RegisteredMem::new(qp.pd(), 4096)?;
    unsafe { std::ptr::write_bytes(mem0.as_mut_ptr(), 0x14, 4096) };
    unsafe { std::ptr::write_bytes(mem1.as_mut_ptr(), 0x0, 4096) };

    qp.recv(&[mem1.as_slice()], 0)?;
    qp.send(&[mem0.as_slice()], None, None, 0, true, false)?;
    qp.scq().poll_one_blocking_consumed();

    let mut wc = vec![ExpWc::default(); 1];
    while qp.rcq().exp_poll_into(&mut wc)? == 0 {}

    let ts = wc[0].timestamp();
    if let Some(ts) = ts {
        use chrono::DateTime;
        let (secs, nsecs) = (ts / 1_000_000_000, ts % 1_000_000_000);
        println!(
            "Timestamp: {:?}",
            DateTime::from_timestamp(secs as i64, nsecs as u32)
        );
    } else {
        eprintln!("Failed to get timestamp from wc!");
    }

    Ok(())
}
