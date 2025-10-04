use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstructions, TransactionStatusMeta};

use crate::events::{addresses::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID}, swaps::utils::mint_of, transfer::{TransferFinder, TransferV2}, transfers::private::Sealed};

impl Sealed for TokenProgramTransferFinder {}
pub struct TokenProgramTransferFinder {}

impl TokenProgramTransferFinder {
    fn is_token_program(program_id: Pubkey) -> bool {
        program_id == TOKEN_PROGRAM_ID || program_id == TOKEN_2022_PROGRAM_ID
    }

    fn amount_from_data(data: &[u8]) -> Option<u64> {
        if data.len() < 9 {
            return None;
        }
        if data[0] != 3 && data[0] != 12 {
            return None; // Not a transfer
        }
        Some(u64::from_le_bytes(data[1..9].try_into().unwrap()))
    }

    fn from_to_indexs(data: &[u8]) -> Option<(usize, usize)> {
        match data[0] {
            3 => Some((0, 1)), // Transfer
            12 => Some((0, 2)), // TransferChecked
            _ => None, // Not a transfer
        }
    }
}

impl TransferFinder for TokenProgramTransferFinder {
    fn find_transfers(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<TransferV2> {
        if Self::is_token_program(ix.program_id) {
            if let Some(amount) = Self::amount_from_data(&ix.data) {
                if let Some((from_index, to_index)) = Self::from_to_indexs(&ix.data) {
                    if from_index < ix.accounts.len() && to_index < ix.accounts.len() {
                        let from_ata = ix.accounts[from_index].pubkey;
                        let to_ata = ix.accounts[to_index].pubkey;
                        let mint = mint_of(&from_ata, account_keys, meta)
                            .or_else(|| mint_of(&to_ata, account_keys, meta));
                        if let Some(mint) = mint {
                            return vec![TransferV2::new(
                                None,
                                ix.program_id.to_string(),
                                mint,
                                amount,
                                from_ata.to_string(),
                                to_ata.to_string(),
                                0, // slot to be filled later
                                0, // inclusion_order to be filled later
                                0, // ix_index to be filled later
                                None, // inner_ix_index to be filled later
                            )];
                        }
                    }
                }
            }
        }
        let mut transfers = vec![];
        inner_ixs.instructions.iter().enumerate().for_each(|(i, inner_ix)| {
            if inner_ix.program_id_index as usize >= account_keys.len() {
                return;
            }
            if !Self::is_token_program(account_keys[inner_ix.program_id_index as usize]) {
                return;
            }
            if let Some(amount) = Self::amount_from_data(&inner_ix.data) {
                if let Some((from_index, to_index)) = Self::from_to_indexs(&inner_ix.data) {
                    if from_index < inner_ix.accounts.len() && to_index < inner_ix.accounts.len() {
                        let from_ata = inner_ix.accounts[from_index] as usize;
                        let to_ata = inner_ix.accounts[to_index] as usize;
                        if from_ata >= account_keys.len() || to_ata >= account_keys.len() {
                            return;
                        }
                        if from_ata == to_ata {
                            // Don't log self transfers
                            return;
                        }
                        let from_ata_pubkey = account_keys[from_ata];
                        let to_ata_pubkey = account_keys[to_ata];
                        let mint = mint_of(&from_ata_pubkey, account_keys, meta)
                            .or_else(|| mint_of(&to_ata_pubkey, account_keys, meta));
                        if let Some(mint) = mint {
                            transfers.push(TransferV2::new(
                                Some(ix.program_id.to_string()),
                                account_keys[inner_ix.program_id_index as usize].to_string(),
                                mint,
                                amount,
                                from_ata_pubkey.to_string(),
                                to_ata_pubkey.to_string(),
                                0, // slot to be filled later
                                0, // inclusion_order to be filled later
                                0, // ix_index to be filled later
                                Some(i as u32), // inner_ix_index to be filled later
                            ));
                        }
                    }
                }
            }
        });
        transfers
    }
}