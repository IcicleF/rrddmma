# rrddmma

> **A Rust RDMA library.**

[![Crates.io](https://img.shields.io/crates/v/rrddmma)](https://crates.io/crates/rrddmma)
[![Crates.io](https://img.shields.io/crates/d/rrddmma)](https://crates.io/crates/rrddmma)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE-MIT)

This library is more for academic use than for industry.
It is highly specialized to Mellanox/NVIDIA ConnectX network adapter series.

**WARNING: the interfaces are unstable and under continuous change!**

> **Note (fork `mlnx5-ext-atomic`):** this branch ports the two features that
> previously required legacy MLNX_OFED v4.9 (`ibv_exp_*`) — **extended masked
> atomics** and **Dynamically Connected transport** — onto the stock
> **rdma-core / `mlx5dv_*`** path. They now work out of the box with distro
> rdma-core packages and modern MLNX_OFED v5.x / DOCA-OFED installs, with no
> experimental-verbs setup required. Both features are hardware-verified on
> ConnectX-7 (RoCE), including ext atomics posted over a DCI.

## Linkage

This library links to `libibverbs` in one of two ways, selected by Cargo
features.

1. **rdma-core (default).** If `libibverbs` >= 1.8.28 is discoverable through
   `pkg-config`, `rrddmma` links against it and additionally links the `mlx5`
   userspace provider (`libmlx5`), which exposes the `mlx5dv_*` vendor
   extensions. This covers plain distribution rdma-core packages as well as
   MLNX_OFED v5.x and DOCA-OFED installs that register with `pkg-config`.

   On Debian/Ubuntu:

   ```shell
   sudo apt install libibverbs-dev libmlx5-1
   ```

   On this path, extended atomics are posted as MLX5 raw WQEs
   (`mlx5dv_wr_raw_wqe`) and DC objects are created with `mlx5dv_create_qp`;
   no experimental QP flags are needed (see [below](#platform-specific-features)).

2. **Legacy MLNX_OFED v4.9-x.** Enable the `legacy` feature (or `exp`, which
   implies `legacy` and also binds the `ibv_exp_*` experimental verbs). The
   v4.9 installation is located via `ofed_info`; set `MLNX_OFED_LIB_DIR` /
   `MLNX_OFED_INCLUDE_DIR` to override the default search paths.

   ```toml
   rrddmma = { version = "0.7.3", features = ["exp"] }
   ```

## Platform-specific Features

The available feature set differs between the rdma-core and the legacy v4.x
stack, so some items are not documented on [docs.rs](https://docs.rs/rrddmma).

- **Extended (masked) atomics:** `Qp::ext_compare_swap::<N>()` and
  `Qp::ext_fetch_add::<N>()`, with independent per-dword compare / swap / add
  masks (`ExtCompareSwapParams`). `N` is the operand width in bytes.
  - **rdma-core (default):** the WQE is built directly and posted through
    `mlx5dv_wr_raw_wqe`; the QP must simply be an mlx5 QP (every QP created by
    this library on the rdma-core path is). 8- and 16-byte operations are
    hardware-verified, including on DC initiator QPs. Nothing has to be
    enabled at QP creation time.
  - **MLNX_OFED v4.x (`exp`):** call
    `enable_feature(ExpFeature::ExtendedAtomics, N)` when building the QP,
    where `N` can be 8, 16, or 32.
- **Dynamically Connected QPs:** a `Dct` target type, together with initiator
  QPs of type `QpType::DcIni` that send to `Dct`s.
  - **rdma-core (default):** DCTs are created with `mlx5dv_create_qp` and DC
    sends install their address vector with `mlx5dv_wr_set_dc_addr`.
  - **MLNX_OFED v4.x (`exp`):** DCTs are created with `ibv_exp_create_dct`.

Runnable examples live in [`examples/`](examples):

- `extcas.rs` / `extfaa.rs` — masked compare-swap / fetch-add over RC;
  `loopback_extcas.rs` is a self-contained local loopback.
- `dc.rs`, `dc_local.rs` — DCT/DCI basics; `loopback_dc.rs` additionally
  fires ext atomics over a DCI loopback.

To get the complete set of feature-gated documentation, build it locally:

```shell
cargo doc --open
```

## Some Design Principles

### Panics in Fallible Methods

It is widely recognized as a bad design pattern to panic in fallible methods (i.e., methods that return a `Result`).
However, in RDMA, this is not the case because there are actually two different types of errors:

- **Programming errors**: These are logic errors caused by the programmers who do not follow the instructions of the
  manual. Examples include forgetting to bind a QP to a local port before making or binding a peer on it.
  These errors should be caught during development and testing, should never happen in production, and are never recoverable.
  `rrddmma` panics for these errors.
- **Runtime errors**: These are errors reported by the `libibverbs` library, such as failing to create a QP due to
  resource exhaustion. These errors are recoverable and should be handled by the caller.
  `rrddmma` returns `Err`s for these errors.
