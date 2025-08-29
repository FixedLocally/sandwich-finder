use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::swaps::{addresses::OPENBOOK_V2_PUBKEY, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed};

impl Sealed for OpenbookV2SwapFinder {}

pub struct OpenbookV2SwapFinder {}

/// Openbook is actually a CLOB program, but there's placeTakeOrder which works like a swap
/// Parsing pending events is not supported here
/// Market base/quote: 6/7
/// User base/quote: 9/10
impl OpenbookV2SwapFinder {
    fn is_ask(data: &[u8]) -> bool {
        data[8] == 1
    }
}
impl SwapFinder for OpenbookV2SwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[2].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[2] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_ask(&ix.data) {
            (
                ix.accounts[9].pubkey,
                ix.accounts[10].pubkey,
            )
        } else {
            (
                ix.accounts[10].pubkey,
                ix.accounts[9].pubkey,
            )
        }
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_ask(&inner_ix.data) {
            (
                account_keys[inner_ix.accounts[9] as usize],
                account_keys[inner_ix.accounts[10] as usize],
            )
        } else {
            (
                account_keys[inner_ix.accounts[10] as usize],
                account_keys[inner_ix.accounts[9] as usize],
            )
        }
    }

    fn pool_ata_ix(_ix: &Instruction) -> (Pubkey, Pubkey) {
        if Self::is_ask(&_ix.data) {
            (
                _ix.accounts[7].pubkey,
                _ix.accounts[6].pubkey,
            )
        } else {
            (
                _ix.accounts[6].pubkey,
                _ix.accounts[7].pubkey,
            )
        }
    }

    fn pool_ata_inner_ix(_inner_ix: &InnerInstruction, _account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        if Self::is_ask(&_inner_ix.data) {
            (
                _account_keys[_inner_ix.accounts[7] as usize],
                _account_keys[_inner_ix.accounts[6] as usize],
            )
        } else {
            (
                _account_keys[_inner_ix.accounts[6] as usize],
                _account_keys[_inner_ix.accounts[7] as usize],
            )
        }
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        // placeTakeOrder
        Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &OPENBOOK_V2_PUBKEY, &[0x03, 0x2c, 0x47, 0x03, 0x1a, 0xc7, 0xcb, 0x55], 35)
    }
}