use anyhow;
use rrddmma::ctrl::Connecter;
use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    let cluster = rrddmma::ctrl::Cluster::load_toml("examples/lab.toml")?;
    println!("This is node {}", cluster.rank());

    // Basic context & pd
    let context = rrddmma::Context::open(Some("mlx5_0"), 1, 0)?;
    let pd = rrddmma::Pd::new(context.clone())?;

    rrddmma::ctrl::Barrier::wait(&cluster);
    let mut conns = HashMap::new();
    for (i, links) in cluster.connect_all(&pd, rrddmma::QpType::RC, 64) {
        conns.insert(i, links);
    }
    println!("connected ({})", conns.len());

    let buf = vec![0u8; 4096];
    let mr = rrddmma::Mr::reg_slice(pd.clone(), &buf)?;

    if cluster.rank() == 0 {
        let rem_mr = Connecter::new(&cluster, 1).recv_mr()?;
        println!("received remote mr");

        let qp = &conns[&1][0].0;
        {
            let start_time = std::time::Instant::now();
            qp.write(
                &[mr.get_slice(0..8).unwrap()],
                &rem_mr.as_slice(),
                0,
                None,
                true,
            )?;
            qp.scq().poll_nocqe_blocking(1)?;
            let end_time = std::time::Instant::now();

            println!("write 8B latency: {:?}", end_time - start_time);
        }
    }
    if cluster.rank() == 1 {
        Connecter::new(&cluster, 0).send_mr(&mr)?;
        println!("sent mr to remote");
    }
    rrddmma::ctrl::Barrier::wait(&cluster);

    Ok(())
}
