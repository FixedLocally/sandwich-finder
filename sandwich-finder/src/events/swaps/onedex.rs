use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::{swap::{SwapFinder, SwapV2}, swaps::{addresses::ONEDEX_PUBKEY, private::Sealed, swap_finder_ext::SwapFinderExt}};

impl Sealed for OneDexSwapFinder {}

pub struct OneDexSwapFinder {}

/// 1DEX a single swap instruction
/// user a/b: 6/7, pool a/b: 3/4

impl SwapFinder for OneDexSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[1].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[1] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[6].pubkey,
            ix.accounts[7].pubkey,
        )
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[6] as usize],
            account_keys[inner_ix.accounts[7] as usize],
        )
    }
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[4].pubkey,
            ix.accounts[3].pubkey,
        )
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[4] as usize], // base
            account_keys[inner_ix.accounts[3] as usize], // quote
        )
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // swap
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &ONEDEX_PUBKEY, &[0x08, 0x97, 0xf5, 0x4c, 0xac, 0xcb, 0x90, 0x27], 0, 24),
        ].concat()
    }
}
