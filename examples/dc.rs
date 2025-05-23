#[cfg(not(feature = "legacy"))]
fn main() {
    eprintln!("DC is not yet implemented on MLNX v5.x");
}

#[cfg(feature = "legacy")]
fn main() -> anyhow::Result<()> {
    use rrddmma::{hi::RegisteredMem, prelude::*};
    use std::{thread, time::Duration};

    fn client(ep: QpEndpoint) -> anyhow::Result<()> {
        fn make_dci(dev: &str) -> anyhow::Result<Qp> {
            let Nic { context, ports } = Nic::finder().dev_name(dev).probe()?;
            let pd = Pd::new(&context)?;
            let cq = Cq::new(&context, Cq::DEFAULT_CQ_DEPTH)?;
            let mut qp = Qp::builder()
                .qp_type(QpType::DcIni)
                .caps(QpCaps::for_dc_ini())
                .send_cq(&cq)
                .recv_cq(&cq)
                .sq_sig_all(true)
                .build(&pd)?;
            qp.bind_local_port(&ports[0], None)?;
            Ok(qp)
        }

        let qp = make_dci("mlx5_0").inspect_err(|e| eprintln!("Err: {:?}", e))?;
        let peer = qp.make_peer(ep)?;

        // Send the message to the server.
        let mem = RegisteredMem::new_with_content(qp.pd(), "Hello, rrddmma!".as_bytes())?;
        for _ in 0..100000 {
            qp.send(&[mem.as_slice()], Some(&peer), None, 0, true, true)?;
            qp.scq().poll_one_blocking()?;
        }
        Ok(())
    }

    fn make_dct(dev: &str) -> anyhow::Result<Dct> {
        let Nic { context, ports } = Nic::finder().dev_name(dev).probe()?;
        let pd = Pd::new(&context)?;
        let cq = Cq::new(&context, Cq::DEFAULT_CQ_DEPTH)?;
        let dct = Dct::builder()
            .pd(&pd)
            .cq(&cq)
            .port(&ports[0], None)
            .inline_size(64)
            .build(&context)?;
        Ok(dct)
    }

    let dct = make_dct("mlx5_0")?;
    let ep = dct.endpoint();
    let cli = thread::spawn(move || client(ep));

    // Receive a message from the client.
    let mem = RegisteredMem::new(dct.pd(), 4096)?;
    let mut total_recv_time = Duration::ZERO;
    for i in 0..100000 {
        total_recv_time += {
            let start = std::time::Instant::now();
            dct.srq().recv(&[mem.as_slice()], 0)?;
            start.elapsed()
        };

        let wc = dct.cq().poll_one_blocking()?;
        if (i + 1) % 10000 == 0 {
            println!("{}", String::from_utf8_lossy(&mem[..wc.ok()?]));
        }
    }
    cli.join().unwrap()?;

    println!("Average recv time: {:?}", total_recv_time / 100000);
    Ok(())
}
