#[cfg(mlnx5)]
fn main() {
    eprintln!("MLNX_OFED v5.x or newer does not support ExtAtomics");
}

#[cfg(mlnx4)]
fn main() -> anyhow::Result<()> {
    use rrddmma::rdma::qp::ExpFeature;
    use rrddmma::{ctrl, prelude::*, wrap::RegisteredMem};
    use std::{
        net::Ipv4Addr,
        ptr::{self, NonNull},
        thread,
    };

    const LEN: usize = 16;

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
            .enable_feature(ExpFeature::ExtendedAtomics)
            .build(&pd)?;
        qp.bind_local_port(&ports[0], None)?;
        Ok(qp)
    }

    fn client(remote: MrRemote) -> anyhow::Result<()> {
        let mut qp = make_qp("mlx5_0")?;
        ctrl::Connecter::new(Some(Ipv4Addr::LOCALHOST))?.connect(&mut qp)?;

        // Issue a CAS.
        fn ptr_to(val: &[u64; 2]) -> NonNull<u64> {
            NonNull::new(val.as_ptr() as *mut u64).unwrap()
        }
        let add = [0xccu64, 0x01u64];
        let mask = [0x0u64, 0x0u64];

        let mut mem = RegisteredMem::new(qp.pd(), 4096)?;
        unsafe {
            ptr::write_bytes(mem.as_mut_ptr(), 0, LEN);
            qp.ext_fetch_add::<LEN>(
                mem.slice(0, LEN).unwrap(),
                remote,
                ptr_to(&add),
                ptr_to(&mask),
                0,
                true,
            )?;
        }
        qp.scq().poll_one_blocking_consumed();
        unsafe {
            println!(
                "cli: {:#x} {:#x}",
                ptr::read::<u64>(mem.as_ptr() as _).swap_bytes(),
                ptr::read::<u64>(mem.as_ptr().add(8) as _).swap_bytes()
            )
        };
        Ok(())
    }

    let mut qp = make_qp("mlx5_0")?;
    let mut mem = RegisteredMem::new(qp.pd(), 4096)?;
    unsafe {
        ptr::write_volatile(mem.as_mut_ptr() as *mut u64, 0x11u64);
        ptr::write_volatile(mem.as_mut_ptr().add(8) as *mut u64, 0x01u64);
    }

    let slice = MrRemote::from(mem.slice(0, LEN).unwrap());
    let cli = thread::spawn(move || client(slice));
    ctrl::Connecter::new(None)?.connect(&mut qp)?;

    unsafe {
        println!(
            "svr: {:#x} {:#x}",
            ptr::read::<u64>(mem.as_ptr() as _),
            ptr::read::<u64>(mem.as_ptr().add(8) as _)
        )
    };

    cli.join().unwrap()?;
    Ok(())
}
