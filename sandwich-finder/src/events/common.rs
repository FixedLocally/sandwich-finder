use derive_getters::Getters;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Getters, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Timestamp {
    slot: u64,
    inclusion_order: u32,
    ix_index: u32,
    inner_ix_index: Option<u32>,
}

impl Timestamp {
    pub fn new(slot: u64, inclusion_order: u32, ix_index: u32, inner_ix_index: Option<u32>) -> Self {
        Self {
            slot,
            inclusion_order,
            ix_index,
            inner_ix_index,
        }
    }
}