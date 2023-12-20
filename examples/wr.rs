use rrddmma::{wrap::RegisteredMem, *};
use std::{net::Ipv4Addr, thread};

fn make_qp(dev: &str) -> anyhow::Result<Qp> {
    let Nic { context, ports } = Nic::finder().dev_name(dev).probe()?;
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

fn client() -> anyhow::Result<()> {
    let mut qp = make_qp("mlx5_0")?;
    ctrl::Connecter::new(Some(Ipv4Addr::LOCALHOST))?.connect(&mut qp)?;

    // Send the message to the server.
    let mem = RegisteredMem::new_with_content(qp.pd(), "Hello, rrddmma!".as_bytes())?;
    send_wr::<1>()
        .set_flag_signaled()
        .set_wr_send(None)
        .set_sge(0, &mem.as_slice())
        .post_on(&qp)?;
    qp.scq().poll_one_blocking()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = thread::spawn(client);

    let mut qp = make_qp("mlx5_0")?;
    ctrl::Connecter::new(None)?.connect(&mut qp)?;

    // Receive a message from the client.
    let mem = RegisteredMem::new(qp.pd(), 4096)?;
    qp.recv(&[mem.as_slice()], 0)?;
    let wc = qp.rcq().poll_one_blocking()?;
    println!("{}", String::from_utf8_lossy(&mem[..wc.ok()?]));

    cli.join().unwrap()?;
    Ok(())
}
