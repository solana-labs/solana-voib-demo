# Solana VoIB Demo

## Introduction

Demonstration of the tokenization business model to implement Voice over
Internet & Blockchain (VoIB).

### Video Streaming Demo
Video streaming devices (currently Raspberry Pis) implement thin clients. They
communicate with a *gatekeeper* application on a network gateway, which will open
a connection upon request from a device. The data is not part of the transaction
to Solana, so the data itself is not stored on chain. Devices pay for data before
opening a connection by funding an account controlled by the bandwidth-prepay
program. The gatekeeper can send instructions to the program either spending the
account funds as data is sent, or refunding any remaining balance once the
connection is closed. The [client-tester](./client-tester) and
[tcp-echo-server](./tcp-echo-server) modules can interact with the gatekeeper
code to test data transmission locally, not requiring any extra hardware.

The video demo right now is designed to run on a Raspberry Pi connected to an
official [Raspberry Pi Foundation camera v2](https://www.raspberrypi.org/products/camera-module-v2/),
and an official [Raspberry Pi Foundation touchscreen](https://www.raspberrypi.org/products/raspberry-pi-touch-display/).

## Installing

To build the UI, you need to install gtk. Instructions [here](http://gtk-rs.org/docs/requirements.html).


### Installing ffmpeg and mpv

`ffmpeg` and `mpv` need to be installed on the Pis for the video demo. We run
with versions built on the Pis by [this script](https://www.raspberrypi.org/forums/viewtopic.php?p=1249934),
but have had varying success with the script. We have had to resort to copying
successful builds onto unsuccessful machines. Versions of `ffmpeg` and `mpv`
installed by other methods such as `apt-get` will likely work, but may have
higher latency and/or CPU usage.


## Cross compiling for the Raspberry Pi

While it is possible to build the project on a Raspberry Pi directly using
Cargo, it will take a very long time (Initial build >30 min on a Pi 3 B+).
Instead, you can easily build a cross compiler with Docker, then use `scp` or
some other method to transfer the binaries to the Pis.

To start, install [Docker](https://www.docker.com/) and ensure that it is
running in the background.

Next, navigate to the [docker-xc](./docker-xc) directory and run (This will
take a while)

```shell
$ docker build -t rust-pi-xc .
```

This builds a new docker image called `rust-pi-xc` that is ready to cross
compile any rust project to Cargo's `armv7-unknown-linux-gnueabihf` target.

Now, any time you want to cross compile the `stream-video` package, go to the
[stream-video](./stream-video) directory and run

```shell
$ docker run -v </absolute/path/to/repo/root>:/mnt -v </absolute/path/to/$HOME/.cargo/registry>:/root/.cargo/registry rust-pi-xc /mnt/stream-video/rpxc.sh
```

This runs `rpxc.sh` in the Docker container. The script tells the instance of
Cargo in the container to cross compile the `stream-video` project. The script
needs to be adapted to build other projects. `docker run`'s `-v` options link
directories in the host system to directories in the container, so that the
build files persist. Once the compile is done, the binaries can be found in
`target/armv7-unknown-linux-gnueabihf/debug/`. The rest of this guide assumes
that you have copied the compiled binaries to the `target/xc/` directory in the
project directory on the Pis.


## Setting up the video demo

### Deploying the bandwidth prepay program

This repository depends on a [Solana](https://github.com/solana-labs/solana)
cluster, currently synced to v0.18.0. On the machine that will run the solana
cluster, navigate to `solana-voib-demo` root and clone `solana` with the command:

```shell
$ git clone --branch v0.18.0 https://github.com/solana-labs/solana.git
```

Then build solana:

```shell
$ cd solana && cargo build --all
```

Deploy the bandwidth-prepay program:

```shell
$ cd bandwidth-prepay-program
$ ./deploy.sh
```

### Setting up keypairs

The initiator, the gatekeeper, and the provider all need keypairs for the demo.
The initiator can be an instance of [client-tester](./client-tester) for a local
demo, or it can be an instance of [stream-video](./stream-video) running on a Pi
for a video demo.

To setup the device's pubkey, navigate to either the `client-tester` directory
locally, or the `stream-video` directory on a Pi and run

```shell
$ ./setup.sh
```
This puts the device's id.json into the `config-local` direcory.

To setup the gatekeeper's pubkey, navigate to the `gatekeeper` directory and run

```shell
$ ./setup.sh gatekeeper
```
This puts the gatekeeper's id.json and pubkey.json into the `config-local` directory.

To setup the provider's pubkey, run

```shell
$ ./setup.sh provider
```
This puts the provider's id.json and pubkey.json in gatekeeper/config-local/

Both the `gatekeeper-pubkey.json` and `provider-pubkey.json` files need to be
copied to the `client-tester`'s or the `stream-video`'s `config-local` directory.


## Running the video demo

The video demo runs in five parts (and should be started in this order):
1. The solana cluster
2. The provider drone
3. The gatekeeper program
4. The video listener
5. The video connecter

The video receiver and sender should run on separate Pis. The gatekeeper and
cluster can be run on the same computer, or separate ones. A complete local
demo, sending zeros instead of video data, can be run by replacing the video
receiver with [tcp-echo-server](./tcp-echo-server), and replacing the video
sender with [client-tester](./client-tester).

### Starting the Solana cluster

See the [Solana Book](https://solana-labs.github.io/book/getting-started.html)
for instructions on how to start a testnet from the `solana` repo. You can use
either a single-node or multi-node testnet.

### Starting the provider drone

The provider drone distributes lamports from the provider's account to clients
upon request.

In a new shell, navigate to the `provider-drone` directory and run

```shell
$ cargo run -- -k ../gatekeeper/config-local/provider-id.json
```

For useful messages from the drone, run with the environment variable
```shell
RUST_LOG=provider_drone=info,solana_drone::drone=info
```

### Starting the gatekeeper program

In a new shell, navigate to the `gatekeeper` directory and run

```shell
$ cargo run --bin gatekeeper -- -k config-local/gatekeeper-id.json
```
This will listen on the default port of 8122.

You can get a complete set of command line options by running

```shell
$ cargo run --bin gatekeeper -- -h
```

If you would like account balance change notifications and other debug messages,
run with the environment variable

```shell
RUST_LOG=gatekeeper,gatekeeper::contract=info
```

### Starting the video listener

#### Running the GUI

The GUI operates bi-directionally, so it can act as either the video listener,
or the video connecter, and can switch while running. A connection can be
started from the GUI by pressing one of the call buttons at the top of the
screen. To start it, begin by navigating to the `stream-video` directory. Then,
ensure that the settings in `config-local/config.toml` are correct for your
setup. If you do not have a `config-local/config.toml`, use
`template-config.toml` as a template in creating one. Finally, run the GUI in
one of the following ways:

1. Running the cross-compiled version
```shell
$ DISPLAY=:0.0 ../target/xc/stream_gui
```

2. Running a locally compiled version
```shell
$ DISPLAY=:0.0 cargo run --bin stream_gui
```

To get helpful debug messages, run with
```shell
RUST_LOG=stream_gui,stream_video::stream_video=debug
```

#### Running the CLI

On the Pi, navigate to the `stream-video` directory and run one of the
following:

1. Running the cross-compiled version
```shell
$ ../target/xc/stream_cli listen
```

2. Running a locally compiled version
```shell
$ cargo run --bin stream_cli -- listen
```

To get helpful debug messages, run with
```shell
RUST_LOG=stream_cli,stream_video::stream_video=debug
```

#### Local demo

The local demo replacement is to run `cargo run -- -p <PORT>` from the
`tcp-echo-server` directory. `<PORT>` specifies the listening port.

### Starting the video connecter

#### Running the GUI

See "Running the GUI" in "Starting the video listener"

#### Running the CLI

On the Pi, navigate to the `stream-video` directory and run one of the
following:

1. Running the cross-compiled version
```shell
$ ../target/xc/stream_cli connect -g </path/to/gatekeeper-pubkey.json> -v </path/to/provider-pubkey.json> -k </path/to/id.json> -G <GATEKEEPER_ADDRESS:PORT> -f <FULLNODE_ADDRESS> -l <NUMBER> -d <DESTINATION_ADDRESS:PORT>
```

2. Running a locally compiled version
```shell
$ cargo run --bin stream_cli -- connect -g </path/to/gatekeeper-pubkey.json> -v </path/to/provider-pubkey.json> -k </path/to/id.json> -G <GATEKEEPER_ADDRESS:PORT> -f <FULLNODE_ADDRESS> -l <NUMBER> -d <DESTINATION_ADDRESS:PORT>
```
where `<FULLNODE_ADDRESS>` is the IP address of a node in the solana cluster,
`<NUMBER>` is the number of tokens to prepay into the contract, and
`<DESTINATION_ADDRESS:PORT>` is the address and port of the video listener.


You can get a complete set of command line options by running

```shell
$ cargo run --bin stream_cli -- connect -h
```

To get helpful debug messages, run with
```shell
RUST_LOG=stream_cli,stream_video::stream_video=debug
```

#### Local demo

The local demo replacement is to run the `client-tester`. The arguments are the
same, with the addition of the optional arguments `-n <NUMBER>` to specify the
number of packets to send before closing the connection, and `-s <SIZE>` to
specity the size in bytes of the packets. A complete set of its CLI options can
be found by running `cargo run -- -h` from the `client-tester` directory.

### Observing provider funds

You can optionally observe changes to the provider account's balance by
navigating to the `gatekeeper` directory and running

```shell
$ cargo run --bin provider-account -- -f <FULLNODE ADDRESS> -p </path/to/provider-pubkey.json>
```
where `<FULLNODE_ADDRESS>` is the IP address of a node in the solana cluster.
