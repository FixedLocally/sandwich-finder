use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::{addresses::ALPHA_PUBKEY, swap::{SwapFinder, SwapV2}, swaps::{private::Sealed, swap_finder_ext::SwapFinderExt}};

impl Sealed for AlphaSwapFinder {}

pub struct AlphaSwapFinder {}

/// SolFi a single swap instruction
/// [1] is a_to_b
/// user a/b: 3/2, pool a/b: 5/4
impl AlphaSwapFinder {
    fn is_a_to_b(data: &[u8]) -> bool {
        data[1] == 1
    }
}
impl SwapFinder for AlphaSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[2].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[2] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&ix.data) {
            (
                ix.accounts[3].pubkey,
                ix.accounts[4].pubkey,
            )
        } else {
            (
                ix.accounts[4].pubkey,
                ix.accounts[3].pubkey,
            )
        }
        
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&inner_ix.data) {
            (
                account_keys[inner_ix.accounts[3] as usize],
                account_keys[inner_ix.accounts[4] as usize],
            )
        } else {
            (
                account_keys[inner_ix.accounts[4] as usize],
                account_keys[inner_ix.accounts[3] as usize],
            )
        }
    }
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&ix.data) {
            (
                ix.accounts[6].pubkey, // quote
                ix.accounts[5].pubkey, // base
            )
        } else {
            (
                ix.accounts[5].pubkey, // quote
                ix.accounts[6].pubkey, // base
            )
        }
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&inner_ix.data) {
            (
                account_keys[inner_ix.accounts[6] as usize], // quote
                account_keys[inner_ix.accounts[5] as usize], // base
            )
        } else {
            (
                account_keys[inner_ix.accounts[5] as usize], // quote
                account_keys[inner_ix.accounts[6] as usize], // base
            )
        }
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // swap
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &ALPHA_PUBKEY, &[0x0c], 0, 18),
        ].concat()
    }
}
