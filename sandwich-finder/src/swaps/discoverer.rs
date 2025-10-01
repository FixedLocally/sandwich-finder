use std::collections::HashSet;

use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::swaps::{finder::{SwapFinder, SwapV2}, private::Sealed, utils::token_transferred_inner};

impl Sealed for Discoverer {}

pub struct Discoverer {}

/// Outputs txid and program that triggered >=2 swaps in its inner instructions and emit a special swap event.
impl SwapFinder for Discoverer {
    fn amm_ix(_ix: &Instruction) -> Pubkey {
        Pubkey::default()
    }

    fn amm_inner_ix(_inner_ix: &InnerInstruction, _account_keys: &Vec<Pubkey>) -> Pubkey {
        Pubkey::default()
    }

    fn user_ata_ix(_ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            Pubkey::default(),
            Pubkey::default(),
        )
    }

    fn user_ata_inner_ix(_inner_ix: &InnerInstruction, _account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            Pubkey::default(),
            Pubkey::default(),
        )
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        // ignore known programs
        match ix.program_id {
            // RAYDIUM_V4_PUBKEY | RAYDIUM_V5_PUBKEY | RAYDIUM_LP_PUBKEY | RAYDIUM_CL_PUBKEY | PDF_PUBKEY | PDF2_PUBKEY | WHIRLPOOL_PUBKEY | DLMM_PUBKEY | METEORA_PUBKEY => vec![],
            _ => {
                let mut transfer_count = 0;
                let mut authorities = HashSet::new();
                let mut mints = HashSet::new();
                for inner_ix in &inner_ixs.instructions {
                    if let Some((_from, _to, mint, _amount)) = token_transferred_inner(&inner_ix, &account_keys, &meta) {
                        transfer_count += 1;
                        match inner_ix.data[0] {
                            2 => { // System transfer
                                if inner_ix.accounts.len() >= 1 {
                                    let authority = account_keys[inner_ix.accounts[0] as usize];
                                    authorities.insert(authority);
                                }
                            },
                            3 => { // Transfer
                                if inner_ix.accounts.len() >= 3 {
                                    let authority = account_keys[inner_ix.accounts[2] as usize];
                                    authorities.insert(authority);
                                }
                            },
                            12 => { // TransferChecked
                                if inner_ix.accounts.len() >= 4 {
                                    let authority = account_keys[inner_ix.accounts[3] as usize];
                                    authorities.insert(authority);
                                }
                            },
                            _ => {}
                        }
                        mints.insert(mint);
                    }
                }
                if transfer_count >= 2 && authorities.len() >= 2 && mints.len() >= 2 {
                    return vec![
                        SwapV2::new(None, ix.program_id.to_string(), "".to_string(), "".to_string(), "".to_string(), 0, 0, "".to_string(), "".to_string(), None, None, 0, 0, 0, 0, None),
                    ];
                }
                vec![]
            }
        }
    }
}