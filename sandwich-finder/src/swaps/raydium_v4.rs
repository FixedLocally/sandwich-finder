use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::swaps::{addresses::RAYDIUM_V4_PUBKEY, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed};

impl Sealed for RaydiumV4SwapFinder {}

pub struct RaydiumV4SwapFinder {}

/// Ray v4 swaps have the discriminant [0x09], followed by the input amount and the min amount out
/// Swap direction is determined the input/output token accounts ([-3], [-2] respectively)
/// The pool's ATA are at [-12] and [-13] but due to the ordering the order can't be reliably determined
impl SwapFinder for RaydiumV4SwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[1].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[1] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[ix.accounts.len() - 3].pubkey,
            ix.accounts[ix.accounts.len() - 2].pubkey,
        )
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[inner_ix.accounts.len() - 3] as usize],
            account_keys[inner_ix.accounts[inner_ix.accounts.len() - 2] as usize],
        )
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &RAYDIUM_V4_PUBKEY, &[0x09], 0, 17),
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &RAYDIUM_V4_PUBKEY, &[0x0b], 0, 17),
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &RAYDIUM_V4_PUBKEY, &[0x10], 0, 17),
        ].concat()
    }
}