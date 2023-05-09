# rrddmma

> **A Rust RDMA library.**

[![Crates.io](https://img.shields.io/crates/v/rrddmma)](https://crates.io/crates/rrddmma)
[![Crates.io](https://img.shields.io/crates/d/rrddmma)](https://crates.io/crates/rrddmma)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE-MIT)

This library provides safe wrappers of the unsafe `ibverbs` interfaces, while preserving the original *post-poll* semantics.
Currently, there is no async support.

## Implementation

Beneath the interfaces exposed, every data structure maintains allocated `ibv_*` resources with an `Arc` if there are any.
As a result, such data structures can always be cloned.
Although this seems to introduce an unnecessary extra layer of indirection, it will also significantly relieve the programmer's
stress when they need to share the resources among threads.
