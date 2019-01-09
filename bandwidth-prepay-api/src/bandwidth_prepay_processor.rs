use crate::bandwidth_prepay_instruction::BandwidthPrepayInstruction;
use crate::bandwidth_prepay_state::{BandwidthPrepayError, BandwidthPrepayState};
use bincode::deserialize;
use solana_sdk::account::KeyedAccount;
use solana_sdk::instruction::InstructionError;
use solana_sdk::pubkey::Pubkey;

fn initialize_account(keyed_accounts: &mut [KeyedAccount]) -> Result<(), BandwidthPrepayError> {
    if let Ok(state) = BandwidthPrepayState::deserialize(&keyed_accounts[1].account.data) {
        if state != BandwidthPrepayState::default() {
            Err(BandwidthPrepayError::AlreadyInitialized)?
        }
    }
    let state = BandwidthPrepayState {
        initiator_id: *keyed_accounts[0].signer_key().unwrap(),
        gatekeeper_id: *keyed_accounts[2].unsigned_key(),
        provider_id: *keyed_accounts[3].unsigned_key(),
    };
    state.serialize(&mut keyed_accounts[1].account.data)
}

fn spend(keyed_accounts: &mut [KeyedAccount], amount: u64) -> Result<(), BandwidthPrepayError> {
    let gatekeeper_account_index = 0;
    let contract_account_index = 1;
    let provider_account_index = 2;
    let state =
        BandwidthPrepayState::deserialize(&keyed_accounts[contract_account_index].account.data)?;

    if let Some(gatekeeper_pubkey) = keyed_accounts[gatekeeper_account_index].signer_key() {
        if gatekeeper_pubkey != &state.gatekeeper_id {
            Err(BandwidthPrepayError::NoGatekeeperAccount)?
        }
    } else {
        Err(BandwidthPrepayError::NotSignedByGatekeeper)?
    }
    if keyed_accounts[provider_account_index].unsigned_key() != &state.provider_id {
        Err(BandwidthPrepayError::NoProviderAccount)?
    }
    if keyed_accounts[contract_account_index].account.lamports < amount {
        Err(BandwidthPrepayError::BalanceTooLow)?
    }

    keyed_accounts[contract_account_index].account.lamports -= amount;
    keyed_accounts[provider_account_index].account.lamports += amount;

    Ok(())
}

fn refund(keyed_accounts: &mut [KeyedAccount]) -> Result<(), BandwidthPrepayError> {
    let gatekeeper_account_index = 0;
    let contract_account_index = 1;
    let initiator_account_index = 2;
    let state =
        BandwidthPrepayState::deserialize(&keyed_accounts[contract_account_index].account.data)?;

    if let Some(gatekeeper_pubkey) = keyed_accounts[gatekeeper_account_index].signer_key() {
        if gatekeeper_pubkey != &state.gatekeeper_id {
            Err(BandwidthPrepayError::NoGatekeeperAccount)?
        }
    } else {
        Err(BandwidthPrepayError::NotSignedByGatekeeper)?
    }
    if keyed_accounts[initiator_account_index].unsigned_key() != &state.initiator_id {
        Err(BandwidthPrepayError::NoInitiatorAccount)?
    }

    keyed_accounts[initiator_account_index].account.lamports +=
        keyed_accounts[contract_account_index].account.lamports;
    keyed_accounts[contract_account_index].account.lamports = 0;

    Ok(())
}

