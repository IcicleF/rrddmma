use rrddmma::{wrap::RegisteredMem, *};
use std::{net::Ipv4Addr, thread};

fn client(pd: Pd) -> anyhow::Result<()> {
    let cq = Cq::new(pd.context(), Cq::DEFAULT_CQ_DEPTH)?;
    let qp = Qp::new(pd.clone(), QpInitAttr::default_rc(cq))?;

    ctrl::Connecter::new(Some(Ipv4Addr::LOCALHOST))?.connect(&qp)?;

    // Send the message to the server.
    let mem = RegisteredMem::new_with_content(pd.clone(), "Hello, rrddmma!".as_bytes())?;
    qp.send(&[mem.as_slice()], None, None, 0, true, true)?;
    qp.scq().poll_nocqe_blocking(1)?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    // Create RDMA context using the first device found, on its first port and
    // use the GID index 0. This is an appropriate setting on an Infiniband
    // fabric. For RoCE, you may need to change the GID index.
    let context = Context::open(None, 1, 0)?;
    let pd = Pd::new(context)?;

    let cli = {
        let pd = pd.clone();
        thread::spawn(move || client(pd))
    };

    let cq = Cq::new(pd.context(), Cq::DEFAULT_CQ_DEPTH)?;
    let qp = Qp::new(pd.clone(), QpInitAttr::default_rc(cq))?;

    ctrl::Connecter::new(None)?.connect(&qp)?;

    // Receive a message from the client.
    let mem = RegisteredMem::new(pd.clone(), 4096)?;
    qp.recv(&[mem.as_slice()], 0)?;
    let mut wc = [Wc::default()];
    qp.rcq().poll_blocking(&mut wc)?;
    println!("{}", String::from_utf8_lossy(&mem[..wc[0].result()?]));

    cli.join().unwrap()?;
    Ok(())
}
