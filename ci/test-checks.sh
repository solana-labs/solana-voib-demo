#!/usr/bin/env bash
set -e
shopt -s extglob

cd "$(dirname "$0")/.."

source ci/_

annotate() {
  ${BUILDKITE:-false} && {
    buildkite-agent annotate "$@"
  }
}

ci/version-check.sh stable

export RUST_BACKTRACE=1
export RUSTFLAGS="-D warnings"

for D in !(solana)/; do
  for i in "$PWD"/"$D"*.toml; do
    if [[ -f "$i" ]]; then
      ci/affects-files.sh ^"$D" || {
        annotate --style info \
          "Skipped checking {$D} as no relavant files were modified"
        break
      }
      (
        _ echo "Checking $D"
        cd "$D"
        _ cargo fmt -- --check
        _ cargo clippy -- --version
        _ cargo clippy -- --deny=warnings
      )
    fi
  done
done

_ cargo audit

echo --- ok
