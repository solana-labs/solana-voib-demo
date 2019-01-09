use crate::bandwidth_prepay_state::BandwidthPrepayState;
use crate::id;
use serde_derive::{Deserialize, Serialize};
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::system_instruction;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum BandwidthPrepayInstruction {
    InitializeAccount,
    Spend(u64),
    Refund,
}

pub fn initialize(
    initiator_id: &Pubkey,
    contract_id: &Pubkey,
    gatekeeper_id: &Pubkey,
    provider_id: &Pubkey,
    lamports: u64,
) -> Vec<Instruction> {
    let space = BandwidthPrepayState::max_size() as u64;
    vec![
        system_instruction::create_account(&initiator_id, contract_id, lamports, space, &id()),
        initialize_account(initiator_id, contract_id, gatekeeper_id, provider_id),
    ]
}

fn initialize_account(
    initiator_id: &Pubkey,
    contract_id: &Pubkey,
    gatekeeper_id: &Pubkey,
    provider_id: &Pubkey,
) -> Instruction {
    let account_metas = vec![
        AccountMeta::new(*initiator_id, true),
        AccountMeta::new(*contract_id, false),
        AccountMeta::new(*gatekeeper_id, false),
        AccountMeta::new(*provider_id, false),
    ];
    Instruction::new(
        id(),
        &BandwidthPrepayInstruction::InitializeAccount,
        account_metas,
    )
}

pub fn spend(
    gatekeeper_id: &Pubkey,
    contract_id: &Pubkey,
    provider_id: &Pubkey,
    amount: u64,
) -> Instruction {
    let account_metas = vec![
        AccountMeta::new(*gatekeeper_id, true),
        AccountMeta::new(*contract_id, false),
        AccountMeta::new(*provider_id, false),
    ];
    Instruction::new(
        id(),
        &BandwidthPrepayInstruction::Spend(amount),
        account_metas,
    )
}

pub fn refund(gatekeeper_id: &Pubkey, contract_id: &Pubkey, initiator_id: &Pubkey) -> Instruction {
    let account_metas = vec![
        AccountMeta::new(*gatekeeper_id, true),
        AccountMeta::new(*contract_id, false),
        AccountMeta::new(*initiator_id, false),
    ];
    Instruction::new(id(), &BandwidthPrepayInstruction::Refund, account_metas)
}
