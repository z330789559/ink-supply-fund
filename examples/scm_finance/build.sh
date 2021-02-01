#!/usr/bin/env bash
set -eu

pushd auter_erc20 && cargo +nightly contract build --generate code-only && popd &&
cargo +nightly contract build