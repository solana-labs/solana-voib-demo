use clap::{App, Arg};
use log::*;
use pubsub_client::client::{start_pubsub, Event};
use pubsub_client::request::PubSubRequest;
use serde_json::Value;
use solana_sdk::account::Account;
use solana_sdk::pubkey::read_pubkey;
use std::net::SocketAddr;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = App::new("Write Pubkey")
        .version("0.1.0")
        .about("Reads a private key id.json file and writes the corresponding pubkey to the specified file")
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
                .help("Fullnode host to use for PubSub"),
        )
        .get_matches();
    let provider_pubkey = read_pubkey(matches.value_of("provider").unwrap())?;
    println!("Provider pubkey: {}", provider_pubkey);

    let fullnode = matches
        .value_of("fullnode")
        .unwrap_or("127.0.0.1")
        .parse()
        .unwrap(); // TODO: Need error handling
    let ws_addr = SocketAddr::new(fullnode, 8900); // TODO: don't hard-code this port

    let pubsub_thread = start_pubsub(
        format!("ws://{}", ws_addr),
        PubSubRequest::Account,
        &provider_pubkey,
    )
    .unwrap();

    loop {
        if let Ok(event) = pubsub_thread.receiver.try_recv() {
            match event {
                Event::Message(notification) => {
                    let json: Value =
                        serde_json::from_str(&notification.into_text().unwrap()).unwrap();
                    let account_json = json["params"]["result"].clone();
                    let account: Account = serde_json::from_value(account_json).unwrap();
                    println!(
                        "received notification. account balance: {}",
                        account.lamports
                    );
                }
                Event::Disconnect(_, _) => {
                    warn!("PubSub connection dropped");
                }
                _ => {}
            };
        }
        thread::sleep(Duration::from_millis(10));
    }
}
