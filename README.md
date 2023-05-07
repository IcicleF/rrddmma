# rrddmma

A Rust RDMA library based on [rdma-sys](https://github.com/datenlord/rdma-sys).

This library provides safe wrappers of the unsafe `ibverbs` interfaces, while preserving the original *post-poll* semantics.
Currently, there is no async support.

## Example

```rust
use rrddmma::*;
use anyhow::Result;

fn main() -> Result<()> {
    let context = rrddmma::Context::open(Some("mlx5_0"), 1, 0)?;
    let pd = rrddmma::Pd::new(&context)?;
}
```