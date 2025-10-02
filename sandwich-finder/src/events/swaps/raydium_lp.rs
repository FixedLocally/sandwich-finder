use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::{swap::{SwapFinder, SwapV2}, swaps::{addresses::RAYDIUM_LP_PUBKEY, private::Sealed, swap_finder_ext::SwapFinderExt}};

impl Sealed for RaydiumLPSwapFinder {}

pub struct RaydiumLPSwapFinder {}

/// Ray Launchpad swaps have two variants:
/// 1. buy_exact_in [0xfa, 0xea, 0x0d, 0x7b, 0xd5, 0x9c, 0x13, 0xec] (4, 5=base, 6=quote)
/// 2. sell_exact_in [0x95, 0x27, 0xde, 0x9b, 0xd3, 0x7c, 0x98, 0x1a] (4, 5=base, 6=quote)
/// 3. buy_exact_out [0x18, 0xd3, 0x74, 0x28, 0x69, 0x03, 0x99, 0x38] (4, 5=base, 6=quote)
/// 4. sell_exact_out [0x5f, 0xc8, 0x47, 0x22, 0x8, 0x9, 0xb, 0xa6] (4, 5=base, 6=quote)
/// In/out amounts follows the discriminant, with one being exact and the other being the worst acceptable value.
/// share_fee_rate follows the above but we don't care.
/// Swap direction is determined by the instruction's name.
/// Buy = quote->base, sell = base->quote.
/// All 4 instructions follow the same structure:
/// [4]=amm, [5]=user base ata, [6]=user quote ata, [7]=pool base ATA, [8]=pool quote ATA
impl RaydiumLPSwapFinder {
    fn user_in_out_index(ix_data: &[u8]) -> (usize, usize) {
        if ix_data[0] == 0xfa || ix_data[0] == 0x18 {
            // buy
            (6, 5) // quote, base
        } else {
            // sell
            (5, 6) // base, quote
        }
    }

    fn pool_in_out_index(ix_data: &[u8]) -> (usize, usize) {
        if ix_data[0] == 0xfa || ix_data[0] == 0x18 {
            // buy
            (7, 8) // base, quote
        } else {
            // sell
            (8, 7) // quote, base
        }
    }
}

impl SwapFinder for RaydiumLPSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[4].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[4] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        let (in_index, out_index) = Self::user_in_out_index(&ix.data);
        (
            ix.accounts[in_index].pubkey,
            ix.accounts[out_index].pubkey,
        )
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        let (in_index, out_index) = Self::user_in_out_index(&inner_ix.data);
        (
            account_keys[inner_ix.accounts[in_index] as usize],
            account_keys[inner_ix.accounts[out_index] as usize],
        )
    }
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        let (in_index, out_index) = Self::pool_in_out_index(&ix.data);
        (
            ix.accounts[in_index].pubkey,
            ix.accounts[out_index].pubkey,
        )
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        let (in_index, out_index) = Self::pool_in_out_index(&inner_ix.data);
        (
            account_keys[inner_ix.accounts[in_index] as usize],
            account_keys[inner_ix.accounts[out_index] as usize],
        )
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // buy_exact_in
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &RAYDIUM_LP_PUBKEY, &[0xfa, 0xea, 0x0d, 0x7b, 0xd5, 0x9c, 0x13, 0xec], 0, 32),
            // sell_exact_in
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &RAYDIUM_LP_PUBKEY, &[0x95, 0x27, 0xde, 0x9b, 0xd3, 0x7c, 0x98, 0x1a], 0, 32),
            // buy_exact_out
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &RAYDIUM_LP_PUBKEY, &[0x18, 0xd3, 0x74, 0x28, 0x69, 0x03, 0x99, 0x38], 0, 32),
            // sell_exact_out
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &RAYDIUM_LP_PUBKEY, &[0x5f, 0xc8, 0x47, 0x22, 0x08, 0x09, 0x0b, 0xa6], 0, 32),
        ].concat()
    }
}