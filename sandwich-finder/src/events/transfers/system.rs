use solana_sdk::pubkey::Pubkey;
use yellowstone_grpc_proto::prelude::{InnerInstructions, TransactionStatusMeta};

use crate::events::{addresses::{SYSTEM_PROGRAM_ID, WSOL_MINT}, transfer::{TransferFinder, TransferV2}, transfers::private::Sealed};

impl Sealed for SystemProgramTransferfinder {}
/// [0x02, 0x00, 0x00, 0x00, u64]
pub struct SystemProgramTransferfinder{}

impl SystemProgramTransferfinder {
    fn amount_from_data(data: &[u8]) -> Option<u64> {
        if data.len() < 12 {
            return None;
        }
        if !data.starts_with(&[0x02, 0x00, 0x00, 0x00]) {
            return None;
        }
        Some(u64::from_le_bytes(data[4..12].try_into().unwrap()))
    }
}

impl TransferFinder for SystemProgramTransferfinder {
    fn find_transfers(ix: &solana_sdk::instruction::Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, _meta: &TransactionStatusMeta) -> Vec<TransferV2> {
        if ix.program_id == SYSTEM_PROGRAM_ID {
            if let Some(amount) = Self::amount_from_data(&ix.data) {
                if ix.accounts.len() < 2 {
                    return vec![];
                }
                return vec![TransferV2::new(
                    None,
                    SYSTEM_PROGRAM_ID.to_string(),
                    WSOL_MINT.to_string(),
                    amount,
                    ix.accounts[0].pubkey.to_string(),
                    ix.accounts[1].pubkey.to_string(),
                    0,
                    0,
                    0,
                    0,
                    None,
                )];
            }
            return vec![];
        }
        let mut transfers = vec![];
        inner_ixs.instructions.iter().enumerate().for_each(|(i, inner_ix)| {
            if inner_ix.program_id_index as usize >= account_keys.len() {
                return;
            }
            if account_keys[inner_ix.program_id_index as usize] != SYSTEM_PROGRAM_ID {
                return;
            }
            if inner_ix.accounts.len() < 2 {
                return;
            }
            if let Some(amount) = Self::amount_from_data(&inner_ix.data) {
                let from = inner_ix.accounts[0] as usize;
                let to = inner_ix.accounts[1] as usize;
                if from >= account_keys.len() || to >= account_keys.len() {
                    return;
                }
                if from == to {
                    // Don't log self transfers
                    return;
                }
                transfers.push(TransferV2::new(
                    Some(ix.program_id.to_string()),
                    SYSTEM_PROGRAM_ID.to_string(),
                    WSOL_MINT.to_string(),
                    amount,
                    account_keys[from].to_string(),
                    account_keys[to].to_string(),
                    0,
                    0,
                    0,
                    0,
                    Some(i as u32),
                ));
            } else {
                return;
            }
        });
        transfers
    }
}