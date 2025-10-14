use std::sync::Arc;

use derive_getters::Getters;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Getters)]
#[serde(rename_all = "camelCase")]
pub struct TransactionV2 {
    slot: u64,
    inclusion_order: u32,
    sig: Arc<str>,
    fee: u64,
    cu_actual: u64,
}

impl TransactionV2 {
    pub fn new(slot: u64, inclusion_order: u32, sig: Arc<str>, fee: u64, cu_actual: u64) -> Self {
        Self {
            slot,
            inclusion_order,
            sig,
            fee,
            cu_actual,
        }
    }
}