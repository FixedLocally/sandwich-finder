use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstructions, TransactionStatusMeta};

use crate::events::{addresses::{SYSTEM_PROGRAM_ID, WSOL_MINT}, transfer::{TransferFinder, TransferV2}, transfers::private::Sealed};

impl Sealed for SystemProgramTransferfinder {}
/// [0x02, 0x00, 0x00, 0x00, u64]
pub struct SystemProgramTransferfinder{}

impl SystemProgramTransferfinder {
    fn amount_and_dest_from_data(data: &[u8]) -> Option<(usize, u64)> {
        if data.len() < 12 {
            return None;
        }
        match data[0] {
            0 => Some((1, u64::from_le_bytes(data[4..12].try_into().unwrap()))), // CreateAccount
            2 => Some((1, u64::from_le_bytes(data[4..12].try_into().unwrap()))), // Transfer
            3 => {
                // 0..4: discriminator, 4..36: base, 36..44: seed len, 44..(44+seed len): seed, (44+seed len)..(52+seed len): lamports
                let start = 44 + u64::from_le_bytes(data[36..44].try_into().unwrap()) as usize;
                let end = start + 8;
                Some((1, u64::from_le_bytes(data[start..end].try_into().unwrap())))
            }, // CreateAccountWithSeed
            13 => Some((2, u64::from_le_bytes(data[4..12].try_into().unwrap()))), // TransferWithSeed
            _ => None,
        }
    }
}

impl TransferFinder for SystemProgramTransferfinder {
    fn find_transfers(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, _meta: &TransactionStatusMeta) -> Vec<TransferV2> {
        if ix.program_id == SYSTEM_PROGRAM_ID {
            if let Some((to, amount)) = Self::amount_and_dest_from_data(&ix.data) {
                if ix.accounts.len() < 2 {
                    return vec![];
                }
                return vec![TransferV2::new(
                    None,
                    SYSTEM_PROGRAM_ID.to_string().into(),
                    ix.accounts[0].pubkey.to_string().into(),
                    WSOL_MINT.to_string().into(),
                    amount,
                    ix.accounts[0].pubkey.to_string().into(),
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
            if account_keys[inner_ix.program_id_index as usize] != SYSTEM_PROGRAM_ID {
                return;
            }
            if inner_ix.accounts.len() < 2 {
                return;
            }
            if let Some((to, amount)) = Self::amount_and_dest_from_data(&inner_ix.data) {
                let from = inner_ix.accounts[0] as usize;
                let to = inner_ix.accounts[to] as usize;
                if from >= account_keys.len() || to >= account_keys.len() {
                    return;
                }
                if from == to {
                    // Don't log self transfers
                    return;
                }
                transfers.push(TransferV2::new(
                    Some(ix.program_id.to_string().into()),
                    SYSTEM_PROGRAM_ID.to_string().into(),
                    account_keys[from].to_string().into(),
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