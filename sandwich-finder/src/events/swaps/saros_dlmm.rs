use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::swaps::{addresses::SAROS_DLMM_PUBKEY, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed};

impl Sealed for SarosDLMMSwapFinder {}

pub struct SarosDLMMSwapFinder {}

/// Saros DLMM has a single swap instruction
/// [24] is a_to_b
/// user a/b: 7/8, pool a/b: 5/6
impl SarosDLMMSwapFinder {
    fn is_a_to_b(data: &[u8]) -> bool {
        data[24] == 1
    }
}

impl SwapFinder for SarosDLMMSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[0].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[0] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&ix.data) {
            (
                ix.accounts[7].pubkey,
                ix.accounts[8].pubkey,
            )
        } else {
            (
                ix.accounts[8].pubkey,
                ix.accounts[7].pubkey,
            )
        }
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&inner_ix.data) {
            (
                account_keys[inner_ix.accounts[7] as usize], // base
                account_keys[inner_ix.accounts[8] as usize], // quote
            )
        } else {
            (
                account_keys[inner_ix.accounts[8] as usize], // quote
                account_keys[inner_ix.accounts[7] as usize], // base
            )
        }
    }
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&ix.data) {
            (
                ix.accounts[6].pubkey,
                ix.accounts[5].pubkey,
            )
        } else {
            (
                ix.accounts[5].pubkey,
                ix.accounts[6].pubkey,
            )
        }
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&inner_ix.data) {
            (
                account_keys[inner_ix.accounts[6] as usize], // base
                account_keys[inner_ix.accounts[5] as usize], // quote
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
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &SAROS_DLMM_PUBKEY, &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8], 0, 25),
        ].concat()
    }
}
