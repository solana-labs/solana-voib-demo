#!/usr/bin/env bash

CONFIG_PATH="config-local"
NAME=$1

set -x

if [[ -d "$CONFIG_PATH" ]]
then
    rm "$CONFIG_PATH"/"$NAME"-id.json "$CONFIG_PATH"/"$NAME"-pubkey.json
else
    mkdir "$CONFIG_PATH"
fi

set -ex

cargo run --manifest-path ../solana/keygen/Cargo.toml new -o "$CONFIG_PATH"/"$NAME"-id.json
cargo run --manifest-path ../solana/keygen/Cargo.toml pubkey "$CONFIG_PATH"/"$NAME"-id.json -o "$CONFIG_PATH"/"$NAME"-pubkey.json
