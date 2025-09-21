use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::swaps::{addresses::SOLFI_PUBKEY, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed};

impl Sealed for SolFiSwapFinder {}

pub struct SolFiSwapFinder {}

/// SolFi a single swap instruction
/// [17] is !a_to_b
/// user a/b: 4/5, pool a/b: 2/3
impl SolFiSwapFinder {
    fn is_a_to_b(data: &[u8]) -> bool {
        data[17] == 0
    }
}

impl SwapFinder for SolFiSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[1].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[1] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&ix.data) {
            (
                ix.accounts[4].pubkey,
                ix.accounts[5].pubkey,
            )
        } else {
            (
                ix.accounts[5].pubkey,
                ix.accounts[4].pubkey,
            )
        }
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&inner_ix.data) {
            (
                account_keys[inner_ix.accounts[4] as usize], // base
                account_keys[inner_ix.accounts[5] as usize], // quote
            )
        } else {
            (
                account_keys[inner_ix.accounts[5] as usize], // quote
                account_keys[inner_ix.accounts[4] as usize], // base
            )
        }
    }
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&ix.data) {
            (
                ix.accounts[3].pubkey,
                ix.accounts[2].pubkey,
            )
        } else {
            (
                ix.accounts[2].pubkey,
                ix.accounts[3].pubkey,
            )
        }
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&inner_ix.data) {
            (
                account_keys[inner_ix.accounts[3] as usize], // base
                account_keys[inner_ix.accounts[2] as usize], // quote
            )
        } else {
            (
                account_keys[inner_ix.accounts[2] as usize], // quote
                account_keys[inner_ix.accounts[3] as usize], // base
            )
        }
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // swap
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &SOLFI_PUBKEY, &[0x07], 0, 18),
        ].concat()
    }
}
