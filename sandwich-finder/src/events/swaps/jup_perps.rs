use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::{addresses::JUP_PERPS_PUBKEY, swap::{SwapFinder, SwapV2}, swaps::{private::Sealed, swap_finder_ext::SwapFinderExt}};

enum JupPerpsSwapVariant {
    Swap2,
    SwapWithTokenLedger,
    InstantIncreasePositionPreSwap,
}

impl Sealed for JupPerpsSwapFinder {}

pub struct JupPerpsSwapFinder {}

impl JupPerpsSwapFinder {
    fn variant_from_data(data: &[u8]) -> Option<JupPerpsSwapVariant> {
        if data.starts_with(&[0x41, 0x4b, 0x3f, 0x4c, 0xeb, 0x5b, 0x5b, 0x88]) {
            Some(JupPerpsSwapVariant::Swap2)
        } else if data.starts_with(&[0x8b, 0x8d, 0xee, 0xc5, 0x29, 0xd3, 0xac, 0x13]) {
            Some(JupPerpsSwapVariant::SwapWithTokenLedger)
        } else if data.starts_with(&[0xc5, 0x26, 0x56, 0xa5, 0xc7, 0x17, 0x26, 0xea]) {
            Some(JupPerpsSwapVariant::InstantIncreasePositionPreSwap)
        } else {
            None
        }
    }
}

/// Jup perps swaps have two variants:
/// 1. swap2 [0x41, 0x4b, 0x3f, 0x4c, 0xeb, 0x5b, 0x5b, 0x88]
/// 2. swapWithTokenLedger [0x8b, 0x8d, 0xee, 0xc5, 0x29, 0xd3, 0xac, 0x13]
/// 3. instantIncreasePositionPreSwap [0xc5, 0x26, 0x56, 0xa5, 0xc7, 0x17, 0x26, 0xea]
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
        match Self::variant_from_data(&ix.data) {
            Some(JupPerpsSwapVariant::Swap2) | Some(JupPerpsSwapVariant::SwapWithTokenLedger) => (
                ix.accounts[13].pubkey,
                ix.accounts[9].pubkey,
            ),
            Some(JupPerpsSwapVariant::InstantIncreasePositionPreSwap) => (
                ix.accounts[11].pubkey,
                ix.accounts[8].pubkey,
            ),
            None => unreachable!(),
        }
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        match Self::variant_from_data(&inner_ix.data) {
            Some(JupPerpsSwapVariant::Swap2) | Some(JupPerpsSwapVariant::SwapWithTokenLedger) => (
                account_keys[inner_ix.accounts[13] as usize],
                account_keys[inner_ix.accounts[9] as usize],
            ),
            Some(JupPerpsSwapVariant::InstantIncreasePositionPreSwap) => (
                account_keys[inner_ix.accounts[11] as usize],
                account_keys[inner_ix.accounts[8] as usize],
            ),
            _ => unreachable!(),
        }
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // swap_base_input
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &JUP_PERPS_PUBKEY, &[0x41, 0x4b, 0x3f, 0x4c, 0xeb, 0x5b, 0x5b, 0x88], 0, 24),
            // swap_base_output
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &JUP_PERPS_PUBKEY, &[0x8b, 0x8d, 0xee, 0xc5, 0x29, 0xd3, 0xac, 0x13], 0, 24),
            // instant_increase_position_pre_swap
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &JUP_PERPS_PUBKEY, &[0xc5, 0x26, 0x56, 0xa5, 0xc7, 0x17, 0x26, 0xea], 0, 24),
        ].concat()
    }
}