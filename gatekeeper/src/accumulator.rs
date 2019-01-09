use std::time::Instant;

pub struct Accumulator {
    pub total_data_amount: u64,
    pub amount_charged: u64,
    pub initiator_fund: u64,
    pub now: Instant,
}

impl Default for Accumulator {
    fn default() -> Accumulator {
        Accumulator {
            total_data_amount: 0,
            amount_charged: 0,
            initiator_fund: 0,
            now: Instant::now(),
        }
    }
}
