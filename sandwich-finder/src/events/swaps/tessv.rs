use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::swaps::{addresses::TESS_V_PUBKEY, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed};

impl Sealed for TessVSwapFinder {}

pub struct TessVSwapFinder {}

/// TessV a single swap instruction
/// [1] is a_to_b
/// user a/b: 5/6, pool a/b: 3/4
impl TessVSwapFinder {
    fn is_a_to_b(data: &[u8]) -> bool {
        data[1] == 1
    }
}

impl SwapFinder for TessVSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[1].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[1] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&ix.data) {
            (
                ix.accounts[5].pubkey,
                ix.accounts[6].pubkey,
            )
        } else {
            (
                ix.accounts[6].pubkey,
                ix.accounts[5].pubkey,
            )
        }
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&inner_ix.data) {
            (
                account_keys[inner_ix.accounts[5] as usize], // base
                account_keys[inner_ix.accounts[6] as usize], // quote
            )
        } else {
            (
                account_keys[inner_ix.accounts[6] as usize], // quote
                account_keys[inner_ix.accounts[5] as usize], // base
            )
        }
    }
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&ix.data) {
            (
                ix.accounts[4].pubkey,
                ix.accounts[3].pubkey,
            )
        } else {
            (
                ix.accounts[3].pubkey,
                ix.accounts[4].pubkey,
            )
        }
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&inner_ix.data) {
            (
                account_keys[inner_ix.accounts[4] as usize], // base
                account_keys[inner_ix.accounts[3] as usize], // quote
            )
        } else {
            (
                account_keys[inner_ix.accounts[3] as usize], // quote
                account_keys[inner_ix.accounts[4] as usize], // base
            )
        }
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // swap
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &TESS_V_PUBKEY, &[0x10], 0, 18),
        ].concat()
    }
}
