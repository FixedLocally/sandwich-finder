use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::{events::swaps::{addresses::{PDF_PUBKEY, WSOL_MINT}, finder::{SwapFinder, SwapV2}, private::Sealed}, utils::pubkey_from_slice};

impl Sealed for PumpFunSwapFinder {}

pub struct PumpFunSwapFinder {}

// Includes both the ix and event discrimant
const LOG_DISCRIMINANT: &[u8] = &[
    0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d,
    0xbd, 0xdb, 0x7f, 0xd3, 0x4e, 0xe6, 0x61, 0xee,
];

/// Pump.fun have two variants:
/// 1. buy [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea] (3, 6=in sol, 5=out token)
/// 2. sell [0x33, 0xe6, 0x85, 0xa4, 0x01, 0x7f, 0x83, 0xad] (3, 6=out sol, 5=in token)
/// In/out amounts follows the discriminant, with the first one being exact and the other being the worst acceptable value.
/// SOL transfers use the system program instead of token program.
/// Swap direction is determined instruction's name.
/// This one requires custom logic for event parsing since it issues so many transfer for all sorts of fees (all in SOL).
/// mint[16..48], sol amount [48..56], token amount [56..64], is buy [64], fee [177..185], creator fee [225..233]
impl PumpFunSwapFinder {
    fn user_in_out_index(ix_data: &[u8]) -> (usize, usize) {
        if ix_data[0] == 0x66 {
            // buy
            (6, 5)
        } else {
            // sell
            (5, 6)
        }
    }

    fn swap_from_pdf_trade_event(outer_program: Option<String>, amm: Pubkey, input_ata: Pubkey, output_ata: Pubkey, data: &[u8], inner_ix_index: Option<u32>) -> SwapV2 {
        let mint = pubkey_from_slice(&data[16..48]);
        let sol_amount = u64::from_le_bytes(data[48..56].try_into().unwrap());
        let token_amount = u64::from_le_bytes(data[56..64].try_into().unwrap());
        let is_buy = data[64] != 0;
        let fee = u64::from_le_bytes(data[177..185].try_into().unwrap());
        let creator_fee = u64::from_le_bytes(data[225..233].try_into().unwrap());
        let (input_mint, output_mint) = if is_buy {
            (WSOL_MINT, mint)
        } else {
            (mint, WSOL_MINT)
        };
        let (input_amount, output_amount) = if is_buy {
            (sol_amount + fee + creator_fee, token_amount)
        } else {
            (token_amount, sol_amount - fee - creator_fee)
        };
        SwapV2::new(
            outer_program,
            PDF_PUBKEY.to_string(),
            amm.to_string(),
            input_mint.to_string(),
            output_mint.to_string(),
            input_amount,
            output_amount,
            input_ata.to_string(),
            output_ata.to_string(),
            // todo: should try to locate the actual ix
            None,
            None,
            0,
            0,
            0,
            0,
            inner_ix_index,
        )
    }
}

impl SwapFinder for PumpFunSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[3].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[3] as usize]
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

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, _meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        if ix.program_id == PDF_PUBKEY {
            for inner_ix in inner_ixs.instructions.iter() {
                if inner_ix.data.len() == 266 && inner_ix.data[0..16] == LOG_DISCRIMINANT[..] {
                    let is_buy = inner_ix.data[64] != 0;
                    let (in_index, out_index) = if is_buy {
                        (6, 5) // in sol, out token
                    } else {
                        (5, 6) // in token, out sol
                    };
                    return vec![
                        Self::swap_from_pdf_trade_event(
                            None,
                            ix.accounts[3].pubkey,
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
            if account_keys[inner_ix.program_id_index as usize] != PDF_PUBKEY {
                continue; // Not a Pump.fun instruction
            }
            if inner_ix.data.len() < 24 {
                continue; // Not a swap
            }
            if inner_ix.data.starts_with(&[0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea]) ||
               inner_ix.data.starts_with(&[0x33, 0xe6, 0x85, 0xa4, 0x01, 0x7f, 0x83, 0xad]) {
                // Valid swap instruction
                let (input_ata, output_ata) = Self::user_ata_inner_ix(inner_ix, account_keys);
                for j in i + 1..inner_ixs.instructions.len() {
                    let next_inner_ix = &inner_ixs.instructions[j];
                    if next_inner_ix.program_id_index >= account_keys.len() as u32 {
                        continue; // Skip invalid program ID
                    }
                    if account_keys[next_inner_ix.program_id_index as usize] != PDF_PUBKEY {
                        continue; // Not a Pump.fun instruction
                    }
                    if next_inner_ix.data.len() != 266 || next_inner_ix.data[0..16] != LOG_DISCRIMINANT[..] {
                        continue; // Not an event
                    }
                    swaps.push(Self::swap_from_pdf_trade_event(
                        Some(ix.program_id.to_string()),
                        Self::amm_inner_ix(inner_ix, account_keys),
                        input_ata,
                        output_ata,
                        &next_inner_ix.data,
                        Some(i as u32),
                    ));
                    next_logical_ix = j + 1;
                }
            }
        }
        swaps
    }
}