#!/usr/bin/env bash

CONFIG_PATH="config-local"

set -x

if [[ -d "$CONFIG_PATH" ]]
then
    rm "$CONFIG_PATH"/*"id.json"
else
    mkdir "$CONFIG_PATH"
fi

set -ex

cargo run --manifest-path ../solana/keygen/Cargo.toml new -o "$CONFIG_PATH"/id.json
