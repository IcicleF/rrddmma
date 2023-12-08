# rrddmma

> **A Rust RDMA library.**

[![Crates.io](https://img.shields.io/crates/v/rrddmma)](https://crates.io/crates/rrddmma)
[![Crates.io](https://img.shields.io/crates/d/rrddmma)](https://crates.io/crates/rrddmma)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE-MIT)


## Linkage

1.  `rrddmma` respects existing [MLNX_OFED](https://network.nvidia.com/products/infiniband-drivers/linux/mlnx_ofed/) installations.

    - MLNX_OFED v4.9-x LTS installations will enable `ibv_exp_*` features.
      Its installations is assumed to be in `/usr/include` (headers) and `/usr/lib` (static & dynamic libraries).
      You may specify these paths via `MLNX_OFED_INCLUDE_DIR` and `MLNX_OFED_LIB_DIR` environment variables.
    - MLNX_OFED v5.x installations will enable `mlx5dv_*` features.

2.  Otherwise, `rrddmma` will try to find an existing `libibverbs` installation via `pkg-config`.
    If this approach is taken, `rrddmma` will respect provider-specific functionalities.

    - `mlx5` provider will enable `mlx5dv_*` features.

3.  Otherwise, `rrddmma` will try to download [rdma-core](https://github.com/linux-rdma/rdma-core) and build from source.
    If this approach is taken, only `libibverbs` interfaces are supported.
    Also, you need to ensure that the dependencies are properly installed.


## Implementation

Beneath the interfaces exposed, every data structure maintains allocated `ibv_*` resources with an `Arc` if there are any.
As a result, such data structures can always be cloned.
Although this seems to introduce an unnecessary extra layer of indirection, it will also significantly relieve the programmer's
stress when they need to share the resources among threads.
