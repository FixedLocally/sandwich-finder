use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::swaps::{addresses::PANCAKE_SWAP_PUBKEY, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed};

impl Sealed for PancakeSwapSwapFinder {}

pub struct PancakeSwapSwapFinder {}

/// PancakeSwap swaps have two variants, swap and swap_v2, both share the same account list
impl SwapFinder for PancakeSwapSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[1].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[1] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[3].pubkey,
            ix.accounts[4].pubkey,
        )
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[3] as usize],
            account_keys[inner_ix.accounts[4] as usize],
        )
    }

    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[6].pubkey,
            ix.accounts[5].pubkey,
        )
    }

    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[6] as usize],
            account_keys[inner_ix.accounts[5] as usize],
        )
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &PANCAKE_SWAP_PUBKEY, &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8], 0, 41),
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &PANCAKE_SWAP_PUBKEY, &[0x2b, 0x04, 0xed, 0x0b, 0x1a, 0xc9, 0x1e, 0x62], 0, 41),
        ].concat()
    }
}