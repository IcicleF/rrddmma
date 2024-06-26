# rrddmma

> **A Rust RDMA library.**

[![Crates.io](https://img.shields.io/crates/v/rrddmma)](https://crates.io/crates/rrddmma)
[![Crates.io](https://img.shields.io/crates/d/rrddmma)](https://crates.io/crates/rrddmma)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE-MIT)

This library is more for academic use than for industry.
It is highly specialized to Mellanox/NVIDIA ConnectX network adapter series.

**WARNING: the interfaces are unstable and under continuous change!**

## Linkage

This library supports multiple linkage types to the `ibverbs` library.

1. First, this library respects
   existing [MLNX_OFED](https://network.nvidia.com/products/infiniband-drivers/linux/mlnx_ofed/) installations.
   It works on both v4.9-x and v5.x versions.

   - MLNX_OFED v4.9-x will enable experimental, see [below](#undocumented-features).
   - ~~MLNX*OFED v5.x will enable `mlx5dv*\*` features.~~ (TODO)

2. Otherwise, `rrddmma` will try to find an existing `libibverbs` installation via `pkg-config`.

   - ~~This will enable enable `mlx5dv_*` features.~~ (TODO)

3. Otherwise, `rrddmma` will try to download [rdma-core](https://github.com/linux-rdma/rdma-core) and build from source.
   You need to ensure that the dependencies are properly installed.
   In Ubuntu and other Debian-derived OSs, these are:

   ```shell
   sudo apt install -y build-essential cmake gcc libclang-dev libudev-dev libsystemd-dev \
                       libnl-3-dev libnl-route-3-dev ninja-build pkg-config valgrind \
                       python3-dev cython3 python3-docutils pandoc
   ```

   Building from source is different from the previous two approaches in that `libibverbs` is linked statically and
   cannot detect providers at runtime.
   This library currently only allows the `mlx5` provider.

   - ~~This will enable enable `mlx5dv_*` features.~~ (TODO)

## Undocumented Features

Due to the distinct feature set of MLNX_OFED v4.x and v5.x drivers, enabled features of this library is also different
on different machines. As a result, some features are undocumented in [docs.rs](https://docs.rs/rrddmma). Here are some
of them:>

- **Dynamically Connected QPs:** available on MLNX_OFED v4.x. A `Dct` type will be available, and you may create QPs
  of type `QpType::DcIni` to send messages to `Dct`s.
- **Extended Atomics:** available on MLNX_OFED v4.x. `Qp::ext_compare_swap()` and `Qp::ext_fetch_add()` will be
  available. To use this feature, call `enable_feature(ExpFeature::ExtendedAtomics, N)` when you build a QP, in which
  `N` can be 8, 16, or 32.

To get a complete set of features, you can build the documentation locally:

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
