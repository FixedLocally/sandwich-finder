use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::swaps::{addresses::METEORA_DBC_PUBKEY, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed};

impl Sealed for MeteoraDBCSwapFinder {}

pub struct MeteoraDBCSwapFinder {}

/// Meteora bonding curve swaps have two variants
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
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &METEORA_DBC_PUBKEY, &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8], 0, 24),
            // swap2
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &METEORA_DBC_PUBKEY, &[0x41, 0x4b, 0x3f, 0x4c, 0xeb, 0x5b, 0x5b, 0x88], 0, 24),
        ].concat()
    }
}