use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::swaps::{addresses::RAYDIUM_V5_PUBKEY, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed};

impl Sealed for RaydiumV5SwapFinder {}

pub struct RaydiumV5SwapFinder {}

/// Ray v5 swaps have two variants:
/// 1. swap_base_input [0x8f, 0xbe, 0x5a, 0xda, 0xc4, 0x1e, 0x33, 0xde]
/// 2. swap_base_output [0x37, 0xd9, 0x62, 0x56, 0xa3, 0x4a, 0xb4, 0xad]
/// In/out amounts follows the discriminant, with one being exact and the other being the worst acceptable value.
/// Swap direction is determined by the input/output token accounts ([4], [5] respectively)
/// Unlike v4, the ordering of the pool's ATA also depends on the swap direction.
impl SwapFinder for RaydiumV5SwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[3].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[3] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[4].pubkey,
            ix.accounts[5].pubkey,
        )
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[4] as usize],
            account_keys[inner_ix.accounts[5] as usize],
        )
    }
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[7].pubkey,
            ix.accounts[6].pubkey,
        )
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[7] as usize],
            account_keys[inner_ix.accounts[6] as usize],
        )
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // swap_base_input
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &RAYDIUM_V5_PUBKEY, &[0x8f, 0xbe, 0x5a, 0xda, 0xc4, 0x1e, 0x33, 0xde], 0, 24),
            // swap_base_output
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &RAYDIUM_V5_PUBKEY, &[0x37, 0xd9, 0x62, 0x56, 0xa3, 0x4a, 0xb4, 0xad], 0, 24),
        ].concat()
    }
}