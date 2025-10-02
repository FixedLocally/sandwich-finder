use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::{swap::{SwapFinder, SwapV2}, swaps::{addresses::METEORA_DAMMV2_PUBKEY, private::Sealed, swap_finder_ext::SwapFinderExt}};

impl Sealed for MeteoraDammV2Finder {}

pub struct MeteoraDammV2Finder {}

/// Meteora bonding curve swaps have one variant
impl SwapFinder for MeteoraDammV2Finder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[1].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[1] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[2].pubkey,
            ix.accounts[3].pubkey,
        )
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[2] as usize],
            account_keys[inner_ix.accounts[3] as usize],
        )
    }

    fn blacklist_ata_indexs() -> Vec<usize> {        
        vec![11] // referral
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // swap
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &METEORA_DAMMV2_PUBKEY, &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8], 0, 24),
        ].concat()
    }
}