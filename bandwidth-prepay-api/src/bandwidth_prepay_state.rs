use bincode::{deserialize, serialize_into, serialized_size};
use serde_derive::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::{error, fmt};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum BandwidthPrepayError {
    AlreadyInitialized,
    UserdataTooSmall,
    UserdataDeserializeFailure,
    NotSignedByGatekeeper,
    BalanceTooLow,
    NoGatekeeperAccount,
    NoProviderAccount,
    NoInitiatorAccount,
}

impl fmt::Display for BandwidthPrepayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid")
    }
}

impl error::Error for BandwidthPrepayError {}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct BandwidthPrepayState {
    pub gatekeeper_id: Pubkey,
    pub provider_id: Pubkey,
    pub initiator_id: Pubkey,
}

impl BandwidthPrepayState {
    pub fn deserialize(input: &[u8]) -> Result<Self, BandwidthPrepayError> {
        deserialize(input).map_err(|_| BandwidthPrepayError::UserdataDeserializeFailure)
    }

    pub fn serialize(&self, output: &mut [u8]) -> Result<(), BandwidthPrepayError> {
        serialize_into(output, self).map_err(|_| BandwidthPrepayError::UserdataTooSmall)
    }

    pub fn max_size() -> usize {
        let bandwidth_prepay_state = BandwidthPrepayState::default();
        serialized_size(&bandwidth_prepay_state).unwrap() as usize
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::id;
    use solana_sdk::account::Account;

    #[test]
    fn test_max_size() {
        let number = BandwidthPrepayState::max_size();
        assert_eq!(number, 96);
    }

    #[test]
    fn test_serializer() {
        let mut a = Account::new(0, 96, &id());
        let b = BandwidthPrepayState::default();
        b.serialize(&mut a.data).unwrap();
        let c = BandwidthPrepayState::deserialize(&a.data).unwrap();
        assert_eq!(b, c);
    }

    #[test]
    fn test_serializer_userdata_too_small() {
        let mut a = Account::new(0, 1, &id());
        let b = BandwidthPrepayState::default();
        assert_eq!(
            b.serialize(&mut a.data),
            Err(BandwidthPrepayError::UserdataTooSmall)
        );
    }
}
