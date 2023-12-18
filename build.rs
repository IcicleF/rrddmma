use std::env::{self, consts};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerbsVersion {
    V4,
    V5,
}

struct MlnxOfedLink {
    ver: VerbsVersion,
    include_dirs: Vec<String>,
}

impl MlnxOfedLink {
    fn new(ver: VerbsVersion, include_dirs: Vec<String>) -> Self {
        Self { ver, include_dirs }
    }
}

/// Try to link to existing `MLNX_OFED` installation.
fn link_mlnx_ofed() -> Result<MlnxOfedLink, ()> {
    let output = Command::new("ofed_info")
        .arg("-n")
        .output()
        .map_err(|_| ())?;

    let ver_num = output.stdout.get(0).ok_or(())?;
    match *ver_num {
        b'4' => {
            // MLNX_OFED v4.9-x LTS will not register the `libibverbs` library to
            // `pkg-config`, so search for it manually.
            //
            // We assume the default installation path as `/usr`.
            // By default, we do not need to specify the include and library paths,
            // as they are already in the default search paths.
            let lib_dir = if let Ok(lib_dir) = env::var("MLNX_OFED_LIB_DIR") {
                Path::new(&lib_dir).to_owned()
            } else {
                Path::new("/usr/lib").to_owned()
            };

            let dylib_name = format!("{}ibverbs{}", consts::DLL_PREFIX, consts::DLL_SUFFIX);
            if lib_dir.join(dylib_name).exists() || lib_dir.join("libibverbs.a").exists() {
                println!("cargo:rustc-link-search=native={}", lib_dir.display());
                println!("cargo:rustc-link-lib=ibverbs");
                let include_dirs = if let Ok(include_dir) = env::var("MLNX_OFED_INCLUDE_DIR") {
                    vec![include_dir]
                } else {
                    Vec::new()
                };
                Ok(MlnxOfedLink::new(VerbsVersion::V4, include_dirs))
            } else {
                Err(())
            }
        }
        b'5' => {
            // MLNX_OFED v5.x LTS will register the `libibverbs` library to `pkg-config`.
            let lib = pkg_config::Config::new()
                .atleast_version("1.8.28")
                .statik(false)
                .probe("libibverbs")
                .map_err(|_| ())?;

            Ok(MlnxOfedLink::new(
                VerbsVersion::V5,
                lib.include_paths
                    .iter()
                    .map(|p| p.to_str().unwrap().to_owned())
                    .collect(),
            ))
        }
        _ => Err(()),
    }
}

/// Build flow:
///
/// 1. Try to link to existing `MLNX_OFED` installation.
/// 2. If failed, try to link to existing `libibverbs` installation.
/// 3. If failed, build `libibverbs` from source.
fn main() {
    // Refuse to compile on non-64-bit platforms.
    if cfg!(not(target_pointer_width = "64")) {
        panic!("`rrddmma` currently only supports 64-bit platforms");
    }

    // Respect existing `MLNX_OFED` installation.
    if let Ok(link) = link_mlnx_ofed() {
        println!("cargo:rerun-if-changed=src/bindings/verbs.h");
        println!("cargo:rerun-if-env-changed=MLNX_OFED_INCLUDE_DIR");
        println!("cargo:rerun-if-env-changed=MLNX_OFED_LIB_DIR");
        gen_verb_bindings(link.ver, link.include_dirs);
        return;
    }

    // Respect existing `libibverbs` installation.
    todo!()
}

fn gen_verb_bindings(ver: VerbsVersion, include_dirs: Vec<String>) {
    let include_args = include_dirs.iter().map(|p| format!("-I{}", p));
    let mut builder = bindgen::builder()
        .clang_args(include_args)
        .header("src/bindings/verbs.h")
        .allowlist_function("ibv_.*")
        .allowlist_type("ibv_.*")
        .allowlist_type("verbs_.*")
        .allowlist_type("ib_uverbs_access_flags")
        .opaque_type("pthread_.*")
        .blocklist_type("in6_addr")
        .blocklist_type("sockaddr.*")
        .blocklist_type("timespec")
        .blocklist_type("ibv_ah_attr")
        .blocklist_type("ibv_async_event")
        .blocklist_type("ibv_flow_spec")
        .blocklist_type("ibv_gid")
        .blocklist_type("ibv_global_route")
        .blocklist_type("ibv_mw_bind_info")
        .blocklist_type("ibv_ops_wr")
        .blocklist_type("ibv_send_wr")
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
        .constified_enum_module("ib_uverbs_advise_mr_advice");

    match ver {
        VerbsVersion::V4 => {
            println!("cargo:rustc-cfg=mlnx4");

            // `ibv_exp_*` bindings
            builder = builder
                .bitfield_enum("verbs_context_mask")
                .bitfield_enum("ibv_exp_device_cap_flags")
                .bitfield_enum("ibv_exp_device_attr_comp_mask")
                .bitfield_enum("ibv_exp_device_attr_comp_mask2")
                .constified_enum_module("ibv_exp_atomic_cap")
                .constified_enum_module("ibv_exp_dm_memcpy_dir")
                .bitfield_enum("ibv_exp_roce_gid_type");
        }
        VerbsVersion::V5 => {
            println!("cargo:rustc-cfg=mlnx5");

            // TODO: RDMA-Core bindings
        }
    }

    let bindings = builder
        .derive_copy(true)
        .derive_debug(false)
        .derive_default(true)
        .generate_comments(true)
        .layout_tests(false)
        .prepend_enum_name(false)
        .size_t_is_usize(true)
        .disable_untagged_union()
        .rustified_enum("ibv_event_type")
        .generate()
        .expect("failed to generate bindings");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("verbs_bindings.rs");
    bindings
        .write_to_file(dest)
        .expect("failed to write bindings");
}
