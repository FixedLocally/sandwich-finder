use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::swaps::{addresses::DLMM_PUBKEY, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed};

impl Sealed for DLMMSwapFinder {}

pub struct DLMMSwapFinder {}

/// There's a grand total of 6 swap variants for DLMM
/// But all 6 of them have user_token_{in,out} at the [4] and [5] respectively
impl SwapFinder for DLMMSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        return ix.accounts[0].pubkey;
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        return account_keys[inner_ix.accounts[0] as usize];
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        return (
            ix.accounts[4].pubkey,
            ix.accounts[5].pubkey,
        );
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        return (
            account_keys[inner_ix.accounts[4] as usize],
            account_keys[inner_ix.accounts[5] as usize],
        );
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // swap
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &DLMM_PUBKEY, &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8], 24),
            // swap2
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &DLMM_PUBKEY, &[0x41, 0x4b, 0x3f, 0x4c, 0xeb, 0x5b, 0x5b, 0x88], 24),
            // swap_exact_out
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &DLMM_PUBKEY, &[0xfa, 0x49, 0x65, 0x21, 0x26, 0xcf, 0x4b, 0xb8], 24),
            // swap_exact_out2
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &DLMM_PUBKEY, &[0x2b, 0xd7, 0xf7, 0x84, 0x89, 0x3c, 0xf3, 0x51], 24),
            // swap_with_price_impact
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &DLMM_PUBKEY, &[0x38, 0xad, 0xe6, 0xd0, 0xad, 0xe4, 0x9c, 0xcd], 24),
            // swap_with_price_impact2
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &DLMM_PUBKEY, &[0x4a, 0x62, 0xc0, 0xd6, 0xb1, 0x33, 0x4b, 0x33], 24),
        ].concat()
    }
}