pub mod bandwidth_prepay_instruction;
pub mod bandwidth_prepay_processor;
pub mod bandwidth_prepay_state;

const BANDWIDTH_PREPAY_PROGRAM_ID: [u8; 32] = [
    128, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0,
];

solana_sdk::solana_name_id!(
    BANDWIDTH_PREPAY_PROGRAM_ID,
    "9ecPa9EqqwcjzPTCNLisYaGskkc3j5b12xdcBZNP7sxK"
);
