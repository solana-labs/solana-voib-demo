use clap::{App, Arg, SubCommand};
use client::bandwidth_client::BandwidthClient;
use provider_drone::DEFAULT_DRONE_PORT;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::read_pubkey;
use solana_sdk::signature::{read_keypair, KeypairUtil};
use std::net::SocketAddr;
use stream_video::stream_video::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let matches = App::new("Data Counter Tester")
        .subcommand(
            SubCommand::with_name("connect")
                .about("Initiates a connection")
                .arg(
                    Arg::with_name("keypair")
                        .short("k")
                        .long("keypair")
                        .value_name("PATH")
                        .takes_value(true)
                        .required(true)
                        .help("/path/to/id.json"),
                )
                .arg(
                    Arg::with_name("gatekeeper_pubkey")
                        .short("g")
                        .long("gatekeeper-pubkey")
                        .value_name("PATH")
                        .takes_value(true)
                        .required(true)
                        .help("/path/to/gatekeeper/pubkey.json"),
                )
                .arg(
                    Arg::with_name("provider")
                        .short("p")
                        .long("provider")
                        .value_name("PATH")
                        .takes_value(true)
                        .required(true)
                        .help("/path/to/provider/pubkey.json"),
                )
                .arg(
                    Arg::with_name("fullnode")
                        .short("f")
                        .long("fullnode")
                        .value_name("IP ADDRESS")
                        .takes_value(true)
                        .help("Fullnode host to use for RPC"),
                )
                .arg(
                    Arg::with_name("gatekeeper_addr")
                        .short("G")
                        .long("gatekeeper")
                        .value_name("HOST:PORT")
                        .takes_value(true)
                        .required(true)
                        .help("Gatekeeper RPC endpoint"),
                )
                .arg(
                    Arg::with_name("destination")
                        .short("d")
                        .long("destination")
                        .value_name("HOST:PORT")
                        .takes_value(true)
                        .required(true)
                        .help("Destination address"),
                )
                .arg(
                    Arg::with_name("lamports")
                        .short("l")
                        .long("lamports")
                        .value_name("NUM")
                        .takes_value(true)
                        .help("Number of lamports to fund contract with"),
                ),
        )
        .subcommand(
            SubCommand::with_name("listen")
                .about("Listens for a connection")
                .arg(
                    Arg::with_name("port")
                        .short("p")
                        .long("port")
                        .value_name("PORT")
                        .takes_value(true)
                        .help("Port to listen for connection on, defaults to 8123"),
                ),
        )
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("listen") {
        let port = matches.value_of("port").unwrap_or("8123");
        let port = port.parse::<u16>()?;

        let mut video_listener = VideoManager::new_video_listener(port, None)?;

        video_listener.wait()?;
    } else {
        let matches = matches.subcommand_matches("connect").unwrap();

        let client_account = read_keypair(matches.value_of("keypair").unwrap())?;
        let gatekeeper_pubkey = read_pubkey(matches.value_of("gatekeeper_pubkey").unwrap())?;
        let provider_pubkey = read_pubkey(matches.value_of("provider").unwrap())?;

        // Set up Solana bandwidth prepayment contract
        let host = matches
            .value_of("fullnode")
            .unwrap_or("127.0.0.1")
            .parse()
            .unwrap(); // TODO: Need error handling
        let rpc_addr = SocketAddr::new(host, 8899); // TODO: don't hard-code this port

        let lamports: u64 = if let Some(lamport_str) = matches.value_of("lamports") {
            lamport_str.parse().unwrap()
        } else {
            5_000_000
        };

        let fullnode_client = RpcClient::new_socket(rpc_addr);
        let client = BandwidthClient::new(client_account, fullnode_client);

        let drone_addr = SocketAddr::new(host, DEFAULT_DRONE_PORT);
        client.request_airdrop(&drone_addr, lamports + 1)?;
        let prepay_account =
            client.initialize_contract(lamports, &gatekeeper_pubkey, &provider_pubkey);

        // Start connection
        let gatekeeper_addr = matches.value_of("gatekeeper_addr").unwrap();
        let destination = matches.value_of("destination").unwrap();
        let destination: SocketAddr = destination.parse()?;

        let connection_addr =
            client.request_connection(gatekeeper_addr, destination, &prepay_account.pubkey())?;

        let mut video_connecter = VideoManager::new_video_connecter(&connection_addr, None)?;

        video_connecter.wait()?;
    };

    Ok(())
}
