use clap::{crate_description, crate_name, crate_version, App, Arg, ArgMatches};
use solana_drone::drone::DRONE_PORT;
use solana_sdk::pubkey::{read_pubkey, Pubkey};
use std::net::SocketAddr;

/// Holds the configuration for a single run of the benchmark
pub struct Config {
    pub tpu_addr: SocketAddr,
    pub rpc_addr: SocketAddr,
    pub ws_addr: SocketAddr,
    pub drone_addr: SocketAddr,
    pub num_gateways: u8,
    pub num_clients: u8,
    pub fee_interval: u16,
    pub lamports: u64,
    pub provider: Pubkey,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            tpu_addr: SocketAddr::from(([127, 0, 0, 1], 8001)),
            rpc_addr: SocketAddr::from(([127, 0, 0, 1], 8899)),
            ws_addr: SocketAddr::from(([127, 0, 0, 1], 8900)),
            drone_addr: SocketAddr::from(([127, 0, 0, 1], DRONE_PORT)),
            num_gateways: 5,
            num_clients: 4,
            fee_interval: 1000,
            lamports: 100_000,
            provider: Pubkey::new_rand(),
        }
    }
}

/// Defines and builds the CLI args for a run of the benchmark
pub fn build_args<'a, 'b>() -> App<'a, 'b> {
    App::new(crate_name!())
        .about(crate_description!())
        .version(crate_version!())
        .arg(
            Arg::with_name("fullnode")
                .short("f")
                .long("fullnode")
                .value_name("IP ADDRESS")
                .takes_value(true)
                .help("Fullnode host to use for RPC and TPU"),
        )
        .arg(
            Arg::with_name("num_gateways")
                .short("g")
                .long("gateways")
                .value_name("NUM")
                .takes_value(true)
                .help("Number of gateways to spin up; default is 5"),
        )
        .arg(
            Arg::with_name("num_clients")
                .short("c")
                .long("clients")
                .value_name("NUM")
                .takes_value(true)
                .help("Number of clients to connect to each gateway; default is 4"),
        )
        .arg(
            Arg::with_name("fee_interval")
                .short("i")
                .long("interval")
                .value_name("MILLIS")
                .takes_value(true)
                .help("How often to charge contract; default is 1000 ms"),
        )
        .arg(
            Arg::with_name("lamports")
                .short("l")
                .long("lamports")
                .value_name("NUM")
                .takes_value(true)
                .help("Number of lamports to fund each contract with; default is 100_000"),
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
}

/// Parses a clap `ArgMatches` structure into a `Config`
/// # Arguments
/// * `matches` - command line arguments parsed by clap
/// # Panics
/// Panics if there is trouble parsing any of the arguments
pub fn extract_args<'a>(matches: &ArgMatches<'a>) -> Config {
    let mut args = Config::default();

    let fullnode = matches
        .value_of("fullnode")
        .unwrap_or("127.0.0.1")
        .parse()
        .unwrap();
    args.rpc_addr = SocketAddr::new(fullnode, 8899); // TODO: don't hard-code this port
    args.ws_addr = SocketAddr::new(fullnode, 8900); // TODO: don't hard-code this port
    args.drone_addr = SocketAddr::new(fullnode, 9900); // TODO: don't hard-code this port

    if let Some(num) = matches.value_of("num_gateways") {
        args.num_gateways = num.to_string().parse().expect("can't parse gateways");
    }
    if let Some(num) = matches.value_of("num_clients") {
        args.num_clients = num.to_string().parse().expect("can't parse clients");
    }
    if let Some(num) = matches.value_of("fee_interval") {
        args.fee_interval = num.to_string().parse().expect("can't parse interval");
    }
    if let Some(num) = matches.value_of("lamports") {
        args.lamports = num.to_string().parse().expect("can't parse lamports");
    }

    if matches.is_present("provider") {
        args.provider =
            read_pubkey(matches.value_of("provider").unwrap()).expect("can't read provider pubkey");
    }

    args
}
