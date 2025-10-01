use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::events::swaps::{addresses::WHIRLPOOL_PUBKEY, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed};

impl Sealed for WhirlpoolSwapFinder {}

pub struct WhirlpoolSwapFinder {}

/// Whirlpool 1-hop swaps have two variants:
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
            ix.accounts[4].pubkey // swapV2
        } else {
            ix.accounts[2].pubkey // swap
        }
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        if Self::is_swap_v2(&inner_ix.data) {
            account_keys[inner_ix.accounts[4] as usize] // swapV2
        } else {
            account_keys[inner_ix.accounts[2] as usize] // swap
        }
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
        [
            // swap
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &WHIRLPOOL_PUBKEY, &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8], 0, 24),
            // swap_v2
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &WHIRLPOOL_PUBKEY, &[0x2b, 0x04, 0xed, 0x0b, 0x1a, 0xc9, 0x1e, 0x62], 0, 24),
        ].concat()
    }
}

pub struct WhirlpoolTwoHopSwapFinder<
    const A2B: usize,
    const AMM_INDEX: usize,
    const USER_A_INDEX: usize,
    const USER_B_INDEX: usize,
    const POOL_A_INDEX: usize,
    const POOL_B_INDEX: usize,
    const DATA_SIZE: usize,
    const D0: u8,
    const D1: u8,
    const D2: u8,
    const D3: u8,
    const D4: u8,
    const D5: u8,
    const D6: u8,
    const D7: u8,
>;

impl<
    const A2B: usize,
    const AMM: usize,
    const UA: usize,
    const UB: usize,
    const PA: usize,
    const PB: usize,
    const DS: usize,
    const D0: u8,
    const D1: u8,
    const D2: u8,
    const D3: u8,
    const D4: u8,
    const D5: u8,
    const D6: u8,
    const D7: u8,
> Sealed for WhirlpoolTwoHopSwapFinder<A2B, AMM, UA, UB, PA, PB, DS, D0, D1, D2, D3, D4, D5, D6, D7> {}

impl<
    const A2B: usize,
    const AMM: usize,
    const UA: usize,
    const UB: usize,
    const PA: usize,
    const PB: usize,
    const DS: usize,
    const D0: u8,
    const D1: u8,
    const D2: u8,
    const D3: u8,
    const D4: u8,
    const D5: u8,
    const D6: u8,
    const D7: u8,
> WhirlpoolTwoHopSwapFinder<A2B, AMM, UA, UB, PA, PB, DS, D0, D1, D2, D3, D4, D5, D6, D7> {
    pub fn is_from_a_to_b(ix_data: &[u8]) -> bool {
        ix_data[A2B] != 0
    }
}

impl<
    const A2B: usize,
    const AMM: usize,
    const UA: usize,
    const UB: usize,
    const PA: usize,
    const PB: usize,
    const DS: usize,
    const D0: u8,
    const D1: u8,
    const D2: u8,
    const D3: u8,
    const D4: u8,
    const D5: u8,
    const D6: u8,
    const D7: u8,
> SwapFinder for WhirlpoolTwoHopSwapFinder<A2B, AMM, UA, UB, PA, PB, DS, D0, D1, D2, D3, D4, D5, D6, D7> {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[AMM].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[AMM] as usize]
    }
    
    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_from_a_to_b(&ix.data) {
            (ix.accounts[UA].pubkey, ix.accounts[UB].pubkey) // aToB
        } else {
            (ix.accounts[UB].pubkey, ix.accounts[UA].pubkey) // bToA
        }
    }
    
    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_from_a_to_b(&inner_ix.data) {
            (
                account_keys[inner_ix.accounts[UA] as usize],
                account_keys[inner_ix.accounts[UB] as usize],
            ) // aToB
        } else {
            (
                account_keys[inner_ix.accounts[UB] as usize],
                account_keys[inner_ix.accounts[UA] as usize],
            ) // bToA
        }
    }

    fn pool_ata_ix(_ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_from_a_to_b(&_ix.data) {
            (_ix.accounts[PB].pubkey, _ix.accounts[PA].pubkey) // aToB
        } else {
            (_ix.accounts[PA].pubkey, _ix.accounts[PB].pubkey) // bToA
        }
    }

    fn pool_ata_inner_ix(_inner_ix: &InnerInstruction, _account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_from_a_to_b(&_inner_ix.data) {
            (
                _account_keys[_inner_ix.accounts[PB] as usize],
                _account_keys[_inner_ix.accounts[PA] as usize],
            ) // aToB
        } else {
            (
                _account_keys[_inner_ix.accounts[PA] as usize],
                _account_keys[_inner_ix.accounts[PB] as usize],
            ) // bToA
        }
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &WHIRLPOOL_PUBKEY, &[D0, D1, D2, D3, D4, D5, D6, D7], 0, DS)
    }
}

/// Whirlpool also has 2-hop swaps with two variants
/// It's much easier to run 2 passes for the 2 hops
/// Hop 1: [amm, userA, poolA, userB, poolB] = [2, 4, 5, 6, 7]
/// Hop 2: [amm, userA, poolA, userB, poolB] = [3, 8, 9, 10, 11]
/// 
pub type WhirlpoolTwoHopSwapFinder1 = WhirlpoolTwoHopSwapFinder<25, 2, 4, 6, 5, 7, 59, 0xc3, 0x60, 0xed, 0x6c, 0x44, 0xa2, 0xdb, 0xe6>;
pub type WhirlpoolTwoHopSwapFinder2 = WhirlpoolTwoHopSwapFinder<26, 3, 8, 10, 9, 11, 59, 0xc3, 0x60, 0xed, 0x6c, 0x44, 0xa2, 0xdb, 0xe6>;

/// For TwoHopSwapV2 there's only 3 transfers, but the second one is reused (both the output of the 1st hop and the input of the 2nd hop)
/// The structure looks something like this
/// A->B B->C
/// pump->sol->usdt
/// [8]  UA /UA1 [pump]
/// [9]  P1A/PA1 [pump]
/// [10] P1B/PB1/UA2 [sol]
/// [11] P2B/UB1/PA2 [sol]
/// [12] P2C    /PB2 [usdt]
/// [13] UC     /UB2 [usdt]
/// swap 1: UA->P1A, P1B->P2B
/// swap 2: P1B->P2B, P2C->UC
/// We set A2B to 0 since it's one of the discriminant bytes and is guaranteed to be non zero
pub type WhirlpoolTwoHopSwapV2Finder1 = WhirlpoolTwoHopSwapFinder<0, 0, 8, 11, 9, 10, 59, 0xba, 0x8f, 0xd1, 0x1d, 0xfe, 0x02, 0xc2, 0x75>;
pub type WhirlpoolTwoHopSwapV2Finder2 = WhirlpoolTwoHopSwapFinder<0, 1, 10, 13, 11, 12, 59, 0xba, 0x8f, 0xd1, 0x1d, 0xfe, 0x02, 0xc2, 0x75>;
