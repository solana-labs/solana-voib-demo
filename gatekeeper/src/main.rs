use clap::{App, Arg};
use gatekeeper::connection_params::NewConnParams;
use gatekeeper::contract::*;
use gatekeeper::gatekeeper::forwarder;
use jsonrpc_core::types::error::{Error, ErrorCode};
use jsonrpc_core::{IoHandler, Params};
use jsonrpc_tcp_server::ServerBuilder;
use log::*;
use serde_json::{json, Value};
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_request::RpcRequest;
use solana_client::thin_client::create_client;
use solana_drone::drone::request_airdrop_transaction;
use solana_sdk::client::{AsyncClient, SyncClient};
use solana_sdk::signature::{read_keypair, KeypairUtil};
use std::net::SocketAddr;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let matches = App::new("Data Counter Forwarder")
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
            Arg::with_name("fullnode")
                .short("f")
                .long("fullnode")
                .value_name("IP ADDRESS")
                .takes_value(true)
                .help("Fullnode host to use for RPC"),
        )
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .value_name("PORT")
                .takes_value(true)
                .help("Port to bind RPC listener to. Defaults to 8122"),
        )
        .arg(
            Arg::with_name("fee_interval")
                .short("i")
                .long("interval")
                .value_name("SECS")
                .takes_value(true)
                .help("How often to charge contract"),
        )
        .get_matches();
    let gatekeeper_keypair_path = matches.value_of("keypair").unwrap().to_string();
    let gatekeeper = read_keypair(&gatekeeper_keypair_path).unwrap();
    info!("Gatekeeper Pubkey: {:?}", gatekeeper.pubkey());

    let fullnode = matches
        .value_of("fullnode")
        .unwrap_or("127.0.0.1")
        .parse()
        .unwrap(); // TODO: Need error handling
    let rpc_addr = SocketAddr::new(fullnode, 8899); // TODO: don't hard-code this port
    let ws_addr = SocketAddr::new(fullnode, 8900); // TODO: don't hard-code this port
    let drone_addr = SocketAddr::new(fullnode, 9900); // TODO: don't hard-code this port

    let rpc_client = RpcClient::new_socket(rpc_addr);
    let response = rpc_client.retry_make_rpc_request(&RpcRequest::GetClusterNodes, None, 5)?;
    let node = response[0].as_object().unwrap();
    let tpu_addr = node.get("tpu").unwrap().as_str().unwrap().parse()?;

    let client = create_client((rpc_addr, tpu_addr), (8000, 10_000));

    let port = matches.value_of("port").unwrap_or("8122");

    // Calculate fee interval in milliseconds
    let fee_interval: u16 = if let Some(interval) = matches.value_of("fee_interval") {
        interval.parse().unwrap()
    } else {
        1
    } * 1000;

    // TODO: handle initial account funding properly, probably separate from this script
    let balance = client.get_balance(&gatekeeper.pubkey()).unwrap_or(0);
    if balance == 0 {
        let (blockhash, _) = client.get_recent_blockhash().unwrap();
        match request_airdrop_transaction(&drone_addr, &gatekeeper.pubkey(), 1, blockhash) {
            Ok(transaction) => {
                let signature = client.async_send_transaction(transaction).unwrap();
                client.get_signature_status(&signature).unwrap();
            }
            Err(e) => {
                error!(
                    "Error requesting airdrop: {:?} to addr: {:?} amount: 1",
                    e, drone_addr
                );
            }
        }
    }

    let client = Arc::new(client);

    let mut io = IoHandler::default();
    io.add_method("newConnection", move |params: Params| {
        let flat_params: serde_json::map::Map<String, Value> = params.parse()?;
        let parsed_params = NewConnParams {
            contract_pubkey: verify_pubkey(
                flat_params["contract_pubkey"].as_str().unwrap().to_string(),
            )?,
            destination: flat_params["destination"].as_str().unwrap().to_string(),
            fee_interval,
        };
        let initiator_pubkey = verify_pubkey(
            flat_params["initiator_pubkey"]
                .as_str()
                .unwrap()
                .to_string(),
        )?;
        info!(
            "Received forward request to '{}', contract: {:?}",
            &parsed_params.destination, &parsed_params.contract_pubkey
        );

        let gatekeeper = read_keypair(&gatekeeper_keypair_path).unwrap();

        let (balance, contract_state) =
            check_contract(&parsed_params, &client, &gatekeeper.pubkey()).map_err(|e| {
                error!(
                    "could not check contract: {:?} {:?}",
                    parsed_params.contract_pubkey, e
                );
                Error::invalid_request()
            })?;
        if balance == 0 {
            error!("prepay balance is 0: {:?}", parsed_params.contract_pubkey);
            return Err(Error::invalid_request());
        }
        if contract_state.initiator_id != initiator_pubkey {
            error!(
                "initator pubkey {} does not match contract state",
                initiator_pubkey
            );
            return Err(Error::invalid_request());
        }

        info!(
            "Starting new connection to '{}'",
            &parsed_params.destination
        );

        let client = client.clone();
        let (send, recv) = channel();
        thread::spawn(move || {
            forwarder(
                &parsed_params,
                &gatekeeper,
                &client,
                &contract_state,
                balance,
                ws_addr,
                send,
            )
        });
        match recv.recv() {
            Ok(new_port) => {
                let ret = json!({ "port": format!("{}", new_port) });
                info!(
                    "Started new gatekeeper channel at {}, returning {:?}",
                    new_port, ret
                );
                Ok(ret)
            }
            Err(_e) => {
                error!("Could not get port from forwarder thread");
                Err(Error::new(ErrorCode::ServerError(2)))
            }
        }
    });

    let gatekeeper = ServerBuilder::new(io).start(&format!("0.0.0.0:{}", port).parse()?)?;
    info!("Gatekeeper listening on port {}", port);

    gatekeeper.wait();

    Ok(())
}
