use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::{addresses::CLEARPOOL_PUBKEY, swap::{SwapFinder, SwapV2}, swaps::{private::Sealed, swap_finder_ext::SwapFinderExt}};

impl Sealed for ClearpoolSwapFinder {}

pub struct ClearpoolSwapFinder {}

impl ClearpoolSwapFinder {
    fn is_a_to_b(data: &[u8]) -> bool {
        data[41] == 1
    }

    fn user_ata_indexes(data: &[u8]) -> (usize, usize) {
        if Self::is_a_to_b(data) {
            (3, 5)
        } else {
            (5, 3)
        }
    }

    fn pool_ata_indexes(data: &[u8]) -> (usize, usize) {
        if Self::is_a_to_b(data) {
            (6, 4)
        } else {
            (4, 6)
        }
    }
}

impl SwapFinder for ClearpoolSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[2].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[2] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        let (in_idx, out_idx) = Self::user_ata_indexes(&ix.data);
        (
            ix.accounts[in_idx].pubkey,
            ix.accounts[out_idx].pubkey,
        )
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        let (in_idx, out_idx) = Self::user_ata_indexes(&inner_ix.data);
        (
            account_keys[inner_ix.accounts[in_idx] as usize],
            account_keys[inner_ix.accounts[out_idx] as usize],
        )
    }
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        let (in_idx, out_idx) = Self::pool_ata_indexes(&ix.data);
        (
            ix.accounts[in_idx].pubkey,
            ix.accounts[out_idx].pubkey,
        )
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        let (in_idx, out_idx) = Self::pool_ata_indexes(&inner_ix.data);
        (
            account_keys[inner_ix.accounts[in_idx] as usize],
            account_keys[inner_ix.accounts[out_idx] as usize],
        )
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // swap
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &CLEARPOOL_PUBKEY, &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8], 0, 42),
        ].concat()
    }
}
