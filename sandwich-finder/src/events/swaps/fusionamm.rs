use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::{addresses::FUSIONAMM_PUBKEY, swap::{SwapFinder, SwapV2}, swaps::{private::Sealed, swap_finder_ext::SwapFinderExt}};

impl Sealed for FusionAmmSwapFinder {}

pub struct FusionAmmSwapFinder {}

impl FusionAmmSwapFinder {
    fn is_a_to_b(data: &[u8]) -> bool {
        data[41] == 1
    }
}

impl SwapFinder for FusionAmmSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[4].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[4] as usize]
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
                account_keys[inner_ix.accounts[7] as usize],
                account_keys[inner_ix.accounts[8] as usize],
            )
        } else {
            (
                account_keys[inner_ix.accounts[8] as usize],
                account_keys[inner_ix.accounts[7] as usize],
            )
        }
    }
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&ix.data) {
            (
                ix.accounts[10].pubkey,
                ix.accounts[9].pubkey,
            )
        } else {
            (
                ix.accounts[9].pubkey,
                ix.accounts[10].pubkey,
            )
        }
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&inner_ix.data) {
            (
                account_keys[inner_ix.accounts[10] as usize],
                account_keys[inner_ix.accounts[9] as usize],
            )
        } else {
            (
                account_keys[inner_ix.accounts[9] as usize],
                account_keys[inner_ix.accounts[10] as usize],
            )
        }
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // swap
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &FUSIONAMM_PUBKEY, &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8], 0, 42),
        ].concat()
    }
}
