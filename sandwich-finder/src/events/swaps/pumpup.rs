use std::sync::Arc;

use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::{events::{addresses::PUMPUP_PUBKEY, swap::{SwapFinder, SwapV2}, swaps::private::Sealed}, utils::pubkey_from_slice};

impl Sealed for PumpupSwapFinder {}

pub struct PumpupSwapFinder {}

// Includes both the ix and event discrimant
const LOG_DISCRIMINANT: &[u8] = &[
    0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d,
    0xa3, 0x26, 0x5b, 0x65, 0x78, 0x94, 0x97, 0x5a,
];
const SWAP: &[u8] = &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8];

/// pdmd appears to have 1 variant
impl PumpupSwapFinder {
    fn swap_from_pdf_trade_event(outer_program: Option<Arc<str>>, amm: Pubkey, input_ata: Pubkey, output_ata: Pubkey, data: &[u8], inner_ix_index: Option<u32>) -> SwapV2 {
        let input_mint = pubkey_from_slice(&data[89..121]);
        let input_amount = u64::from_le_bytes(data[121..129].try_into().unwrap());
        let output_mint = pubkey_from_slice(&data[49..81]);
        let output_amount = u64::from_le_bytes(data[81..89].try_into().unwrap());
        SwapV2::new(
            outer_program,
            PUMPUP_PUBKEY.to_string().into(),
            pubkey_from_slice(&data[137..169]).to_string().into(),
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

impl SwapFinder for PumpupSwapFinder {
    fn amm_ix(ix: &Instruction) -> Pubkey {
        ix.accounts[0].pubkey
    }

    fn amm_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> Pubkey {
        account_keys[inner_ix.accounts[0] as usize]
    }

    fn user_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[3].pubkey,
            ix.accounts[4].pubkey,
        )
    }

    fn user_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[3] as usize],
            account_keys[inner_ix.accounts[4] as usize],
        )
    }

    fn pool_ata_ix(ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            ix.accounts[2].pubkey,
            ix.accounts[1].pubkey,
        )
    }

    fn pool_ata_inner_ix(inner_ix: &InnerInstruction, account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            account_keys[inner_ix.accounts[2] as usize],
            account_keys[inner_ix.accounts[1] as usize],
        )
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, _meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        if ix.program_id == PUMPUP_PUBKEY {
            for inner_ix in inner_ixs.instructions.iter() {
                if inner_ix.data.len() >= 193 && inner_ix.data[0..16] == LOG_DISCRIMINANT[..] {
                    return vec![
                        Self::swap_from_pdf_trade_event(
                            None,
                            ix.accounts[0].pubkey,
                            ix.accounts[3].pubkey,
                            ix.accounts[4].pubkey,
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
            if account_keys[inner_ix.program_id_index as usize] != PUMPUP_PUBKEY {
                continue; // Not a sugar instruction
            }
            if inner_ix.data.len() < 24 {
                continue; // Not a swap
            }
            match &inner_ix.data[..8] {
                SWAP => {
                    // Valid swap instruction
                    let (input_ata, output_ata) = Self::user_ata_inner_ix(inner_ix, account_keys);
                    for j in i + 1..inner_ixs.instructions.len() {
                        let next_inner_ix = &inner_ixs.instructions[j];
                        if next_inner_ix.program_id_index >= account_keys.len() as u32 {
                            continue; // Skip invalid program ID
                        }
                        if account_keys[next_inner_ix.program_id_index as usize] != PUMPUP_PUBKEY {
                            continue; // Not a Pump.fun instruction
                        }
                        if next_inner_ix.data.len() < 193 || next_inner_ix.data[0..16] != LOG_DISCRIMINANT[..] {
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