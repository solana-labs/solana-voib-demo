#!/bin/bash -e
#
# Deploy program to a solana repo
#

buildScript="
    cargo fmt -- --check;
    cargo build;
    cargo clippy -- --deny=warnings;
    cargo test;
"

bash -xce "$buildScript"
for i in ../target/debug/libbandwidth_prepay_program.{dylib,so}; do
  [[ -e $i ]] && cp -fv "$i" ../solana/target/debug/
done
cd ../solana
git apply ../bandwidth-prepay-program/load_prepay.patch
