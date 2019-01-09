#!/usr/bin/env bash
set -e

cd "$(dirname "$0")/.."

source ci/_

annotate() {
  ${BUILDKITE:-false} && {
    buildkite-agent annotate "$@"
  }
}

export RUST_BACKTRACE=1
export RUSTFLAGS="-D warnings"

_ cargo test -- --nocapture
_ cargo test --manifest-path stream-video/Cargo.toml --features=ui-only
