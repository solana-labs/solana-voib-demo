use crate::cli::Config;
use crate::gen_keys::GenKeys;
use bandwidth_prepay_api::bandwidth_prepay_instruction;
use gatekeeper::accumulator::Accumulator;
use gatekeeper::connection_params::NewConnParams;
use gatekeeper::contract::{check_contract, submit_transaction_loop};
use gatekeeper::gatekeeper::process_data;
use log::*;
use pubsub_client::client::start_pubsub;
use pubsub_client::request::PubSubRequest;
use solana_sdk::client::Client;
use solana_sdk::message::Message;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, KeypairUtil, Signature};
use solana_sdk::system_instruction;
use solana_sdk::transport::Result as TransportResult;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread::{self, sleep, Builder};
use std::time::Duration;

pub fn do_bandwidth_tps<T>(
    client: T,
    config: Config,
    gatekeeper_keypairs: Vec<Keypair>,
    client_keypairs: Vec<Keypair>,
    contracts: Vec<Pubkey>,
) -> TransportResult<()>
where
    T: 'static + Client + Send + Sync,
{
    let Config {
        ws_addr,
        num_gateways,
        fee_interval,
        ..
    } = config;

    let client = Arc::new(client);
    let gatekeeper_keypairs: Vec<_> = gatekeeper_keypairs.into_iter().map(Arc::new).collect();
    let client_keypairs: Vec<_> = client_keypairs.into_iter().map(Arc::new).collect();

    let threads: Vec<_> = contracts
        .into_iter()
        .enumerate()
        .map(|(i, contract_pubkey)| {
            let client = client.clone();
            let gatekeeper_index = (i + 1) % num_gateways as usize;
            let gatekeeper = gatekeeper_keypairs[gatekeeper_index].clone();
            let client_keypairs = client_keypairs.clone();
            let refund_lamports = config.lamports / 5;
            Builder::new()
                .name("gatekeeper".to_string())
                .spawn(move || {
                    let params = NewConnParams {
                        contract_pubkey,
                        destination: "somewhere".to_string(),
                        fee_interval,
                    };

                    let pubsub_thread = start_pubsub(
                        format!("ws://{}", ws_addr),
                        PubSubRequest::Account,
                        &params.contract_pubkey,
                    )
                    .unwrap();

                    let (solana_sender, solana_receiver) = channel();
                    thread::spawn(move || {
                        submit_transaction_loop(&solana_receiver);
                    });

                    let (balance, contract_state) =
                        check_contract(&params, &client, &gatekeeper.pubkey()).unwrap();

                    let mut accumulator = Accumulator::default();
                    accumulator.initiator_fund = balance;

                    // Offset contract payments to decrease AccountInUse errors paying provider
                    sleep(Duration::from_millis(150 * i as u64));
                    let mut counter = 0;
                    loop {
                        if process_data(
                            &params,
                            &gatekeeper,
                            &client,
                            &contract_state,
                            &mut accumulator,
                            &pubsub_thread.receiver,
                            1024,
                            &solana_sender,
                        ) {
                            break;
                        }
                        counter += 1;
                        if counter == 200 + 20 * i as u64 {
                            // Every 20sec
                            fund_contract(
                                &client,
                                &contract_pubkey,
                                &client_keypairs[i],
                                refund_lamports,
                            )
                            .unwrap();
                            counter = 0;
                        }
                        sleep(Duration::from_millis(100));
                    }
                    info!(
                        "Bytes transmitted via gatekeeper {}: {}",
                        gatekeeper.pubkey(),
                        accumulator.total_data_amount
                    );
                })
                .unwrap()
        })
        .collect();

    for t in threads {
        if let Err(err) = t.join() {
            println!("  join() failed with: {:?}", err);
        }
    }

    Ok(())
}

fn fund_contract<T: Client>(
    client: &Arc<T>,
    contract_pubkey: &Pubkey,
    client_keypair: &Keypair,
    lamports: u64,
) -> TransportResult<Signature> {
    let (blockhash, _) = client.get_recent_blockhash().unwrap();
    let instruction =
        system_instruction::transfer(&client_keypair.pubkey(), contract_pubkey, lamports);
    let message = Message::new(vec![instruction]);
    let signature = client.async_send_message(&[&client_keypair], message, blockhash)?;
    client.get_signature_status(&signature)?;
    Ok(signature)
}

pub fn initialize_contracts<T: Client>(
    client: &T,
    client_keypairs: &[Keypair],
    lamports: u64,
    provider: &Pubkey,
    gatekeeper_keypairs: &[Keypair],
) -> TransportResult<Vec<(Pubkey, Signature)>> {
    let (blockhash, _) = client.get_recent_blockhash().unwrap();
    let mut contracts = Vec::new();
    for (i, keypair) in client_keypairs.iter().enumerate() {
        let gatekeeper_index = (i + 1) % gatekeeper_keypairs.len();
        let contract_pubkey = Pubkey::new_rand();
        let instructions = bandwidth_prepay_instruction::initialize(
            &keypair.pubkey(),
            &contract_pubkey,
            &gatekeeper_keypairs[gatekeeper_index].pubkey(),
            provider,
            lamports,
        );
        let message = Message::new(instructions);
        let signature = client.async_send_message(&[&keypair], message, blockhash)?;
        client.get_signature_status(&signature)?;
        contracts.push((contract_pubkey, signature));
    }
    Ok(contracts)
}

