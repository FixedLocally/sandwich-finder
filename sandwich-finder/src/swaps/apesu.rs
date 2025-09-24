use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::swaps::{addresses::APESU_PUBKEY, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed};

impl Sealed for ApesuSwapFinder {}

pub struct ApesuSwapFinder {}

/// Apesu a single swap instruction
/// [24]==1 is a_to_b, ==3 is b_to_a, unsure what 0/2/4 does, never seen either
/// user a/b: 1/2, pool a/b: 3/4
/// name is identified through crank's source of funds
impl ApesuSwapFinder {
    fn is_a_to_b(data: &[u8]) -> bool {
        data[24] == 1
    }
}

impl SwapFinder for ApesuSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[0].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[0] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&ix.data) {
            (
                ix.accounts[1].pubkey,
                ix.accounts[2].pubkey,
            )
        } else {
            (
                ix.accounts[2].pubkey,
                ix.accounts[1].pubkey,
            )
        }
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&inner_ix.data) {
            (
                account_keys[inner_ix.accounts[1] as usize],
                account_keys[inner_ix.accounts[2] as usize],
            )
        } else {
            (
                account_keys[inner_ix.accounts[2] as usize],
                account_keys[inner_ix.accounts[1] as usize],
            )
        }
    }
    
    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&ix.data) {
            (
                ix.accounts[4].pubkey,
                ix.accounts[3].pubkey,
            )
        } else {
            (
                ix.accounts[3].pubkey,
                ix.accounts[4].pubkey,
            )
        }
    }
    
    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_a_to_b(&inner_ix.data) {
            (
                account_keys[inner_ix.accounts[4] as usize], // base
                account_keys[inner_ix.accounts[3] as usize], // quote
            )
        } else {
            (
                account_keys[inner_ix.accounts[3] as usize], // quote
                account_keys[inner_ix.accounts[4] as usize], // base
            )
        }
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        [
            // swap
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &APESU_PUBKEY, &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8], 0, 25),
        ].concat()
    }
}
