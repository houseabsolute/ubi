#!/bin/bash

set -e

# This seems to be just enough to force a recompilation, so clippy is actually
# executed. But it doesn't require rebuilding every dep, so it's pretty fast.
rm -fr target/debug/deps/*ubi*
cargo clippy --all-targets --all-features -- -D clippy::all
