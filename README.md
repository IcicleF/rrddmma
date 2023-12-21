# rrddmma

> **A Rust RDMA library.**

[![Crates.io](https://img.shields.io/crates/v/rrddmma)](https://crates.io/crates/rrddmma)
[![Crates.io](https://img.shields.io/crates/d/rrddmma)](https://crates.io/crates/rrddmma)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE-MIT)

This library is more for academic use than for industry.
It is highly specialized to Mellanox/NVIDIA ConnectX network adapter series.


## Linkage

This library supports multiple linkage types to the `ibverbs` library.

1.  First, this library respects existing [MLNX_OFED](https://network.nvidia.com/products/infiniband-drivers/linux/mlnx_ofed/) installations.
    It works on both v4.9-x and v5.x versions.
    - ~~MLNX_OFED v4.9-x will enable experimental verbs.~~ (TODO)
    - ~~MLNX_OFED v5.x will enable `mlx5dv_*` features.~~ (TODO)

2.  Otherwise, `rrddmma` will try to find an existing `libibverbs` installation via `pkg-config`.
    - ~~This will enable enable `mlx5dv_*` features.~~ (TODO)

3.  Otherwise, `rrddmma` will try to download [rdma-core](https://github.com/linux-rdma/rdma-core) and build from source.
    You need to ensure that the dependencies are properly installed.
    In Ubuntu and other Debian-derived OSs, these are:

    ```shell
    sudo apt install -y build-essential cmake gcc libclang-dev libudev-dev libsystemd-dev \
                        libnl-3-dev libnl-route-3-dev ninja-build pkg-config valgrind \
                        python3-dev cython3 python3-docutils pandoc
    ```

    Building from source is different from the previous two approaches in that `libibverbs` is linked statically and cannot detect providers at runtime.
    This library currently only allows the `mlx5` provider.
    - ~~This will enable enable `mlx5dv_*` features.~~ (TODO)

