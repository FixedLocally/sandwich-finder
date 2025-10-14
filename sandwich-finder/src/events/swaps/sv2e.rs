use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::{addresses::SV2E_PUBKEY, swap::{SwapFinder, SwapV2}, swaps::{private::Sealed, swap_finder_ext::SwapFinderExt}};

impl Sealed for Sv2eSwapFinder {}

pub struct Sv2eSwapFinder {}

/// SV2E... a single swap instruction
/// [17] is a_to_b
/// user a/b: 6/7, pool a/b: 4/5
impl Sv2eSwapFinder {
    fn is_a_to_b(data: &[u8]) -> bool {
        data[17] == 1
    }
}

impl SwapFinder for Sv2eSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[1].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[1] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&ix.data) {
            (
                ix.accounts[7].pubkey,
                ix.accounts[6].pubkey,
            )
        } else {
            (
                ix.accounts[6].pubkey,
                ix.accounts[7].pubkey,
            )
        }
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&inner_ix.data) {
            (
                account_keys[inner_ix.accounts[7] as usize],
                account_keys[inner_ix.accounts[6] as usize],
            )
        } else {
            (
                account_keys[inner_ix.accounts[6] as usize],
                account_keys[inner_ix.accounts[7] as usize],
            )
        }
    }
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
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
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
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

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // swap
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &SV2E_PUBKEY, &[0x07], 0, 18),
        ].concat()
    }
}
