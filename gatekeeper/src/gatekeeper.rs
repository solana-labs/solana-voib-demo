use crate::accumulator::Accumulator;
use crate::business_logic::business_logic;
use crate::connection_params::NewConnParams;
use crate::contract::*;
use bandwidth_prepay_api::bandwidth_prepay_state::BandwidthPrepayState;
use log::*;
use mio::net::TcpStream;
use mio::unix::UnixReady;
use mio::{Events, Poll, PollOpt, Ready, Token};
use pubsub_client::client::{start_pubsub, Event};
use pubsub_client::request::PubSubRequest;
use serde_json::Value;
use solana_sdk::account::Account;
use solana_sdk::client::Client;
use solana_sdk::signature::{Keypair, KeypairUtil};
use solana_sdk::transaction::Transaction;
use std::io::ErrorKind;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

const DESTINATION: Token = Token(0);
const ORIGIN: Token = Token(1);

pub fn forwarder<T>(
    params: &NewConnParams,
    gatekeeper: &Keypair,
    client: &Arc<T>,
    contract_state: &BandwidthPrepayState,
    starting_balance: u64,
    ws_addr: SocketAddr,
    sender: Sender<u16>,
) where
    T: 'static + Client + Send + Sync,
{
    let poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(1024);

    info!("Connecting to {}", params.destination);
    let destination = std::net::TcpStream::connect(params.destination.clone()).unwrap(); // Blocking call, unlike mio's sockets
    let mut destination = TcpStream::from_stream(destination).unwrap(); // Convert to mio socket
    info!("Connected to {}", destination.peer_addr().unwrap());
    poll.register(
        &destination,
        DESTINATION,
        Ready::readable(),
        PollOpt::edge(),
    )
    .unwrap();

    let listener = TcpListener::bind("0.0.0.0:0".to_string()).unwrap();
    sender.send(listener.local_addr().unwrap().port()).unwrap();

    let (socket, addr) = listener.accept().unwrap();
    info!("Gatekeeper connected to {}", addr);

    let mut origin = TcpStream::from_stream(socket).unwrap();
    poll.register(
        &origin,
        ORIGIN,
        Ready::readable() | UnixReady::hup(),
        PollOpt::edge(),
    )
    .unwrap();

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

    let mut accumulator = Accumulator::default();
    let mut data = [0 as u8; 1024];
    accumulator.initiator_fund = starting_balance;
    let initiator = origin.peer_addr().unwrap();
    let recipient = destination.peer_addr().unwrap();

    'outer: loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            match event.token() {
                ORIGIN => {
                    if UnixReady::from(event.readiness()).is_hup() {
                        break 'outer;
                    }
                    if event.readiness().is_readable() {
                        while match origin.read(&mut data) {
                            Ok(data_amount) => {
                                if process_data(
                                    params,
                                    gatekeeper,
                                    client,
                                    contract_state,
                                    &mut accumulator,
                                    &pubsub_thread.receiver,
                                    data_amount as u64,
                                    &solana_sender,
                                ) {
                                    break 'outer;
                                }
                                destination.write_all(&data[0..data_amount]).unwrap();
                                true
                            }
                            Err(ref e) if e.kind() == ErrorKind::WouldBlock => false,
                            Err(ref e) if e.kind() == ErrorKind::ConnectionReset => {
                                break 'outer;
                            }
                            Err(e) => Err(e).unwrap(),
                        } {}
                    }
                }
                DESTINATION => {
                    while match destination.read(&mut data) {
                        Ok(data_amount) => {
                            if process_data(
                                params,
                                gatekeeper,
                                client,
                                contract_state,
                                &mut accumulator,
                                &pubsub_thread.receiver,
                                data_amount as u64,
                                &solana_sender,
                            ) {
                                break 'outer;
                            }
                            origin.write_all(&data[0..data_amount]).unwrap();
                            true
                        }
                        Err(ref e) if e.kind() == ErrorKind::WouldBlock => false,
                        Err(ref e) if e.kind() == ErrorKind::ConnectionReset => {
                            break 'outer;
                        }
                        Err(e) => Err(e).unwrap(),
                    } {}
                }
                token => info!("Invalid token: {:?}", token),
            }
        }
    }
    if let Ok((_, contract_state)) = check_contract(params, client, &gatekeeper.pubkey()) {
        if accumulator.amount_charged > 0 {
            charge_contract(
                params,
                client,
                &contract_state,
                gatekeeper,
                accumulator.amount_charged,
            )
            .unwrap();
        }
        refund(params, client, &contract_state, gatekeeper).unwrap();
    }

    info!(
        "Bytes transmitted between {} and {}: {}",
        initiator, recipient, accumulator.total_data_amount
    );

    // close the socket
    drop(listener);
}

pub fn process_data<T: Client>(
    params: &NewConnParams,
    gatekeeper: &Keypair,
    client: &Arc<T>,
    contract_state: &BandwidthPrepayState,
    accumulator: &mut Accumulator,
    pubsub_receiver: &Receiver<Event>,
    data_amount: u64,
    solana_sender: &Sender<(Arc<T>, Transaction)>,
) -> bool {
    if let Ok(event) = pubsub_receiver.try_recv() {
        match event {
            Event::Message(notification) => {
                let json: Value = serde_json::from_str(&notification.into_text().unwrap()).unwrap();
                let account_json = json["params"]["result"].clone();
                let account: Account = serde_json::from_value(account_json).unwrap();
                info!(
                    "received notification. account balance: {}",
                    account.lamports
                );
                accumulator.initiator_fund = account.lamports;
            }
            Event::Disconnect(_, _) => {
                warn!("PubSub connection dropped");
            }
            _ => {}
        };
    }

    let cost = business_logic(data_amount);
    if accumulator.amount_charged + cost <= accumulator.initiator_fund {
        accumulator.amount_charged += cost;
        accumulator.total_data_amount += data_amount;

        if accumulator.now.elapsed().as_millis() > u128::from(params.fee_interval) {
            info!(
                "Account balance: {}, Cost: {}",
                accumulator.initiator_fund, accumulator.amount_charged
            );
            let transaction = build_and_sign_spend_transaction(
                client,
                gatekeeper,
                &params.contract_pubkey,
                &contract_state.provider_id,
                accumulator.amount_charged,
            );
            let client = client.clone();
            if let Err(e) = solana_sender.send((client, transaction)) {
                error!("Error sending amount to be charged: {}", e);
            } else {
                accumulator.initiator_fund -= accumulator.amount_charged;
                accumulator.amount_charged = 0;
            }
            accumulator.now = Instant::now();
        }
        false
    } else {
        info!(
            "Account balance: {}, Cost: {}",
            accumulator.initiator_fund, accumulator.amount_charged
        );
        charge_contract(
            params,
            client,
            contract_state,
            gatekeeper,
            accumulator.amount_charged,
        )
        .unwrap();
        if accumulator.initiator_fund - accumulator.amount_charged > 0 {
            refund(params, client, contract_state, gatekeeper).unwrap();
        }
        true
    }
}
