#[macro_export]
macro_rules! bandwidth_prepay_program {
    () => {
        (
            "bandwidth_prepay_program".to_string(),
            bandwidth_prepay_api::id(),
        )
    };
}

use bandwidth_prepay_api::bandwidth_prepay_processor::process_instruction;
solana_sdk::solana_entrypoint!(process_instruction);
