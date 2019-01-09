#!/usr/bin/env bash
#
# Reference: https://github.com/koalaman/shellcheck/wiki/Directive
set -e

cd "$(dirname "$0")/.."
(
  set -x
  find . -name "*.sh" \
      -not -regex ".*/target/.*" \
      -not -regex ".*/solana/.*" \
      -print0 \
    | xargs -0 \
        ci/docker-run.sh koalaman/shellcheck --color=always --external-sources --shell=bash
)
echo --- ok
