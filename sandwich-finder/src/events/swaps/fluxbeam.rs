use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::{swap::{SwapFinder, SwapV2}, swaps::{addresses::FLUXBEAM_PUBKEY, private::Sealed, swap_finder_ext::SwapFinderExt}};

impl Sealed for FluxbeamSwapFinder {}

pub struct FluxbeamSwapFinder {}

/// Fluxbeam swaps have one variant
impl SwapFinder for FluxbeamSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[0].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[0] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[3].pubkey,
            ix.accounts[6].pubkey,
        )
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[3] as usize],
            account_keys[inner_ix.accounts[6] as usize],
        )
    }

    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[5].pubkey,
            ix.accounts[4].pubkey,
        )
    }

    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[5] as usize],
            account_keys[inner_ix.accounts[4] as usize],
        )
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &FLUXBEAM_PUBKEY, &[0x01], 0, 17),
        ].concat()
    }
}