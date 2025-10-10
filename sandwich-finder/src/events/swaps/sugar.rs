use std::sync::Arc;

use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::{events::{addresses::{SUGAR_PUBKEY, WSOL_MINT}, swap::{SwapFinder, SwapV2}, swaps::private::Sealed}, utils::pubkey_from_slice};

impl Sealed for SugarSwapFinder {}

pub struct SugarSwapFinder {}

// Includes both the ix and event discrimant
const LOG_DISCRIMINANT: &[u8] = &[
    0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d,
    0xbd, 0xdb, 0x7f, 0xd3, 0x4e, 0xe6, 0x61, 0xee,
];
const BUY_EXACT_IN: &[u8] = &[0xfa, 0xea, 0x0d, 0x7b, 0xd5, 0x9c, 0x13, 0xec];
const BUY_EXACT_OUT: &[u8] = &[0x18, 0xd3, 0x74, 0x28, 0x69, 0x03, 0x99, 0x38];
const BUY_MAX_OUT: &[u8] = &[0x60, 0xb1, 0xcb, 0x75, 0xb7, 0x41, 0xc4, 0xb1];
const SELL_EXACT_IN: &[u8] = &[0x95, 0x27, 0xde, 0x9b, 0xd3, 0x7c, 0x98, 0x1a];
const SELL_EXACT_OUT: &[u8] = &[0x5f, 0xc8, 0x47, 0x22, 0x08, 0x09, 0x0b, 0xa6];

/// ~~Pump.fun~~ Sugar have a few variants but it doesn't matter since we rely on the logging instruction here
/// buyExactIn, buyExactOut, buyMaxOut, sellExactIn, sellExactOut
/// This one requires custom logic for event parsing since it issues so many transfer for all sorts of fees (all in SOL).
/// mint[16..48], sol amount [48..56], token amount [56..64], is buy [64], user [65..97]
/// suspiciously sumilar to pump.fun
impl SugarSwapFinder {
    fn user_in_out_index(ix_data: &[u8]) -> (usize, usize) {
        match &ix_data[..8] {
            BUY_EXACT_IN | BUY_EXACT_OUT | BUY_MAX_OUT => (6, 5), // in sol, out token
            SELL_EXACT_IN | SELL_EXACT_OUT => (5, 7), // in token, out sol
            _ => (0, 0), // Unknown instruction
        }
    }

    fn pool_in_out_index(ix_data: &[u8]) -> (usize, usize) {
        match &ix_data[..8] {
            BUY_EXACT_IN | BUY_EXACT_OUT | BUY_MAX_OUT => (4, 3), // in token, out sol
            SELL_EXACT_IN | SELL_EXACT_OUT => (3, 4), // in sol, out token
            _ => (0, 0), // Unknown instruction
        }
    }

    fn swap_from_pdf_trade_event(outer_program: Option<Arc<str>>, amm: Pubkey, input_ata: Pubkey, output_ata: Pubkey, data: &[u8], inner_ix_index: Option<u32>) -> SwapV2 {
        let mint = pubkey_from_slice(&data[16..48]);
        let sol_amount = u64::from_le_bytes(data[48..56].try_into().unwrap());
        let token_amount = u64::from_le_bytes(data[56..64].try_into().unwrap());
        let is_buy = data[64] != 0;
        // let fee = u64::from_le_bytes(data[177..185].try_into().unwrap());
        // let creator_fee = u64::from_le_bytes(data[225..233].try_into().unwrap());
        let fee = if is_buy {
            sol_amount * 9 / 991 // 0.9% fee according to their docs
        } else {
            0
        };
        let (input_mint, output_mint) = if is_buy {
            (WSOL_MINT, mint)
        } else {
            (mint, WSOL_MINT)
        };
        let (input_amount, output_amount) = if is_buy {
            (sol_amount + fee, token_amount)
        } else {
            (token_amount, sol_amount - fee)
        };
        SwapV2::new(
            outer_program,
            SUGAR_PUBKEY.to_string().into(),
            pubkey_from_slice(&data[65..97]).to_string().into(),
            amm.to_string().into(),
            input_mint.to_string().into(),
            output_mint.to_string().into(),
            input_amount,
            output_amount,
            input_ata.to_string().into(),
            output_ata.to_string().into(),
            // todo: should try to locate the actual ix
            None,
            None,
            0,
            0,
            0,
            inner_ix_index,
            0,
        )
    }
}

impl SwapFinder for SugarSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[2].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[2] as usize]
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

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, _meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        if ix.program_id == SUGAR_PUBKEY {
            for inner_ix in inner_ixs.instructions.iter() {
                if inner_ix.data.len() == 137 && inner_ix.data[0..16] == LOG_DISCRIMINANT[..] {
                    let (in_index, out_index) = Self::user_in_out_index(&ix.data);
                    return vec![
                        Self::swap_from_pdf_trade_event(
                            None,
                            ix.accounts[2].pubkey,
                            ix.accounts[in_index].pubkey,
                            ix.accounts[out_index].pubkey,
                            &inner_ix.data,
                            None,
                        )
                    ];
                }
            } 
        }
        let mut swaps = vec![];
        let mut next_logical_ix = 0;
        for (i, inner_ix) in inner_ixs.instructions.iter().enumerate() {
            if inner_ix.program_id_index >= account_keys.len() as u32 || i < next_logical_ix {
                continue; // Skip already processed instructions or invalid program ID
            }
            if account_keys[inner_ix.program_id_index as usize] != SUGAR_PUBKEY {
                continue; // Not a sugar instruction
            }
            if inner_ix.data.len() < 24 {
                continue; // Not a swap
            }
            match &inner_ix.data[..8] {
                BUY_EXACT_IN | BUY_EXACT_OUT | BUY_MAX_OUT |
                SELL_EXACT_IN | SELL_EXACT_OUT => {
                    // Valid swap instruction
                    let (input_ata, output_ata) = Self::user_ata_inner_ix(inner_ix, account_keys);
                    for j in i + 1..inner_ixs.instructions.len() {
                        let next_inner_ix = &inner_ixs.instructions[j];
                        if next_inner_ix.program_id_index >= account_keys.len() as u32 {
                            continue; // Skip invalid program ID
                        }
                        if account_keys[next_inner_ix.program_id_index as usize] != SUGAR_PUBKEY {
                            continue; // Not a Pump.fun instruction
                        }
                        if next_inner_ix.data.len() != 137 || next_inner_ix.data[0..16] != LOG_DISCRIMINANT[..] {
                            continue; // Not an event
                        }
                        swaps.push(Self::swap_from_pdf_trade_event(
                            Some(ix.program_id.to_string().into()),
                            Self::amm_inner_ix(inner_ix, account_keys),
                            input_ata,
                            output_ata,
                            &next_inner_ix.data,
                            Some(i as u32),
                        ));
                        next_logical_ix = j + 1;
                    }
                },
                _ => continue, // Not a swap
                
            }
        }
        swaps
    }
}