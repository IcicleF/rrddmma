#[cfg(mlnx5)]
fn main() {
    eprintln!("MLNX_OFED v5.x or newer does not support ExtAtomics");
}

#[cfg(mlnx4)]
fn main() -> anyhow::Result<()> {
    use rrddmma::rdma::qp::{ExpFeature, ExtCompareSwapParams};
    use rrddmma::{ctrl, prelude::*, wrap::RegisteredMem};
    use std::{
        net::Ipv4Addr,
        ptr::{self, NonNull},
        thread,
    };

    const LEN: usize = 8;

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
        let cmp = [0x0u64, 0x0u64];
        let cmp_mask = [0x0u64, 0x0u64];
        let swap = [0xdeadbeef01234567u64, 0x8badf00d8badf00du64];
        let swap_mask = [0xffffffffffffffffu64, 0xffffffffffffffffu64];

        let mut mem = RegisteredMem::new(qp.pd(), 4096)?;
        unsafe {
            ptr::write_bytes(mem.as_mut_ptr(), 0, LEN);
            let params = ExtCompareSwapParams {
                compare: ptr_to(&cmp),
                swap: ptr_to(&swap),
                compare_mask: ptr_to(&cmp_mask),
                swap_mask: ptr_to(&swap_mask),
            };
            qp.ext_compare_swap::<LEN>(&mem.slice(0, LEN).unwrap(), &remote, &params, 0, true)?;
        }
        qp.scq().poll_one_blocking_consumed();
        if LEN > 8 {
            unsafe {
                println!(
                    "client: {:#x} {:#x}",
                    ptr::read::<u64>(mem.as_ptr() as _).swap_bytes(),
                    ptr::read::<u64>(mem.as_ptr().add(8) as _).swap_bytes()
                )
            };
        } else {
            unsafe { println!("client: {:#x}", ptr::read::<u64>(mem.as_ptr() as _)) };
        }
        Ok(())
    }

    println!("LEN = {:?}", LEN);

    let mut qp = make_qp("mlx5_0")?;
    let mut mem = RegisteredMem::new(qp.pd(), 4096)?;
    unsafe {
        ptr::write_volatile(mem.as_mut_ptr() as *mut u64, 0x0123456789abcdefu64);
        ptr::write_volatile(mem.as_mut_ptr().add(8) as *mut u64, 0x1145141919810abcu64);
    }

    let slice = MrRemote::from(mem.slice(0, LEN).unwrap());
    let cli = thread::spawn(move || client(slice));
    ctrl::Connecter::new(None)?.connect(&mut qp)?;

    cli.join().unwrap()?;
    unsafe {
        println!(
            "server: {:#x} {:#x}",
            ptr::read::<u64>(mem.as_ptr() as _),
            ptr::read::<u64>(mem.as_ptr().add(8) as _)
        )
    };

    Ok(())
}