pub fn fund_keypairs<T: Client>(
    client: &T,
    funder: &Keypair,
    keypairs: &[Keypair],
    lamports: u64,
) -> TransportResult<Vec<Signature>> {
    let (blockhash, _) = client.get_recent_blockhash().unwrap();
    let mut signatures = Vec::new();
    for keypair in keypairs {
        let signature = client.async_transfer(lamports, funder, &keypair.pubkey(), blockhash)?;
        client.get_signature_status(&signature)?;
        signatures.push(signature);
    }
    Ok(signatures)
}

pub fn generate_keypairs(num: u64) -> Vec<Keypair> {
    let mut seed = [0_u8; 32];
    seed.copy_from_slice(&Keypair::new().pubkey().as_ref());
    let mut rnd = GenKeys::new(seed);
    rnd.gen_n_keypairs(num)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bandwidth_prepay_api::bandwidth_prepay_processor::process_instruction;
    use bandwidth_prepay_api::bandwidth_prepay_state::BandwidthPrepayState;
    use bandwidth_prepay_api::id;
    use solana_runtime::bank::Bank;
    use solana_runtime::bank_client::BankClient;
    use solana_sdk::client::SyncClient;
    use solana_sdk::genesis_block::create_genesis_block;
    use solana_sdk::system_instruction;

    fn create_bank(lamports: u64) -> (Bank, Keypair) {
        let (genesis_block, mint_keypair) = create_genesis_block(lamports);
        let mut bank = Bank::new(&genesis_block);
        bank.add_instruction_processor(id(), process_instruction);
        (bank, mint_keypair)
    }

    #[test]
    fn test_initialize_contract() {
        let (bank, alice_keypair) = create_bank(10_000);
        let bank_client = BankClient::new(bank);
        // TODO: Multiples don't currently work due to AccountInUse errors during bank processing
        // Update test when fixed
        let client_keypairs = vec![Keypair::new()];

        let provider = Keypair::new().pubkey();

        let gatekeeper_keypairs = vec![Keypair::new()];

        for keypair in &client_keypairs {
            let instruction =
                system_instruction::transfer(&alice_keypair.pubkey(), &keypair.pubkey(), 100);
            let message = Message::new(vec![instruction]);
            bank_client
                .send_message(&[&alice_keypair], message)
                .unwrap();
        }

        for keypair in &gatekeeper_keypairs {
            let instruction =
                system_instruction::transfer(&alice_keypair.pubkey(), &keypair.pubkey(), 1);
            let message = Message::new(vec![instruction]);
            bank_client
                .send_message(&[&alice_keypair], message)
                .unwrap();
        }

        let (contract, _signature) = initialize_contracts(
            &bank_client,
            &client_keypairs,
            90,
            &provider,
            &gatekeeper_keypairs,
        )
        .unwrap()[0];
        let mut balance = 0;
        while balance == 0 {
            balance = bank_client.get_balance(&contract).unwrap();
        }
        assert_eq!(balance, 90);
        let account_data = bank_client.get_account_data(&contract).unwrap().unwrap();
        let state = BandwidthPrepayState::deserialize(&account_data).unwrap();
        assert_eq!(state.gatekeeper_id, gatekeeper_keypairs[0].pubkey());
        assert_eq!(state.provider_id, provider);
        assert_eq!(state.initiator_id, client_keypairs[0].pubkey());
    }

    #[test]
    fn test_fund_keypairs() {
        let (bank, alice_keypair) = create_bank(10_000);
        let bank_client = BankClient::new(bank);
        // TODO: Multiples don't currently work due to AccountInUse errors during bank processing
        // Update test when fixed
        let keypairs = vec![Keypair::new()];

        let signatures = fund_keypairs(&bank_client, &alice_keypair, &keypairs, 100);
        assert_eq!(signatures.unwrap().len(), 1);
        let mut balance = 0;
        while balance == 0 {
            balance = bank_client.get_balance(&keypairs[0].pubkey()).unwrap();
        }
        assert_eq!(balance, 100);
    }

    #[test]
    fn test_generate_keypairs() {
        let keypairs = generate_keypairs(10);
        assert_eq!(keypairs.len(), 10);

        // This keypair generation is expected to be non-deterministic
        let more_keypairs = generate_keypairs(10);
        for (i, keypair) in keypairs.iter().enumerate() {
            assert_ne!(keypair.pubkey(), more_keypairs[i].pubkey());
        }
    }
}
