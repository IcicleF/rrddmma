#!/bin/bash
build_type=${1:-"debug"}

if [ $build_type = "debug" ]; then
    cargo build
else
    build_type="release"
    cargo build --release
fi
pdsh -w ssh:ec[1-3] "RUST_LOG=info /home/gaoj/workspace/rust/rrddmma/target/$build_type/rrddmma"
