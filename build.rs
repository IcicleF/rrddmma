use std::env::{self, consts};
use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, Result};

/// Verbs version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerbsVersion {
    Legacy,
    LegacyWithExp,
    RdmaCore,
}

/// Binding options.
#[derive(Debug, Clone)]
struct Bindings {
    ver: VerbsVersion,
    includes: Vec<String>,
    linkages: Vec<String>,
}

/// Try to bind to legacy `MLNX_OFED` (v4.x) installation.
fn link_ibverbs_legacy() -> Result<Bindings> {
    let output = Command::new("ofed_info").arg("-n").output().map_err(|_| {
        anyhow!("failed to run `ofed_info`, which is required to link to legacy MLNX_OFED versions")
    })?;

    // Parse the version number until the first period (`.`).
    let ver_num = output
        .stdout
        .iter()
        .take_while(|&&c| c != b'.')
        .copied()
        .collect::<Vec<_>>();
    let ver_num = String::from_utf8(ver_num)
        .map_err(|e| anyhow!("failed to parse `ofed_info` output: {:?}", e))?
        .parse::<u32>()
        .map_err(|e| anyhow!("failed to parse version number: {:?}", e))?;
    if ver_num != 4 {
        return Err(anyhow!(
            "unsupported MLNX_OFED version {} for legacy MLNX_OFED linkage",
            ver_num
        ));
    }

    // MLNX_OFED v4.9-x LTS will not register the `libibverbs` library to
    // `pkg-config`, so search for it manually.
    //
    // We assume the default installation path as `/usr`.
    // By default, we do not need to specify the include and library paths,
    // as they are already in the default search paths.
    const DEFAULT_INSTALLATION_PATH: &str = "/usr/lib";
    let libdir_str = if let Ok(lib_dir) = env::var("MLNX_OFED_LIB_DIR") {
        lib_dir
    } else {
        DEFAULT_INSTALLATION_PATH.to_owned()
    };
    let lib_dir = Path::new(&libdir_str);

    const LIBRARIES: [&str; 3] = ["ibverbs", "mlx5", "mlx4"];
    let mut linkages = Vec::new();
    for lib in LIBRARIES {
        let lib_name = format!("{}{}{}", consts::DLL_PREFIX, lib, consts::DLL_SUFFIX);
        if lib_dir.join(lib_name).exists() {
            println!("cargo:rustc-link-search=native={}", lib_dir.display());
            linkages.push(lib.to_owned());
            continue;
        }
        assert!(
            lib != "ibverbs",
            "cannot find ibverbs library; you may use `MLNX_OFED_LIB_DIR` to specify a path"
        );
    }

    // At least link on `libibverbs`.
    let includes = if let Ok(includes) = env::var("MLNX_OFED_INCLUDE_DIR") {
        includes
            .split(':')
            .map(|p| p.to_owned())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    Ok(Bindings {
        ver: VerbsVersion::Legacy,
        includes,
        linkages,
    })
}

/// Try to bind to RDMA-Core `libibverbs` installation.
fn link_ibverbs_rdmacore() -> Result<Bindings> {
    // There should be pkg-config support. Use it.
    let lib = pkg_config::Config::new()
        .atleast_version("1.8.28")
        .statik(false)
        .probe("libibverbs")
        .map_err(|e| anyhow!("failed to probe `libibverbs`: {:?}", e))?;

    Ok(Bindings {
        ver: VerbsVersion::RdmaCore,
        includes: lib
            .include_paths
            .iter()
            .map(|p| p.to_str().unwrap().to_owned())
            .collect(),
        linkages: lib.libs.iter().map(|s| s.to_owned()).collect(),
    })
}

/// Link to specific `libibverbs` installation.
fn link_ibverbs() -> Result<Bindings> {
    if cfg!(feature = "legacy") {
        let mut bindings = link_ibverbs_legacy()?;
        if cfg!(feature = "exp") {
            bindings.ver = VerbsVersion::LegacyWithExp;
        }
        Ok(bindings)
    } else {
        link_ibverbs_rdmacore()
    }
}

/// Build flow:
///
/// 1. Try to link to existing `MLNX_OFED` installation.
/// 2. If failed, try to link to existing `libibverbs` installation.
/// 3. If failed, build `libibverbs` from source.
fn main() {
    // Refuse to compile on non-64-bit or non-Linux platforms.
    if cfg!(not(target_pointer_width = "64")) {
        panic!("`rrddmma` only supports 64-bit platforms");
    }
    if cfg!(not(target_os = "linux")) {
        panic!("`rrddmma` only supports Linux platforms");
    }

    // Respect existing MLNX_OFED or DOCA_OFED installation.
    match link_ibverbs() {
        Ok(bindings) => gen_verb_bindings(bindings),
        Err(e) => panic!("{:?}", e),
    }
}

fn gen_verb_bindings(bindings: Bindings) {
    // Linkages.
    for lib in bindings.linkages {
        println!("cargo:rustc-link-lib={}", lib);
    }

    // Includes.
    let include_args = bindings.includes.iter().map(|p| format!("-I{}", p));

    // Common arguments.
    let mut builder = bindgen::builder()
        .clang_args(include_args)
        .header("src/bindings/verbs.h")
        .allowlist_function("ibv_.*")
        .allowlist_type("ibv_.*")
        .allowlist_type("verbs_.*")
        .allowlist_type("ib_uverbs_access_flags")
        .blocklist_type("pthread_.*")
        .blocklist_type("in6_addr")
        .blocklist_type("sockaddr.*")
        .blocklist_type("timespec")
        .blocklist_type("ibv_ah_attr")
        .blocklist_type("ibv_async_event")
        .blocklist_type("ibv_flow_spec")
        .blocklist_type("ibv_gid")
        .blocklist_type("ibv_global_route")
        .blocklist_type("ibv_send_wr.*")
        .blocklist_type("ibv_wc")
        .bitfield_enum("ibv_device_cap_flags")
        .bitfield_enum("ibv_odp_transport_cap_bits")
        .bitfield_enum("ibv_odp_general_caps")
        .bitfield_enum("ibv_rx_hash_function_flags")
        .bitfield_enum("ibv_rx_hash_fields")
        .bitfield_enum("ibv_raw_packet_caps")
        .bitfield_enum("ibv_tm_cap_flags")
        .bitfield_enum("ibv_pci_atomic_op_size")
        .bitfield_enum("ibv_port_cap_flags")
        .bitfield_enum("ibv_port_cap_flags2")
        .bitfield_enum("ibv_create_cq_wc_flags")
        .bitfield_enum("ibv_wc_flags")
        .bitfield_enum("ibv_access_flags")
        .bitfield_enum("ibv_xrcd_init_attr_mask")
        .bitfield_enum("ibv_rereg_mr_flags")
        .bitfield_enum("ibv_srq_attr_mask")
        .bitfield_enum("ibv_srq_init_attr_mask")
        .bitfield_enum("ibv_wq_init_attr_mask")
        .bitfield_enum("ibv_wq_flags")
        .bitfield_enum("ibv_wq_attr_mask")
        .bitfield_enum("ibv_ind_table_init_attr_mask")
        .bitfield_enum("ibv_qp_init_attr_mask")
        .bitfield_enum("ibv_qp_create_flags")
        .bitfield_enum("ibv_qp_create_send_ops_flags")
        .bitfield_enum("ibv_qp_open_attr_mask")
        .bitfield_enum("ibv_qp_attr_mask")
        .bitfield_enum("ibv_send_flags")
        .bitfield_enum("ibv_ops_flags")
        .bitfield_enum("ibv_cq_attr_mask")
        .bitfield_enum("ibv_flow_flags")
        .bitfield_enum("ibv_flow_action_esp_mask")
        .bitfield_enum("ibv_cq_init_attr_mask")
        .bitfield_enum("ibv_create_cq_attr_flags")
        .bitfield_enum("ibv_parent_domain_init_attr_mask")
        .bitfield_enum("ibv_read_counters_flags")
        .bitfield_enum("ibv_values_mask")
        .bitfield_enum("ib_uverbs_access_flags")
        .constified_enum_module("ibv_node_type")
        .constified_enum_module("ibv_transport_type")
        .constified_enum_module("ibv_atomic_cap")
        .constified_enum_module("ibv_mtu")
        .constified_enum_module("ibv_port_state")
        .constified_enum_module("ibv_wc_status")
        .constified_enum_module("ibv_wc_opcode")
        .constified_enum_module("ibv_mw_type")
        .constified_enum_module("ibv_rate")
        .constified_enum_module("ibv_srq_type")
        .constified_enum_module("ibv_wq_type")
        .constified_enum_module("ibv_wq_state")
        .constified_enum_module("ibv_qp_type")
        .constified_enum_module("ibv_qp_state")
        .constified_enum_module("ibv_mig_state")
        .constified_enum_module("ibv_wr_opcode")
        .constified_enum_module("ibv_ops_wr_opcode")
        .constified_enum_module("ibv_flow_attr_type")
        .constified_enum_module("ibv_flow_spec_type")
        .constified_enum_module("ibv_counter_description")
        .constified_enum_module("ibv_rereg_mr_err_code")
        .constified_enum_module("ib_uverbs_advise_mr_advice")
        .rustified_enum("ibv_event_type");

    match bindings.ver {
        VerbsVersion::LegacyWithExp => {
            // `ibv_exp_*` bindings
            builder = builder
                .blocklist_type("ibv_exp_send_wr.*")
                .bitfield_enum("verbs_context_mask")
                .bitfield_enum("ibv_exp_device_cap_flags")
                .bitfield_enum("ibv_exp_device_attr_comp_mask")
                .bitfield_enum("ibv_exp_device_attr_comp_mask_2")
                .bitfield_enum("ibv_exp_qp_init_attr_comp_mask")
                .bitfield_enum("ibv_exp_qp_attr_mask")
                .bitfield_enum("ibv_exp_send_flags")
                .bitfield_enum("ibv_exp_roce_gid_type")
                // .bitfield_enum("ibv_exp_query_gid_attr")
                .bitfield_enum("ibv_exp_qp_attr_comp_mask")
                .bitfield_enum("ibv_exp_dct_init_attr_comp_mask")
                .bitfield_enum("ibv_exp_dct_attr_comp_mask")
                .constified_enum_module("ibv_exp_atomic_cap")
                .constified_enum_module("ibv_exp_wr_opcode")
                .constified_enum_module("ibv_exp_calc_op")
                .constified_enum_module("ibv_exp_calc_data_type")
                .constified_enum_module("ibv_exp_calc_data_size")
                .constified_enum_module("ibv_exp_dm_memcpy_dir");
        }
        VerbsVersion::RdmaCore => {
            // RDMA-Core bindings
            builder = builder
                .blocklist_type("ibv_ops_wr")
                .blocklist_type("_compat_ibv_port_attr");
        }
        _ => {}
    }

    let bindings = builder
        .derive_copy(true)
        .derive_debug(true)
        .derive_default(true)
        .generate_comments(true)
        .layout_tests(false)
        .prepend_enum_name(false)
        .size_t_is_usize(true)
        .disable_untagged_union()
        .generate()
        .expect("failed to generate bindings");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("verbs_bindings.rs");
    bindings
        .write_to_file(dest)
        .expect("failed to write bindings");
}
