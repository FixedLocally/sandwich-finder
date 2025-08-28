use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use yellowstone_grpc_proto::prelude::{InnerInstruction, InnerInstructions, TransactionStatusMeta};

use crate::swaps::{addresses::{DLMM_PUBKEY, METEORA_PUBKEY, PDF2_PUBKEY, PDF_PUBKEY, RAYDIUM_CL_PUBKEY, RAYDIUM_LP_PUBKEY, RAYDIUM_V4_PUBKEY, RAYDIUM_V5_PUBKEY, TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID, WHIRLPOOL_PUBKEY}, finder::{SwapFinder, SwapV2}, private::Sealed};

impl Sealed for Discoverer {}

pub struct Discoverer {}

/// Outputs txid and program that triggered >=2 swaps in its inner instructions and emit a special swap event.
impl SwapFinder for Discoverer {
    fn amm_ix(_ix: &Instruction) -> Pubkey {
        Pubkey::default()
    }

    fn amm_inner_ix(_inner_ix: &InnerInstruction, _account_keys: &Vec<Pubkey>) -> Pubkey {
        Pubkey::default()
    }

    fn user_ata_ix(_ix: &Instruction) -> (Pubkey, Pubkey) {
        (
            Pubkey::default(),
            Pubkey::default(),
        )
    }

    fn user_ata_inner_ix(_inner_ix: &InnerInstruction, _account_keys: &Vec<Pubkey>) -> (Pubkey, Pubkey) {
        (
            Pubkey::default(),
            Pubkey::default(),
        )
    }

    fn find_swaps(ix: &Instruction, inner_ixs: &InnerInstructions, account_keys: &Vec<Pubkey>, _meta: &TransactionStatusMeta) -> Vec<SwapV2> {
        // ignore known programs
        match ix.program_id {
            RAYDIUM_V4_PUBKEY | RAYDIUM_V5_PUBKEY | RAYDIUM_LP_PUBKEY | RAYDIUM_CL_PUBKEY | PDF_PUBKEY | PDF2_PUBKEY | WHIRLPOOL_PUBKEY | DLMM_PUBKEY | METEORA_PUBKEY => vec![],
            _ => {
                let mut swap_count = 0;
                for inner_ix in &inner_ixs.instructions {
                    let program_id = account_keys[inner_ix.program_id_index as usize];
                    match program_id {
                        TOKEN_PROGRAM_ID | TOKEN_2022_PROGRAM_ID => {
                            if inner_ix.data.len() < 9 {
                                continue;
                            }
                            match inner_ix.data[0] {
                                3 | 12 => { // Transfer or TransferChecked
                                    swap_count += 1;
                                },
                                _ => {}
                            }
                        },
                        _ => {}
                    }
                }
                if swap_count >= 2 {
                    return vec![
                        SwapV2::new(None, ix.program_id.to_string(), "".to_string(), "".to_string(), "".to_string(), 0, 0, "".to_string(), "".to_string(), 0, 0, 0, 0, None),
                    ];
                }
                vec![]
            }
        }
    }
}