use bandwidth_tps::bandwidth_tps::{
    do_bandwidth_tps, fund_keypairs, generate_keypairs, initialize_contracts,
};
use bandwidth_tps::cli;
use log::*;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_request::RpcRequest;
use solana_client::thin_client::create_client;
use solana_drone::drone::request_airdrop_transaction;
use solana_sdk::client::{AsyncClient, SyncClient};
use solana_sdk::signature::{Keypair, KeypairUtil};
use std::io;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    solana_logger::setup();
    let matches = cli::build_args().get_matches();
    let mut config = cli::extract_args(&matches);

    let rpc_client = RpcClient::new_socket(config.rpc_addr);
    let response = rpc_client
        .retry_make_rpc_request(&RpcRequest::GetClusterNodes, None, 5)
        .unwrap();
    let node = response[0].as_object().unwrap();
    config.tpu_addr = node.get("tpu").unwrap().as_str().unwrap().parse()?;

    let client = create_client((config.rpc_addr, config.tpu_addr), (8000, 10_000));

    let provider = Keypair::new();
    let (blockhash, _) = client.get_recent_blockhash().unwrap();
    match request_airdrop_transaction(
        &config.drone_addr,
        &provider.pubkey(),
        ((4 * config.lamports) + 1)
            * (u64::from(config.num_gateways) + 1)
            * u64::from(config.num_clients),
        blockhash,
    ) {
        Ok(transaction) => {
            let signature = client.async_send_transaction(transaction)?;
            client.get_signature_status(&signature)?;
        }
        Err(e) => {
            error!(
                "Error requesting airdrop: {:?} to addr: {:?} amount: 1",
                e, config.drone_addr
            );
        }
    }
    info!("Generating gatekeeper keypairs");
    let gatekeeper_keypairs = generate_keypairs(u64::from(config.num_gateways));
    info!("Funding gatekeepers");
    let _ = fund_keypairs(&client, &provider, &gatekeeper_keypairs, 1)?;
    info!("Generating client keypairs");
    let client_keypairs = generate_keypairs((config.num_gateways * config.num_clients).into());
    info!("Funding clients");
    let _ = fund_keypairs(
        &client,
        &provider,
        &client_keypairs,
        (4 * config.lamports) + 1,
    )?;
    let contracts = initialize_contracts(
        &client,
        &client_keypairs,
        config.lamports,
        &config.provider,
        &gatekeeper_keypairs,
    )?
    .into_iter()
    .map(|(contract, _)| contract)
    .collect();

    info!("Ready...");
    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(_) => (),
        Err(e) => error!("Couldn't read stdin: {}", e),
    }

    do_bandwidth_tps(
        client,
        config,
        gatekeeper_keypairs,
        client_keypairs,
        contracts,
    )?;

    Ok(())
}
