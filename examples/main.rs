use anyhow;
use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    let cluster = rrddmma::tcp::cluster::Cluster::load_toml("examples/lab.toml")?;
    println!("This is node {}", cluster.id());

    // Basic context & pd
    let context = rrddmma::rdma::context::Context::open(Some("mlx5_0"), 1, 0)?;
    let pd = rrddmma::rdma::pd::Pd::alloc(&context)?;

    rrddmma::tcp::barrier::Barrier::wait(&cluster);
    let mut conns = HashMap::new();
    for (i, qp, peer) in cluster.connect_all(&pd) {
        conns.insert(i, (qp, peer));
    }
    println!("Established");

    Ok(())
}
