#!/bin/bash
BUILD_TYPE="debug"
params=${@:1}

if [ $BUILD_TYPE = "debug" ]; then
    cargo build
else
    BUILD_TYPE="release"
    cargo build --release
fi

pdsh -w ssh:ec[1-3] "/home/gaoj/workspace/rust/rrddmma/target/$BUILD_TYPE/rrddmma $params"
pdsh -w ssh:ec[1-3] "killall rrddmma >> /dev/null 2>&1"
