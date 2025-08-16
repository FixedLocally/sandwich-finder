use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::{swaps::{addresses::{PDF2_PUBKEY, WSOL_MINT}, finder::{SwapFinder, SwapFinderExt, SwapV2}, private::Sealed}, utils::pubkey_from_slice};

impl Sealed for PumpAmmSwapFinder {}

pub struct PumpAmmSwapFinder {}

// Includes both the ix and event discrimant
const LOG_DISCRIMINANT: &[u8] = &[
    0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d,
    0xbd, 0xdb, 0x7f, 0xd3, 0x4e, 0xe6, 0x61, 0xee,
];

/// Pump.fun have two variants:
/// 1. buy [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea]
/// 2. sell [0x33, 0xe6, 0x85, 0xa4, 0x01, 0x7f, 0x83, 0xad]
/// In/out amounts follows the discriminant, with the first one being exact and the other being the worst acceptable value.
/// Swap direction is determined instruction's name.
impl PumpAmmSwapFinder {
    fn user_in_out_index(ix_data: &[u8]) -> (usize, usize) {
        if ix_data[0] == 0x66 {
            // buy
            (6, 5)
        } else {
            // sell
            (5, 6)
        }
    }

    fn pool_in_out_index(ix_data: &[u8]) -> (usize, usize) {
        if ix_data[0] == 0x66 {
            // buy
            (7, 8)
        } else {
            // sell
            (8, 7)
        }
    }
}

impl SwapFinder for PumpAmmSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        return ix.accounts[0].pubkey;
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        return account_keys[inner_ix.accounts[0] as usize];
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        let (in_index, out_index) = Self::user_in_out_index(&ix.data);
        return (
            ix.accounts[in_index].pubkey,
            ix.accounts[out_index].pubkey,
        );
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        let (in_index, out_index) = Self::user_in_out_index(&inner_ix.data);
        return (
            account_keys[inner_ix.accounts[in_index] as usize],
            account_keys[inner_ix.accounts[out_index] as usize],
        );
    }

    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        let (in_index, out_index) = Self::pool_in_out_index(&ix.data);
        return (
            ix.accounts[in_index].pubkey,
            ix.accounts[out_index].pubkey,
        );
    }

    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        let (in_index, out_index) = Self::pool_in_out_index(&inner_ix.data);
        return (
            account_keys[inner_ix.accounts[in_index] as usize],
            account_keys[inner_ix.accounts[out_index] as usize],
        );
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        return [
            // buy
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &PDF2_PUBKEY, &[0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea], 24),
            // sell
            Self::find_swaps_generic(ix, inner_ixs, account_keys, meta, &PDF2_PUBKEY, &[0x33, 0xe6, 0x85, 0xa4, 0x01, 0x7f, 0x83, 0xad], 24),
        ].concat();
    }
}