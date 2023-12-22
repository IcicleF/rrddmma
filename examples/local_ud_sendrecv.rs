use rrddmma::{ctrl, prelude::*, wrap::RegisteredMem};
use std::{net::Ipv4Addr, sync::mpsc::*, thread};

fn make_qp(dev: &str) -> anyhow::Result<Qp> {
    let Nic { context, ports } = Nic::finder().dev_name(dev).probe()?;
    let pd = Pd::new(&context)?;
    let cq = Cq::new(&context, Cq::DEFAULT_CQ_DEPTH)?;
    let mut qp = Qp::builder()
        .qp_type(QpType::Ud)
        .caps(QpCaps::default())
        .send_cq(&cq)
        .recv_cq(&cq)
        .sq_sig_all(true)
        .build(&pd)?;
    qp.bind_local_port(&ports[0], None)?;
    Ok(qp)
}

fn client(rx: Receiver<()>) -> anyhow::Result<()> {
    let mut qp = make_qp("mlx5_0")?;
    let peer = ctrl::Connecter::new(Some(Ipv4Addr::LOCALHOST))?
        .connect(&mut qp)?
        .unwrap();
    rx.recv()?;

    // Send the message to the server.
    let mem = RegisteredMem::new_with_content(qp.pd(), "Hello, rrddmma!".as_bytes())?;
    qp.send(&[mem.as_slice()], Some(&peer), None, 0, true, true)?;
    qp.scq().poll_one_blocking()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let (tx, rx) = channel();
    let cli = thread::spawn(move || client(rx));

    let mut qp = make_qp("mlx5_0")?;
    ctrl::Connecter::new(None)?.connect(&mut qp)?;

    // Receive a message from the client.
    let mem = RegisteredMem::new(qp.pd(), 4096)?;
    qp.recv(&[mem.as_slice()], 0)?;
    tx.send(())?;
    let wc = qp.rcq().poll_one_blocking()?;
    println!("{}", String::from_utf8_lossy(&mem[Qp::GRH_SIZE..wc.ok()?]));

    cli.join().unwrap()?;
    Ok(())
}
