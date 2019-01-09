use serde_derive::Deserialize;
use solana_sdk::pubkey::Pubkey;

#[derive(Deserialize)]
pub struct NewConnParams {
    pub contract_pubkey: Pubkey,
    pub destination: String,
    pub fee_interval: u16,
}
