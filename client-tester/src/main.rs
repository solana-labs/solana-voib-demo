use clap::{App, Arg};
use client::bandwidth_client::BandwidthClient;
use pbr::ProgressBar;
use provider_drone::DEFAULT_DRONE_PORT;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::read_pubkey;
use solana_sdk::signature::{read_keypair, KeypairUtil};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let matches = App::new("Data Counter Tester")
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
            Arg::with_name("packet_size")
                .short("s")
                .long("packet-size")
                .value_name("SIZE")
                .takes_value(true)
                .help("Size of a packet in bytes. Defaults to 1024"),
        )
        .arg(
            Arg::with_name("num_packets")
                .short("n")
                .long("num-packets")
                .value_name("NUM")
                .takes_value(true)
                .help("Number of packets to send. Defaults to 1_000_000"),
        )
        .arg(
            Arg::with_name("lamports")
                .short("l")
                .long("lamports")
                .value_name("NUM")
                .takes_value(true)
                .help("Number of lamports to fund contract with"),
        )
        .get_matches();

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

    // Make connection request
    let packet_size: usize = matches.value_of("packet_size").unwrap_or("1024").parse()?;
    let num_packets: usize = matches
        .value_of("num_packets")
        .unwrap_or("1000000")
        .parse()?;

    let fullnode_client = RpcClient::new_socket(rpc_addr);
    let client = BandwidthClient::new(client_account, fullnode_client);

    let drone_addr = SocketAddr::new(host, DEFAULT_DRONE_PORT);
    client.request_airdrop(&drone_addr, lamports + 1)?;
    let prepay_account = client.initialize_contract(lamports, &gatekeeper_pubkey, &provider_pubkey);

    let gatekeeper_addr = matches.value_of("gatekeeper_addr").unwrap();
    let destination = matches.value_of("destination").unwrap();
    let destination: SocketAddr = destination.parse()?;

    let data_addr =
        client.request_connection(gatekeeper_addr, destination, &prepay_account.pubkey())?;

    let mut data_addr = TcpStream::connect(data_addr)?;

    let to_send: Vec<u8> = vec![0; packet_size];

    let mut pings: Vec<u32> = Vec::with_capacity(num_packets);

    let mut pb = ProgressBar::new(num_packets as u64);
    pb.message("Packets recieved: ");

    let begin = Instant::now();
    for _ in 0..num_packets {
        let start = Instant::now();
        data_addr.write_all(&to_send)?;

        let mut data = [0 as u8; 1024];
        let amount = data_addr.read(&mut data)?;
        pings.push(start.elapsed().subsec_micros());
        assert_eq!(amount, packet_size);
        pb.inc();
    }
    let time = begin.elapsed().subsec_micros();
    pb.finish();

    let total: u32 = pings.iter().sum();

    println!("Sent {} bytes {} times", packet_size, num_packets);
    println!("Average ping time was {}us", total as usize / pings.len());
    println!(
        "Total bandwidth: {}MBps",
        ((packet_size * num_packets * 2) as f64 / (f64::from(time) / 1_000_000f64)) / 1_000_000f64
    );

    data_addr.shutdown(Shutdown::Both)?;

    Ok(())
}
