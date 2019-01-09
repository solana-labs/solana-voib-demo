use clap::{crate_version, App, Arg};
use client::bandwidth_client::BandwidthClient;
use log::*;
use provider_drone::DEFAULT_DRONE_PORT;
use solana_client::rpc_client::RpcClient;
use solana_drone::drone::{Drone, DRONE_PORT};
use solana_drone::socketaddr;
use solana_sdk::signature::read_keypair;
use std::error;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::net::TcpListener;
use tokio::prelude::{Future, Sink, Stream};
use tokio_codec::{BytesCodec, Decoder};

fn main() -> Result<(), Box<dyn error::Error>> {
    env_logger::init();
    let matches = App::new("drone")
        .version(crate_version!())
        .arg(
            Arg::with_name("keypair")
                .short("k")
                .long("keypair")
                .value_name("PATH")
                .takes_value(true)
                .required(true)
                .help("File from which to read the provider's keypair"),
        )
        .arg(
            Arg::with_name("fullnode")
                .short("f")
                .long("fullnode")
                .value_name("IP ADDRESS")
                .takes_value(true)
                .help("Fullnode host to use for RPC and drone"),
        )
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .value_name("PORT")
                .takes_value(true)
                .help("Port to bind to"),
        )
        .arg(
            Arg::with_name("lamports")
                .short("l")
                .long("lamports")
                .value_name("NUM")
                .takes_value(true)
                .help("Fund provider with this many lamports"),
        )
        .get_matches();

    let provider_keypair =
        read_keypair(matches.value_of("keypair").unwrap()).expect("failed to read keypair");

    let host = matches
        .value_of("fullnode")
        .unwrap_or("127.0.0.1")
        .parse()
        .unwrap(); // TODO: Need error handling
    let rpc_addr = SocketAddr::new(host, 8899); // TODO: don't hard-code this port
    let drone_addr = SocketAddr::new(host, DRONE_PORT);

    // Airdrop provider starting fund
    let lamports: u64 = if let Some(lamport_str) = matches.value_of("lamports") {
        lamport_str.parse().unwrap()
    } else {
        400_000_000
    };
    let fullnode_client = RpcClient::new_socket(rpc_addr);
    let client = BandwidthClient::new(provider_keypair, fullnode_client);
    client.request_airdrop(&drone_addr, lamports)?;

    let port: u16 = if let Some(port_str) = matches.value_of("port") {
        port_str.parse().unwrap()
    } else {
        DEFAULT_DRONE_PORT
    };
    let drone_addr = socketaddr!(0, port);

    let drone = Arc::new(Mutex::new(Drone::new(client.id, None, None)));

    let drone1 = drone.clone();
    thread::spawn(move || loop {
        let time = drone1.lock().unwrap().time_slice;
        thread::sleep(time);
        drone1.lock().unwrap().clear_request_count();
    });

    let socket = TcpListener::bind(&drone_addr).unwrap();
    info!("Provider Drone started. Listening on: {}", drone_addr);
    let done = socket
        .incoming()
        .map_err(|e| warn!("failed to accept socket; error = {:?}", e))
        .for_each(move |socket| {
            let drone2 = drone.clone();
            let framed = BytesCodec::new().framed(socket);
            let (writer, reader) = framed.split();

            let processor = reader.and_then(move |bytes| {
                let response_bytes = drone2
                    .lock()
                    .unwrap()
                    .process_drone_request(&bytes)
                    .unwrap();
                Ok(response_bytes)
            });
            let server = writer
                .send_all(processor.or_else(|err| {
                    Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Drone response: {:?}", err),
                    ))
                }))
                .then(|_| Ok(()));
            tokio::spawn(server)
        });
    tokio::run(done);
    Ok(())
}
