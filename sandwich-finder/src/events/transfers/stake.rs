use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstructions, TransactionStatusMeta};

use crate::events::{addresses::{STAKE_PROGRAM_ID, WSOL_MINT}, transfer::{TransferFinder, TransferV2}, transfers::private::Sealed};

impl Sealed for StakeProgramTransferfinder {}
/// [0x02, 0x00, 0x00, 0x00, u64]
pub struct StakeProgramTransferfinder{}

impl StakeProgramTransferfinder {
    /// Returns (from, to, auth, amount)
    fn amount_and_endpoint_from_data(data: &[u8]) -> Option<(usize, usize, usize, u64)> {
        if data.len() < 12 {
            return None;
        }
        match data[0] {
            4 => Some((0, 1, 4, u64::from_le_bytes(data[4..12].try_into().unwrap()))), // Withdraw
            _ => None,
        }
    }
}

impl TransferFinder for StakeProgramTransferfinder {
    fn find_transfers(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, _meta: &TransactionStatusMeta) -> Vec<TransferV2> {
        if ix.program_id == STAKE_PROGRAM_ID {
            if let Some((from, to, auth, amount)) = Self::amount_and_endpoint_from_data(&ix.data) {
                if ix.accounts.len() < 2 {
                    return vec![];
                }
                return vec![TransferV2::new(
                    None,
                    STAKE_PROGRAM_ID.to_string().into(),
                    ix.accounts[auth].pubkey.to_string().into(),
                    WSOL_MINT.to_string().into(),
                    amount,
                    ix.accounts[from].pubkey.to_string().into(),
                    ix.accounts[to].pubkey.to_string().into(),
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
            if account_keys[inner_ix.program_id_index as usize] != STAKE_PROGRAM_ID {
                return;
            }
            if inner_ix.accounts.len() < 2 {
                return;
            }
            if let Some((from, to, auth, amount)) = Self::amount_and_endpoint_from_data(&inner_ix.data) {
                let from = inner_ix.accounts[from] as usize;
                let to = inner_ix.accounts[to] as usize;
                let auth = inner_ix.accounts[auth] as usize;
                if from >= account_keys.len() || to >= account_keys.len() || auth >= account_keys.len() {
                    return;
                }
                if from == to {
                    // Don't log self transfers
                    return;
                }
                transfers.push(TransferV2::new(
                    Some(ix.program_id.to_string().into()),
                    STAKE_PROGRAM_ID.to_string().into(),
                    account_keys[auth].to_string().into(),
                    WSOL_MINT.to_string().into(),
                    amount,
                    account_keys[from].to_string().into(),
                    account_keys[to].to_string().into(),
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