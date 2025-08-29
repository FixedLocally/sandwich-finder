use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::swaps::{addresses::METEORA_DBC_PUBKEY, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed};

impl Sealed for MeteoraDBCSwapFinder {}

pub struct MeteoraDBCSwapFinder {}

/// Ray v5 swaps have two variants:
/// 1. swap_base_input [0x8f, 0xbe, 0x5a, 0xda, 0xc4, 0x1e, 0x33, 0xde]
/// 2. swap_base_output [0x37, 0xd9, 0x62, 0x56, 0xa3, 0x4a, 0xb4, 0xad]
/// In/out amounts follows the discriminant, with one being exact and the other being the worst acceptable value.
/// Swap direction is determined by the input/output token accounts ([4], [5] respectively)
/// Unlike v4, the ordering of the pool's ATA also depends on the swap direction.
impl SwapFinder for MeteoraDBCSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[2].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[2] as usize]
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

    fn blacklist_ata_indexs() -> Vec<usize> {        
        vec![12] // referral
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // swap
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &METEORA_DBC_PUBKEY, &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8], 24),
            // swap2
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &METEORA_DBC_PUBKEY, &[0x41, 0x4b, 0x3f, 0x4c, 0xeb, 0x5b, 0x5b, 0x88], 24),
        ].concat()
    }
}