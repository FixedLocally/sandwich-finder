use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::{swap::{SwapFinder, SwapV2}, swaps::{addresses::HUMIDIFI_PUBKEY, private::Sealed, swap_finder_ext::SwapFinderExt}};

impl Sealed for HumidiFiSwapFinder {}

pub struct HumidiFiSwapFinder {}

/// HumidiFi doesn't have any published IDL so it's guesswork from solscan/jup txs
/// from multiple samples it appears base->quote swaps have 0x38 at [16] of the calldata, 0x39 for quote->base swaps
/// all swaps seem to have 0xff2dffe0bae9c33d at [17..25]
/// pool base/quote are at [2] and [3], user base/quote are at [4] and [5]

impl HumidiFiSwapFinder {
    fn is_base_to_quote(data: &[u8]) -> bool {
        data[16] == 0x38
    }

    fn is_quote_to_base(data: &[u8]) -> bool {
        data[16] == 0x39
    }
}

impl SwapFinder for HumidiFiSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[1].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[1] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_base_to_quote(&ix.data) {
            // base->quote
            (
                ix.accounts[4].pubkey, // base
                ix.accounts[5].pubkey, // quote
            )
        } else if Self::is_quote_to_base(&ix.data) {
            // quote->base
            (
                ix.accounts[5].pubkey, // quote
                ix.accounts[4].pubkey, // base
            )
        } else {
            panic!("Unknown HumidiFi swap direction");
        }
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_base_to_quote(&inner_ix.data) {
            // base->quote
            (
                account_keys[inner_ix.accounts[4] as usize], // base
                account_keys[inner_ix.accounts[5] as usize], // quote
            )
        } else if Self::is_quote_to_base(&inner_ix.data) {
            // quote->base
            (
                account_keys[inner_ix.accounts[5] as usize], // quote
                account_keys[inner_ix.accounts[4] as usize], // base
            )
        } else {
            panic!("Unknown HumidiFi swap direction");
        }
    }
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_base_to_quote(&ix.data) {
            // base->quote
            (
                ix.accounts[3].pubkey, // quote
                ix.accounts[2].pubkey, // base
            )
        } else if Self::is_quote_to_base(&ix.data) {
            // quote->base
            (
                ix.accounts[2].pubkey, // base
                ix.accounts[3].pubkey, // quote
            )
        } else {
            panic!("Unknown HumidiFi swap direction");
        }
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_base_to_quote(&inner_ix.data) {
            // base->quote
            (
                account_keys[inner_ix.accounts[3] as usize], // quote
                account_keys[inner_ix.accounts[2] as usize], // base
            )
        } else if Self::is_quote_to_base(&inner_ix.data) {
            // quote->base
            (
                account_keys[inner_ix.accounts[2] as usize], // base
                account_keys[inner_ix.accounts[3] as usize], // quote
            )
        } else {
            panic!("Unknown HumidiFi swap direction");
        }
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // swap
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &HUMIDIFI_PUBKEY, &[0xff, 0x2d, 0xff, 0xe0, 0xba, 0xe9, 0xc3, 0x3d], 17, 25),
        ].concat()
    }
}
