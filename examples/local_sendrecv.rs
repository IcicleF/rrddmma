use rrddmma::*;
use std::{net::Ipv4Addr, thread};

fn client(pd: Pd) -> anyhow::Result<()> {
    let buf = "Hello rrddmma!".as_bytes().to_vec();
    let mr = Mr::reg_slice(pd.clone(), &buf)?;
    let cq = Cq::new(pd.context(), Cq::DEFAULT_CQ_DEPTH).unwrap();
    let qp = Qp::new(
        pd.clone(),
        QpInitAttr::new(cq.clone(), cq.clone(), QpCaps::default(), QpType::RC, true),
    )?;

    ctrl::Connecter::new(Some(Ipv4Addr::LOCALHOST))?.connect(&qp)?;

    // Post a send to the server.
    qp.send(&[mr.as_slice()], 0, true, true)?;
    qp.scq().poll_nocqe_blocking(1)?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    // Create RDMA context using the first device found, on its first port and
    // use the GID index 0. This is an appropriate setting on an Infiniband
    // fabric. For RoCE, you may need to change the GID index.
    let context = Context::open(None, 1, 0)?;
    let pd = Pd::new(context.clone())?;

    let cli = {
        let pd = pd.clone();
        thread::spawn(move || client(pd))
    };

    let buf = vec![0u8; 4096];
    let mr = Mr::reg_slice(pd.clone(), &buf)?;
    let cq = Cq::new(pd.context(), Cq::DEFAULT_CQ_DEPTH).unwrap();
    let qp = Qp::new(
        pd.clone(),
        QpInitAttr::new(cq.clone(), cq.clone(), QpCaps::default(), QpType::RC, true),
    )?;

    ctrl::Connecter::new(None)?.connect(&qp)?;

    // Receive a message from the client.
    qp.recv(&[mr.as_slice()], 0)?;
    let mut wc = [Wc::default()];
    qp.rcq().poll_blocking(&mut wc)?;

    let msg = String::from_utf8_lossy(&buf[..wc[0].result()?]);
    println!("{}", msg);

    cli.join().unwrap()?;

    Ok(())
}
