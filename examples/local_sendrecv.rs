use rrddmma::{wrap::RegisteredMem, *};
use std::{net::Ipv4Addr, thread};

fn client(pd: Pd) -> anyhow::Result<()> {
    let cq = Cq::new(pd.context().clone(), Cq::DEFAULT_CQ_DEPTH)?;
    let qp = Qp::new(pd.clone(), QpBuilder::default_rc(cq))?;

    ctrl::Connecter::new(Some(Ipv4Addr::LOCALHOST))?.connect(&qp)?;

    // Send the message to the server.
    let mem = RegisteredMem::new_with_content(pd, "Hello, rrddmma!".as_bytes())?;
    qp.send(&[mem.as_slice()], None, None, 0, true, true)?;
    qp.scq().poll_one_blocking().and_then(|wc| wc.result())?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    // Create RDMA context using the first active port found, use the GID index 3.
    // This should be an appropriate configuration for both IB and RoCE.
    let context = Context::open(None, 1, 3)?;
    let pd = Pd::new(context)?;

    let cli = {
        let pd = pd.clone();
        thread::spawn(move || client(pd))
    };

    let cq = Cq::new(pd.context().clone(), Cq::DEFAULT_CQ_DEPTH)?;
    let qp = Qp::new(pd.clone(), QpBuilder::default_rc(cq))?;

    ctrl::Connecter::new(None)?.connect(&qp)?;

    // Receive a message from the client.
    let mem = RegisteredMem::new(pd, 4096)?;
    qp.recv(&[mem.as_slice()], 0)?;
    let wc = qp.rcq().poll_one_blocking()?;
    println!("{}", String::from_utf8_lossy(&mem[..wc.result()?]));

    cli.join().unwrap()?;
    Ok(())
}
