use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::{addresses::JUP_PERPS_PUBKEY, swap::{SwapFinder, SwapV2}, swaps::{private::Sealed, swap_finder_ext::SwapFinderExt}};

impl Sealed for JupPerpsSwapFinder {}

pub struct JupPerpsSwapFinder {}

/// Jup perps swaps have two variants:
/// 1. swap2 [0x41, 0x4b, 0x3f, 0x4c, 0xeb, 0x5b, 0x5b, 0x88]
/// 2. swapWithTokenLedger [0x8b, 0x8d, 0xee, 0xc5, 0x29, 0xd3, 0xac, 0x13]
/// In/min out amounts follows the discriminant
impl SwapFinder for JupPerpsSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[5].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[5] as usize]
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
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[13].pubkey,
            ix.accounts[9].pubkey,
        )
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[13] as usize],
            account_keys[inner_ix.accounts[9] as usize],
        )
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // swap_base_input
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &JUP_PERPS_PUBKEY, &[0x41, 0x4b, 0x3f, 0x4c, 0xeb, 0x5b, 0x5b, 0x88], 0, 24),
            // swap_base_output
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &JUP_PERPS_PUBKEY, &[0x8b, 0x8d, 0xee, 0xc5, 0x29, 0xd3, 0xac, 0x13], 0, 24),
        ].concat()
    }
}