use rrddmma::{wrap::RegisteredMem, *};
use std::{net::Ipv4Addr, thread};

fn client() -> anyhow::Result<()> {
    let Nic { context, ports } = Nic::finder().dev_name("mlx5_0").probe()?;
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
    ctrl::Connecter::new(Some(Ipv4Addr::LOCALHOST))?.connect(&mut qp)?;

    // Send the message to the server.
    let mem = RegisteredMem::new_with_content(qp.pd(), "Hello, rrddmma!".as_bytes())?;
    qp.send(&[mem.as_slice()], None, None, 0, true, true)?;
    qp.scq().poll_one_blocking()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = thread::spawn(client);

    let Nic { context, ports } = Nic::finder().dev_name("mlx5_0").probe()?;
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
    ctrl::Connecter::new(None)?.connect(&mut qp)?;

    // Receive a message from the client.
    let mem = RegisteredMem::new(qp.pd(), 4096)?;
    qp.recv(&[mem.as_slice()], 0)?;
    let wc = qp.rcq().poll_one_blocking()?;
    println!("{}", String::from_utf8_lossy(&mem[..wc.ok()?]));

    cli.join().unwrap()?;
    Ok(())
}
