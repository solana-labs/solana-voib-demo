# Note: when the rust version is changed also modify
# ci/buildkite.yml to pick up the new image tag
FROM rust:1.37.0

RUN set -x \
 && apt update \
 && apt-get install apt-transport-https \
 && echo deb https://apt.buildkite.com/buildkite-agent stable main > /etc/apt/sources.list.d/buildkite-agent.list \
 && apt-key adv --no-tty --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys 32A37959C2FA5C3C99EFBC32A79206696452D198 \
 && apt update \
 && apt install -y \
      buildkite-agent \
      clang-7 \
      cmake \
      lcov \
      libclang-common-7-dev \
      libgtk-3-dev \
      mscgen \
      rsync \
      sudo \
      \
 && rm -rf /var/lib/apt/lists/* \
 && rustup component add rustfmt \
 && rustup component add clippy \
 && cargo install cargo-audit \
 && rustc --version \
 && cargo --version
