use bandwidth_prepay_api::bandwidth_prepay_instruction;
use log::{error, info};
use serde_derive::Deserialize;
use serde_json::json;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_request::RpcError;
use solana_drone::drone::request_airdrop_transaction;
use solana_sdk::message::Message;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, KeypairUtil};
use solana_sdk::transaction::Transaction;
use std::collections::HashMap;
use std::error;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs};

const MESSAGE_TERMINATOR: &str = "\n";

#[derive(Debug, Deserialize)]
struct RpcResponse {
    jsonrpc: String,
    result: HashMap<String, String>,
    id: u64,
}

pub struct BandwidthClient {
    pub id: Keypair,
    fullnode_client: RpcClient,
}

impl BandwidthClient {
    pub fn new(id: Keypair, fullnode_client: RpcClient) -> Self {
        Self {
            id,
            fullnode_client,
        }
    }

    pub fn request_airdrop(&self, drone_addr: &SocketAddr, lamports: u64) -> Result<(), RpcError> {
        let (blockhash, _) = self.fullnode_client.get_recent_blockhash().map_err(|err| {
            info!("get_recent_blockhash failed: {:?}", err);
            RpcError::RpcRequestError(err.to_string())
        })?;

        let mut transaction =
            request_airdrop_transaction(drone_addr, &self.id.pubkey(), lamports, blockhash)
                .map_err(|err| {
                    info!("request_airdrop_transaction failed: {:?}", err);
                    RpcError::RpcRequestError(err.to_string())
                })?;
        let _ = self
            .fullnode_client
            .send_and_confirm_transaction(&mut transaction, &[&self.id])
            .map_err(|err| {
                info!("request_airdrop: SendTransaction error: {:?}", err);
                RpcError::RpcRequestError(err.to_string())
            })?;
        Ok(())
    }

    pub fn initialize_contract(
        &self,
        lamports: u64,
        gatekeeper_pubkey: &Pubkey,
        provider_pubkey: &Pubkey,
    ) -> Keypair {
        let prepay_account = Keypair::new(); // New contract account
        let (blockhash, _) = self
            .fullnode_client
            .get_recent_blockhash()
            .unwrap_or_default();

        let instructions = bandwidth_prepay_instruction::initialize(
            &self.id.pubkey(),
            &prepay_account.pubkey(),
            &gatekeeper_pubkey,
            &provider_pubkey,
            lamports,
        );
        let message = Message::new(instructions);
        let mut transaction = Transaction::new(&[&self.id], message, blockhash);
        let _ = self
            .fullnode_client
            .send_and_confirm_transaction(&mut transaction, &[&self.id])
            .unwrap();

        prepay_account
    }

    pub fn request_connection<A, B>(
        &self,
        gatekeeper_addr: A,
        destination_addr: B,
        prepay_account: &Pubkey,
    ) -> Result<SocketAddr, Box<dyn error::Error>>
    where
        SocketAddr: std::convert::From<B>,
        A: ToSocketAddrs,
    {
        let mut gatekeeper = TcpStream::connect(gatekeeper_addr)?;

        let destination_addr = SocketAddr::from(destination_addr);

        let request_json = json!({
            "jsonrpc": "2.0",
            "method": "newConnection",
            "params": {
                "destination": format!("{}", destination_addr),
                "contract_pubkey": format!("{}", prepay_account),
                "initiator_pubkey": format!("{}", self.id.pubkey()),
            },
            "id": 1,
        });
        let request = serde_json::to_string(&request_json).unwrap();
        let payload = format!("{}{}", request, MESSAGE_TERMINATOR);
        info!("Sending: {}", payload);

        gatekeeper.write_all(&payload.as_bytes())?;
        let mut response = [0 as u8; 1024];
        let len = gatekeeper.read(&mut response)?;
        let response: RpcResponse = match serde_json::from_slice(&response[..len]) {
            Ok(r) => r,
            Err(e) => {
                error!(
                    "Could not parse RPC reply. Got: '{}'",
                    String::from_utf8_lossy(&response[..len]).replace("\n", "\\n")
                );
                return Err(Box::new(e));
            }
        };
        info!("Recieved: {:?}", response);

        let mut conn_addr = gatekeeper.peer_addr()?;
        conn_addr.set_port(
            response
                .result
                .get(&"port".to_string())
                .expect("No port returned")
                .parse()?,
        );

        gatekeeper.shutdown(Shutdown::Both)?;
        Ok(conn_addr)
    }
}
