use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::swaps::{addresses::WHIRLPOOL_PUBKEY, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed};

impl Sealed for WhirlpoolSwapFinder {}

pub struct WhirlpoolSwapFinder {}

/// Whirlpool swaps have two variants:
/// 1. swap [0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8]
/// 2. swapV2 [0x2b, 0x04, 0xed, 0x0b, 0x1a, 0xc9, 0x1e, 0x62]
/// For swap, [amm, userA, poolA, userB, poolB] = [2, 3, 4, 5, 6]
/// For swapV2, [amm, userA, poolA, userB, poolB] = [4, 7, 8, 9, 10]
/// As far as swap amounts are concerned, both instructions has the same data layout
/// in amount, min out, sqrt price limit, amount is in, aToB
/// aToB determines trade direction.
impl WhirlpoolSwapFinder {
    fn is_swap_v2(ix_data: &[u8]) -> bool {
        ix_data.starts_with(&[0x2b, 0x04, 0xed, 0x0b, 0x1a, 0xc9, 0x1e, 0x62])
    }

    fn is_from_a_to_b(ix_data: &[u8]) -> bool {
        ix_data[41] != 0
    }
}

impl SwapFinder for WhirlpoolSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        if Self::is_swap_v2(&ix.data) {
            return ix.accounts[4].pubkey; // swapV2
        }
        return ix.accounts[2].pubkey; // swap
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        if Self::is_swap_v2(&inner_ix.data) {
            return account_keys[inner_ix.accounts[4] as usize]; // swapV2
        }
        return account_keys[inner_ix.accounts[2] as usize]; // swap
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        match (Self::is_swap_v2(&ix.data), Self::is_from_a_to_b(&ix.data)) {
            (true, true) => (ix.accounts[7].pubkey, ix.accounts[9].pubkey), // swapV2, aToB
            (true, false) => (ix.accounts[9].pubkey, ix.accounts[7].pubkey), // swapV2, bToA
            (false, true) => (ix.accounts[3].pubkey, ix.accounts[5].pubkey), // swap, aToB
            (false, false) => (ix.accounts[5].pubkey, ix.accounts[3].pubkey), // swap, bToA
        }
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        match (Self::is_swap_v2(&inner_ix.data), Self::is_from_a_to_b(&inner_ix.data)) {
            (true, true) => (
                account_keys[inner_ix.accounts[7] as usize],
                account_keys[inner_ix.accounts[9] as usize],
            ), // swapV2, aToB
            (true, false) => (
                account_keys[inner_ix.accounts[9] as usize],
                account_keys[inner_ix.accounts[7] as usize],
            ), // swapV2, bToA
            (false, true) => (
                account_keys[inner_ix.accounts[3] as usize],
                account_keys[inner_ix.accounts[5] as usize],
            ), // swap, aToB
            (false, false) => (
                account_keys[inner_ix.accounts[5] as usize],
                account_keys[inner_ix.accounts[3] as usize],
            ), // swap, bToA
        }
    }
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        match (Self::is_swap_v2(&ix.data), Self::is_from_a_to_b(&ix.data)) {
            (true, true) => (ix.accounts[10].pubkey, ix.accounts[8].pubkey), // swapV2, aToB
            (true, false) => (ix.accounts[8].pubkey, ix.accounts[10].pubkey), // swapV2, bToA
            (false, true) => (ix.accounts[6].pubkey, ix.accounts[4].pubkey), // swap, aToB
            (false, false) => (ix.accounts[4].pubkey, ix.accounts[6].pubkey), // swap, bToA
        }
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        match (Self::is_swap_v2(&inner_ix.data), Self::is_from_a_to_b(&inner_ix.data)) {
            (true, true) => (
                account_keys[inner_ix.accounts[10] as usize],
                account_keys[inner_ix.accounts[8] as usize],
            ), // swapV2, aToB
            (true, false) => (
                account_keys[inner_ix.accounts[8] as usize],
                account_keys[inner_ix.accounts[10] as usize],
            ), // swapV2, bToA
            (false, true) => (
                account_keys[inner_ix.accounts[6] as usize],
                account_keys[inner_ix.accounts[4] as usize],
            ), // swap, aToB
            (false, false) => (
                account_keys[inner_ix.accounts[4] as usize],
                account_keys[inner_ix.accounts[6] as usize],
            ), // swap, bToA
        }
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        return [
            // swap_base_input
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &WHIRLPOOL_PUBKEY, &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8], 24),
            // swap_base_output
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &WHIRLPOOL_PUBKEY, &[0x2b, 0x04, 0xed, 0x0b, 0x1a, 0xc9, 0x1e, 0x62], 24),
        ].concat();
    }
}