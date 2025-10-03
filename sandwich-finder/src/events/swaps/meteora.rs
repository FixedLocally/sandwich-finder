use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::{addresses::METEORA_PUBKEY, swap::{SwapFinder, SwapV2}, swaps::{private::Sealed, swap_finder_ext::SwapFinderExt}};

impl Sealed for MeteoraSwapFinder {}

pub struct MeteoraSwapFinder {}

/// Ray v4 swaps have the discriminant [0x09], followed by the input amount and the min amount out
/// Swap direction is determined the input/output token accounts ([-3], [-2] respectively)
/// The pool's ATA are at [-12] and [-13] but due to the ordering the order can't be reliably determined
impl SwapFinder for MeteoraSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[0].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[0] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[1].pubkey,
            ix.accounts[2].pubkey,
        )
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[1] as usize],
            account_keys[inner_ix.accounts[2] as usize],
        )
    }

    fn pool_ata_ix(_ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            Pubkey::default(),
            Pubkey::default(),
        )
    }

    // The 1st inner ix is either a transfer for the fee or the "vault deposit"
    fn ixs_to_skip() -> usize {
        1
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &METEORA_PUBKEY, &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8], 0, 17)
    }
}