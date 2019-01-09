FROM ubuntu:latest
FROM gcc:latest

RUN dpkg --add-architecture armhf

ENV CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc
ENV PKG_CONFIG_ALLOW_CROSS=1
ENV PKG_CONFIG_PATH=/usr/lib/arm-linux-gnueabihf/pkgconfig/

RUN apt-get update && \
    apt-get install -y curl libgtk-3-dev:armhf libgdk-pixbuf2.0-dev:armhf \
    libpango1.0-dev:armhf libharfbuzz-dev:armhf libicu-dev:armhf \
    gcc-arm-linux-gnueabihf libssl-dev:armhf

WORKDIR /tmp

RUN curl https://sh.rustup.rs -sSf > rustup.sh
RUN chmod 755 rustup.sh
RUN ./rustup.sh -y
RUN rm /tmp/rustup.sh

RUN $HOME/.cargo/bin/rustup target add armv7-unknown-linux-gnueabihf

ENTRYPOINT ["/bin/bash"]

WORKDIR /mnt

RUN rm -rf /root/.cargo/registry

RUN mkdir /root/.cargo/registry

VOLUME ["/mnt", "/root/.cargo/registry"]