pub fn process_instruction(
    _program_id: &Pubkey,
    keyed_accounts: &mut [KeyedAccount],
    data: &[u8],
) -> Result<(), InstructionError> {
    let instruction = deserialize(data).map_err(|_| InstructionError::InvalidInstructionData)?;

    match instruction {
        BandwidthPrepayInstruction::InitializeAccount => initialize_account(keyed_accounts),
        BandwidthPrepayInstruction::Spend(amount) => spend(keyed_accounts, amount),
        BandwidthPrepayInstruction::Refund => refund(keyed_accounts),
    }
    .map_err(|e| InstructionError::CustomError(e as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bandwidth_prepay_instruction;
    use crate::id;
    use solana_runtime::bank::Bank;
    use solana_runtime::bank_client::BankClient;
    use solana_sdk::client::SyncClient;
    use solana_sdk::genesis_block::create_genesis_block;
    use solana_sdk::message::Message;
    use solana_sdk::signature::{Keypair, KeypairUtil};
    use solana_sdk::system_instruction;

    fn create_bank(lamports: u64) -> (Bank, Keypair) {
        let (genesis_block, mint_keypair) = create_genesis_block(lamports);
        let mut bank = Bank::new(&genesis_block);
        bank.add_instruction_processor(id(), process_instruction);
        (bank, mint_keypair)
    }

    #[test]
    fn test_bandwidth_prepay_initialize() {
        let (bank, alice_keypair) = create_bank(10_000);
        let bank_client = BankClient::new(bank);

        let alice_pubkey = alice_keypair.pubkey();
        let contract = Keypair::new().pubkey();
        let gatekeeper = Keypair::new().pubkey();
        let provider = Keypair::new().pubkey();

        let instructions = bandwidth_prepay_instruction::initialize(
            &alice_pubkey,
            &contract,
            &gatekeeper,
            &provider,
            500,
        );
        let message = Message::new(instructions);
        bank_client
            .send_message(&[&alice_keypair], message)
            .unwrap();
        assert_eq!(bank_client.get_balance(&contract).unwrap(), 500);
        assert_eq!(bank_client.get_balance(&alice_pubkey).unwrap(), 9_500);
        let account = bank_client.get_account_data(&contract).unwrap().unwrap();
        assert_eq!(account.len(), BandwidthPrepayState::max_size());
        let state = BandwidthPrepayState::deserialize(&account).unwrap();
        assert_eq!(state.gatekeeper_id, gatekeeper);
        assert_eq!(state.provider_id, provider);
        assert_eq!(state.initiator_id, alice_pubkey);
    }

    #[test]
    fn test_bandwidth_prepay_spend() {
        let (bank, alice_keypair) = create_bank(10_000);
        let bank_client = BankClient::new(bank);

        let alice_pubkey = alice_keypair.pubkey();
        let contract = Keypair::new().pubkey();
        let provider = Keypair::new().pubkey();
        let gatekeeper = Keypair::new();

        // Initialize contract
        let instructions = bandwidth_prepay_instruction::initialize(
            &alice_pubkey,
            &contract,
            &gatekeeper.pubkey(),
            &provider,
            500,
        );
        let message = Message::new(instructions);
        bank_client
            .send_message(&[&alice_keypair], message)
            .unwrap();

        // Make sure gatekeeper account exists
        let instruction = system_instruction::transfer(&alice_pubkey, &gatekeeper.pubkey(), 1);
        let message = Message::new(vec![instruction]);
        bank_client
            .send_message(&[&alice_keypair], message)
            .unwrap();
        assert_eq!(bank_client.get_balance(&gatekeeper.pubkey()).unwrap(), 1);

        let instruction =
            bandwidth_prepay_instruction::spend(&gatekeeper.pubkey(), &contract, &provider, 100);
        let message = Message::new(vec![instruction]);
        bank_client.send_message(&[&gatekeeper], message).unwrap();
        assert_eq!(bank_client.get_balance(&contract).unwrap(), 400);
        assert_eq!(bank_client.get_balance(&provider).unwrap(), 100);
    }

    #[test]
    fn test_bandwidth_prepay_refund() {
        let (bank, alice_keypair) = create_bank(10_000);
        let bank_client = BankClient::new(bank);

        let alice_pubkey = alice_keypair.pubkey();
        let contract = Keypair::new().pubkey();
        let provider = Keypair::new().pubkey();
        let gatekeeper = Keypair::new();

        // Initialize contract
        let instructions = bandwidth_prepay_instruction::initialize(
            &alice_pubkey,
            &contract,
            &gatekeeper.pubkey(),
            &provider,
            500,
        );
        let message = Message::new(instructions);
        bank_client
            .send_message(&[&alice_keypair], message)
            .unwrap();

        // Make sure gatekeeper account exists
        let instruction = system_instruction::transfer(&alice_pubkey, &gatekeeper.pubkey(), 1);
        let message = Message::new(vec![instruction]);
        bank_client
            .send_message(&[&alice_keypair], message)
            .unwrap();
        assert_eq!(bank_client.get_balance(&gatekeeper.pubkey()).unwrap(), 1);

        let instruction =
            bandwidth_prepay_instruction::refund(&gatekeeper.pubkey(), &contract, &alice_pubkey);
        let message = Message::new(vec![instruction]);
        bank_client.send_message(&[&gatekeeper], message).unwrap();
        assert_eq!(bank_client.get_balance(&contract).unwrap(), 0);
        assert_eq!(bank_client.get_balance(&provider).unwrap(), 0);
        assert_eq!(bank_client.get_balance(&alice_pubkey).unwrap(), 9_999);
    }
}
