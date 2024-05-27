use std::ptr;
use std::ptr::NonNull;

use rrddmma::rdma::qp::ExtCompareSwapParams;

#[cfg(mlnx5)]
fn main() {
    eprintln!("DC is not yet implemented on MLNX v5.x");
}

#[cfg(mlnx4)]
fn main() -> anyhow::Result<()> {
    use rrddmma::{prelude::*, wrap::RegisteredMem};
    use std::thread;

    const LEN: usize = 16;

    fn client(ep: QpEndpoint, remote: MrRemote) -> anyhow::Result<()> {
        fn make_dci(dev: &str) -> anyhow::Result<Qp> {
            use rrddmma::rdma::qp::ExpFeature::*;

            let Nic { context, ports } = Nic::finder().dev_name(dev).probe()?;
            let pd = Pd::new(&context)?;
            let cq = Cq::new(&context, Cq::DEFAULT_CQ_DEPTH)?;
            let mut qp = Qp::builder()
                .qp_type(QpType::DcIni)
                .caps(QpCaps::for_dc_ini())
                .send_cq(&cq)
                .recv_cq(&cq)
                .sq_sig_all(true)
                .enable_feature(ExtendedAtomics)
                .build(&pd)?;
            qp.bind_local_port(&ports[0], None)?;
            Ok(qp)
        }

        let mut qp =
            make_dci("mlx5_0").inspect_err(|e| eprintln!("Err in creating DCI QP: {:?}", e))?;
        let peer = qp.make_peer(ep)?;
        qp.set_dc_peer(peer);

        // Issue a CAS.
        fn ptr_to(val: &[u64; 2]) -> NonNull<u64> {
            NonNull::new(val.as_ptr() as *mut u64).unwrap()
        }
        let cmp = [0x0123456789abcdefu64, 0x1145141919810abcu64];
        let cmp_mask = [0xffffffffffffffffu64, 0xffffffffffffffffu64];
        let swap = [0xdeadbeefdeadbeefu64, 0x8badf00d8badf00du64];
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
        unsafe {
            println!(
                "client: {:#x} {:#x}",
                ptr::read::<u64>(mem.as_ptr() as _).swap_bytes(),
                ptr::read::<u64>(mem.as_ptr().add(8) as _).swap_bytes()
            )
        };
        Ok(())
    }

    fn make_dct(dev: &str) -> anyhow::Result<Dct> {
        let Nic { context, ports } = Nic::finder().dev_name(dev).probe()?;
        let pd = Pd::new(&context)?;
        let cq = Cq::new(&context, Cq::DEFAULT_CQ_DEPTH)?;
        let dct = Dct::builder()
            .pd(&pd)
            .cq(&cq)
            .port(&ports[0], None)
            .inline_size(64)
            .build(&context)?;
        Ok(dct)
    }

    let dct = make_dct("mlx5_0")?;
    let mut mem = RegisteredMem::new(dct.pd(), 4096)?;
    unsafe {
        ptr::write_volatile(mem.as_mut_ptr() as *mut u64, 0x0123456789abcdefu64);
        ptr::write_volatile(mem.as_mut_ptr().add(8) as *mut u64, 0x1145141919810abcu64);
    }
    let slice = MrRemote::from(mem.slice(0, LEN).unwrap());

    let ep = dct.endpoint();
    let cli = thread::spawn(move || client(ep, slice));

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
