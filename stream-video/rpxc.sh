#!/usr/bin/env bash

cd /mnt/stream-video || exit

"${HOME}"/.cargo/bin/cargo build --target armv7-unknown-linux-gnueabihf 
